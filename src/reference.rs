use anyhow::{bail, Result};

use crate::books::normalize_book;

/// A parsed scripture reference. Depending on which fields are set it can denote
/// a whole book, a whole chapter, a single verse, a verse range, or an explicit
/// list of verses.
///
/// - whole book:     `chapter = None`
/// - whole chapter:  `chapter = Some(c)`, `verse = None`
/// - single verse:   `verse = Some(v)`, `verse_end = None`, `verse_list` empty
/// - verse range:    `verse = Some(start)`, `verse_end = Some(end)`
/// - explicit list:  `verse_list` non-empty (`verse` holds the first for callers
///   that only understand a single anchor verse)
pub struct ReferenceQuery {
    pub book: String,
    pub chapter: Option<u16>,
    pub verse: Option<u16>,
    pub verse_end: Option<u16>,
    pub verse_list: Vec<u16>,
}

struct VerseSpec {
    verse: Option<u16>,
    verse_end: Option<u16>,
    list: Vec<u16>,
}

pub fn parse_reference(tokens: &[String]) -> Result<ReferenceQuery> {
    if tokens.is_empty() {
        bail!("Reference is required");
    }

    let joined = tokens.join(" ");
    let (book_part, chapter, spec) = if joined.contains(':') {
        let parts: Vec<&str> = joined.split(':').collect();
        if parts.len() != 2 {
            bail!("Invalid reference: {}", joined);
        }
        let left = parts[0].trim();
        let right = parts[1].trim();
        let spec =
            parse_verse_spec(right).ok_or_else(|| anyhow::anyhow!("Invalid verse: {}", right))?;
        let (book_part, chapter) = split_book_and_chapter(left)?;
        (book_part, Some(chapter), Some(spec))
    } else {
        split_trailing_numbers(&joined)?
    };

    let book =
        normalize_book(&book_part).ok_or_else(|| anyhow::anyhow!("Unknown book: {}", book_part))?;

    let (verse, verse_end, verse_list) = match spec {
        Some(spec) => (spec.verse, spec.verse_end, spec.list),
        None => (None, None, Vec::new()),
    };

    Ok(ReferenceQuery {
        book: book.to_string(),
        chapter,
        verse,
        verse_end,
        verse_list,
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

fn split_trailing_numbers(input: &str) -> Result<(String, Option<u16>, Option<VerseSpec>)> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        bail!("Reference is required");
    }
    let mut book_parts = parts.clone();
    let mut chapter = None;
    let mut spec = None;

    if let Some(last) = parts.last() {
        let last_spec = parse_verse_spec(last);
        if let Some(last_num) = parse_u16(last) {
            // The final token is a bare number: it is the chapter, unless the
            // token before it is also a number (then chapter + verse).
            if parts.len() >= 2 {
                if let Some(prev_num) = parse_u16(parts[parts.len() - 2]) {
                    spec = Some(VerseSpec {
                        verse: Some(last_num),
                        verse_end: None,
                        list: Vec::new(),
                    });
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
        } else if last_spec.is_some() && parts.len() >= 2 {
            // The final token is a verse spec like "16-18" or "16,18"; the token
            // before it must be the chapter (e.g. "John 3 16-18").
            if let Some(prev_num) = parse_u16(parts[parts.len() - 2]) {
                spec = last_spec;
                chapter = Some(prev_num);
                book_parts = parts[..parts.len() - 2].to_vec();
            }
        }
    }

    let book = if book_parts.is_empty() {
        input.to_string()
    } else {
        book_parts.join(" ")
    };

    Ok((book, chapter, spec))
}

/// Parse the portion after the chapter into a verse selector: a single verse
/// (`16`), a range (`16-18`), or a comma list which may itself contain ranges
/// (`16,18,20` or `16-18,20`).
fn parse_verse_spec(input: &str) -> Option<VerseSpec> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    if input.contains(',') {
        let mut list = Vec::new();
        for part in input.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((start, end)) = parse_range(part) {
                if end < start {
                    return None;
                }
                list.extend(start..=end);
            } else {
                list.push(parse_u16(part)?);
            }
        }
        if list.is_empty() {
            return None;
        }
        return Some(VerseSpec {
            verse: list.first().copied(),
            verse_end: None,
            list,
        });
    }

    if let Some((start, end)) = parse_range(input) {
        if end < start {
            return None;
        }
        return Some(VerseSpec {
            verse: Some(start),
            verse_end: Some(end),
            list: Vec::new(),
        });
    }

    Some(VerseSpec {
        verse: Some(parse_u16(input)?),
        verse_end: None,
        list: Vec::new(),
    })
}

fn parse_range(input: &str) -> Option<(u16, u16)> {
    let (a, b) = input.split_once('-')?;
    Some((parse_u16(a.trim())?, parse_u16(b.trim())?))
}

fn parse_u16(input: &str) -> Option<u16> {
    input.parse::<u16>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(tokens: &[&str]) -> ReferenceQuery {
        let owned: Vec<String> = tokens.iter().map(|s| s.to_string()).collect();
        parse_reference(&owned).unwrap()
    }

    #[test]
    fn single_verse_colon() {
        let r = q(&["John", "3:16"]);
        assert_eq!(r.book, "John");
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.verse_end, None);
        assert!(r.verse_list.is_empty());
    }

    #[test]
    fn single_verse_spaced() {
        let r = q(&["John", "3", "16"]);
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
    }

    #[test]
    fn whole_chapter() {
        let r = q(&["Psalm", "23"]);
        assert_eq!(r.book, "Psalms");
        assert_eq!(r.chapter, Some(23));
        assert_eq!(r.verse, None);
    }

    #[test]
    fn whole_book() {
        let r = q(&["Jude"]);
        assert_eq!(r.book, "Jude");
        assert_eq!(r.chapter, None);
    }

    #[test]
    fn range_colon() {
        let r = q(&["John", "3:16-18"]);
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.verse_end, Some(18));
    }

    #[test]
    fn range_spaced() {
        let r = q(&["John", "3", "16-18"]);
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.verse_end, Some(18));
    }

    #[test]
    fn list_colon() {
        let r = q(&["John", "3:16,18,20"]);
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse_list, vec![16, 18, 20]);
        assert_eq!(r.verse, Some(16));
    }

    #[test]
    fn list_with_range() {
        let r = q(&["John", "3:16-18,20"]);
        assert_eq!(r.verse_list, vec![16, 17, 18, 20]);
    }

    #[test]
    fn multiword_book_range() {
        let r = q(&["1", "John", "4:7-9"]);
        assert_eq!(r.book, "1 John");
        assert_eq!(r.chapter, Some(4));
        assert_eq!(r.verse, Some(7));
        assert_eq!(r.verse_end, Some(9));
    }

    #[test]
    fn reversed_range_is_error() {
        let owned = vec!["John".to_string(), "3:18-16".to_string()];
        assert!(parse_reference(&owned).is_err());
    }
}
