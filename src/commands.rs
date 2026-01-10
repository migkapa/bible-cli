use anyhow::{bail, Context, Result};
use chrono::{Datelike, Local};
use futures::StreamExt;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::ai::{AiProvider, ChatMessage, ProviderRequest, StreamEvent};
use crate::books::normalize_book;
use crate::cache::{preload_kjv, read_manifest, CachePaths};
use crate::cli::{AiArgs, CacheArgs, EchoArgs, MoodArgs, ReadArgs, SearchArgs};
use crate::moods::{all_moods, find_mood};
use crate::output::{MarkdownRenderer, OutputStyle, ThinkingIndicator};
use crate::reference::{parse_reference, ReferenceQuery};
use crate::verses::{find_verse, load_verses, max_chapter, Verse};

pub fn run_cache(args: &CacheArgs, paths: &CachePaths) -> Result<()> {
    if args.preload {
        let count = preload_kjv(paths, args.source.as_deref())?;
        println!("KJV cached: {} verses", count);
        return Ok(());
    }

    println!("Cache root: {}", paths.root.display());
    if paths.verses_path.exists() {
        if let Some(manifest) = read_manifest(&paths.manifest_path) {
            println!("KJV: ready ({} verses)", manifest.verse_count);
            println!("Source: {}", manifest.source);
            println!("Updated: {}", manifest.created_at);
        } else {
            println!("KJV: ready");
        }
    } else {
        println!("KJV: missing. Run `bible cache --preload`.");
    }

    Ok(())
}

pub fn run_read(args: &ReadArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let reference = parse_reference(&args.reference)?;
    let verses = load_verses(&paths.verses_path)
        .context("KJV not cached. Run `bible cache --preload`.")?;

    match (reference.chapter, reference.verse) {
        (None, _) => print_book_overview(&verses, &reference),
        (Some(chapter), None) => print_chapter(&verses, &reference.book, chapter, output),
        (Some(chapter), Some(verse)) => print_single(&verses, &reference.book, chapter, verse, output),
    }
}

pub fn run_search(args: &SearchArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let verses = load_verses(&paths.verses_path)
        .context("KJV not cached. Run `bible cache --preload`.")?;
    let needle = args.query.to_lowercase();

    let book_filter = match args.book.as_ref() {
        Some(book) => {
            let normalized = normalize_book(book)
                .ok_or_else(|| anyhow::anyhow!("Unknown book: {}", book))?;
            Some(normalized.to_string())
        }
        None => None,
    };

    let mut matches = Vec::new();
    for verse in &verses {
        if let Some(ref book) = book_filter {
            if &verse.book != book {
                continue;
            }
        }
        if verse.text.to_lowercase().contains(&needle) {
            matches.push(verse);
        }
        if matches.len() >= args.limit {
            break;
        }
    }

    if matches.is_empty() {
        println!("No matches found.");
        return Ok(());
    }

    for verse in matches {
        println!("{}", output.verse_line(verse));
    }
    Ok(())
}

pub fn run_today(paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let verses = load_verses(&paths.verses_path)
        .context("KJV not cached. Run `bible cache --preload`.")?;
    let date = Local::now().date_naive();
    let day_seed = date.num_days_from_ce() as usize;
    let idx = day_seed % verses.len();
    let verse = &verses[idx];

    let prompt = daily_prompt(day_seed);
    println!("{}", output.verse_line(verse));
    println!("Prompt: {}", prompt);
    Ok(())
}

pub fn run_random(paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let verses = load_verses(&paths.verses_path)
        .context("KJV not cached. Run `bible cache --preload`.")?;
    let mut rng = thread_rng();
    let verse = verses
        .choose(&mut rng)
        .ok_or_else(|| anyhow::anyhow!("No verses available"))?;
    println!("{}", output.verse_line(verse));
    Ok(())
}

pub fn run_echo(args: &EchoArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let reference = parse_reference(&args.reference)?;
    let chapter = reference
        .chapter
        .ok_or_else(|| anyhow::anyhow!("Chapter is required"))?;
    let verse_number = reference
        .verse
        .ok_or_else(|| anyhow::anyhow!("Verse is required"))?;

    let verses = load_verses(&paths.verses_path)
        .context("KJV not cached. Run `bible cache --preload`.")?;

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
    let mood = find_mood(mood_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown mood: {}", mood_name))?;

    let verses = load_verses(&paths.verses_path)
        .context("KJV not cached. Run `bible cache --preload`.")?;

    println!("Mood: {}", mood.name);
    for reference in mood.refs {
        if let Some(verse) = find_verse(&verses, reference.book, reference.chapter, reference.verse) {
            println!("{}", output.verse_line(verse));
        }
    }

    Ok(())
}

pub async fn run_ai(args: &AiArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let reference = parse_reference(&args.reference)?;
    let verses = load_verses(&paths.verses_path)
        .context("KJV not cached. Run `bible cache --preload`.")?;

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
            } else if matches!(provider_name.to_lowercase().as_str(), "openai" | "anthropic") {
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

fn print_chapter(verses: &[Verse], book: &str, chapter: u16, output: &OutputStyle) -> Result<()> {
    let mut matches: Vec<&Verse> = verses
        .iter()
        .filter(|v| v.book == book && v.chapter == chapter)
        .collect();
    if matches.is_empty() {
        bail!("No verses found for {} {}", book, chapter);
    }
    matches.sort_by_key(|v| v.verse);
    for verse in matches {
        println!("{}", output.verse_line(verse));
    }
    Ok(())
}

fn print_single(
    verses: &[Verse],
    book: &str,
    chapter: u16,
    verse: u16,
    output: &OutputStyle,
) -> Result<()> {
    let verse = find_verse(verses, book, chapter, verse)
        .ok_or_else(|| anyhow::anyhow!("Verse not found"))?;
    println!("{}", output.verse_line(verse));
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
