use anyhow::{bail, Context, Result};
use chrono::{Datelike, Local, NaiveDate};
use futures::StreamExt;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{thread_rng, SeedableRng};
use regex::RegexBuilder;
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::ai::{AiProvider, ChatMessage, ProviderRequest, StreamEvent};
use crate::books::{is_old_testament, normalize_book};
use crate::cache::{
    installed_translations, known_source, known_translation, known_translations,
    load_default_translation, normalize_translation_id, preload, read_manifest, remove_translation,
    save_default_translation, CachePaths, DEFAULT_TRANSLATION,
};
use crate::cli::{
    AiArgs, CacheArgs, DiffArgs, EchoArgs, ExportArgs, ExportTarget, MoodArgs, ParallelArgs,
    PlanAction, PlanArgs, PlanDoneArgs, PlanTodayArgs, PlanUndoArgs, RandomArgs, ReadArgs,
    SearchArgs, Testament, TodayArgs, TopicArgs, TranslationAction, TranslationArgs, TuiArgs,
};
use crate::diff::{diff_tokens, DiffOp};
use crate::hashing::splitmix64;
use crate::moods::{all_moods, find_mood};
use crate::output::{
    verse_id, verse_reference, Format, MarkdownRenderer, OutputStyle, ThinkingIndicator,
};
use crate::plans::{
    all_plans, build_days, clear_state, find_plan, load_state, portion_label, save_state, PlanDef,
    PlanState,
};
use crate::reference::{parse_reference, parse_references};
use crate::topics::{all_topics, find_topic};
use crate::tui;
use crate::verses::{load_verses, Verse, VerseIndex};

pub fn run_cache(args: &CacheArgs, paths: &CachePaths) -> Result<()> {
    let id = &paths.translation;

    if args.preload {
        let count = preload(paths, id, args.source.as_deref())?;
        println!("{} cached: {} verses", id.to_uppercase(), count);
        return Ok(());
    }

    if args.status {
        return run_cache_status(paths);
    }

    println!("Cache root: {}", paths.root.display());
    if paths.verses_path().exists() {
        if let Some(manifest) = read_manifest(&paths.manifest_path()) {
            println!(
                "{}: ready ({} verses)",
                id.to_uppercase(),
                manifest.verse_count
            );
            println!("Source: {}", manifest.source);
            println!("Updated: {}", manifest.created_at);
        } else {
            println!("{}: ready", id.to_uppercase());
        }
    } else {
        println!(
            "{}: missing. Run `bible cache --preload`.",
            id.to_uppercase()
        );
    }

    Ok(())
}

fn run_cache_status(paths: &CachePaths) -> Result<()> {
    println!("Cache root: {}", paths.root.display());
    let installed = installed_translations(paths);
    if installed.is_empty() {
        println!("No translations installed. Run `bible cache --preload`.");
        return Ok(());
    }
    // A leading "*" marks the active translation.
    for t in installed {
        let marker = if t.id == paths.translation { "*" } else { " " };
        match t.manifest {
            Some(m) => println!(
                "{} {:<6} {} verses, {}  (updated {})",
                marker,
                t.id,
                m.verse_count,
                human_size(t.size_bytes),
                m.created_at
            ),
            None => println!("{} {:<6} {}", marker, t.id, human_size(t.size_bytes)),
        }
    }
    Ok(())
}

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

/// "Not cached" hint that names the translation and how to install it.
fn missing_cache_msg(id: &str) -> String {
    if known_source(id).is_some() {
        format!(
            "{} not cached. Run `bible translation add {}`.",
            id.to_uppercase(),
            id
        )
    } else {
        format!(
            "{} not cached. Run `bible translation add {} --source <url-or-path>` (see `bible translation available`).",
            id.to_uppercase(),
            id
        )
    }
}

/// Parse a `--with kjv,bbe` list into normalized, de-duplicated translation ids.
fn parse_translation_list(with: &str) -> Result<Vec<String>> {
    let mut ids: Vec<String> = Vec::new();
    for raw in with.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let id = normalize_translation_id(raw)?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// Load each translation in `ids`, failing with an install hint for any missing.
fn load_translations(paths: &CachePaths, ids: &[String]) -> Result<Vec<Vec<Verse>>> {
    let mut loaded = Vec::with_capacity(ids.len());
    for id in ids {
        if !paths.is_installed(id) {
            bail!("{}", missing_cache_msg(id));
        }
        loaded.push(load_verses(&paths.verses_path_for(id))?);
    }
    Ok(loaded)
}

/// Load the active translation's verses, with an install hint if it is missing.
fn load_active(paths: &CachePaths) -> Result<Vec<Verse>> {
    load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))
}

pub fn run_read(args: &ReadArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let references = parse_references(&args.reference)?;
    let verses = load_active(paths)?;
    let index = VerseIndex::build(&verses);

    // A lone multi-chapter book reads as an overview in the human view; data
    // formats (and one-chapter books like Jude) get the verses themselves.
    if let [reference] = references.as_slice() {
        if reference.chapter.is_none() && !output.is_structured() {
            if let Some(chapters) = index.max_chapter(&reference.book).filter(|&c| c > 1) {
                print_book_overview(&index, &reference.book, chapters);
                return Ok(());
            }
        }
    }

    let selected = index.resolve_all(&references)?;
    output.emit_verses(&selected);
    Ok(())
}

pub fn run_search(args: &SearchArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let verses = load_active(paths)?;

    let book_filter = match args.book.as_ref() {
        Some(book) => {
            let normalized =
                normalize_book(book).ok_or_else(|| anyhow::anyhow!("Unknown book: {}", book))?;
            Some(normalized.to_string())
        }
        None => None,
    };

    let matcher = build_matcher(args)?;

    // Scan the whole corpus so counts and ordering are complete, then limit for
    // display (unless --count, which reports the full total).
    let mut matches: Vec<&Verse> = Vec::new();
    for verse in &verses {
        if let Some(ref book) = book_filter {
            if &verse.book != book {
                continue;
            }
        }
        if matcher.is_match(&verse.text) {
            matches.push(verse);
        }
    }

    if args.count {
        println!("{}", matches.len());
        return Ok(());
    }

    if matches.is_empty() {
        if !output.is_structured() {
            println!("No matches found.");
        }
        return Ok(());
    }

    matches.truncate(args.limit);
    output.emit_verses(&matches);
    Ok(())
}

/// A compiled query matcher: substring (default), whole-word, or full regex.
/// All matching is case-insensitive.
enum Matcher {
    Substring(String),
    Regex(regex::Regex),
}

impl Matcher {
    fn is_match(&self, text: &str) -> bool {
        match self {
            Matcher::Substring(needle) => text.to_lowercase().contains(needle),
            Matcher::Regex(re) => re.is_match(text),
        }
    }
}

fn build_matcher(args: &SearchArgs) -> Result<Matcher> {
    if args.regex || args.word {
        let pattern = if args.word {
            // Whole-word match; the query is escaped unless it is already a regex.
            let inner = if args.regex {
                args.query.clone()
            } else {
                regex::escape(&args.query)
            };
            format!(r"\b(?:{})\b", inner)
        } else {
            args.query.clone()
        };
        let re = RegexBuilder::new(&pattern)
            .case_insensitive(true)
            .build()
            .with_context(|| format!("Invalid regex: {}", args.query))?;
        Ok(Matcher::Regex(re))
    } else {
        Ok(Matcher::Substring(args.query.to_lowercase()))
    }
}

pub fn run_today(args: &TodayArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let verses = load_active(paths)?;

    let book_filter = normalize_book_filter(args.book.as_deref())?;
    let pool = filter_verses(&verses, book_filter.as_deref(), args.testament);
    if pool.is_empty() {
        bail!("No verses match those constraints.");
    }

    let date = Local::now().date_naive();
    let day_seed = date.num_days_from_ce() as usize;
    // Hash the day number: a plain modulo walked through the corpus one verse
    // per day, so tomorrow's verse was always the one after today's.
    let verse = pool[(splitmix64(day_seed as u64) % pool.len() as u64) as usize];

    output.emit_verses(&[verse]);
    if !output.is_structured() {
        println!("Prompt: {}", daily_prompt(day_seed));
    }
    Ok(())
}

pub fn run_random(args: &RandomArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let verses = load_active(paths)?;

    let book_filter = normalize_book_filter(args.book.as_deref())?;
    let mut pool = filter_verses(&verses, book_filter.as_deref(), args.testament);
    if let Some(max) = args.max_words {
        pool.retain(|v| v.text.split_whitespace().count() <= max);
    }
    if pool.is_empty() {
        bail!("No verses match those constraints.");
    }

    let count = args.count.max(1).min(pool.len());
    let chosen: Vec<&Verse> = if let Some(seed) = args.seed {
        let mut rng = StdRng::seed_from_u64(seed);
        pool.choose_multiple(&mut rng, count).copied().collect()
    } else {
        let mut rng = thread_rng();
        pool.choose_multiple(&mut rng, count).copied().collect()
    };

    output.emit_verses(&chosen);
    Ok(())
}

/// Normalize an optional `--book` argument to its canonical name, erroring on
/// an unknown book.
fn normalize_book_filter(book: Option<&str>) -> Result<Option<String>> {
    match book {
        Some(book) => {
            let normalized =
                normalize_book(book).ok_or_else(|| anyhow::anyhow!("Unknown book: {}", book))?;
            Ok(Some(normalized.to_string()))
        }
        None => Ok(None),
    }
}

/// Filter the verse list by an optional book and/or testament.
fn filter_verses<'a>(
    verses: &'a [Verse],
    book: Option<&str>,
    testament: Option<Testament>,
) -> Vec<&'a Verse> {
    verses
        .iter()
        .filter(|v| match book {
            Some(b) => v.book == b,
            None => true,
        })
        .filter(|v| match testament {
            Some(Testament::Ot) => is_old_testament(&v.book) == Some(true),
            Some(Testament::Nt) => is_old_testament(&v.book) == Some(false),
            None => true,
        })
        .collect()
}

pub fn run_echo(args: &EchoArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let reference = parse_reference(&args.reference)?;
    if reference.verse.is_none() {
        bail!("Echo needs a verse, e.g. `bible echo John 3:16` or `bible echo John 3:16-18`");
    }

    let verses = load_active(paths)?;
    let index = VerseIndex::build(&verses);
    let selected = index.resolve(&reference)?;
    let context = index.with_context(&selected, args.window as usize);

    if output.is_structured() {
        output.emit_verses(&context);
        return Ok(());
    }

    // Every verse the reference names is marked, so ranges and lists echo too.
    for verse in &context {
        let marker = if selected.iter().any(|s| std::ptr::eq(*s, *verse)) {
            "*"
        } else {
            " "
        };
        println!("{}", output.marked_verse_line(marker, verse));
    }

    Ok(())
}

pub fn run_mood(args: &MoodArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    if args.list || args.mood.is_none() {
        println!("Available moods:");
        for mood in all_moods() {
            println!("- {}: {}", mood.name, mood.description);
        }
        return Ok(());
    }

    let mood_name = args.mood.as_ref().unwrap();
    let mood =
        find_mood(mood_name).ok_or_else(|| anyhow::anyhow!("Unknown mood: {}", mood_name))?;

    let verses = load_active(paths)?;
    let index = VerseIndex::build(&verses);

    let selected: Vec<&Verse> = mood
        .refs
        .iter()
        .filter_map(|r| index.get(r.book, r.chapter, r.verse))
        .collect();

    if !output.is_structured() {
        println!("Mood: {}", mood.name);
    }
    output.emit_verses(&selected);

    Ok(())
}

/// Upper bound on verses sent to an AI provider, so `bible ai Psalms` cannot
/// silently ship a whole book (2,461 verses) in one request.
const MAX_AI_VERSES: usize = 300;

pub async fn run_ai(args: &AiArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let references = parse_references(&args.reference)?;
    if references.iter().any(|r| r.chapter.is_none()) {
        bail!("AI prompts need a chapter, e.g. `bible ai John 3` or `bible ai John 3:16`");
    }
    let verses = load_active(paths)?;
    let index = VerseIndex::build(&verses);

    let mut selected: Vec<&Verse> = Vec::new();
    for reference in &references {
        let passage = index.resolve(reference)?;
        if args.window > 0 {
            selected.extend(index.with_context(&passage, args.window as usize));
        } else {
            selected.extend(passage);
        }
    }
    if selected.len() > MAX_AI_VERSES {
        bail!(
            "{} is {} verses; AI prompts are limited to {}. Try a shorter passage.",
            index.label(&selected),
            selected.len(),
            MAX_AI_VERSES
        );
    }

    if args.chat {
        return run_ai_chat_streaming(args, &selected, output).await;
    }

    // Non-chat mode: single request with streaming
    run_ai_single_streaming(args, &selected, output).await
}

/// The model to use: an explicit `--model`, else the provider's default.
fn resolve_model(provider: &str, model: Option<&str>) -> Result<String> {
    match model {
        Some(model) => Ok(model.to_string()),
        None => AiProvider::default_model(provider)
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown provider: {} (supported: openai, anthropic)",
                    provider
                )
            }),
    }
}

async fn run_ai_single_streaming(
    args: &AiArgs,
    selected: &[&Verse],
    output: &OutputStyle,
) -> Result<()> {
    // Print verses first
    for verse in selected {
        println!("{}", output.verse_line(verse));
    }
    println!();

    let provider = AiProvider::from_name(&args.provider)?;
    let prompt = build_ai_prompt(selected);
    let request = ProviderRequest {
        model: resolve_model(&args.provider, args.model.as_deref())?,
        system: Some("You are a thoughtful Bible assistant.".to_string()),
        messages: vec![chat_message("user", prompt)],
        max_tokens: Some(args.max_tokens),
        temperature: Some(args.temperature),
    };

    let indicator = ThinkingIndicator::new();
    indicator.start();

    let mut stream = provider.stream_request(&request);
    let mut response_text = String::new();
    let mut first_token = true;

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::Start => {}
            StreamEvent::Delta(text) => {
                if first_token {
                    indicator.finish();
                    first_token = false;
                }
                print!("{}", text);
                io::stdout().flush()?;
                response_text.push_str(&text);
            }
            StreamEvent::Done => break,
        }
    }

    if first_token {
        indicator.finish();
    }

    println!();
    println!();

    // Optionally render with markdown if content looks like it has formatting
    if output.color && contains_markdown(&response_text) {
        let renderer = MarkdownRenderer::new(true);
        output.print_dim("(Formatted response)");
        renderer.render(&response_text);
    }

    Ok(())
}

async fn run_ai_chat_streaming(
    args: &AiArgs,
    selected: &[&Verse],
    output: &OutputStyle,
) -> Result<()> {
    const BASE_MESSAGES: usize = 1;
    const MAX_HISTORY_MESSAGES: usize = 16;
    const SYSTEM_PROMPT: &str = "You are a thoughtful Bible assistant. Use the passage context in the conversation. Format your responses with markdown when helpful.";

    let mut current_model = resolve_model(&args.provider, args.model.as_deref())?;
    let mut current_provider = args.provider.to_lowercase();

    // Print verses
    output.print_separator();
    for verse in selected {
        println!("{}", output.verse_line(verse));
    }
    output.print_separator();
    println!();
    output.print_chat_intro();
    println!();

    let passage = build_passage_text(selected);
    let mut history = vec![chat_message("user", format!("Passage:\n{}", passage))];

    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    let markdown_renderer = MarkdownRenderer::new(output.color);

    loop {
        output.print_user_prompt();

        let input_line: String = match lines.next_line().await? {
            Some(line) => line,
            None => break,
        };

        let line = input_line.trim();
        if line.is_empty() {
            continue;
        }

        // Handle commands
        match line {
            "/exit" | "/quit" => break,
            "/reset" => {
                history.truncate(BASE_MESSAGES);
                output.print_dim("(chat reset)");
                continue;
            }
            "/help" => {
                print_chat_help(output);
                continue;
            }
            _ => {}
        }

        if let Some(rest) = line.strip_prefix("/model") {
            let model = rest.trim().to_string();
            if model.is_empty() {
                output.print_dim(&format!("Current model: {}", current_model));
                output.print_dim("Usage: /model <name>");
            } else {
                current_model = model;
                output.print_dim(&format!("Model set to {}", current_model));
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("/provider") {
            let provider_name = rest.trim().to_string();
            if provider_name.is_empty() {
                output.print_dim(&format!("Current provider: {}", current_provider));
                output.print_dim("Usage: /provider <openai|anthropic>");
            } else if let Some(model) = AiProvider::default_model(&provider_name) {
                // Model names are provider-specific, so the model switches too.
                current_provider = provider_name.to_lowercase();
                current_model = model.to_string();
                output.print_dim(&format!(
                    "Provider set to {} (model {})",
                    current_provider, current_model
                ));
            } else {
                output.print_dim(&format!(
                    "Unknown provider: {} (supported: openai, anthropic)",
                    provider_name
                ));
            }
            continue;
        }

        // Add user message to history
        history.push(chat_message("user", line));
        trim_chat_history(&mut history, BASE_MESSAGES, MAX_HISTORY_MESSAGES);

        // Create provider and request
        let provider = match AiProvider::from_name(&current_provider) {
            Ok(p) => p,
            Err(e) => {
                output.print_dim(&format!("Error: {}", e));
                history.pop(); // Remove the failed message
                continue;
            }
        };

        let request = ProviderRequest {
            model: current_model.clone(),
            system: Some(SYSTEM_PROMPT.to_string()),
            messages: history.clone(),
            max_tokens: Some(args.max_tokens),
            temperature: Some(args.temperature),
        };

        println!();

        // Show thinking indicator and stream response
        let indicator = ThinkingIndicator::new();
        indicator.start();

        let mut stream = provider.stream_request(&request);
        let mut response_text = String::new();
        let mut first_token = true;

        while let Some(event) = stream.next().await {
            match event {
                Ok(StreamEvent::Start) => {}
                Ok(StreamEvent::Delta(text)) => {
                    if first_token {
                        indicator.finish();
                        first_token = false;
                    }
                    print!("{}", text);
                    io::stdout().flush()?;
                    response_text.push_str(&text);
                }
                Ok(StreamEvent::Done) => break,
                Err(e) => {
                    indicator.finish();
                    output.print_dim(&format!("\nError: {}", e));
                    break;
                }
            }
        }

        if first_token {
            indicator.finish();
        }

        println!();

        // Render markdown version if the response has formatting
        if output.color && !response_text.is_empty() && contains_markdown(&response_text) {
            println!();
            output.print_separator();
            markdown_renderer.render(&response_text);
            output.print_separator();
        }

        println!();

        // Add assistant response to history
        if !response_text.is_empty() {
            history.push(chat_message("assistant", &response_text));
            trim_chat_history(&mut history, BASE_MESSAGES, MAX_HISTORY_MESSAGES);
        }
    }

    Ok(())
}

fn print_book_overview(index: &VerseIndex, book: &str, chapters: u16) {
    println!(
        "{} has {} chapters and {} verses.",
        book,
        chapters,
        index.book(book).len()
    );
    println!("Tip: bible read {} <chapter>", book);
}

fn daily_prompt(seed: usize) -> &'static str {
    const PROMPTS: &[&str] = &[
        "What word or phrase sticks with you today?",
        "Where does this verse meet your day?",
        "What is one small action this invites?",
        "What is the hardest line to live, and why?",
        "Read it twice, slowly. What changes?",
    ];
    PROMPTS[seed % PROMPTS.len()]
}

fn build_ai_prompt(selected: &[&Verse]) -> String {
    let mut prompt = String::from(
        "You are a helpful assistant. Provide a concise reflection on the passage below.\n\n",
    );
    prompt.push_str("Passage:\n");
    prompt.push_str(&build_passage_text(selected));
    prompt.push_str("\nResponse:");
    prompt
}

fn build_passage_text(selected: &[&Verse]) -> String {
    let mut passage = String::new();
    for verse in selected {
        let line = format!(
            "{} {}:{} {}\n",
            verse.book, verse.chapter, verse.verse, verse.text
        );
        passage.push_str(&line);
    }
    passage
}

fn trim_chat_history(history: &mut Vec<ChatMessage>, base_messages: usize, max_recent: usize) {
    if history.len() <= base_messages + max_recent {
        return;
    }
    let keep_from = history.len().saturating_sub(max_recent);
    history.drain(base_messages..keep_from);
}

fn print_chat_help(output: &OutputStyle) {
    output.print_dim("Commands:");
    output.print_dim("  /help     Show this help");
    output.print_dim("  /model    Show or change the model");
    output.print_dim("  /provider Show or change the provider");
    output.print_dim("  /reset    Clear conversation history");
    output.print_dim("  /exit     Quit chat");
}

fn chat_message(role: &str, content: impl Into<String>) -> ChatMessage {
    ChatMessage {
        role: role.to_string(),
        content: content.into(),
    }
}

fn contains_markdown(text: &str) -> bool {
    // Check for common markdown patterns
    text.contains("```")
        || text.contains("**")
        || text.contains("##")
        || text.contains("- ")
        || text.contains("1. ")
        || text.contains("> ")
}

pub fn run_topic(args: &TopicArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    if args.list || args.topic.is_none() {
        println!("Available topics:");
        for t in all_topics() {
            println!("- {}: {}", t.name, t.description);
        }
        return Ok(());
    }

    let name = args.topic.as_ref().unwrap();
    let topic = find_topic(name).ok_or_else(|| anyhow::anyhow!("Unknown topic: {}", name))?;

    if args.refs_only {
        for r in topic.refs {
            println!("{} {}:{}", r.book, r.chapter, r.verse);
        }
        return Ok(());
    }

    let verses = load_active(paths)?;
    let index = VerseIndex::build(&verses);
    let selected: Vec<&Verse> = topic
        .refs
        .iter()
        .filter_map(|r| index.get(r.book, r.chapter, r.verse))
        .collect();

    if !output.is_structured() {
        println!("Topic: {}", topic.name);
    }
    output.emit_verses(&selected);
    Ok(())
}

pub fn run_export(args: &ExportArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let references = parse_references(&args.reference)?;
    let verses = load_active(paths)?;
    let index = VerseIndex::build(&verses);
    let selected = index.resolve_all(&references)?;
    let _ = output; // export format is controlled by --to, not the global format

    match args.to {
        ExportTarget::Md => {
            println!(
                "## {} ({})",
                index.label(&selected),
                paths.translation.to_uppercase()
            );
            println!();
            for v in &selected {
                println!("**{} {}:{}** {}", v.book, v.chapter, v.verse, v.text);
                println!();
            }
        }
        ExportTarget::Anki => {
            for v in &selected {
                // front<TAB>back; tabs/newlines in text are unlikely but stripped.
                let text = v.text.replace(['\t', '\n'], " ");
                println!("{} {}:{}\t{}", v.book, v.chapter, v.verse, text);
            }
        }
        ExportTarget::Json => {
            println!("{}", crate::output::verses_to_json(&selected));
        }
        ExportTarget::Txt => {
            for v in &selected {
                println!("{}", v.text);
            }
        }
    }
    Ok(())
}

pub fn run_parallel(args: &ParallelArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let references = parse_references(&args.reference)?;

    let ids = parse_translation_list(&args.with)?;
    if ids.is_empty() {
        bail!("Provide translations to compare, e.g. --with kjv,bbe");
    }

    // Load every requested translation up front.
    let loaded = load_translations(paths, &ids)?;
    let indexes: Vec<VerseIndex> = loaded.iter().map(|v| VerseIndex::build(v)).collect();

    // The first translation defines the versification we iterate over.
    let base = indexes[0].resolve_all(&references)?;

    // Each translation's text for a verse, in --with order (None where missing).
    let texts = |v: &Verse| -> Vec<Option<&str>> {
        indexes
            .iter()
            .map(|index| {
                index
                    .get(&v.book, v.chapter, v.verse)
                    .map(|t| t.text.as_str())
            })
            .collect()
    };

    match output.format {
        Format::Plain => {}
        Format::Json | Format::Ndjson => {
            let records: Vec<serde_json::Value> = base
                .iter()
                .map(|v| {
                    let translations: serde_json::Map<String, serde_json::Value> = ids
                        .iter()
                        .zip(texts(v))
                        .map(|(id, text)| (id.clone(), serde_json::json!(text)))
                        .collect();
                    serde_json::json!({
                        "id": verse_id(v),
                        "reference": verse_reference(v),
                        "translations": translations,
                    })
                })
                .collect();
            output.emit_json_records(&records);
            return Ok(());
        }
        Format::Tsv => {
            // id, reference, then one text column per translation in --with order.
            for v in &base {
                let mut row = vec![verse_id(v), verse_reference(v)];
                row.extend(texts(v).into_iter().map(|t| t.unwrap_or("").to_string()));
                println!("{}", row.join("\t"));
            }
            return Ok(());
        }
        Format::Ref => {
            for v in &base {
                println!("{}", verse_reference(v));
            }
            return Ok(());
        }
        Format::Raw => {
            // Each translation's text on its own line, verse by verse.
            for v in &base {
                for text in texts(v) {
                    println!("{}", text.unwrap_or(""));
                }
            }
            return Ok(());
        }
    }

    // Human view: per verse, the reference then each translation's text, labeled
    // and aligned by translation id.
    let label_width = ids.iter().map(|id| id.len()).max().unwrap_or(3);
    for (n, v) in base.iter().enumerate() {
        if n > 0 {
            println!();
        }
        let reference = format!("{} {}:{}", v.book, v.chapter, v.verse);
        output.print_reference_heading(&reference);
        for (i, id) in ids.iter().enumerate() {
            let text = indexes[i]
                .get(&v.book, v.chapter, v.verse)
                .map(|t| t.text.as_str())
                .unwrap_or("(missing)");
            println!("  {:width$}  {}", id, text, width = label_width);
        }
    }
    Ok(())
}

pub fn run_translation(args: &TranslationArgs, paths: &CachePaths) -> Result<()> {
    match &args.action {
        TranslationAction::List => {
            let installed = installed_translations(paths);
            if installed.is_empty() {
                println!("No translations installed. Run `bible cache --preload`.");
                return Ok(());
            }
            let width = installed.iter().map(|t| t.id.len()).max().unwrap_or(0);
            for t in installed {
                let marker = if t.id == paths.translation { "*" } else { " " };
                let count = t
                    .manifest
                    .map(|m| format!("{} verses", m.verse_count))
                    .unwrap_or_default();
                let name = known_translation(&t.id).map(|k| k.name).unwrap_or("");
                let line = format!("{} {:<width$}  {:<12}  {}", marker, t.id, count, name);
                println!("{}", line.trim_end());
            }
            Ok(())
        }
        TranslationAction::Available => {
            println!("Install any of these with `bible translation add <id>`:");
            let width = known_translations().iter().map(|t| t.id.len()).max();
            let name_width = known_translations().iter().map(|t| t.name.len()).max();
            for t in known_translations() {
                let marker = if t.id == paths.translation { "*" } else { " " };
                let status = if paths.is_installed(t.id) {
                    "installed"
                } else {
                    ""
                };
                let line = format!(
                    "{} {:<width$}  {:<name_width$}  {}",
                    marker,
                    t.id,
                    t.name,
                    status,
                    width = width.unwrap_or(0),
                    name_width = name_width.unwrap_or(0)
                );
                println!("{}", line.trim_end());
            }
            Ok(())
        }
        TranslationAction::Add(a) => {
            let id = normalize_translation_id(&a.id)?;
            let count = preload(paths, &id, a.source.as_deref())?;
            println!("{} installed: {} verses", id.to_uppercase(), count);
            Ok(())
        }
        TranslationAction::Default(a) => {
            let id = normalize_translation_id(&a.id)?;
            if !paths.is_installed(&id) {
                bail!(
                    "{} is not installed. Run `bible translation add {}` first.",
                    id.to_uppercase(),
                    id
                );
            }
            save_default_translation(&paths.root, Some(&id))?;
            println!("Default translation set to {}", id);
            Ok(())
        }
        TranslationAction::Remove(a) => {
            let id = normalize_translation_id(&a.id)?;
            if remove_translation(paths, &id)? {
                println!("Removed {}", id);
                // Don't leave the config pointing at a translation that is gone.
                if load_default_translation(&paths.root).as_deref() == Some(id.as_str()) {
                    save_default_translation(&paths.root, None)?;
                    println!(
                        "{} was the default translation; the default is now {}.",
                        id, DEFAULT_TRANSLATION
                    );
                }
            } else {
                println!("{} was not installed", id);
            }
            Ok(())
        }
    }
}

pub fn run_tui(args: &TuiArgs, paths: &CachePaths) -> Result<()> {
    let verses = load_active(paths)?;

    tui::run(verses, args.book.clone(), args.r#ref.clone())
}

pub fn run_plan(args: &PlanArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    match &args.action {
        PlanAction::List => {
            let active = load_state(&paths.root);
            println!("Available plans:");
            for p in all_plans() {
                let marker = match &active {
                    Some(s) if s.plan_id == p.id => "*",
                    _ => " ",
                };
                println!(
                    "{} {:<20} {:>3} days  {} — {}",
                    marker, p.id, p.days, p.name, p.description
                );
            }
            Ok(())
        }
        PlanAction::Start(a) => {
            let plan = find_plan(&a.id)
                .ok_or_else(|| anyhow::anyhow!("Unknown plan: {}. See `bible plan list`.", a.id))?;
            // Starting over used to silently wipe the active plan's progress.
            if let Some(active) = load_state(&paths.root) {
                if let Some(active_plan) = find_plan(&active.plan_id) {
                    let done = active.done_count(active_plan.days);
                    if done > 0 && !a.force {
                        bail!(
                            "{} is in progress ({}/{} days done). Run `bible plan start {} --force` to replace it; its progress will be lost.",
                            active_plan.name,
                            done,
                            active_plan.days,
                            plan.id
                        );
                    }
                }
            }
            let state = PlanState {
                plan_id: plan.id.to_string(),
                started: Local::now().date_naive().format("%Y-%m-%d").to_string(),
                completed: Vec::new(),
            };
            save_state(&paths.root, &state)?;
            println!(
                "Started {} ({} days). Try `bible plan today`.",
                plan.name, plan.days
            );
            Ok(())
        }
        PlanAction::Today(a) => run_plan_today(a, paths, output),
        PlanAction::Done(a) => run_plan_done(a, paths),
        PlanAction::Undo(a) => run_plan_undo(a, paths),
        PlanAction::Status => run_plan_status(paths),
        PlanAction::Stop => {
            match load_state(&paths.root) {
                Some(state) if clear_state(&paths.root)? => {
                    println!("Stopped {}.", state.plan_id)
                }
                _ => println!("No active plan."),
            }
            Ok(())
        }
    }
}

/// Load the active plan state and its definition, or fail with a start hint.
fn active_plan(paths: &CachePaths) -> Result<(PlanState, &'static PlanDef)> {
    let state = load_state(&paths.root).ok_or_else(|| {
        anyhow::anyhow!("No active plan. Try: bible plan start nt-90 (see `bible plan list`)")
    })?;
    let plan = find_plan(&state.plan_id).ok_or_else(|| {
        anyhow::anyhow!(
            "Active plan '{}' is unknown; run `bible plan stop` and start a new one.",
            state.plan_id
        )
    })?;
    Ok((state, plan))
}

/// The scheduled day number for today (1 on the start date), clamped to the plan.
fn scheduled_day(state: &PlanState, plan: &PlanDef) -> u32 {
    let today = Local::now().date_naive();
    let started = NaiveDate::parse_from_str(&state.started, "%Y-%m-%d").unwrap_or(today);
    let elapsed = (today - started).num_days() + 1;
    elapsed.clamp(1, plan.days as i64) as u32
}

fn run_plan_today(args: &PlanTodayArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let (state, plan) = active_plan(paths)?;

    let day = match args.day {
        Some(day) => {
            if day == 0 || day > plan.days {
                bail!("{} has days 1-{}.", plan.name, plan.days);
            }
            day
        }
        None => match state.next_day(plan.days) {
            Some(day) => day,
            None => {
                println!(
                    "{} is complete — all {} days read. Try `bible plan start <id>` for the next one.",
                    plan.name, plan.days
                );
                return Ok(());
            }
        },
    };

    let verses = load_active(paths)?;
    let days = build_days(plan, &verses)?;
    let portion = &days[(day - 1) as usize];

    if args.refs_only {
        for c in portion {
            println!("{} {}", c.book, c.chapter);
        }
        return Ok(());
    }

    if !output.is_structured() {
        output.print_reference_heading(&format!(
            "{} — Day {}/{}: {}",
            plan.name,
            day,
            plan.days,
            portion_label(portion)
        ));
        let behind = scheduled_day(&state, plan) as i64 - day as i64;
        if behind > 0 {
            output.print_dim(&format!(
                "({} day{} behind — read on!)",
                behind,
                if behind == 1 { "" } else { "s" }
            ));
        }
        println!();
    }

    let index = VerseIndex::build(&verses);
    let mut selected: Vec<&Verse> = Vec::new();
    for c in portion {
        selected.extend(index.chapter(c.book, c.chapter));
    }
    output.emit_verses(&selected);
    Ok(())
}

fn run_plan_done(args: &PlanDoneArgs, paths: &CachePaths) -> Result<()> {
    let (mut state, plan) = active_plan(paths)?;

    let day = match args.day {
        Some(day) => {
            if day == 0 || day > plan.days {
                bail!("{} has days 1-{}.", plan.name, plan.days);
            }
            day
        }
        None => match state.next_day(plan.days) {
            Some(day) => day,
            None => {
                println!("{} is already complete.", plan.name);
                return Ok(());
            }
        },
    };

    if !state.completed.contains(&day) {
        state.completed.push(day);
        state.completed.sort_unstable();
        save_state(&paths.root, &state)?;
    }

    let done = state.done_count(plan.days);
    let remaining = plan.days - done;
    println!(
        "Day {}/{} done — {}% — {} day{} remaining",
        day,
        plan.days,
        done * 100 / plan.days,
        remaining,
        if remaining == 1 { "" } else { "s" }
    );
    if remaining == 0 {
        println!("{} complete. Well done!", plan.name);
    }
    Ok(())
}

fn run_plan_undo(args: &PlanUndoArgs, paths: &CachePaths) -> Result<()> {
    let (mut state, plan) = active_plan(paths)?;

    let day = match args.day {
        Some(day) if state.completed.contains(&day) => day,
        Some(day) => bail!("Day {} is not marked done.", day),
        None => match state.completed.iter().max() {
            Some(&day) => day,
            None => {
                println!("Nothing to undo: no days of {} are marked done.", plan.name);
                return Ok(());
            }
        },
    };

    state.completed.retain(|&d| d != day);
    save_state(&paths.root, &state)?;
    println!(
        "Day {}/{} unmarked — {}/{} days done",
        day,
        plan.days,
        state.done_count(plan.days),
        plan.days
    );
    Ok(())
}

fn run_plan_status(paths: &CachePaths) -> Result<()> {
    let (state, plan) = active_plan(paths)?;

    let done = state.done_count(plan.days);
    const BAR_WIDTH: u32 = 30;
    let filled = (done * BAR_WIDTH / plan.days) as usize;
    let bar = "█".repeat(filled) + &"░".repeat(BAR_WIDTH as usize - filled);

    println!("{} ({})", plan.name, plan.id);
    println!(
        "[{}] {}/{} days ({}%)",
        bar,
        done,
        plan.days,
        done * 100 / plan.days
    );
    println!("Started {}", state.started);

    let scheduled = scheduled_day(&state, plan);
    if done >= plan.days {
        println!("Complete. Well done!");
    } else if done > scheduled {
        let ahead = done - scheduled;
        println!(
            "Ahead of pace by {} day{}.",
            ahead,
            if ahead == 1 { "" } else { "s" }
        );
    } else if done + 1 >= scheduled {
        println!("On pace.");
    } else {
        let behind = scheduled - done - 1;
        println!(
            "Behind by {} day{} — next up: day {}.",
            behind,
            if behind == 1 { "" } else { "s" },
            state.next_day(plan.days).unwrap_or(plan.days)
        );
    }
    Ok(())
}

pub fn run_diff(args: &DiffArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let references = parse_references(&args.reference)?;

    let mut ids = parse_translation_list(&args.with)?;
    // A single id is diffed against the active translation.
    if ids.len() == 1 && ids[0] != paths.translation {
        ids.insert(0, paths.translation.clone());
    }
    if ids.len() < 2 {
        bail!("Provide at least two distinct translations, e.g. --with kjv,bbe");
    }

    let loaded = load_translations(paths, &ids)?;
    let indexes: Vec<VerseIndex> = loaded.iter().map(|v| VerseIndex::build(v)).collect();

    // The first translation is the base; it defines versification and word order.
    let base = indexes[0].resolve_all(&references)?;
    let others = &ids[1..];

    // Each verse's ops against each other translation (None where it lacks the verse).
    let verse_diffs = |v: &'_ Verse| -> Vec<Option<Vec<(&'static str, String)>>> {
        let base_tokens: Vec<&str> = v.text.split_whitespace().collect();
        indexes[1..]
            .iter()
            .map(|index| {
                index.get(&v.book, v.chapter, v.verse).map(|other| {
                    let other_tokens: Vec<&str> = other.text.split_whitespace().collect();
                    diff_tokens(&base_tokens, &other_tokens)
                        .iter()
                        .map(|op| (op.name(), op.text().to_string()))
                        .collect()
                })
            })
            .collect()
    };

    match output.format {
        Format::Plain => {}
        Format::Json | Format::Ndjson => {
            let records: Vec<serde_json::Value> = base
                .iter()
                .map(|v| {
                    let diffs: serde_json::Map<String, serde_json::Value> = others
                        .iter()
                        .zip(verse_diffs(v))
                        .map(|(id, ops)| {
                            let ops = ops.map(|ops| {
                                ops.into_iter()
                                    .map(|(op, text)| serde_json::json!({ "op": op, "text": text }))
                                    .collect::<Vec<_>>()
                            });
                            (id.clone(), serde_json::json!(ops))
                        })
                        .collect();
                    serde_json::json!({
                        "id": verse_id(v),
                        "reference": verse_reference(v),
                        "base": ids[0],
                        "diffs": diffs,
                    })
                })
                .collect();
            output.emit_json_records(&records);
            return Ok(());
        }
        Format::Tsv => {
            // Long format, one row per token: id, translation, op, token.
            for v in &base {
                for (id, ops) in others.iter().zip(verse_diffs(v)) {
                    for (op, text) in ops.into_iter().flatten() {
                        println!("{}\t{}\t{}\t{}", verse_id(v), id, op, text);
                    }
                }
            }
            return Ok(());
        }
        Format::Ref => {
            for v in &base {
                println!("{}", verse_reference(v));
            }
            return Ok(());
        }
        Format::Raw => {
            bail!("diff has no raw form; use --json, --format ndjson|tsv|ref, or --color never")
        }
    }

    // Human view: per verse, the base line with removals highlighted, then each
    // other translation with additions highlighted; shared words are dimmed.
    let label_width = ids.iter().map(|id| id.len()).max().unwrap_or(3);
    for (n, v) in base.iter().enumerate() {
        if n > 0 {
            println!();
        }
        output.print_reference_heading(&format!("{} {}:{}", v.book, v.chapter, v.verse));

        let base_tokens: Vec<&str> = v.text.split_whitespace().collect();
        let per_other: Vec<Option<Vec<DiffOp>>> = others
            .iter()
            .enumerate()
            .map(|(i, _)| {
                indexes[i + 1].get(&v.book, v.chapter, v.verse).map(|o| {
                    let other_tokens: Vec<&str> = o.text.split_whitespace().collect();
                    diff_tokens(&base_tokens, &other_tokens)
                })
            })
            .collect();

        // A base token is "common" when every present translation keeps it.
        let mut common = vec![true; base_tokens.len()];
        let mut any_present = false;
        for ops in per_other.iter().flatten() {
            any_present = true;
            let mut kept = vec![false; base_tokens.len()];
            for op in ops {
                if let DiffOp::Equal { base_idx, .. } = op {
                    kept[*base_idx] = true;
                }
            }
            for (c, k) in common.iter_mut().zip(&kept) {
                *c &= k;
            }
        }

        let base_line: Vec<String> = base_tokens
            .iter()
            .enumerate()
            .map(|(i, t)| {
                if !any_present || !common[i] {
                    output.removed_span(t)
                } else {
                    output.dim_span(t)
                }
            })
            .collect();
        println!(
            "  {:width$}  {}",
            ids[0],
            base_line.join(" "),
            width = label_width
        );

        for (i, id) in others.iter().enumerate() {
            match &per_other[i] {
                Some(ops) => {
                    let line: Vec<String> = ops
                        .iter()
                        .filter_map(|op| match op {
                            DiffOp::Equal { text, .. } => Some(output.dim_span(text)),
                            DiffOp::Insert { text } => Some(output.added_span(text)),
                            DiffOp::Delete { .. } => None,
                        })
                        .collect();
                    println!("  {:width$}  {}", id, line.join(" "), width = label_width);
                }
                None => println!("  {:width$}  (missing)", id, width = label_width),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_provider_gets_its_own_default_model() {
        assert_eq!(resolve_model("openai", None).unwrap(), "gpt-4o-mini");
        assert!(resolve_model("Anthropic", None)
            .unwrap()
            .starts_with("claude-"));
        assert_eq!(
            resolve_model("anthropic", Some("my-model")).unwrap(),
            "my-model"
        );
        assert!(resolve_model("gemini", None).is_err());
    }
}
