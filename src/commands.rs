use anyhow::{bail, Context, Result};
use chrono::{Datelike, Local};
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};

use crate::books::normalize_book;
use crate::cache::{preload_kjv, read_manifest, CachePaths};
use crate::cli::{AiArgs, CacheArgs, EchoArgs, MoodArgs, ReadArgs, SearchArgs};
use crate::moods::{all_moods, find_mood};
use crate::output::OutputStyle;
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

pub fn run_ai(args: &AiArgs, paths: &CachePaths, output: &OutputStyle) -> Result<()> {
    let reference = parse_reference(&args.reference)?;
    let verses = load_verses(&paths.verses_path)
        .context("KJV not cached. Run `bible cache --preload`.")?;

    let selected = select_ai_verses(&verses, &reference, args.window)?;
    let prompt = build_ai_prompt(&selected);
    let response = call_provider(args, &prompt)?;

    for verse in &selected {
        println!("{}", output.verse_line(verse));
    }
    println!();
    println!("{}", response.trim());

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
    for verse in selected {
        let line = format!(
            "{} {}:{} {}\n",
            verse.book, verse.chapter, verse.verse, verse.text
        );
        prompt.push_str(&line);
    }
    prompt.push_str("\nResponse:");
    prompt
}

fn call_provider(args: &AiArgs, prompt: &str) -> Result<String> {
    match args.provider.to_lowercase().as_str() {
        "openai" => call_openai(args, prompt),
        provider => bail!("Unknown provider: {}", provider),
    }
}

fn call_openai(args: &AiArgs, prompt: &str) -> Result<String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .context("OPENAI_API_KEY is required for the OpenAI provider")?;
    let client = reqwest::blocking::Client::new();
    let request = OpenAiRequest {
        model: &args.model,
        messages: vec![
            OpenAiMessage {
                role: "system",
                content: "You are a thoughtful Bible assistant.",
            },
            OpenAiMessage {
                role: "user",
                content: prompt,
            },
        ],
        max_tokens: args.max_tokens,
        temperature: args.temperature,
    };

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .context("Failed to call OpenAI")?
        .error_for_status()
        .context("OpenAI returned an error")?;

    let body: OpenAiResponse = response.json().context("Invalid OpenAI response")?;
    let content = body
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .ok_or_else(|| anyhow::anyhow!("OpenAI response missing content"))?;
    Ok(content)
}

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage<'a>>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
struct OpenAiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessageResponse,
}

#[derive(Deserialize)]
struct OpenAiMessageResponse {
    content: Option<String>,
}
