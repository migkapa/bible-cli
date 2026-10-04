use std::fmt;

use anyhow::{anyhow, bail, Result};

use crate::books::{is_single_chapter, normalize_book};

/// A parsed scripture reference. Depending on which fields are set it can denote
/// a whole book, a chapter or run of chapters, a single verse, a verse range
/// (possibly crossing chapters), or an explicit list of verses.
///
/// - whole book:     `chapter = None`
/// - whole chapter:  `chapter = Some(c)`, `verse = None`, `chapter_end = None`
/// - chapter range:  `chapter = Some(c)`, `chapter_end = Some(d)`, no verses
///   (`Genesis 1-3`)
/// - single verse:   `verse = Some(v)`, `verse_end = None`, `verse_list` empty
/// - verse range:    `verse = Some(start)`, `verse_end = Some(end)`; with
///   `chapter_end = Some(d)` the range ends at verse `end` of chapter `d`
///   (`John 3:16-4:2`)
/// - explicit list:  `verse_list` non-empty (`verse` holds the first for callers
///   that only understand a single anchor verse)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceQuery {
    pub book: String,
    pub chapter: Option<u16>,
    pub verse: Option<u16>,
    pub verse_end: Option<u16>,
    pub verse_list: Vec<u16>,
    pub chapter_end: Option<u16>,
}

impl fmt::Display for ReferenceQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.book)?;
        let Some(chapter) = self.chapter else {
            return Ok(());
        };
        write!(f, " {}", chapter)?;
        if !self.verse_list.is_empty() {
            return write!(f, ":{}", compress_runs(&self.verse_list));
        }
        match (self.verse, self.chapter_end, self.verse_end) {
            (None, Some(end_chapter), _) => write!(f, "-{}", end_chapter),
            (Some(v), Some(end_chapter), Some(end)) => {
                write!(f, ":{}-{}:{}", v, end_chapter, end)
            }
            (Some(v), None, Some(end)) => write!(f, ":{}-{}", v, end),
            (Some(v), _, None) => write!(f, ":{}", v),
            (None, None, _) => Ok(()),
        }
    }
}

/// `[16, 17, 18, 20]` -> `16-18,20`, which parses back to the same list.
fn compress_runs(list: &[u16]) -> String {
    let mut parts = Vec::new();
    let mut i = 0;
    while i < list.len() {
        let start = list[i];
        let mut end = start;
        while i + 1 < list.len() && end.checked_add(1) == Some(list[i + 1]) {
            i += 1;
            end = list[i];
        }
        parts.push(if start == end {
            start.to_string()
        } else {
            format!("{}-{}", start, end)
        });
        i += 1;
    }
    parts.join(",")
}

/// Format passages in the parser's own grammar, leaving out a book name
/// repeated from the previous passage: `Genesis 1:1; 2:4; John 3:16`. This
/// depends only on what was typed, never on a translation's versification.
pub fn format_references(queries: &[ReferenceQuery]) -> String {
    let mut out = String::new();
    let mut prev_book: Option<&str> = None;
    for query in queries {
        if !out.is_empty() {
            out.push_str("; ");
        }
        let full = query.to_string();
        match (prev_book, query.chapter) {
            // "Genesis 2:4" -> "2:4": a bare chapter continues the book.
            (Some(book), Some(_)) if book == query.book => {
                out.push_str(&full[query.book.len() + 1..])
            }
            _ => out.push_str(&full),
        }
        prev_book = Some(&query.book);
    }
    out
}

/// Parse one or more passages separated by `;`, e.g. `John 3:16; Romans 8:28`.
/// A segment without a book continues the previous one: `Genesis 1:1; 2:4`.
pub fn parse_references(tokens: &[String]) -> Result<Vec<ReferenceQuery>> {
    let joined = tokens.join(" ");
    let mut out: Vec<ReferenceQuery> = Vec::new();
    for segment in joined.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let parsed = parse_passage(segment).or_else(|err| match out.last() {
            // Tried second, so a segment like "1 John 4:8" still names its book.
            Some(prev) if segment.starts_with(|c: char| c.is_ascii_digit()) => {
                parse_passage(&format!("{} {}", prev.book, segment)).map_err(|_| err)
            }
            _ => Err(err),
        })?;
        out.push(parsed);
    }
    if out.is_empty() {
        bail!("Reference is required");
    }
    Ok(out)
}

/// Parse exactly one passage; `;`-separated lists are rejected.
pub fn parse_reference(tokens: &[String]) -> Result<ReferenceQuery> {
    let mut refs = parse_references(tokens)?;
    if refs.len() > 1 {
        bail!(
            "Expected a single passage, not a list: {}",
            tokens.join(" ")
        );
    }
    Ok(refs.remove(0))
}

fn parse_passage(input: &str) -> Result<ReferenceQuery> {
    let normalized = normalize_separators(input);
    let text = normalized.trim().trim_end_matches(['.', ',']).trim_end();
    if text.is_empty() {
        bail!("Reference is required");
    }

    let has_colon = text.contains(':');
    let (book_part, mut query) = match text.split_once(':') {
        Some((left, right)) => {
            let (book_part, chapter) = split_book_and_chapter(left.trim())?;
            (book_part, parse_after_colon(chapter, right.trim())?)
        }
        None => split_trailing_numbers(text)?,
    };

    let book = normalize_book(&book_part).ok_or_else(|| anyhow!("Unknown book: {}", book_part))?;
    query.book = book.to_string();
    if !has_colon && is_single_chapter(book) {
        apply_single_chapter_shorthand(&mut query);
    }
    if query.chapter.is_none() && !query.verse_list.is_empty() {
        // A bare list ("Jude 5,7") only makes sense for a one-chapter book.
        if !is_single_chapter(book) {
            bail!(
                "A verse list needs a chapter, e.g. `{} 3:16,18` (separate chapters with ';')",
                book
            );
        }
        query.chapter = Some(1);
    }
    // One-chapter or one-verse "ranges" are just that chapter or verse.
    if query.chapter_end.is_some() && query.chapter_end == query.chapter {
        query.chapter_end = None;
    }
    if query.chapter_end.is_none() && query.verse_end.is_some() && query.verse_end == query.verse {
        query.verse_end = None;
    }

    let has_zero = [
        query.chapter,
        query.chapter_end,
        query.verse,
        query.verse_end,
    ]
    .contains(&Some(0))
        || query.verse_list.contains(&0);
    if has_zero {
        bail!("Chapter and verse numbers start at 1: {}", input.trim());
    }
    Ok(query)
}

/// Canonicalize punctuation before parsing: dashes become `-`, OSIS-style dots
/// become separators (`John.3.16` -> `John 3:16`), a chapter glued to its book
/// gets a space (`Jn3:16`), and spaces around `-` and `,` are dropped
/// (`3:16 - 18` -> `3:16-18`).
fn normalize_separators(input: &str) -> String {
    let chars: Vec<char> = input
        .chars()
        .map(|c| match c {
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            c => c,
        })
        .collect();
    let is_sep = |c: char| c == '-' || c == ',';

    let mut out = String::with_capacity(input.len());
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next_is_digit = chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
        match c {
            '.' if next_is_digit && prev.is_some_and(|p| p.is_ascii_digit()) => out.push(':'),
            '.' if next_is_digit && prev.is_some_and(char::is_alphabetic) => out.push(' '),
            c if c.is_whitespace() => {
                let after_sep = out.ends_with(is_sep);
                let before_sep = chars[i + 1..]
                    .iter()
                    .find(|n| !n.is_whitespace())
                    .is_some_and(|&n| is_sep(n));
                if !after_sep && !before_sep && !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            c if c.is_ascii_digit() && prev.is_some_and(char::is_alphabetic) => {
                out.push(' ');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

fn split_book_and_chapter(input: &str) -> Result<(String, u16)> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() < 2 {
        bail!("Expected <book> <chapter>:<verse>, got '{}:...'", input);
    }
    let last = parts[parts.len() - 1];
    let chapter = parse_u16(last).ok_or_else(|| anyhow!("Invalid chapter: {}", last))?;
    let book = parts[..parts.len() - 1].join(" ");
    Ok((book, chapter))
}

/// Parse what follows `<book> <chapter>:` — a verse selector (`16`, `16-18`,
/// `16,18,20`) or a range into a later chapter (`16-4:2`).
fn parse_after_colon(chapter: u16, right: &str) -> Result<ReferenceQuery> {
    let invalid = || anyhow!("Invalid verse: {}", right);
    let mut query = ReferenceQuery {
        chapter: Some(chapter),
        ..ReferenceQuery::default()
    };

    if right.contains(':') {
        let (start, end) = right.split_once('-').ok_or_else(invalid)?;
        let (end_chapter, end_verse) = end.split_once(':').ok_or_else(invalid)?;
        let start = parse_u16(start).ok_or_else(invalid)?;
        let end_chapter = parse_u16(end_chapter).ok_or_else(invalid)?;
        let end_verse = parse_u16(end_verse).ok_or_else(invalid)?;
        if end_chapter < chapter || (end_chapter == chapter && end_verse < start) {
            bail!("Invalid range: {}:{} ends before it starts", chapter, right);
        }
        query.verse = Some(start);
        query.verse_end = Some(end_verse);
        if end_chapter > chapter {
            query.chapter_end = Some(end_chapter);
        }
        return Ok(query);
    }

    let spec = parse_verse_spec(right).ok_or_else(invalid)?;
    query.verse = spec.verse;
    query.verse_end = spec.verse_end;
    query.verse_list = spec.list;
    Ok(query)
}

/// Split a colon-free reference into its book and trailing numbers:
/// `John 3 16`, `John 3 16-18`, `John 3`, `Genesis 1-3`, or just `John`.
fn split_trailing_numbers(input: &str) -> Result<(String, ReferenceQuery)> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let n = parts.len();
    let mut query = ReferenceQuery::default();
    let mut book_len = n;

    // "John 99999" is a number too big to be a chapter, not part of the name.
    if let Some(last) = parts.last().filter(|_| n >= 2) {
        if last.bytes().all(|b| b.is_ascii_digit()) && parse_u16(last).is_none() {
            bail!("Number out of range: {}", last);
        }
    }

    if n >= 3 && parse_u16(parts[n - 2]).is_some() {
        if let Some(spec) = parse_verse_spec(parts[n - 1]) {
            // "John 3 16", "John 3 16-18", "John 3 16,18"
            query.chapter = parse_u16(parts[n - 2]);
            query.verse = spec.verse;
            query.verse_end = spec.verse_end;
            query.verse_list = spec.list;
            book_len = n - 2;
        }
    }
    if book_len == n && n >= 2 {
        let last = parts[n - 1];
        if let Some(chapter) = parse_u16(last) {
            query.chapter = Some(chapter);
            book_len = n - 1;
        } else if let Some((first, end)) = parse_range(last) {
            // "Genesis 1-3"; an equal end is collapsed later, after "Jude 1-1"
            // has had the chance to mean a verse.
            if end < first {
                bail!("Invalid chapter range: {}", last);
            }
            query.chapter = Some(first);
            query.chapter_end = Some(end);
            book_len = n - 1;
        } else if let Some(spec) = parse_verse_spec(last).filter(|s| !s.list.is_empty()) {
            // "Jude 5,7": a verse list with no chapter (valid for one-chapter
            // books only; checked once the book is known).
            query.verse = spec.verse;
            query.verse_list = spec.list;
            book_len = n - 1;
        }
    }

    Ok((parts[..book_len].join(" "), query))
}

/// `Jude 5` means verse 5 of Jude's only chapter, and `Jude 3-5` verses 3-5;
/// `Jude 1` stays the whole chapter.
fn apply_single_chapter_shorthand(query: &mut ReferenceQuery) {
    let Some(first) = query.chapter else {
        return;
    };
    if query.verse.is_some() || !query.verse_list.is_empty() {
        return;
    }
    match query.chapter_end.take() {
        Some(last) => {
            query.chapter = Some(1);
            query.verse = Some(first);
            query.verse_end = Some(last);
        }
        None if first != 1 => {
            query.chapter = Some(1);
            query.verse = Some(first);
        }
        None => {}
    }
}

struct VerseSpec {
    verse: Option<u16>,
    verse_end: Option<u16>,
    list: Vec<u16>,
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
    input.trim().parse::<u16>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(input: &str) -> ReferenceQuery {
        parse_reference(&[input.to_string()]).unwrap_or_else(|e| panic!("{}: {}", input, e))
    }

    fn qs(input: &str) -> Vec<ReferenceQuery> {
        parse_references(&[input.to_string()]).unwrap_or_else(|e| panic!("{}: {}", input, e))
    }

    fn err(input: &str) -> bool {
        parse_references(&[input.to_string()]).is_err()
    }

    #[test]
    fn single_verse_colon() {
        let r = q("John 3:16");
        assert_eq!(r.book, "John");
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.verse_end, None);
        assert!(r.verse_list.is_empty());
    }

    #[test]
    fn single_verse_spaced() {
        let owned: Vec<String> = ["John", "3", "16"].iter().map(|s| s.to_string()).collect();
        let r = parse_reference(&owned).unwrap();
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
    }

    #[test]
    fn whole_chapter() {
        let r = q("Psalm 23");
        assert_eq!(r.book, "Psalms");
        assert_eq!(r.chapter, Some(23));
        assert_eq!(r.verse, None);
        assert_eq!(r.chapter_end, None);
    }

    #[test]
    fn whole_book() {
        let r = q("Romans");
        assert_eq!(r.book, "Romans");
        assert_eq!(r.chapter, None);
        assert_eq!(q("1 John").chapter, None);
    }

    #[test]
    fn range_colon_and_spaced() {
        for input in [
            "John 3:16-18",
            "John 3 16-18",
            "John 3:16 - 18",
            "John 3:16–18",
        ] {
            let r = q(input);
            assert_eq!(
                (r.chapter, r.verse, r.verse_end),
                (Some(3), Some(16), Some(18))
            );
        }
    }

    #[test]
    fn lists() {
        assert_eq!(q("John 3:16,18,20").verse_list, vec![16, 18, 20]);
        assert_eq!(q("John 3:16, 18").verse_list, vec![16, 18]);
        assert_eq!(q("John 3:16-18,20").verse_list, vec![16, 17, 18, 20]);
        assert_eq!(q("John 3:16,18").verse, Some(16));
    }

    #[test]
    fn multiword_book_range() {
        let r = q("1 John 4:7-9");
        assert_eq!(r.book, "1 John");
        assert_eq!(
            (r.chapter, r.verse, r.verse_end),
            (Some(4), Some(7), Some(9))
        );
    }

    #[test]
    fn chapter_ranges() {
        let r = q("Genesis 1-3");
        assert_eq!(
            (r.chapter, r.chapter_end, r.verse),
            (Some(1), Some(3), None)
        );
        let r = q("Matthew 5–7");
        assert_eq!((r.chapter, r.chapter_end), (Some(5), Some(7)));
        // A one-chapter "range" is just that chapter.
        let r = q("Psalm 23-23");
        assert_eq!((r.chapter, r.chapter_end), (Some(23), None));
        assert!(err("Genesis 3-1"));
    }

    #[test]
    fn cross_chapter_ranges() {
        let r = q("John 3:36-4:2");
        assert_eq!(
            (r.chapter, r.verse, r.chapter_end, r.verse_end),
            (Some(3), Some(36), Some(4), Some(2))
        );
        // Ending in the same chapter collapses to a plain range.
        let r = q("John 3:16-3:18");
        assert_eq!(
            (r.verse, r.verse_end, r.chapter_end),
            (Some(16), Some(18), None)
        );
        assert!(err("John 4:2-3:16"));
        assert!(err("John 3:18-3:16"));
    }

    #[test]
    fn osis_and_compact_forms() {
        for input in ["John.3.16", "Jn3:16", "John 3.16", "jn 3 16"] {
            let r = q(input);
            assert_eq!(
                (r.book.as_str(), r.chapter, r.verse),
                ("John", Some(3), Some(16))
            );
        }
        let r = q("1Cor.13.4-7");
        assert_eq!(r.book, "1 Corinthians");
        assert_eq!(
            (r.chapter, r.verse, r.verse_end),
            (Some(13), Some(4), Some(7))
        );
        assert_eq!(q("Ps.23").chapter, Some(23));
        assert_eq!(q("1Jn 4:8").book, "1 John");
        assert_eq!(q("John 3:16.").verse, Some(16));
    }

    #[test]
    fn single_chapter_books_take_verse_numbers() {
        let r = q("Jude 5");
        assert_eq!((r.chapter, r.verse), (Some(1), Some(5)));
        let r = q("Jude 3-5");
        assert_eq!(
            (r.chapter, r.verse, r.verse_end),
            (Some(1), Some(3), Some(5))
        );
        let r = q("Philemon 6");
        assert_eq!((r.chapter, r.verse), (Some(1), Some(6)));
        // "Jude 1" is the whole chapter; explicit forms are left alone.
        let r = q("Jude 1");
        assert_eq!((r.chapter, r.verse), (Some(1), None));
        let r = q("3 John 1:4");
        assert_eq!((r.chapter, r.verse), (Some(1), Some(4)));
        // Multi-chapter books are unaffected.
        assert_eq!(q("John 5").verse, None);
    }

    #[test]
    fn passage_lists() {
        let refs = qs("John 3:16; Romans 8:28");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[1].book, "Romans");

        // A bare chapter[:verse] continues the previous book.
        let refs = qs("Genesis 1:1; 2:4; Psalm 23; 24");
        let got: Vec<String> = refs.iter().map(|r| r.to_string()).collect();
        assert_eq!(
            got,
            ["Genesis 1:1", "Genesis 2:4", "Psalms 23", "Psalms 24"]
        );

        // ...but a numbered book still wins.
        assert_eq!(qs("John 3:16; 1 John 4:8")[1].book, "1 John");
        assert!(parse_reference(&["John 3:16; Romans 8:28".to_string()]).is_err());
        assert!(err("3:16"));
        assert!(err("John 3:16; Hezekiah 1:1"));
    }

    #[test]
    fn reversed_range_is_error() {
        assert!(err("John 3:18-16"));
    }

    #[test]
    fn one_chapter_ranges_lists_and_number_bounds() {
        // "Jude 1-1" is verse 1, just as "Jude 2-2" is verse 2.
        let r = q("Jude 1-1");
        assert_eq!((r.chapter, r.verse, r.verse_end), (Some(1), Some(1), None));
        let r = q("Jude 2-2");
        assert_eq!((r.chapter, r.verse, r.verse_end), (Some(1), Some(2), None));

        // A bare verse list works for one-chapter books only.
        let r = q("Jude 5,7");
        assert_eq!((r.chapter, r.verse_list.clone()), (Some(1), vec![5, 7]));
        assert_eq!(q("Philemon 6, 8").verse_list, vec![6, 8]);
        assert!(err("Psalm 23,24"));

        // Numbers start at 1 and must fit.
        for bad in [
            "John 0",
            "John 3:0",
            "John 0:1",
            "John 3:0-2",
            "Genesis 0-2",
            "Jude 0",
        ] {
            assert!(err(bad), "accepted {}", bad);
        }
        let too_big = parse_reference(&["John 99999".to_string()]).unwrap_err();
        assert_eq!(too_big.to_string(), "Number out of range: 99999");
    }

    #[test]
    fn format_references_elides_repeated_books_and_round_trips() {
        for (input, formatted) in [
            ("Gen 1:1; 2:4; John 3:16", "Genesis 1:1; 2:4; John 3:16"),
            ("jn 3:16,17,18,20", "John 3:16-18,20"),
            ("Mark 9:43,45", "Mark 9:43,45"),
            ("Psalm 23", "Psalms 23"),
            ("Jude 5; 7", "Jude 1:5; 1:7"),
            ("Jude; Romans 8", "Jude; Romans 8"),
            ("John; John 3", "John; 3"),
        ] {
            let parsed = qs(input);
            assert_eq!(format_references(&parsed), formatted, "{}", input);
            assert_eq!(qs(formatted), parsed, "{} does not round-trip", formatted);
        }
    }

    #[test]
    fn display_round_trips() {
        for input in [
            "John 3:16",
            "John 3:16-18",
            "John 3:16,18,20",
            "John 3:36-4:2",
            "Genesis 1-3",
            "Psalms 23",
            "Jude",
            "Jude 1:5",
            "1 Corinthians 13:4-7",
        ] {
            let parsed = q(input);
            assert_eq!(parsed.to_string(), input);
            assert_eq!(q(&parsed.to_string()), parsed);
        }
    }
}
