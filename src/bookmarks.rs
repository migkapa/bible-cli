//! Saved passages with optional notes and tags, persisted in
//! `<root>/bookmarks.json`. A bookmark stores a canonical reference label
//! rather than verse text, so it reads in whichever translation is active.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cache::write_atomic;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    /// Canonical label, e.g. `John 3:16-18` (see `VerseIndex::label`).
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Date added, `YYYY-MM-DD`.
    pub added: String,
}

impl Bookmark {
    /// Add tags not already present, keeping their order.
    pub fn merge_tags(&mut self, tags: &[String]) {
        for tag in tags {
            if !self.tags.contains(tag) {
                self.tags.push(tag.clone());
            }
        }
    }
}

fn bookmarks_path(root: &Path) -> PathBuf {
    root.join("bookmarks.json")
}

/// Load all bookmarks. A missing file is an empty list; an unreadable or
/// corrupt one is an error, so a later save can never replace the user's notes
/// with an empty list.
pub fn load_bookmarks(root: &Path) -> Result<Vec<Bookmark>> {
    let path = bookmarks_path(root);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err).with_context(|| format!("Failed reading {}", path.display())),
    };
    serde_json::from_str(&raw).with_context(|| {
        format!(
            "{} is not valid bookmark JSON; fix it or move it aside",
            path.display()
        )
    })
}

pub fn save_bookmarks(root: &Path, bookmarks: &[Bookmark]) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("Failed creating {}", root.display()))?;
    let raw = serde_json::to_string_pretty(bookmarks)?;
    write_atomic(&bookmarks_path(root), raw.as_bytes()).context("Failed writing bookmarks")
}

/// Normalize a tag: trimmed, lowercase, without a leading `#`, and with inner
/// whitespace turned into dashes. Empty tags are dropped.
pub fn normalize_tag(tag: &str) -> Option<String> {
    let tag = tag.trim().trim_start_matches('#').to_lowercase();
    let tag = tag.split_whitespace().collect::<Vec<_>>().join("-");
    (!tag.is_empty()).then_some(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bible-bookmarks-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn round_trips_and_omits_empty_fields() {
        let root = temp_root("roundtrip");
        assert!(load_bookmarks(&root).unwrap().is_empty());

        let marks = vec![
            Bookmark {
                reference: "John 3:16".to_string(),
                note: Some("memorize".to_string()),
                tags: vec!["love".to_string()],
                added: "2026-10-04".to_string(),
            },
            Bookmark {
                reference: "Psalms 23".to_string(),
                note: None,
                tags: Vec::new(),
                added: "2026-10-04".to_string(),
            },
        ];
        save_bookmarks(&root, &marks).unwrap();
        assert_eq!(load_bookmarks(&root).unwrap(), marks);

        let raw = fs::read_to_string(bookmarks_path(&root)).unwrap();
        assert!(!raw.contains("null"), "empty fields are omitted: {}", raw);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_file_is_an_error_not_an_empty_list() {
        let root = temp_root("corrupt");
        fs::create_dir_all(&root).unwrap();
        fs::write(bookmarks_path(&root), "{ not json").unwrap();
        assert!(load_bookmarks(&root).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tags_normalize_and_merge() {
        assert_eq!(normalize_tag(" #Love "), Some("love".to_string()));
        assert_eq!(normalize_tag("Good News"), Some("good-news".to_string()));
        assert_eq!(normalize_tag("#"), None);

        let mut mark = Bookmark {
            reference: "John 3:16".to_string(),
            note: None,
            tags: vec!["love".to_string()],
            added: "2026-10-04".to_string(),
        };
        mark.merge_tags(&["gospel".to_string(), "love".to_string()]);
        assert_eq!(mark.tags, ["love", "gospel"]);
    }
}
