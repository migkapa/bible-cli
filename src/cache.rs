use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::books::normalize_book;
use crate::verses::Verse;

pub const DEFAULT_TRANSLATION: &str = "kjv";

/// Built-in source URLs for known public-domain translations, so common ones can
/// be installed with just `bible translation add <id>` (no `--source`).
const KNOWN_SOURCES: &[(&str, &str)] = &[
    (
        "kjv",
        "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/KJV.json",
    ),
    (
        "bbe",
        "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/BBE.json",
    ),
];

pub fn known_source(id: &str) -> Option<&'static str> {
    KNOWN_SOURCES
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, url)| *url)
}

#[derive(Debug)]
pub struct CachePaths {
    pub root: PathBuf,
    /// The active translation id (e.g. "kjv").
    pub translation: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub translation: String,
    pub source: String,
    pub created_at: String,
    pub verse_count: usize,
}

/// Persisted user config (currently just the default translation).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub default_translation: Option<String>,
}

/// A translation present in the cache.
pub struct InstalledTranslation {
    pub id: String,
    pub manifest: Option<Manifest>,
    pub size_bytes: u64,
}

impl CachePaths {
    pub fn new(root: PathBuf, translation: String) -> Self {
        Self { root, translation }
    }

    pub fn translations_root(&self) -> PathBuf {
        self.root.join("translations")
    }

    pub fn dir_for(&self, id: &str) -> PathBuf {
        self.translations_root().join(id)
    }

    pub fn verses_path_for(&self, id: &str) -> PathBuf {
        self.dir_for(id).join("verses.jsonl")
    }

    pub fn manifest_path_for(&self, id: &str) -> PathBuf {
        self.dir_for(id).join("manifest.json")
    }

    /// Verses path for the active translation.
    pub fn verses_path(&self) -> PathBuf {
        self.verses_path_for(&self.translation)
    }

    /// Manifest path for the active translation.
    pub fn manifest_path(&self) -> PathBuf {
        self.manifest_path_for(&self.translation)
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.verses_path_for(id).exists()
    }
}

pub fn default_cache_root() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".bible-cli");
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        return PathBuf::from(home).join(".bible-cli");
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Download, normalize, and store a translation under `translations/<id>/`.
/// When `source` is `None`, a known built-in source is used (error if unknown).
pub fn preload(paths: &CachePaths, id: &str, source: Option<&str>) -> Result<usize> {
    let source = match source {
        Some(s) => s.to_string(),
        None => known_source(id)
            .ok_or_else(|| {
                anyhow::anyhow!("No known source for '{}'. Pass --source <url-or-path>.", id)
            })?
            .to_string(),
    };

    let dir = paths.dir_for(id);
    fs::create_dir_all(&dir).with_context(|| format!("Failed creating {}", dir.display()))?;

    let raw = read_source(&source)?;
    let verses = normalize_source_to_verses(&raw)
        .with_context(|| format!("Failed parsing translation source from {}", source))?;

    write_jsonl(&paths.verses_path_for(id), &verses)?;
    write_manifest(&paths.manifest_path_for(id), id, &source, verses.len())?;

    Ok(verses.len())
}

/// List every translation present in the cache, sorted by id.
pub fn installed_translations(paths: &CachePaths) -> Vec<InstalledTranslation> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(paths.translations_root()) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        let verses_path = paths.verses_path_for(&id);
        if !verses_path.exists() {
            continue;
        }
        let size_bytes = fs::metadata(&verses_path).map(|m| m.len()).unwrap_or(0);
        let manifest = read_manifest(&paths.manifest_path_for(&id));
        out.push(InstalledTranslation {
            id,
            manifest,
            size_bytes,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Remove an installed translation's directory. Returns false if it was absent.
pub fn remove_translation(paths: &CachePaths, id: &str) -> Result<bool> {
    let dir = paths.dir_for(id);
    if !dir.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(&dir).with_context(|| format!("Failed removing {}", dir.display()))?;
    Ok(true)
}

fn config_path(root: &Path) -> PathBuf {
    root.join("config.json")
}

pub fn load_config(root: &Path) -> Config {
    fs::read_to_string(config_path(root))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// The configured default translation, if any.
pub fn load_default_translation(root: &Path) -> Option<String> {
    load_config(root).default_translation
}

pub fn save_default_translation(root: &Path, id: &str) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("Failed creating {}", root.display()))?;
    let mut config = load_config(root);
    config.default_translation = Some(id.to_string());
    let raw = serde_json::to_string_pretty(&config)?;
    fs::write(config_path(root), raw).context("Failed writing config")?;
    Ok(())
}

pub fn read_manifest(path: &Path) -> Option<Manifest> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_manifest(path: &Path, id: &str, source: &str, verse_count: usize) -> Result<()> {
    let manifest = Manifest {
        translation: id.to_string(),
        source: source.to_string(),
        created_at: Utc::now().to_rfc3339(),
        verse_count,
    };
    let raw = serde_json::to_string_pretty(&manifest)?;
    fs::write(path, raw)
        .with_context(|| format!("Failed writing manifest to {}", path.display()))?;
    Ok(())
}

fn read_source(source: &str) -> Result<String> {
    let trimmed = source.trim();
    if trimmed.starts_with("file://") {
        let path = trimmed.trim_start_matches("file://");
        return fs::read_to_string(path).with_context(|| format!("Failed reading {}", path));
    }

    let path = Path::new(trimmed);
    if path.exists() {
        return fs::read_to_string(path)
            .with_context(|| format!("Failed reading {}", path.display()));
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return http_get(trimmed);
    }

    bail!("Unsupported source: {}", source)
}

/// Download a URL's body. Runs the blocking HTTP client on a dedicated thread so
/// it never executes inside the async (tokio) runtime, where `reqwest::blocking`
/// would panic.
fn http_get(url: &str) -> Result<String> {
    let url = url.to_string();
    std::thread::spawn(move || -> Result<String> {
        let response =
            reqwest::blocking::get(&url).with_context(|| format!("Failed downloading {}", url))?;
        let status = response.status();
        if !status.is_success() {
            bail!("Download failed with status {}", status);
        }
        response.text().context("Failed reading response body")
    })
    .join()
    .map_err(|_| anyhow::anyhow!("Download thread panicked"))?
}

fn write_jsonl(path: &Path, verses: &[Verse]) -> Result<()> {
    let mut file =
        fs::File::create(path).with_context(|| format!("Failed writing {}", path.display()))?;
    for verse in verses {
        let line = serde_json::to_string(verse)?;
        writeln!(file, "{}", line)?;
    }
    Ok(())
}

fn normalize_source_to_verses(raw: &str) -> Result<Vec<Verse>> {
    let trimmed = strip_bom(raw).trim_start();
    // A file starting with '{' may still be JSONL (one object per line); fall
    // back to line parsing when it is not a single JSON document.
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            return parse_json_value(value);
        }
    }
    parse_jsonl(trimmed)
}

fn strip_bom(input: &str) -> &str {
    input.strip_prefix('\u{feff}').unwrap_or(input)
}

fn parse_jsonl(raw: &str) -> Result<Vec<Verse>> {
    let mut verses = Vec::new();
    for (idx, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let verse: Verse = serde_json::from_str(line)
            .with_context(|| format!("Invalid JSONL on line {}", idx + 1))?;
        verses.push(verse);
    }
    if verses.is_empty() {
        bail!("No verses found in JSONL source");
    }
    Ok(verses)
}

fn parse_json_value(value: Value) -> Result<Vec<Verse>> {
    match value {
        Value::Array(arr) => parse_array(&arr),
        Value::Object(obj) => {
            if let Some(books) = obj.get("books").and_then(|v| v.as_array()) {
                return parse_books(books);
            }
            if let Some(arr) = obj.get("verses").and_then(|v| v.as_array()) {
                return parse_array(arr);
            }
            if let Some(arr) = obj.get("data").and_then(|v| v.as_array()) {
                return parse_array(arr);
            }
            bail!("Unsupported JSON object structure for KJV source");
        }
        _ => bail!("Unsupported JSON structure for KJV source"),
    }
}

fn parse_array(arr: &[Value]) -> Result<Vec<Verse>> {
    let mut verses = Vec::new();
    for item in arr {
        if let Some(verse) = extract_verse(item) {
            verses.push(verse);
        }
    }
    if !verses.is_empty() {
        return Ok(verses);
    }

    let looks_like_books = arr.iter().any(|item| {
        item.as_object()
            .and_then(|obj| obj.get("chapters"))
            .is_some()
    });

    if looks_like_books {
        return parse_books(arr);
    }

    bail!("No verses found in array source");
}

fn parse_books(books: &[Value]) -> Result<Vec<Verse>> {
    let mut verses = Vec::new();
    for book_val in books {
        let Some(book_obj) = book_val.as_object() else {
            continue;
        };
        let book_name = extract_string(book_obj, &["name", "book", "bookName", "book_name"])
            .unwrap_or_else(|| "Unknown".to_string());
        let normalized_book = normalize_book(&book_name)
            .unwrap_or(book_name.as_str())
            .to_string();

        let Some(chapters) = book_obj.get("chapters").and_then(|v| v.as_array()) else {
            continue;
        };

        for (chapter_idx, chapter_val) in chapters.iter().enumerate() {
            // Chapters are either bare verse arrays (numbered by position) or
            // objects with explicit numbers:
            // {"chapter": 2, "verses": [{"verse": 16, "text": "..."}]}.
            let (chapter_num, verses_arr) = match chapter_val {
                Value::Array(arr) => ((chapter_idx + 1) as u16, arr),
                Value::Object(obj) => {
                    let Some(arr) = obj.get("verses").and_then(|v| v.as_array()) else {
                        continue;
                    };
                    let num = extract_u16(obj, &["chapter", "chapter_id", "chapterId"])
                        .unwrap_or((chapter_idx + 1) as u16);
                    (num, arr)
                }
                _ => continue,
            };
            for (verse_idx, verse_val) in verses_arr.iter().enumerate() {
                let (verse_num, text) = match verse_val {
                    Value::String(text) => ((verse_idx + 1) as u16, text.to_string()),
                    Value::Object(obj) => {
                        let num = extract_u16(obj, &["verse", "verse_id", "verseId", "verse_num"])
                            .unwrap_or((verse_idx + 1) as u16);
                        let text =
                            extract_string(obj, &["text", "content", "verse"]).unwrap_or_default();
                        (num, text)
                    }
                    _ => continue,
                };
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                verses.push(Verse {
                    book: normalized_book.clone(),
                    chapter: chapter_num,
                    verse: verse_num,
                    text: text.to_string(),
                });
            }
        }
    }
    if verses.is_empty() {
        bail!("No verses found in books structure");
    }
    Ok(verses)
}

fn extract_verse(value: &Value) -> Option<Verse> {
    let Value::Object(map) = value else {
        return None;
    };

    let book_raw = extract_string(map, &["book", "book_name", "bookName", "bookname"])?;
    let book = normalize_book(&book_raw)
        .unwrap_or(book_raw.as_str())
        .to_string();
    let chapter = extract_u16(map, &["chapter", "chapter_id", "chapterId"])?;
    let verse_num = extract_u16(map, &["verse", "verse_id", "verseId", "verse_num"])?;

    let mut text = extract_string(map, &["text", "content", "verse_text", "text_verse"]);
    if text.is_none() {
        if let Some(Value::String(s)) = map.get("verse") {
            text = Some(s.to_string());
        }
    }
    let text = text.unwrap_or_default();
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    Some(Verse {
        book,
        chapter,
        verse: verse_num,
        text: text.to_string(),
    })
}

fn extract_string(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(Value::String(value)) = map.get(*key) {
            return Some(value.to_string());
        }
    }
    None
}

fn extract_u16(map: &Map<String, Value>, keys: &[&str]) -> Option<u16> {
    for key in keys {
        if let Some(value) = map.get(*key) {
            match value {
                Value::Number(num) => {
                    if let Some(v) = num.as_u64() {
                        if v <= u16::MAX as u64 {
                            return Some(v as u16);
                        }
                    }
                }
                Value::String(s) => {
                    if let Ok(v) = s.parse::<u16>() {
                        return Some(v);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_chapters_with_explicit_numbers() {
        // scrollmapper-style: chapter objects carrying explicit chapter/verse
        // numbers, so a gap in the source cannot shift the numbering.
        let raw = r#"{
            "translation": "KJV",
            "books": [{
                "name": "Matthew",
                "chapters": [{
                    "chapter": 2,
                    "verses": [
                        {"verse": 15, "text": "And was there until the death of Herod."},
                        {"verse": 16, "text": "Then Herod, when he saw that he was mocked."}
                    ]
                }]
            }]
        }"#;
        let verses = normalize_source_to_verses(raw).unwrap();
        assert_eq!(verses.len(), 2);
        assert_eq!(verses[1].book, "Matthew");
        assert_eq!(verses[1].chapter, 2);
        assert_eq!(verses[1].verse, 16);
    }

    #[test]
    fn parses_positional_chapter_arrays() {
        // thiagobodruk-style: chapters as bare arrays, numbered by position.
        let raw = r#"[{
            "name": "Genesis",
            "chapters": [["In the beginning God created the heaven and the earth.", "And the earth was without form."]]
        }]"#;
        let verses = normalize_source_to_verses(raw).unwrap();
        assert_eq!(verses.len(), 2);
        assert_eq!(verses[0].book, "Genesis");
        assert_eq!(verses[0].chapter, 1);
        assert_eq!(verses[1].verse, 2);
    }

    #[test]
    fn falls_back_to_jsonl_when_not_a_single_document() {
        // A JSONL file starts with '{' but is not one JSON document.
        let raw = concat!(
            r#"{"book":"John","chapter":3,"verse":16,"text":"For God so loved the world"}"#,
            "\n",
            r#"{"book":"John","chapter":3,"verse":17,"text":"For God sent not his Son"}"#,
            "\n"
        );
        let verses = normalize_source_to_verses(raw).unwrap();
        assert_eq!(verses.len(), 2);
        assert_eq!(verses[0].verse, 16);
        assert_eq!(verses[1].verse, 17);
    }

    #[test]
    fn known_sources_cover_default_translations() {
        assert!(known_source("kjv").is_some());
        assert!(known_source("bbe").is_some());
        assert!(known_source("asv").is_none());
    }
}
