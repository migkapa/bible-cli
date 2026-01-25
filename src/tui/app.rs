use ratatui::widgets::ListState;

use crate::books::BOOKS;
use crate::verses::Verse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Books,
    Reader,
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
    None,
}

pub struct App {
    pub mode: Mode,
    pub books: ListState,
    pub book_names: Vec<&'static str>,
    pub current_book: String,
    pub current_chapter: u16,
    pub max_chapter: u16,
    pub verses: Vec<Verse>,
    pub chapter_verses: Vec<Verse>,
    pub scroll_offset: u16,
    pub content_height: u16,
    pub should_quit: bool,
}

impl App {
    pub fn new(verses: Vec<Verse>, start_book: Option<String>, _start_ref: Option<String>) -> Self {
        let book_names: Vec<&'static str> = BOOKS.iter().map(|b| b.name).collect();

        // Determine starting book
        let initial_book = start_book
            .and_then(|b| crate::books::normalize_book(&b).map(String::from))
            .unwrap_or_else(|| "Genesis".to_string());

        // Find the book index
        let book_idx = book_names
            .iter()
            .position(|&name| name == initial_book)
            .unwrap_or(0);

        let mut books = ListState::default();
        books.select(Some(book_idx));

        let current_book = book_names[book_idx].to_string();
        let max_chapter = crate::verses::max_chapter(&verses, &current_book).unwrap_or(1);

        let chapter_verses: Vec<Verse> = verses
            .iter()
            .filter(|v| v.book == current_book && v.chapter == 1)
            .cloned()
            .collect();

        Self {
            mode: Mode::Reader,
            books,
            book_names,
            current_book,
            current_chapter: 1,
            max_chapter,
            verses,
            chapter_verses,
            scroll_offset: 0,
            content_height: 0,
            should_quit: false,
        }
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::Quit => self.should_quit = true,
            Message::SwitchMode => {
                self.mode = match self.mode {
                    Mode::Books => Mode::Reader,
                    Mode::Reader => Mode::Books,
                };
            }
            Message::NextItem => match self.mode {
                Mode::Books => self.next_book(),
                Mode::Reader => self.scroll_down(1),
            },
            Message::PrevItem => match self.mode {
                Mode::Books => self.prev_book(),
                Mode::Reader => self.scroll_up(1),
            },
            Message::NextChapter => self.next_chapter(),
            Message::PrevChapter => self.prev_chapter(),
            Message::ScrollDown => self.scroll_down(1),
            Message::ScrollUp => self.scroll_up(1),
            Message::PageDown => self.scroll_down(self.content_height.saturating_sub(2)),
            Message::PageUp => self.scroll_up(self.content_height.saturating_sub(2)),
            Message::GoToTop => self.scroll_offset = 0,
            Message::GoToBottom => self.scroll_to_bottom(),
            Message::SelectBook => {
                if self.mode == Mode::Books {
                    self.load_selected_book();
                    self.mode = Mode::Reader;
                }
            }
            Message::None => {}
        }
    }

    fn next_book(&mut self) {
        let i = match self.books.selected() {
            Some(i) => {
                if i >= self.book_names.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.books.select(Some(i));
    }

    fn prev_book(&mut self) {
        let i = match self.books.selected() {
            Some(i) => {
                if i == 0 {
                    self.book_names.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.books.select(Some(i));
    }

    fn load_selected_book(&mut self) {
        if let Some(idx) = self.books.selected() {
            self.current_book = self.book_names[idx].to_string();
            self.max_chapter =
                crate::verses::max_chapter(&self.verses, &self.current_book).unwrap_or(1);
            self.current_chapter = 1;
            self.scroll_offset = 0;
            self.load_chapter();
        }
    }

    fn next_chapter(&mut self) {
        if self.current_chapter < self.max_chapter {
            self.current_chapter += 1;
            self.scroll_offset = 0;
            self.load_chapter();
        }
    }

    fn prev_chapter(&mut self) {
        if self.current_chapter > 1 {
            self.current_chapter -= 1;
            self.scroll_offset = 0;
            self.load_chapter();
        }
    }

    fn load_chapter(&mut self) {
        self.chapter_verses = self
            .verses
            .iter()
            .filter(|v| v.book == self.current_book && v.chapter == self.current_chapter)
            .cloned()
            .collect();
        self.chapter_verses.sort_by_key(|v| v.verse);
    }

    fn scroll_down(&mut self, amount: u16) {
        let max_scroll = self.calculate_max_scroll();
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.calculate_max_scroll();
    }

    fn calculate_max_scroll(&self) -> u16 {
        // Estimate content height based on verse count and wrapping
        // This is a rough estimate; actual content height depends on terminal width
        let estimated_lines: u16 = self
            .chapter_verses
            .iter()
            .map(|v| {
                // Assume ~80 chars per line, verse number prefix + text
                let text_len = v.text.len() + 8;
                ((text_len / 60) + 1) as u16
            })
            .sum();
        estimated_lines.saturating_sub(self.content_height)
    }

    pub fn set_content_height(&mut self, height: u16) {
        self.content_height = height;
    }
}
