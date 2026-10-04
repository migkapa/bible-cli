use anyhow::{bail, Result};
use ratatui::widgets::ListState;

use crate::books::BOOKS;
use crate::reference::{parse_reference, ReferenceQuery};
use crate::verses::{max_chapter, Verse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Books,
    Reader,
    /// Typing a reference into the go-to prompt.
    Goto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Quit,
    SwitchMode,
    NextItem,
    PrevItem,
    NextChapter,
    PrevChapter,
    ScrollDown,
    ScrollUp,
    PageDown,
    PageUp,
    GoToTop,
    GoToBottom,
    SelectBook,
    /// Open the go-to prompt.
    OpenGoto,
    Input(char),
    Backspace,
    Submit,
    Cancel,
    None,
}

pub struct App {
    pub mode: Mode,
    /// The mode to return to when the go-to prompt closes.
    prompt_return: Mode,
    pub books: ListState,
    pub book_names: Vec<&'static str>,
    pub current_book: String,
    pub current_chapter: u16,
    pub max_chapter: u16,
    pub verses: Vec<Verse>,
    pub chapter_verses: Vec<Verse>,
    pub scroll_offset: u16,
    pub content_height: u16,
    /// Lines the chapter takes once wrapped to the reader's width; set while
    /// rendering, so scrolling stops exactly at the last line.
    pub content_lines: u16,
    /// Verses of the current chapter to emphasize (from `--ref` or go-to).
    pub highlight: Vec<u16>,
    /// A verse to scroll to on the next render, once the wrap width is known.
    pub pending_scroll_to: Option<u16>,
    /// The go-to prompt's text.
    pub input: String,
    /// A one-off message for the status bar (e.g. a go-to error).
    pub status: Option<String>,
    /// The translation being read, shown in the title.
    pub translation: String,
    pub should_quit: bool,
}

impl App {
    pub fn new(
        verses: Vec<Verse>,
        start_book: Option<String>,
        start_ref: Option<String>,
        translation: String,
    ) -> Result<Self> {
        let book_names: Vec<&'static str> = BOOKS.iter().map(|b| b.name).collect();

        let initial_book = start_book
            .and_then(|b| crate::books::normalize_book(&b).map(String::from))
            .unwrap_or_else(|| "Genesis".to_string());
        let book_idx = book_names
            .iter()
            .position(|&name| name == initial_book)
            .unwrap_or(0);

        let mut books = ListState::default();
        books.select(Some(book_idx));

        let mut app = Self {
            mode: Mode::Reader,
            prompt_return: Mode::Reader,
            books,
            book_names,
            current_book: String::new(),
            current_chapter: 1,
            max_chapter: 1,
            verses,
            chapter_verses: Vec::new(),
            scroll_offset: 0,
            content_height: 0,
            content_lines: 0,
            highlight: Vec::new(),
            pending_scroll_to: None,
            input: String::new(),
            status: None,
            translation,
            should_quit: false,
        };
        app.load_selected_book();

        if let Some(reference) = start_ref {
            let query = parse_reference(&[reference])?;
            if let Err(message) = app.go_to(&query) {
                bail!("{}", message);
            }
        }
        Ok(app)
    }

    pub fn update(&mut self, msg: Message) {
        // Any key dismisses the previous status message.
        if msg != Message::None {
            self.status = None;
        }
        match msg {
            Message::Quit => self.should_quit = true,
            Message::SwitchMode => {
                self.mode = match self.mode {
                    Mode::Books => Mode::Reader,
                    Mode::Reader | Mode::Goto => Mode::Books,
                };
            }
            Message::NextItem => match self.mode {
                Mode::Books => self.next_book(),
                _ => self.scroll_down(1),
            },
            Message::PrevItem => match self.mode {
                Mode::Books => self.prev_book(),
                _ => self.scroll_up(1),
            },
            Message::NextChapter => self.next_chapter(),
            Message::PrevChapter => self.prev_chapter(),
            Message::ScrollDown => self.scroll_down(1),
            Message::ScrollUp => self.scroll_up(1),
            Message::PageDown => self.scroll_down(self.content_height.saturating_sub(2)),
            Message::PageUp => self.scroll_up(self.content_height.saturating_sub(2)),
            Message::GoToTop => match self.mode {
                Mode::Books => self.books.select(Some(0)),
                _ => self.scroll_offset = 0,
            },
            Message::GoToBottom => match self.mode {
                Mode::Books => self.books.select(Some(self.book_names.len() - 1)),
                _ => self.scroll_offset = self.max_scroll(),
            },
            Message::SelectBook => {
                if self.mode == Mode::Books {
                    self.load_selected_book();
                    self.mode = Mode::Reader;
                }
            }
            Message::OpenGoto => {
                if self.mode != Mode::Goto {
                    self.prompt_return = self.mode;
                    self.mode = Mode::Goto;
                    self.input.clear();
                }
            }
            Message::Input(c) => self.input.push(c),
            Message::Backspace => {
                self.input.pop();
            }
            Message::Cancel => self.mode = self.prompt_return,
            Message::Submit => self.submit_goto(),
            Message::None => {}
        }
    }

    /// Navigate to the typed reference. A bare `5` or `3:16` stays in the
    /// current book.
    fn submit_goto(&mut self) {
        let input = self.input.trim().to_string();
        self.mode = self.prompt_return;
        if input.is_empty() {
            return;
        }
        let parsed = parse_reference(std::slice::from_ref(&input)).or_else(|err| {
            if input.starts_with(|c: char| c.is_ascii_digit()) {
                parse_reference(&[format!("{} {}", self.current_book, input)]).map_err(|_| err)
            } else {
                Err(err)
            }
        });
        let outcome = match parsed {
            Ok(query) => self.go_to(&query),
            Err(err) => Err(err.to_string()),
        };
        match outcome {
            Ok(()) => self.mode = Mode::Reader,
            Err(message) => self.status = Some(message),
        }
    }

    /// Open a reference's book and first chapter, highlight the verses it names
    /// in that chapter, and scroll to the first of them.
    fn go_to(&mut self, query: &ReferenceQuery) -> std::result::Result<(), String> {
        let max = max_chapter(&self.verses, &query.book)
            .ok_or_else(|| format!("{} is not in this translation", query.book))?;
        let chapter = query.chapter.unwrap_or(1);
        if chapter > max {
            return Err(format!("{} has {} chapters", query.book, max));
        }

        // Whether the reference names verse `v` of its first chapter.
        let named = |v: u16| {
            if !query.verse_list.is_empty() {
                return query.verse_list.contains(&v);
            }
            match (query.verse, query.verse_end, query.chapter_end) {
                (Some(start), _, Some(_)) => v >= start,
                (Some(start), Some(end), None) => (start..=end).contains(&v),
                (Some(verse), None, None) => v == verse,
                (None, _, _) => false,
            }
        };
        let mut highlight: Vec<u16> = self
            .verses
            .iter()
            .filter(|v| v.book == query.book && v.chapter == chapter && named(v.verse))
            .map(|v| v.verse)
            .collect();
        highlight.sort_unstable();
        if query.verse.is_some() && highlight.is_empty() {
            return Err(format!("{} not found", query));
        }

        if let Some(idx) = self.book_names.iter().position(|&n| n == query.book) {
            self.books.select(Some(idx));
        }
        self.current_book = query.book.clone();
        self.max_chapter = max;
        self.current_chapter = chapter;
        self.load_chapter();
        self.pending_scroll_to = highlight.first().copied();
        self.highlight = highlight;
        Ok(())
    }

    fn next_book(&mut self) {
        let i = match self.books.selected() {
            Some(i) if i + 1 < self.book_names.len() => i + 1,
            _ => 0,
        };
        self.books.select(Some(i));
    }

    fn prev_book(&mut self) {
        let i = match self.books.selected() {
            Some(0) | None => self.book_names.len() - 1,
            Some(i) => i - 1,
        };
        self.books.select(Some(i));
    }

    fn load_selected_book(&mut self) {
        if let Some(idx) = self.books.selected() {
            self.current_book = self.book_names[idx].to_string();
            self.max_chapter = max_chapter(&self.verses, &self.current_book).unwrap_or(1);
            self.current_chapter = 1;
            self.load_chapter();
        }
    }

    fn next_chapter(&mut self) {
        if self.current_chapter < self.max_chapter {
            self.current_chapter += 1;
            self.load_chapter();
        }
    }

    fn prev_chapter(&mut self) {
        if self.current_chapter > 1 {
            self.current_chapter -= 1;
            self.load_chapter();
        }
    }

    /// Load the current chapter, scrolled to the top with no highlight.
    fn load_chapter(&mut self) {
        self.chapter_verses = self
            .verses
            .iter()
            .filter(|v| v.book == self.current_book && v.chapter == self.current_chapter)
            .cloned()
            .collect();
        self.chapter_verses.sort_by_key(|v| v.verse);
        self.scroll_offset = 0;
        self.highlight.clear();
        self.pending_scroll_to = None;
    }

    fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(amount)
            .min(self.max_scroll());
    }

    fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn max_scroll(&self) -> u16 {
        self.content_lines.saturating_sub(self.content_height)
    }

    pub fn set_content_height(&mut self, height: u16) {
        self.content_height = height;
    }
}
