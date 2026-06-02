mod markdown;
mod spinner;

use std::env;
use std::io::{self, IsTerminal, Write};

use clap::ValueEnum;
use serde::Serialize;
use termimad::crossterm::style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor};

use crate::books::osis_code;
use crate::cli::ColorMode;
use crate::verses::Verse;

pub use markdown::MarkdownRenderer;
pub use spinner::ThinkingIndicator;

/// How verse output is rendered. `Plain` is the default human-readable form;
/// the rest turn the CLI into a scriptable data source.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Colorized reference + text (default).
    Plain,
    /// A single JSON array of verse records.
    Json,
    /// One JSON object per line (newline-delimited).
    Ndjson,
    /// Tab-separated: id, book, chapter, verse, text.
    Tsv,
    /// Just the reference (e.g. `John 3:16`).
    Ref,
    /// Just the verse text, no reference or color.
    Raw,
}

#[derive(Serialize)]
struct VerseRecord<'a> {
    id: String,
    reference: String,
    book: &'a str,
    chapter: u16,
    verse: u16,
    text: &'a str,
}

/// Serialize verses to a pretty JSON array of records (id, reference, fields).
/// Independent of the active output format — used by `export --to json`.
pub fn verses_to_json(verses: &[&Verse]) -> String {
    let records: Vec<VerseRecord> = verses.iter().map(|v| VerseRecord::new(v)).collect();
    serde_json::to_string_pretty(&records).unwrap_or_else(|_| "[]".to_string())
}

impl<'a> VerseRecord<'a> {
    fn new(v: &'a Verse) -> Self {
        Self {
            id: format!("{}.{}.{}", osis_code(&v.book), v.chapter, v.verse),
            reference: format!("{} {}:{}", v.book, v.chapter, v.verse),
            book: &v.book,
            chapter: v.chapter,
            verse: v.verse,
            text: &v.text,
        }
    }
}

pub struct OutputStyle {
    pub color: bool,
    pub theme: Theme,
    pub format: Format,
}

pub struct Theme {
    pub reference: Color,
    pub marker: Color,
    pub user_prompt: Color,
    pub dim: Color,
    pub separator: Color,
}

impl Theme {
    pub fn claude_code() -> Self {
        Self {
            reference: Color::Cyan,
            marker: Color::Yellow,
            user_prompt: Color::White,
            dim: Color::DarkGrey,
            separator: Color::DarkGrey,
        }
    }
}

impl OutputStyle {
    pub fn new(mode: ColorMode, format: Format) -> Self {
        let mut color = match mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => should_color_auto(),
        };
        // Machine-readable formats are never colorized.
        if !matches!(format, Format::Plain) {
            color = false;
        }
        Self {
            color,
            theme: Theme::claude_code(),
            format,
        }
    }

    /// True when output is a machine-readable format rather than the decorated
    /// human view. Commands use this to suppress prompts, headers, and banners.
    pub fn is_structured(&self) -> bool {
        !matches!(self.format, Format::Plain)
    }

    /// Render a set of verses according to the active format.
    pub fn emit_verses(&self, verses: &[&Verse]) {
        match self.format {
            Format::Plain => {
                for v in verses {
                    println!("{}", self.verse_line(v));
                }
            }
            Format::Raw => {
                for v in verses {
                    println!("{}", v.text);
                }
            }
            Format::Ref => {
                for v in verses {
                    println!("{} {}:{}", v.book, v.chapter, v.verse);
                }
            }
            Format::Tsv => {
                for v in verses {
                    println!(
                        "{}.{}.{}\t{}\t{}\t{}\t{}",
                        osis_code(&v.book),
                        v.chapter,
                        v.verse,
                        v.book,
                        v.chapter,
                        v.verse,
                        v.text
                    );
                }
            }
            Format::Ndjson => {
                for v in verses {
                    if let Ok(line) = serde_json::to_string(&VerseRecord::new(v)) {
                        println!("{}", line);
                    }
                }
            }
            Format::Json => {
                let records: Vec<VerseRecord> =
                    verses.iter().map(|v| VerseRecord::new(v)).collect();
                match serde_json::to_string_pretty(&records) {
                    Ok(json) => println!("{}", json),
                    Err(_) => println!("[]"),
                }
            }
        }
    }

    pub fn verse_line(&self, verse: &Verse) -> String {
        let reference = format!("{} {}:{}", verse.book, verse.chapter, verse.verse);
        if self.color {
            format!(
                "{}{}{}  {}",
                SetForegroundColor(self.theme.reference),
                reference,
                ResetColor,
                verse.text
            )
        } else {
            format!("{}  {}", reference, verse.text)
        }
    }

    pub fn marked_verse_line(&self, marker: &str, verse: &Verse) -> String {
        if self.color && marker == "*" {
            format!(
                "{}{}{} {}",
                SetForegroundColor(self.theme.marker),
                marker,
                ResetColor,
                self.verse_line(verse)
            )
        } else {
            format!("{} {}", marker, self.verse_line(verse))
        }
    }

    pub fn print_user_prompt(&self) {
        if self.color {
            print!(
                "{}{}you>{} ",
                SetForegroundColor(self.theme.user_prompt),
                SetAttribute(Attribute::Bold),
                ResetColor
            );
        } else {
            print!("you> ");
        }
        io::stdout().flush().ok();
    }

    pub fn print_separator(&self) {
        if self.color {
            println!(
                "{}{}{}",
                SetForegroundColor(self.theme.separator),
                "─".repeat(terminal_width().min(60)),
                ResetColor
            );
        }
    }

    pub fn print_chat_intro(&self) {
        if self.color {
            println!(
                "{}Chat mode. /help for commands, /exit to quit.{}",
                SetForegroundColor(self.theme.dim),
                ResetColor
            );
        } else {
            println!("Chat mode. /help for commands, /exit to quit.");
        }
    }

    /// Print a passage reference as a heading (colorized when enabled).
    pub fn print_reference_heading(&self, reference: &str) {
        if self.color {
            println!(
                "{}{}{}",
                SetForegroundColor(self.theme.reference),
                reference,
                ResetColor
            );
        } else {
            println!("{}", reference);
        }
    }

    pub fn print_dim(&self, text: &str) {
        if self.color {
            println!(
                "{}{}{}",
                SetForegroundColor(self.theme.dim),
                text,
                ResetColor
            );
        } else {
            println!("{}", text);
        }
    }
}

fn should_color_auto() -> bool {
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if matches!(env::var("CLICOLOR"), Ok(value) if value == "0") {
        return false;
    }
    if matches!(env::var("TERM"), Ok(value) if value == "dumb") {
        return false;
    }
    io::stdout().is_terminal()
}

fn terminal_width() -> usize {
    termimad::crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}
