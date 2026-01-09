use std::env;
use std::io::{self, IsTerminal};

use crate::cli::ColorMode;
use crate::verses::Verse;

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_REF: &str = "\x1b[36m";
const ANSI_VERSE: &str = "\x1b[32m";
const ANSI_MARK: &str = "\x1b[33m";

pub struct OutputStyle {
    color: bool,
}

impl OutputStyle {
    pub fn new(mode: ColorMode) -> Self {
        let color = match mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => should_color_auto(),
        };
        Self { color }
    }

    pub fn verse_line(&self, verse: &Verse) -> String {
        let reference = format!("{} {}:{}", verse.book, verse.chapter, verse.verse);
        let reference = self.paint(ANSI_REF, &reference);
        let text = self.paint(ANSI_VERSE, &verse.text);
        format!("{} {}", reference, text)
    }

    pub fn marked_verse_line(&self, marker: &str, verse: &Verse) -> String {
        let marker = if marker == "*" {
            self.paint(ANSI_MARK, marker)
        } else {
            marker.to_string()
        };
        format!("{} {}", marker, self.verse_line(verse))
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("{}{}{}", code, text, ANSI_RESET)
        } else {
            text.to_string()
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
