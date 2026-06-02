use anyhow::{bail, Context, Result};
use chrono::{Datelike, Local};
use futures::StreamExt;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{thread_rng, SeedableRng};
use regex::RegexBuilder;
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::ai::{AiProvider, ChatMessage, ProviderRequest, StreamEvent};
use crate::books::{is_old_testament, normalize_book, osis_code};
use crate::cache::{
    installed_translations, preload, read_manifest, remove_translation, save_default_translation,
    CachePaths,
};
use crate::cli::{
    AiArgs, CacheArgs, EchoArgs, ExportArgs, ExportTarget, MoodArgs, ParallelArgs, RandomArgs,
    ReadArgs, SearchArgs, Testament, TodayArgs, TopicArgs, TranslationAction, TranslationArgs,
    TuiArgs,
};
use crate::moods::{all_moods, find_mood};
use crate::output::{MarkdownRenderer, OutputStyle, ThinkingIndicator};
use crate::reference::{parse_reference, ReferenceQuery};
use crate::topics::{all_topics, find_topic};
use crate::tui;
use crate::verses::{load_verses, max_chapter, Verse, VerseIndex};

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
    format!(
        "{} not cached. Run `bible translation add {}` (or `bible cache --preload` for kjv).",
        id.to_uppercase(),
        id
    )
}

pub fn run_read(args: &ReadArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let reference = parse_reference(&args.reference)?;
    let verses =
        load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))?;
    let index = VerseIndex::build(&verses);

    // Whole-book reference: a chapter overview in the human view, or the full
    // book as data in structured formats.
    if reference.chapter.is_none() {
        if output.is_structured() {
            let book_verses = book_verses(&verses, &reference.book);
            if book_verses.is_empty() {
                bail!("Book not found: {}", reference.book);
            }
            output.emit_verses(&book_verses);
            return Ok(());
        }
        return print_book_overview(&verses, &reference);
    }

    let selected = index.resolve(&reference)?;
    output.emit_verses(&selected);
    Ok(())
}

/// All verses of a book in canonical order.
fn book_verses<'a>(verses: &'a [Verse], book: &str) -> Vec<&'a Verse> {
    let mut out: Vec<&Verse> = verses.iter().filter(|v| v.book == book).collect();
    out.sort_by_key(|v| (v.chapter, v.verse));
    out
}

pub fn run_search(args: &SearchArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let verses =
        load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))?;

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
    let verses =
        load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))?;

    let book_filter = normalize_book_filter(args.book.as_deref())?;
    let pool = filter_verses(&verses, book_filter.as_deref(), args.testament);
    if pool.is_empty() {
        bail!("No verses match those constraints.");
    }

    let date = Local::now().date_naive();
    let day_seed = date.num_days_from_ce() as usize;
    let verse = pool[day_seed % pool.len()];

    output.emit_verses(&[verse]);
    if !output.is_structured() {
        println!("Prompt: {}", daily_prompt(day_seed));
    }
    Ok(())
}

pub fn run_random(args: &RandomArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let verses =
        load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))?;

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
    let chapter = reference
        .chapter
        .ok_or_else(|| anyhow::anyhow!("Chapter is required"))?;
    let verse_number = reference
        .verse
        .ok_or_else(|| anyhow::anyhow!("Verse is required"))?;

    let verses =
        load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))?;

    let mut chapter_verses: Vec<&Verse> = verses
        .iter()
        .filter(|v| v.book == reference.book && v.chapter == chapter)
        .collect();
    if chapter_verses.is_empty() {
        bail!("No verses found for {} {}", reference.book, chapter);
    }
    chapter_verses.sort_by_key(|v| v.verse);

    let position = chapter_verses
        .iter()
        .position(|v| v.verse == verse_number)
        .ok_or_else(|| anyhow::anyhow!("Verse not found"))?;

    let window = args.window as usize;
    let start = position.saturating_sub(window);
    let end = (position + window).min(chapter_verses.len() - 1);

    if output.is_structured() {
        let slice: Vec<&Verse> = chapter_verses[start..=end].to_vec();
        output.emit_verses(&slice);
        return Ok(());
    }

    for (idx, verse) in chapter_verses.iter().enumerate().take(end + 1).skip(start) {
        let marker = if idx == position { "*" } else { " " };
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

    let verses =
        load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))?;
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

pub async fn run_ai(args: &AiArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let reference = parse_reference(&args.reference)?;
    let verses =
        load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))?;

    let selected = select_ai_verses(&verses, &reference, args.window)?;

    if args.chat {
        return run_ai_chat_streaming(args, &selected, output).await;
    }

    // Non-chat mode: single request with streaming
    run_ai_single_streaming(args, &selected, output).await
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
        model: args.model.clone(),
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

    let mut current_model = args.model.clone();
    let mut current_provider = args.provider.clone();

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
            } else if matches!(
                provider_name.to_lowercase().as_str(),
                "openai" | "anthropic"
            ) {
                current_provider = provider_name.to_lowercase();
                output.print_dim(&format!("Provider set to {}", current_provider));
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

fn print_book_overview(verses: &[Verse], reference: &ReferenceQuery) -> Result<()> {
    let Some(max_chapter) = max_chapter(verses, &reference.book) else {
        bail!("Book not found: {}", reference.book);
    };
    println!("{} has {} chapters.", reference.book, max_chapter);
    println!("Tip: bible read {} <chapter>", reference.book);
    Ok(())
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

fn select_ai_verses<'a>(
    verses: &'a [Verse],
    reference: &ReferenceQuery,
    window: u16,
) -> Result<Vec<&'a Verse>> {
    let chapter = reference
        .chapter
        .ok_or_else(|| anyhow::anyhow!("Chapter is required for AI prompts"))?;

    let mut chapter_verses: Vec<&Verse> = verses
        .iter()
        .filter(|v| v.book == reference.book && v.chapter == chapter)
        .collect();
    if chapter_verses.is_empty() {
        bail!("No verses found for {} {}", reference.book, chapter);
    }
    chapter_verses.sort_by_key(|v| v.verse);

    let Some(verse_number) = reference.verse else {
        return Ok(chapter_verses);
    };

    let position = chapter_verses
        .iter()
        .position(|v| v.verse == verse_number)
        .ok_or_else(|| anyhow::anyhow!("Verse not found"))?;

    let window = window as usize;
    let start = position.saturating_sub(window);
    let end = (position + window).min(chapter_verses.len() - 1);

    Ok(chapter_verses[start..=end].to_vec())
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

/// Resolve a reference to verses, handling whole-book references (which `read`
/// renders as an overview but `export`/`parallel` treat as the full book).
fn resolve_selection<'a>(
    index: &VerseIndex<'a>,
    verses: &'a [Verse],
    reference: &ReferenceQuery,
) -> Result<Vec<&'a Verse>> {
    if reference.chapter.is_none() {
        let bv = book_verses(verses, &reference.book);
        if bv.is_empty() {
            bail!("Book not found: {}", reference.book);
        }
        Ok(bv)
    } else {
        index.resolve(reference)
    }
}

/// A human label for a contiguous selection, e.g. `John 3:16` or `John 3:16-18`.
fn passage_label(selected: &[&Verse]) -> String {
    match (selected.first(), selected.last()) {
        (Some(first), Some(last)) if selected.len() > 1 => {
            if first.chapter == last.chapter {
                format!(
                    "{} {}:{}-{}",
                    first.book, first.chapter, first.verse, last.verse
                )
            } else {
                format!(
                    "{} {}:{}-{}:{}",
                    first.book, first.chapter, first.verse, last.chapter, last.verse
                )
            }
        }
        (Some(first), _) => format!("{} {}:{}", first.book, first.chapter, first.verse),
        _ => String::new(),
    }
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

    let verses =
        load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))?;
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
    let reference = parse_reference(&args.reference)?;
    let verses =
        load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))?;
    let index = VerseIndex::build(&verses);
    let selected = resolve_selection(&index, &verses, &reference)?;
    let _ = output; // export format is controlled by --to, not the global format

    match args.to {
        ExportTarget::Md => {
            println!(
                "## {} ({})",
                passage_label(&selected),
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
    let reference = parse_reference(&args.reference)?;

    let ids: Vec<String> = args
        .with
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if ids.is_empty() {
        bail!("Provide translations to compare, e.g. --with kjv,bbe");
    }

    // Load every requested translation up front.
    let mut loaded: Vec<Vec<Verse>> = Vec::with_capacity(ids.len());
    for id in &ids {
        if !paths.is_installed(id) {
            bail!(
                "{} is not installed. Run `bible translation add {}`.",
                id.to_uppercase(),
                id
            );
        }
        loaded.push(load_verses(&paths.verses_path_for(id))?);
    }
    let indexes: Vec<VerseIndex> = loaded.iter().map(|v| VerseIndex::build(v)).collect();

    // The first translation defines the versification we iterate over.
    let base = resolve_selection(&indexes[0], &loaded[0], &reference)?;

    if output.is_structured() {
        let mut arr = Vec::new();
        for v in &base {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "id".into(),
                serde_json::Value::String(format!(
                    "{}.{}.{}",
                    osis_code(&v.book),
                    v.chapter,
                    v.verse
                )),
            );
            obj.insert(
                "reference".into(),
                serde_json::Value::String(format!("{} {}:{}", v.book, v.chapter, v.verse)),
            );
            let mut tx = serde_json::Map::new();
            for (i, id) in ids.iter().enumerate() {
                let text = indexes[i].get(&v.book, v.chapter, v.verse);
                tx.insert(
                    id.clone(),
                    match text {
                        Some(t) => serde_json::Value::String(t.text.clone()),
                        None => serde_json::Value::Null,
                    },
                );
            }
            obj.insert("translations".into(), serde_json::Value::Object(tx));
            arr.push(serde_json::Value::Object(obj));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(arr))
                .unwrap_or_else(|_| "[]".to_string())
        );
        return Ok(());
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
            for t in installed {
                let marker = if t.id == paths.translation { "*" } else { " " };
                let detail = t
                    .manifest
                    .map(|m| format!("{} verses", m.verse_count))
                    .unwrap_or_default();
                println!("{} {:<6} {}", marker, t.id, detail);
            }
            Ok(())
        }
        TranslationAction::Add(a) => {
            let count = preload(paths, &a.id, a.source.as_deref())?;
            println!("{} installed: {} verses", a.id.to_uppercase(), count);
            Ok(())
        }
        TranslationAction::Default(a) => {
            if !paths.is_installed(&a.id) {
                bail!(
                    "{} is not installed. Run `bible translation add {}` first.",
                    a.id.to_uppercase(),
                    a.id
                );
            }
            save_default_translation(&paths.root, &a.id)?;
            println!("Default translation set to {}", a.id);
            Ok(())
        }
        TranslationAction::Remove(a) => {
            if remove_translation(paths, &a.id)? {
                println!("Removed {}", a.id);
            } else {
                println!("{} was not installed", a.id);
            }
            Ok(())
        }
    }
}

pub fn run_tui(args: &TuiArgs, paths: &CachePaths) -> Result<()> {
    let verses =
        load_verses(&paths.verses_path()).with_context(|| missing_cache_msg(&paths.translation))?;

    tui::run(verses, args.book.clone(), args.r#ref.clone())
}
