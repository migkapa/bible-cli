mod app;
mod events;
mod ui;

use std::io::stdout;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

pub use app::App;
use events::handle_events;
use ui::render;

use crate::verses::Verse;

pub fn run(
    verses: Vec<Verse>,
    start_book: Option<String>,
    start_ref: Option<String>,
    translation: String,
) -> Result<()> {
    // Build the app first, so a bad --ref is reported on a normal terminal.
    let mut app = App::new(verses, start_book, start_ref, translation)?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Restore the terminal even if rendering panics, so the shell stays usable.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        default_hook(info);
    }));

    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| render(frame, app))?;

        if handle_events(app)? {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;

    use super::app::{App, Message, Mode};
    use super::*;

    /// A corpus with one long chapter (Psalms 119, 176 verses of 20 words)
    /// plus John 3, so wrapped height far exceeds any terminal.
    fn corpus() -> Vec<Verse> {
        let mut verses = Vec::new();
        for verse in 1..=176 {
            verses.push(Verse {
                book: "Psalms".to_string(),
                chapter: 119,
                verse,
                text: format!("verse {} {}", verse, "word ".repeat(20).trim_end()),
            });
        }
        for verse in 1..=36 {
            verses.push(Verse {
                book: "John".to_string(),
                chapter: 3,
                verse,
                text: format!("John three {}", verse),
            });
        }
        verses
    }

    fn draw(terminal: &mut Terminal<TestBackend>, app: &mut App) -> String {
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn app_at(start_ref: Option<&str>) -> App {
        App::new(
            corpus(),
            None,
            start_ref.map(str::to_string),
            "kjv".to_string(),
        )
        .unwrap()
    }

    #[test]
    fn go_to_bottom_reaches_the_last_verse_on_a_narrow_terminal() {
        let mut terminal = Terminal::new(TestBackend::new(64, 16)).unwrap();
        let mut app = app_at(Some("Psalm 119"));
        draw(&mut terminal, &mut app);
        app.update(Message::GoToBottom);
        let screen = draw(&mut terminal, &mut app);
        assert!(screen.contains("verse 176"), "{}", screen);

        // Scrolling further does not run past the end.
        let before = app.scroll_offset;
        app.update(Message::PageDown);
        draw(&mut terminal, &mut app);
        assert_eq!(app.scroll_offset, before);
    }

    #[test]
    fn start_ref_opens_scrolled_to_the_verse() {
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        let mut app = app_at(Some("Psalm 119:100"));
        assert_eq!(
            (app.current_book.as_str(), app.current_chapter),
            ("Psalms", 119)
        );
        assert_eq!(app.highlight, [100]);
        let screen = draw(&mut terminal, &mut app);
        assert!(screen.contains("verse 100"), "{}", screen);
        assert!(!screen.contains("verse 1 word"), "{}", screen);
        assert!(screen.contains("Psalms 119 · KJV"));
    }

    #[test]
    fn bad_start_ref_is_an_error() {
        assert!(App::new(corpus(), None, Some("John 3:99".into()), "kjv".into()).is_err());
        assert!(App::new(corpus(), None, Some("Hezekiah 1".into()), "kjv".into()).is_err());
    }

    #[test]
    fn go_to_prompt_navigates_and_reports_errors() {
        let mut app = app_at(None);
        let type_in = |app: &mut App, text: &str| {
            app.update(Message::OpenGoto);
            assert_eq!(app.mode, Mode::Goto);
            for c in text.chars() {
                app.update(Message::Input(c));
            }
            app.update(Message::Submit);
        };

        type_in(&mut app, "jn 3:16-17");
        assert_eq!(
            (app.current_book.as_str(), app.current_chapter),
            ("John", 3)
        );
        assert_eq!(app.highlight, [16, 17]);
        assert_eq!(app.mode, Mode::Reader);

        // A bare verse reference stays in the current book.
        type_in(&mut app, "3:5");
        assert_eq!(app.highlight, [5]);

        type_in(&mut app, "Hezekiah 1");
        assert!(app.status.is_some());
        assert_eq!(app.current_book, "John");

        // Esc cancels without moving; q is typed, not quit.
        app.update(Message::OpenGoto);
        app.update(Message::Input('q'));
        app.update(Message::Cancel);
        assert!(!app.should_quit);
        assert_eq!(app.mode, Mode::Reader);
    }

    #[test]
    fn g_and_shift_g_move_the_book_list() {
        let mut app = app_at(None);
        app.update(Message::SwitchMode);
        assert_eq!(app.mode, Mode::Books);
        app.update(Message::GoToBottom);
        assert_eq!(app.books.selected(), Some(app.book_names.len() - 1));
        app.update(Message::GoToTop);
        assert_eq!(app.books.selected(), Some(0));
    }
}
