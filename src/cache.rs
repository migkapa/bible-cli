use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::books::normalize_book;
use crate::verses::Verse;

const DEFAULT_KJV_SOURCE: &str =
    "https://raw.githubusercontent.com/thiagobodruk/bible/master/json/en_kjv.json";

#[derive(Debug)]
pub struct CachePaths {
    pub root: PathBuf,
    pub kjv_dir: PathBuf,
    pub verses_path: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub translation: String,
    pub source: String,
    pub created_at: String,
    pub verse_count: usize,
}

pub fn cache_paths(custom_root: Option<PathBuf>) -> CachePaths {
    let root = match custom_root {
        Some(path) => path,
        None => default_cache_root(),
    };
    let kjv_dir = root.join("translations").join("kjv");
    let verses_path = kjv_dir.join("verses.jsonl");
    let manifest_path = kjv_dir.join("manifest.json");
    CachePaths {
        root,
        kjv_dir,
        verses_path,
        manifest_path,
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

pub fn preload_kjv(paths: &CachePaths, source: Option<&str>) -> Result<usize> {
    fs::create_dir_all(&paths.kjv_dir)
        .with_context(|| format!("Failed creating {}", paths.kjv_dir.display()))?;

    let source = source.unwrap_or(DEFAULT_KJV_SOURCE);
    let raw = read_source(source)?;
    let verses = normalize_source_to_verses(&raw)
        .with_context(|| format!("Failed parsing KJV source from {}", source))?;

    write_jsonl(&paths.verses_path, &verses)?;
    write_manifest(&paths.manifest_path, source, verses.len())?;

    Ok(verses.len())
}

pub fn read_manifest(path: &Path) -> Option<Manifest> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_manifest(path: &Path, source: &str, verse_count: usize) -> Result<()> {
    let manifest = Manifest {
        translation: "KJV".to_string(),
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
        return fs::read_to_string(path)
            .with_context(|| format!("Failed reading {}", path));
    }

    let path = Path::new(trimmed);
    if path.exists() {
        return fs::read_to_string(path)
            .with_context(|| format!("Failed reading {}", path.display()));
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let response = reqwest::blocking::get(trimmed)
            .with_context(|| format!("Failed downloading {}", trimmed))?;
        let status = response.status();
        if !status.is_success() {
            bail!("Download failed with status {}", status);
        }
        return response.text().context("Failed reading response body");
    }

    bail!("Unsupported source: {}", source)
}

fn write_jsonl(path: &Path, verses: &[Verse]) -> Result<()> {
    let mut file = fs::File::create(path)
        .with_context(|| format!("Failed writing {}", path.display()))?;
    for verse in verses {
        let line = serde_json::to_string(verse)?;
        writeln!(file, "{}", line)?;
    }
    Ok(())
}

fn normalize_source_to_verses(raw: &str) -> Result<Vec<Verse>> {
    let trimmed = strip_bom(raw).trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        let value: Value = serde_json::from_str(trimmed)?;
        parse_json_value(value)
    } else {
        parse_jsonl(trimmed)
    }
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
        let Some(book_obj) = book_val.as_object() else { continue };
        let book_name = extract_string(book_obj, &["name", "book", "bookName", "book_name"])
            .unwrap_or_else(|| "Unknown".to_string());
        let normalized_book = normalize_book(&book_name)
            .unwrap_or(book_name.as_str())
            .to_string();

        let Some(chapters) = book_obj.get("chapters").and_then(|v| v.as_array()) else {
            continue;
        };

        for (chapter_idx, chapter_val) in chapters.iter().enumerate() {
            let chapter_num = (chapter_idx + 1) as u16;
            let Some(verses_arr) = chapter_val.as_array() else { continue };
            for (verse_idx, verse_val) in verses_arr.iter().enumerate() {
                let verse_num = (verse_idx + 1) as u16;
                let text = if let Some(text) = verse_val.as_str() {
                    text.to_string()
                } else if let Some(obj) = verse_val.as_object() {
                    extract_string(obj, &["text", "content", "verse"]).unwrap_or_default()
                } else {
                    String::new()
                };
                if text.trim().is_empty() {
                    continue;
                }
                verses.push(Verse {
                    book: normalized_book.clone(),
                    chapter: chapter_num,
                    verse: verse_num,
                    text,
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
    let Value::Object(map) = value else { return None };

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

    if text.trim().is_empty() {
        return None;
    }

    Some(Verse {
        book,
        chapter,
        verse: verse_num,
        text,
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
