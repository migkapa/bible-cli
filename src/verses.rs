use anyhow::{anyhow, bail, Context, Result};
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
    let file =
        File::open(path).with_context(|| format!("Translation not found at {}", path.display()))?;
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
        bail!("Translation cache is empty at {}", path.display());
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
    /// Indices of each chapter's verses, sorted by verse number.
    by_chapter: HashMap<(&'a str, u16), Vec<usize>>,
    max_chapter: HashMap<&'a str, u16>,
}

impl<'a> VerseIndex<'a> {
    pub fn build(verses: &'a [Verse]) -> Self {
        let mut by_key = HashMap::with_capacity(verses.len());
        let mut by_chapter: HashMap<(&'a str, u16), Vec<usize>> = HashMap::new();
        let mut max_chapter: HashMap<&'a str, u16> = HashMap::new();
        for (idx, v) in verses.iter().enumerate() {
            by_key.insert((v.book.as_str(), v.chapter, v.verse), idx);
            by_chapter
                .entry((v.book.as_str(), v.chapter))
                .or_default()
                .push(idx);
            let max = max_chapter.entry(v.book.as_str()).or_insert(v.chapter);
            *max = (*max).max(v.chapter);
        }
        for indices in by_chapter.values_mut() {
            indices.sort_by_key(|&i| verses[i].verse);
        }
        Self {
            verses,
            by_key,
            by_chapter,
            max_chapter,
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
            Some(indices) => indices.iter().map(|&i| &self.verses[i]).collect(),
            None => Vec::new(),
        }
    }

    /// The highest chapter number of a book, or `None` if the book is absent.
    pub fn max_chapter(&self, book: &str) -> Option<u16> {
        self.max_chapter.get(book).copied()
    }

    /// All verses of a book in canonical order.
    pub fn book(&self, book: &str) -> Vec<&'a Verse> {
        let max = self.max_chapter(book).unwrap_or(0);
        (1..=max).flat_map(|c| self.chapter(book, c)).collect()
    }

    /// Resolve a parsed reference into the matching verses: a whole book, a
    /// chapter or run of chapters, a verse range (possibly across chapters),
    /// a verse list, or a single verse.
    pub fn resolve(&self, query: &ReferenceQuery) -> Result<Vec<&'a Verse>> {
        let book = query.book.as_str();
        let out: Vec<&'a Verse> = match (query.chapter, query.chapter_end) {
            (None, _) => self.book(book),
            (Some(chapter), Some(end_chapter)) => {
                let mut out = Vec::new();
                for ch in chapter..=end_chapter {
                    for v in self.chapter(book, ch) {
                        let after_start = ch > chapter || query.verse.is_none_or(|s| v.verse >= s);
                        let before_end =
                            ch < end_chapter || query.verse_end.is_none_or(|e| v.verse <= e);
                        if after_start && before_end {
                            out.push(v);
                        }
                    }
                }
                out
            }
            (Some(chapter), None) if !query.verse_list.is_empty() => query
                .verse_list
                .iter()
                .filter_map(|&v| self.get(book, chapter, v))
                .collect(),
            (Some(chapter), None) => match (query.verse, query.verse_end) {
                (Some(start), Some(end)) => (start..=end)
                    .filter_map(|v| self.get(book, chapter, v))
                    .collect(),
                (Some(verse), None) => self.get(book, chapter, verse).into_iter().collect(),
                (None, _) => self.chapter(book, chapter),
            },
        };
        if out.is_empty() {
            return Err(self.not_found(query));
        }
        Ok(out)
    }

    /// Resolve several references, concatenating their verses in order.
    pub fn resolve_all(&self, queries: &[ReferenceQuery]) -> Result<Vec<&'a Verse>> {
        let mut out = Vec::new();
        for query in queries {
            out.extend(self.resolve(query)?);
        }
        Ok(out)
    }

    fn not_found(&self, query: &ReferenceQuery) -> anyhow::Error {
        let Some(max) = self.max_chapter(&query.book) else {
            return anyhow!("{} is not in this translation", query.book);
        };
        match query.chapter {
            Some(chapter) if chapter > max => anyhow!(
                "{} has {} chapter{}",
                query.book,
                max,
                if max == 1 { "" } else { "s" }
            ),
            Some(_) if query.verse.is_some() && query.verse_end.is_none() => {
                anyhow!("{} not found", query)
            }
            _ => anyhow!("No verses found for {}", query),
        }
    }

    /// Widen a selection by up to `window` verses before its first verse and
    /// after its last, staying inside those verses' chapters. Everything
    /// between the first and last verse is included, so gaps in a list are
    /// filled in. Selections spanning books are returned unchanged.
    pub fn with_context(&self, selected: &[&'a Verse], window: usize) -> Vec<&'a Verse> {
        let (Some(first), Some(last)) = (selected.first(), selected.last()) else {
            return Vec::new();
        };
        if first.book != last.book || (last.chapter, last.verse) < (first.chapter, first.verse) {
            return selected.to_vec();
        }
        let (Some((first_pos, _)), Some((last_pos, _))) = (
            self.position_in_chapter(first),
            self.position_in_chapter(last),
        ) else {
            return selected.to_vec();
        };

        let mut out = Vec::new();
        for chapter in first.chapter..=last.chapter {
            let verses = self.chapter(&first.book, chapter);
            if verses.is_empty() {
                continue;
            }
            let start = if chapter == first.chapter {
                first_pos.saturating_sub(window)
            } else {
                0
            };
            let end = if chapter == last.chapter {
                (last_pos + window).min(verses.len() - 1)
            } else {
                verses.len() - 1
            };
            out.extend_from_slice(&verses[start..=end]);
        }
        out
    }

    /// A verse's position within its chapter and the chapter's verse count.
    fn position_in_chapter(&self, v: &Verse) -> Option<(usize, usize)> {
        let indices = self.by_chapter.get(&(v.book.as_str(), v.chapter))?;
        let pos = indices
            .binary_search_by_key(&v.verse, |&i| self.verses[i].verse)
            .ok()?;
        Some((pos, indices.len()))
    }

    /// True when `b` is the verse right after `a` in this translation.
    fn follows(&self, a: &Verse, b: &Verse) -> bool {
        if a.book != b.book {
            return false;
        }
        let (Some((a_pos, a_len)), Some((b_pos, _))) =
            (self.position_in_chapter(a), self.position_in_chapter(b))
        else {
            return false;
        };
        if a.chapter == b.chapter {
            b_pos == a_pos + 1
        } else {
            b.chapter == a.chapter + 1 && a_pos + 1 == a_len && b_pos == 0
        }
    }

    fn starts_chapter(&self, v: &Verse) -> bool {
        self.position_in_chapter(v).is_some_and(|(pos, _)| pos == 0)
    }

    fn ends_chapter(&self, v: &Verse) -> bool {
        self.position_in_chapter(v)
            .is_some_and(|(pos, len)| pos + 1 == len)
    }

    /// A canonical citation for a selection. Contiguous runs are compressed
    /// and whole chapters or books named on their own: `John 3:16-18, 20`,
    /// `John 3:36-4:2`, `Psalms 23`, `Genesis 1-3`, `Jude`, or
    /// `John 3:16; 4:1; Romans 8:28`. Labels parse back to the same selection.
    pub fn label(&self, selected: &[&Verse]) -> String {
        let mut runs: Vec<(&Verse, &Verse)> = Vec::new();
        for &v in selected {
            match runs.last_mut() {
                Some((_, end)) if self.follows(end, v) => *end = v,
                _ => runs.push((v, v)),
            }
        }

        let mut out = String::new();
        // The previous run's book and final chapter, and whether it was a plain
        // verse run that a ", <verse>" continuation may extend.
        let mut prev: Option<(&str, u16, bool)> = None;
        for (start, end) in runs {
            let whole_chapters = self.starts_chapter(start) && self.ends_chapter(end);
            let whole_book = whole_chapters
                && start.chapter == 1
                && self.max_chapter(&start.book) == Some(end.chapter);
            let verse_run = !whole_chapters && start.chapter == end.chapter;
            let verses = if start.verse == end.verse {
                start.verse.to_string()
            } else {
                format!("{}-{}", start.verse, end.verse)
            };
            let body = if whole_book {
                String::new()
            } else if whole_chapters && start.chapter == end.chapter {
                start.chapter.to_string()
            } else if whole_chapters {
                format!("{}-{}", start.chapter, end.chapter)
            } else if verse_run {
                format!("{}:{}", start.chapter, verses)
            } else {
                format!(
                    "{}:{}-{}:{}",
                    start.chapter, start.verse, end.chapter, end.verse
                )
            };

            match prev {
                Some((book, chapter, true))
                    if verse_run && book == start.book && chapter == start.chapter =>
                {
                    out.push_str(", ");
                    out.push_str(&verses);
                }
                Some((book, _, _)) if book == start.book && !whole_book => {
                    out.push_str("; ");
                    out.push_str(&body);
                }
                _ => {
                    if prev.is_some() {
                        out.push_str("; ");
                    }
                    out.push_str(&start.book);
                    if !body.is_empty() {
                        out.push(' ');
                        out.push_str(&body);
                    }
                }
            }
            prev = Some((&start.book, end.chapter, verse_run));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::parse_references;

    /// A small corpus: John 3 (36 verses), John 4 (54), Romans 8 (39), Jude (25),
    /// and Matthew 17 with verse 21 missing, as in some modern translations.
    fn corpus() -> Vec<Verse> {
        let mut verses = Vec::new();
        let mut add = |book: &str, chapter: u16, count: u16| {
            for verse in 1..=count {
                verses.push(Verse {
                    book: book.to_string(),
                    chapter,
                    verse,
                    text: format!("{} {}:{}", book, chapter, verse),
                });
            }
        };
        add("John", 3, 36);
        add("John", 4, 54);
        add("Romans", 8, 39);
        add("Jude", 1, 25);
        add("Matthew", 17, 27);
        verses.retain(|v| !(v.book == "Matthew" && v.verse == 21));
        verses
    }

    fn refs(selection: &[&Verse]) -> Vec<String> {
        selection
            .iter()
            .map(|v| format!("{} {}:{}", v.book, v.chapter, v.verse))
            .collect()
    }

    fn resolve<'a>(index: &VerseIndex<'a>, input: &str) -> Vec<&'a Verse> {
        let queries = parse_references(&[input.to_string()]).unwrap();
        index.resolve_all(&queries).unwrap()
    }

    #[test]
    fn resolves_chapter_and_cross_chapter_ranges() {
        let verses = corpus();
        let index = VerseIndex::build(&verses);
        assert_eq!(resolve(&index, "John 3-4").len(), 36 + 54);
        assert_eq!(
            refs(&resolve(&index, "John 3:35-4:2")),
            ["John 3:35", "John 3:36", "John 4:1", "John 4:2"]
        );
        assert_eq!(resolve(&index, "Jude").len(), 25);
        assert_eq!(refs(&resolve(&index, "Jude 3-4")), ["Jude 1:3", "Jude 1:4"]);
    }

    #[test]
    fn helpful_errors_when_nothing_matches() {
        let verses = corpus();
        let index = VerseIndex::build(&verses);
        let err = |input: &str| {
            let queries = parse_references(&[input.to_string()]).unwrap();
            index.resolve_all(&queries).unwrap_err().to_string()
        };
        assert_eq!(err("John 3:99"), "John 3:99 not found");
        assert_eq!(err("Jude 1:30-40"), "No verses found for Jude 1:30-40");
        assert_eq!(err("Romans 99"), "Romans has 8 chapters");
        assert_eq!(err("Genesis 1"), "Genesis is not in this translation");
    }

    #[test]
    fn context_widens_within_chapters() {
        let verses = corpus();
        let index = VerseIndex::build(&verses);
        let selected = resolve(&index, "John 3:16,18");
        assert_eq!(
            refs(&index.with_context(&selected, 1)),
            [
                "John 3:15",
                "John 3:16",
                "John 3:17",
                "John 3:18",
                "John 3:19"
            ]
        );
        // Clamped at chapter edges.
        let selected = resolve(&index, "John 3:36");
        assert_eq!(
            refs(&index.with_context(&selected, 2)),
            ["John 3:34", "John 3:35", "John 3:36"]
        );
    }

    #[test]
    fn labels_compress_runs_and_round_trip() {
        let verses = corpus();
        let index = VerseIndex::build(&verses);
        for (input, label) in [
            ("John 3:16", "John 3:16"),
            ("John 3:16-18", "John 3:16-18"),
            ("John 3:16,18,20", "John 3:16, 18, 20"),
            ("John 3:16-18,20", "John 3:16-18, 20"),
            ("John 3:36-4:2", "John 3:36-4:2"),
            ("John 3", "John 3"),
            ("John 3:1-36", "John 3"),
            ("John 3-4", "John 3-4"),
            ("John 3:1-4:54", "John 3-4"),
            ("Jude", "Jude"),
            ("Jude 5", "Jude 1:5"),
            ("Matthew 17", "Matthew 17"),
            ("Matthew 17:20-22", "Matthew 17:20-22"),
            ("John 3:16; 4:1", "John 3:16; 4:1"),
            ("John 3:35-4:1; 4:5", "John 3:35-4:1; 4:5"),
            ("John 3:16; Romans 8:28", "John 3:16; Romans 8:28"),
            ("Romans 8:28; John 3", "Romans 8:28; John 3"),
        ] {
            let selection = resolve(&index, input);
            let got = index.label(&selection);
            assert_eq!(got, label, "label for {}", input);
            assert_eq!(
                refs(&resolve(&index, &got)),
                refs(&selection),
                "{} does not round-trip",
                got
            );
        }
    }
}
