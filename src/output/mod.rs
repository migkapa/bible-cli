mod markdown;
mod spinner;

use std::env;
use std::io::{self, IsTerminal, Write};

use termimad::crossterm::style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor};

use crate::cli::ColorMode;
use crate::verses::Verse;

pub use markdown::MarkdownRenderer;
pub use spinner::ThinkingIndicator;

pub struct OutputStyle {
    pub color: bool,
    pub theme: Theme,
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
    pub fn new(mode: ColorMode) -> Self {
        let color = match mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => should_color_auto(),
        };
        Self {
            color,
            theme: Theme::claude_code(),
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
