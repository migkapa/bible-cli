use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verse {
    pub book: String,
    pub chapter: u16,
    pub verse: u16,
    pub text: String,
}

#[derive(Debug, Clone, Copy)]
pub struct VerseRef {
    pub book: &'static str,
    pub chapter: u16,
    pub verse: u16,
}

pub fn load_verses(path: &Path) -> Result<Vec<Verse>> {
    let file = File::open(path).with_context(|| format!("KJV not found at {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut verses = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("Failed reading line {}", idx + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let verse: Verse = serde_json::from_str(&line)
            .with_context(|| format!("Invalid JSON on line {}", idx + 1))?;
        verses.push(verse);
    }
    if verses.is_empty() {
        bail!("KJV cache is empty at {}", path.display());
    }
    Ok(verses)
}

pub fn find_verse<'a>(
    verses: &'a [Verse],
    book: &str,
    chapter: u16,
    verse: u16,
) -> Option<&'a Verse> {
    verses
        .iter()
        .find(|v| v.book == book && v.chapter == chapter && v.verse == verse)
}

pub fn max_chapter(verses: &[Verse], book: &str) -> Option<u16> {
    verses
        .iter()
        .filter(|v| v.book == book)
        .map(|v| v.chapter)
        .max()
}
