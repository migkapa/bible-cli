use anyhow::{bail, Result};

use crate::books::normalize_book;

pub struct ReferenceQuery {
    pub book: String,
    pub chapter: Option<u16>,
    pub verse: Option<u16>,
}

pub fn parse_reference(tokens: &[String]) -> Result<ReferenceQuery> {
    if tokens.is_empty() {
        bail!("Reference is required");
    }

    let joined = tokens.join(" ");
    let (book_part, chapter, verse) = if joined.contains(':') {
        let parts: Vec<&str> = joined.split(':').collect();
        if parts.len() != 2 {
            bail!("Invalid reference: {}", joined);
        }
        let left = parts[0].trim();
        let right = parts[1].trim();
        let verse = parse_u16(right).ok_or_else(|| anyhow::anyhow!("Invalid verse: {}", right))?;
        let (book_part, chapter) = split_book_and_chapter(left)?;
        (book_part, Some(chapter), Some(verse))
    } else {
        split_trailing_numbers(&joined)?
    };

    let book = normalize_book(&book_part)
        .ok_or_else(|| anyhow::anyhow!("Unknown book: {}", book_part))?;

    Ok(ReferenceQuery {
        book: book.to_string(),
        chapter,
        verse,
    })
}

fn split_book_and_chapter(input: &str) -> Result<(String, u16)> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() < 2 {
        bail!("Chapter is required: {}", input);
    }
    let last = parts[parts.len() - 1];
    let chapter = parse_u16(last).ok_or_else(|| anyhow::anyhow!("Invalid chapter: {}", last))?;
    let book = parts[..parts.len() - 1].join(" ");
    Ok((book, chapter))
}

fn split_trailing_numbers(input: &str) -> Result<(String, Option<u16>, Option<u16>)> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        bail!("Reference is required");
    }
    let mut book_parts = parts.clone();
    let mut chapter = None;
    let mut verse = None;

    if let Some(last) = parts.last() {
        if let Some(last_num) = parse_u16(last) {
            if parts.len() >= 2 {
                if let Some(prev_num) = parse_u16(parts[parts.len() - 2]) {
                    verse = Some(last_num);
                    chapter = Some(prev_num);
                    book_parts = parts[..parts.len() - 2].to_vec();
                } else {
                    chapter = Some(last_num);
                    book_parts = parts[..parts.len() - 1].to_vec();
                }
            } else {
                chapter = Some(last_num);
                book_parts = parts[..parts.len() - 1].to_vec();
            }
        }
    }

    let book = if book_parts.is_empty() {
        input.to_string()
    } else {
        book_parts.join(" ")
    };

    Ok((book, chapter, verse))
}

fn parse_u16(input: &str) -> Option<u16> {
    input.parse::<u16>().ok()
}
