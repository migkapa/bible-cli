use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::reference::ReferenceQuery;

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

pub fn max_chapter(verses: &[Verse], book: &str) -> Option<u16> {
    verses
        .iter()
        .filter(|v| v.book == book)
        .map(|v| v.chapter)
        .max()
}

/// An O(1) lookup over a loaded verse list, built once and reused for range,
/// chapter, and single-verse resolution.
pub struct VerseIndex<'a> {
    verses: &'a [Verse],
    by_key: HashMap<(&'a str, u16, u16), usize>,
    by_chapter: HashMap<(&'a str, u16), Vec<usize>>,
}

impl<'a> VerseIndex<'a> {
    pub fn build(verses: &'a [Verse]) -> Self {
        let mut by_key = HashMap::with_capacity(verses.len());
        let mut by_chapter: HashMap<(&'a str, u16), Vec<usize>> = HashMap::new();
        for (idx, v) in verses.iter().enumerate() {
            by_key.insert((v.book.as_str(), v.chapter, v.verse), idx);
            by_chapter
                .entry((v.book.as_str(), v.chapter))
                .or_default()
                .push(idx);
        }
        Self {
            verses,
            by_key,
            by_chapter,
        }
    }

    pub fn get(&self, book: &str, chapter: u16, verse: u16) -> Option<&'a Verse> {
        self.by_key
            .get(&(book, chapter, verse))
            .map(|&idx| &self.verses[idx])
    }

    /// All verses in a chapter, ordered by verse number.
    pub fn chapter(&self, book: &str, chapter: u16) -> Vec<&'a Verse> {
        match self.by_chapter.get(&(book, chapter)) {
            Some(indices) => {
                let mut out: Vec<&Verse> = indices.iter().map(|&i| &self.verses[i]).collect();
                out.sort_by_key(|v| v.verse);
                out
            }
            None => Vec::new(),
        }
    }

    /// Resolve a parsed reference into the matching verses. Returns an error for
    /// a whole-book reference (which has no concrete verse set to render).
    pub fn resolve(&self, query: &ReferenceQuery) -> Result<Vec<&'a Verse>> {
        let Some(chapter) = query.chapter else {
            bail!("Chapter is required to resolve verses");
        };

        if !query.verse_list.is_empty() {
            let mut out = Vec::new();
            for &v in &query.verse_list {
                if let Some(verse) = self.get(&query.book, chapter, v) {
                    out.push(verse);
                }
            }
            if out.is_empty() {
                bail!("No verses found for {} {}", query.book, chapter);
            }
            return Ok(out);
        }

        match (query.verse, query.verse_end) {
            (Some(start), Some(end)) => {
                let out: Vec<&Verse> = (start..=end)
                    .filter_map(|v| self.get(&query.book, chapter, v))
                    .collect();
                if out.is_empty() {
                    bail!(
                        "No verses found for {} {}:{}-{}",
                        query.book,
                        chapter,
                        start,
                        end
                    );
                }
                Ok(out)
            }
            (Some(verse), None) => {
                let v = self
                    .get(&query.book, chapter, verse)
                    .ok_or_else(|| anyhow::anyhow!("Verse not found"))?;
                Ok(vec![v])
            }
            (None, _) => {
                let out = self.chapter(&query.book, chapter);
                if out.is_empty() {
                    bail!("No verses found for {} {}", query.book, chapter);
                }
                Ok(out)
            }
        }
    }
}
