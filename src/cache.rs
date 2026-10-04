use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use crate::books::normalize_book;
use crate::verses::Verse;

pub const DEFAULT_TRANSLATION: &str = "kjv";

/// A public-domain translation that installs with just `bible translation add
/// <id>` (no `--source`).
pub struct KnownTranslation {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
}

const KNOWN_TRANSLATIONS: &[KnownTranslation] = &[
    KnownTranslation {
        id: "kjv",
        name: "King James Version (1769)",
        url: "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/KJV.json",
    },
    KnownTranslation {
        id: "bbe",
        name: "Bible in Basic English (1949/1964)",
        url: "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/BBE.json",
    },
    KnownTranslation {
        id: "asv",
        name: "American Standard Version (1901)",
        url: "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/ASV.json",
    },
    KnownTranslation {
        id: "ylt",
        name: "Young's Literal Translation (1898)",
        url: "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/YLT.json",
    },
    KnownTranslation {
        id: "webster",
        name: "Webster Bible (1833)",
        url: "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/Webster.json",
    },
    KnownTranslation {
        id: "geneva",
        name: "Geneva Bible (1599)",
        url: "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/Geneva1599.json",
    },
    KnownTranslation {
        id: "nheb",
        name: "New Heart English Bible (modern English)",
        url: "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/NHEB.json",
    },
];

pub fn known_translations() -> &'static [KnownTranslation] {
    KNOWN_TRANSLATIONS
}

pub fn known_translation(id: &str) -> Option<&'static KnownTranslation> {
    KNOWN_TRANSLATIONS.iter().find(|t| t.id == id)
}

pub fn known_source(id: &str) -> Option<&'static str> {
    known_translation(id).map(|t| t.url)
}

/// Normalize a user-supplied translation id to lowercase, rejecting anything
/// but ASCII letters, digits, `-` and `_`. Ids become directory names under the
/// cache root, so this is what keeps `bible translation remove ../..` from
/// escaping it.
pub fn normalize_translation_id(id: &str) -> Result<String> {
    let normalized = id.trim().to_ascii_lowercase();
    let valid = normalized.len() <= 32
        && normalized
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && normalized
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        bail!(
            "Invalid translation id '{}': use letters, digits, '-' or '_' (e.g. kjv)",
            id
        );
    }
    Ok(normalized)
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
/// An existing install is only replaced once the new source parses cleanly.
pub fn preload(paths: &CachePaths, id: &str, source: Option<&str>) -> Result<usize> {
    let id = normalize_translation_id(id)?;
    let source = match source {
        Some(s) => s.to_string(),
        None => known_source(&id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No known source for '{}'. Pass --source <url-or-path> (see `bible translation available`).",
                    id
                )
            })?
            .to_string(),
    };

    let raw = read_source(&source)?;
    let verses = normalize_source_to_verses(&raw)
        .with_context(|| format!("Failed parsing translation source from {}", source))?;

    let dir = paths.dir_for(&id);
    fs::create_dir_all(&dir).with_context(|| format!("Failed creating {}", dir.display()))?;
    write_jsonl(&paths.verses_path_for(&id), &verses)?;
    write_manifest(&paths.manifest_path_for(&id), &id, &source, verses.len())?;

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
    // Re-validate here too: this is the one destructive path, so it must never
    // trust its caller to have rejected ids like "../..".
    let id = normalize_translation_id(id)?;
    let dir = paths.dir_for(&id);
    if !dir.is_dir() {
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

/// Set (or with `None`, clear) the configured default translation.
pub fn save_default_translation(root: &Path, id: Option<&str>) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("Failed creating {}", root.display()))?;
    let mut config = load_config(root);
    config.default_translation = id.map(str::to_string);
    let raw = serde_json::to_string_pretty(&config)?;
    write_atomic(&config_path(root), raw.as_bytes()).context("Failed writing config")?;
    Ok(())
}

/// Write a file via a temporary sibling and a rename, so an interrupted write
/// can never leave a truncated cache, config, or progress file behind.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!("{}.tmp", file_name));
    fs::write(&tmp, contents).with_context(|| format!("Failed writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("Failed writing {}", path.display()))?;
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
    write_atomic(path, raw.as_bytes())
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
    let mut out = String::new();
    for verse in verses {
        out.push_str(&serde_json::to_string(verse)?);
        out.push('\n');
    }
    write_atomic(path, out.as_bytes())
}

fn normalize_source_to_verses(raw: &str) -> Result<Vec<Verse>> {
    let trimmed = strip_bom(raw).trim_start();
    // A file starting with '{' may still be JSONL (one object per line); fall
    // back to line parsing when it is not a single JSON document, or when it is
    // a lone JSONL record (a one-line file parses as a single object).
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            let is_object = value.is_object();
            match parse_json_value(value) {
                Ok(verses) => return Ok(verses),
                Err(err) if !is_object => return Err(err),
                Err(_) => {}
            }
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
            bail!(
                "Unsupported JSON object structure (expected \"books\", \"verses\", or \"data\")"
            );
        }
        _ => bail!("Unsupported JSON structure: expected an array or object"),
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
        assert!(known_source("asv").is_some());
        assert!(known_source("niv").is_none());
        // Every built-in id must itself be a valid, already-normalized id.
        for t in known_translations() {
            assert_eq!(normalize_translation_id(t.id).unwrap(), t.id);
        }
    }

    #[test]
    fn single_line_jsonl_is_not_mistaken_for_a_json_document() {
        let raw = r#"{"book":"John","chapter":3,"verse":16,"text":"For God so loved the world"}"#;
        let verses = normalize_source_to_verses(raw).unwrap();
        assert_eq!(verses.len(), 1);
        assert_eq!(verses[0].verse, 16);
    }

    #[test]
    fn translation_ids_are_lowercased_and_validated() {
        assert_eq!(normalize_translation_id("KJV").unwrap(), "kjv");
        assert_eq!(normalize_translation_id(" bbe ").unwrap(), "bbe");
        assert_eq!(
            normalize_translation_id("my-bible_2").unwrap(),
            "my-bible_2"
        );
        for bad in [
            "", "..", "../..", "a/b", "a\\b", ".hidden", "-x", "x y", "kjv\0",
        ] {
            assert!(normalize_translation_id(bad).is_err(), "accepted {:?}", bad);
        }
        assert!(normalize_translation_id(&"a".repeat(33)).is_err());
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bible-cache-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn remove_translation_refuses_to_escape_the_cache() {
        let home = temp_dir("escape");
        let root = home.join(".bible-cli");
        fs::write(home.join("precious.txt"), "keep me").unwrap();
        let paths = CachePaths::new(root.clone(), "kjv".to_string());
        fs::create_dir_all(paths.dir_for("kjv")).unwrap();

        for bad in ["..", "../..", "../../.."] {
            assert!(
                remove_translation(&paths, bad).is_err(),
                "removed {:?}",
                bad
            );
        }
        assert!(home.join("precious.txt").exists());
        assert!(paths.dir_for("kjv").exists());

        // A normal removal still works, and ids are case-insensitive.
        assert!(remove_translation(&paths, "KJV").unwrap());
        assert!(!remove_translation(&paths, "kjv").unwrap());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn preload_installs_from_a_local_file_and_writes_atomically() {
        let dir = temp_dir("preload");
        let source = dir.join("tiny.jsonl");
        fs::write(
            &source,
            "{\"book\":\"John\",\"chapter\":3,\"verse\":16,\"text\":\"For God so loved\"}\n",
        )
        .unwrap();
        let paths = CachePaths::new(dir.join("root"), "kjv".to_string());

        let count = preload(&paths, "Tiny", Some(source.to_str().unwrap())).unwrap();
        assert_eq!(count, 1);
        assert!(paths.is_installed("tiny"));
        assert!(!paths.dir_for("tiny").join("verses.jsonl.tmp").exists());
        assert_eq!(
            read_manifest(&paths.manifest_path_for("tiny"))
                .unwrap()
                .verse_count,
            1
        );

        // A source that fails to parse must not leave a half-made install.
        let broken = dir.join("broken.json");
        fs::write(&broken, "not json at all").unwrap();
        assert!(preload(&paths, "broken", Some(broken.to_str().unwrap())).is_err());
        assert!(!paths.dir_for("broken").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
