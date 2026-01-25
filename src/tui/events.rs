use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, Message, Mode};

pub fn handle_events(app: &mut App) -> Result<bool> {
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            let msg = key_to_message(key, app.mode);
            app.update(msg);
        }
    }
    Ok(app.should_quit)
}

fn key_to_message(key: KeyEvent, mode: Mode) -> Message {
    // Global keybindings
    match key.code {
        KeyCode::Char('q') => return Message::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Message::Quit
        }
        KeyCode::Tab => return Message::SwitchMode,
        KeyCode::Esc => return Message::SwitchMode,
        _ => {}
    }

    // Mode-specific keybindings
    match mode {
        Mode::Books => match key.code {
            KeyCode::Char('j') | KeyCode::Down => Message::NextItem,
            KeyCode::Char('k') | KeyCode::Up => Message::PrevItem,
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => Message::SelectBook,
            KeyCode::Char('g') => Message::GoToTop,
            KeyCode::Char('G') => Message::GoToBottom,
            _ => Message::None,
        },
        Mode::Reader => match key.code {
            KeyCode::Char('j') | KeyCode::Down => Message::ScrollDown,
            KeyCode::Char('k') | KeyCode::Up => Message::ScrollUp,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Message::PageDown
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => Message::PageUp,
            KeyCode::PageDown | KeyCode::Char(' ') => Message::PageDown,
            KeyCode::PageUp => Message::PageUp,
            KeyCode::Char('n') | KeyCode::Right => Message::NextChapter,
            KeyCode::Char('p') | KeyCode::Left => Message::PrevChapter,
            KeyCode::Char('g') => Message::GoToTop,
            KeyCode::Char('G') => Message::GoToBottom,
            KeyCode::Char('h') => Message::SwitchMode,
            _ => Message::None,
        },
    }
}
