use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::books::{BOOKS, OT_BOOK_COUNT};
use crate::verses::{max_chapter, Verse};

/// A built-in reading plan: a run of chapters spread over a fixed number of days.
pub struct PlanDef {
    pub id: &'static str,
    pub name: &'static str,
    pub days: u32,
    pub description: &'static str,
    kind: PlanKind,
}

/// How a plan's chapters are derived from the canon.
enum PlanKind {
    /// A contiguous run of books (positions in `BOOKS`), chunked evenly by chapter.
    Books { start: usize, end: usize },
    /// Psalms spread across the month, plus one Proverbs chapter per day.
    PsalmsProverbs,
}

const CATALOG: &[PlanDef] = &[
    PlanDef {
        id: "bible-1y",
        name: "Bible in a Year",
        days: 365,
        description: "The whole canon, Genesis to Revelation, in 365 days",
        kind: PlanKind::Books {
            start: 0,
            end: BOOKS.len() - 1,
        },
    },
    PlanDef {
        id: "nt-90",
        name: "New Testament in 90 Days",
        days: 90,
        description: "Matthew to Revelation in three months",
        kind: PlanKind::Books {
            start: OT_BOOK_COUNT,
            end: BOOKS.len() - 1,
        },
    },
    PlanDef {
        id: "gospels-30",
        name: "The Gospels in 30 Days",
        days: 30,
        description: "Matthew, Mark, Luke, and John in a month",
        kind: PlanKind::Books {
            start: OT_BOOK_COUNT,
            end: OT_BOOK_COUNT + 3,
        },
    },
    PlanDef {
        id: "psalms-proverbs-31",
        name: "Psalms & Proverbs in a Month",
        days: 31,
        description: "Wisdom for every day: Psalms plus a Proverbs chapter",
        kind: PlanKind::PsalmsProverbs,
    },
];

pub fn all_plans() -> &'static [PlanDef] {
    CATALOG
}

pub fn find_plan(id: &str) -> Option<&'static PlanDef> {
    CATALOG.iter().find(|p| p.id.eq_ignore_ascii_case(id))
}

/// One chapter of one book, as scheduled by a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterRef {
    pub book: &'static str,
    pub chapter: u16,
}

/// Expand a plan into per-day chapter portions. Chapter counts come from the
/// loaded corpus, so any installed translation works.
pub fn build_days(plan: &PlanDef, verses: &[Verse]) -> Result<Vec<Vec<ChapterRef>>> {
    match plan.kind {
        PlanKind::Books { start, end } => {
            let mut chapters = Vec::new();
            for book in &BOOKS[start..=end] {
                chapters.extend(book_chapters(verses, book.name)?);
            }
            Ok(chunk_evenly(&chapters, plan.days as usize))
        }
        PlanKind::PsalmsProverbs => {
            let psalms = book_chapters(verses, "Psalms")?;
            let proverbs = book_chapters(verses, "Proverbs")?;
            let mut days = chunk_evenly(&psalms, plan.days as usize);
            for (day, chapter) in days.iter_mut().zip(&proverbs) {
                day.push(*chapter);
            }
            Ok(days)
        }
    }
}

fn book_chapters(verses: &[Verse], book: &'static str) -> Result<Vec<ChapterRef>> {
    let Some(max) = max_chapter(verses, book) else {
        bail!(
            "{} is missing from the cached corpus; re-run `bible cache --preload`.",
            book
        );
    };
    Ok((1..=max)
        .map(|chapter| ChapterRef { book, chapter })
        .collect())
}

/// Split items into `days` consecutive chunks whose sizes differ by at most one,
/// covering every item exactly once and preserving order.
fn chunk_evenly<T: Copy>(items: &[T], days: usize) -> Vec<Vec<T>> {
    let total = items.len();
    (0..days)
        .map(|i| items[i * total / days..(i + 1) * total / days].to_vec())
        .collect()
}

/// Human label for a day's portion, e.g. `Matthew 5-7` or `Malachi 3-4, Matthew 1`.
pub fn portion_label(portion: &[ChapterRef]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < portion.len() {
        let book = portion[i].book;
        let start = portion[i].chapter;
        let mut end = start;
        let mut j = i + 1;
        while j < portion.len() && portion[j].book == book && portion[j].chapter == end + 1 {
            end = portion[j].chapter;
            j += 1;
        }
        if start == end {
            parts.push(format!("{} {}", book, start));
        } else {
            parts.push(format!("{} {}-{}", book, start, end));
        }
        i = j;
    }
    parts.join(", ")
}

/// Persisted progress for the active plan, stored at `<root>/plan.json` next to
/// `config.json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct PlanState {
    pub plan_id: String,
    /// Start date, `YYYY-MM-DD`.
    pub started: String,
    #[serde(default)]
    pub completed: Vec<u32>,
}

impl PlanState {
    /// The next uncompleted day, if any remain. Reading is sequential, so a
    /// missed day is read next rather than skipped.
    pub fn next_day(&self, total_days: u32) -> Option<u32> {
        (1..=total_days).find(|d| !self.completed.contains(d))
    }

    /// Number of distinct valid days completed.
    pub fn done_count(&self, total_days: u32) -> u32 {
        let mut days: Vec<u32> = self
            .completed
            .iter()
            .copied()
            .filter(|d| (1..=total_days).contains(d))
            .collect();
        days.sort_unstable();
        days.dedup();
        days.len() as u32
    }
}

fn state_path(root: &Path) -> PathBuf {
    root.join("plan.json")
}

pub fn load_state(root: &Path) -> Option<PlanState> {
    let raw = fs::read_to_string(state_path(root)).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save_state(root: &Path, state: &PlanState) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("Failed creating {}", root.display()))?;
    let raw = serde_json::to_string_pretty(state)?;
    fs::write(state_path(root), raw).context("Failed writing plan state")?;
    Ok(())
}

/// Remove the active plan state. Returns false if none existed.
pub fn clear_state(root: &Path) -> Result<bool> {
    let path = state_path(root);
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(&path).with_context(|| format!("Failed removing {}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One verse per chapter is enough for chapter derivation.
    fn corpus(books: &[(&str, u16)]) -> Vec<Verse> {
        let mut verses = Vec::new();
        for (book, chapters) in books {
            for chapter in 1..=*chapters {
                verses.push(Verse {
                    book: book.to_string(),
                    chapter,
                    verse: 1,
                    text: "In the beginning".to_string(),
                });
            }
        }
        verses
    }

    /// Chapter counts for the whole NT (Matthew..Revelation).
    const NT_CHAPTERS: &[(&str, u16)] = &[
        ("Matthew", 28),
        ("Mark", 16),
        ("Luke", 24),
        ("John", 21),
        ("Acts", 28),
        ("Romans", 16),
        ("1 Corinthians", 16),
        ("2 Corinthians", 13),
        ("Galatians", 6),
        ("Ephesians", 6),
        ("Philippians", 4),
        ("Colossians", 4),
        ("1 Thessalonians", 5),
        ("2 Thessalonians", 3),
        ("1 Timothy", 6),
        ("2 Timothy", 4),
        ("Titus", 3),
        ("Philemon", 1),
        ("Hebrews", 13),
        ("James", 5),
        ("1 Peter", 5),
        ("2 Peter", 3),
        ("1 John", 5),
        ("2 John", 1),
        ("3 John", 1),
        ("Jude", 1),
        ("Revelation", 22),
    ];

    #[test]
    fn chunk_evenly_covers_all_items_in_order() {
        let items: Vec<u32> = (1..=260).collect();
        let chunks = chunk_evenly(&items, 90);
        assert_eq!(chunks.len(), 90);
        let flat: Vec<u32> = chunks.iter().flatten().copied().collect();
        assert_eq!(flat, items);
        let sizes: Vec<usize> = chunks.iter().map(|c| c.len()).collect();
        assert!(sizes.iter().all(|&s| s == 2 || s == 3), "{:?}", sizes);
    }

    #[test]
    fn nt_90_covers_each_chapter_exactly_once() {
        let verses = corpus(NT_CHAPTERS);
        let plan = find_plan("nt-90").unwrap();
        let days = build_days(plan, &verses).unwrap();
        assert_eq!(days.len(), 90);
        let flat: Vec<ChapterRef> = days.into_iter().flatten().collect();
        assert_eq!(flat.len(), 260);
        assert_eq!(
            flat.first(),
            Some(&ChapterRef {
                book: "Matthew",
                chapter: 1
            })
        );
        assert_eq!(
            flat.last(),
            Some(&ChapterRef {
                book: "Revelation",
                chapter: 22
            })
        );
    }

    #[test]
    fn psalms_proverbs_pairs_each_day_with_a_proverb() {
        let verses = corpus(&[("Psalms", 150), ("Proverbs", 31)]);
        let plan = find_plan("psalms-proverbs-31").unwrap();
        let days = build_days(plan, &verses).unwrap();
        assert_eq!(days.len(), 31);

        // Every day ends with its matching Proverbs chapter.
        for (i, day) in days.iter().enumerate() {
            let proverb = day.last().unwrap();
            assert_eq!(proverb.book, "Proverbs");
            assert_eq!(proverb.chapter as usize, i + 1);
        }

        // All 150 psalms appear exactly once, in order.
        let psalms: Vec<u16> = days
            .iter()
            .flatten()
            .filter(|c| c.book == "Psalms")
            .map(|c| c.chapter)
            .collect();
        assert_eq!(psalms, (1..=150).collect::<Vec<u16>>());

        // Day 31 includes Psalm 150.
        assert!(days[30].contains(&ChapterRef {
            book: "Psalms",
            chapter: 150
        }));
    }

    #[test]
    fn portion_label_compresses_runs() {
        let portion = [
            ChapterRef {
                book: "Malachi",
                chapter: 3,
            },
            ChapterRef {
                book: "Malachi",
                chapter: 4,
            },
            ChapterRef {
                book: "Matthew",
                chapter: 1,
            },
        ];
        assert_eq!(portion_label(&portion), "Malachi 3-4, Matthew 1");
        assert_eq!(portion_label(&portion[2..]), "Matthew 1");
    }

    #[test]
    fn plan_state_round_trips_and_tracks_days() {
        let dir = std::env::temp_dir().join(format!("bible-plan-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        assert!(load_state(&dir).is_none());
        let state = PlanState {
            plan_id: "nt-90".to_string(),
            started: "2026-07-04".to_string(),
            completed: vec![1, 2, 4],
        };
        save_state(&dir, &state).unwrap();

        let loaded = load_state(&dir).unwrap();
        assert_eq!(loaded.plan_id, "nt-90");
        assert_eq!(loaded.completed, vec![1, 2, 4]);
        // Day 3 was skipped, so it is next.
        assert_eq!(loaded.next_day(90), Some(3));
        assert_eq!(loaded.done_count(90), 3);

        assert!(clear_state(&dir).unwrap());
        assert!(!clear_state(&dir).unwrap());
        assert!(load_state(&dir).is_none());

        let _ = fs::remove_dir_all(&dir);
    }
}
