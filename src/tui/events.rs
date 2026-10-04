use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};

use super::app::{App, Message, Mode};

/// Lines scrolled per mouse-wheel notch.
const WHEEL_LINES: usize = 3;

pub fn handle_events(app: &mut App) -> Result<bool> {
    if event::poll(Duration::from_millis(100))? {
        match event::read()? {
            // Only presses: Windows also reports releases, which would make
            // every key act twice.
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let msg = key_to_message(key, app.mode);
                app.update(msg);
            }
            Event::Mouse(mouse) => {
                let msg = match mouse.kind {
                    MouseEventKind::ScrollDown => Message::ScrollDown,
                    MouseEventKind::ScrollUp => Message::ScrollUp,
                    _ => Message::None,
                };
                if msg != Message::None && app.mode != Mode::Goto {
                    for _ in 0..WHEEL_LINES {
                        app.update(msg);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(app.should_quit)
}

fn key_to_message(key: KeyEvent, mode: Mode) -> Message {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && key.code == KeyCode::Char('c') {
        return Message::Quit;
    }

    // The go-to prompt takes every other key as text.
    if mode == Mode::Goto {
        return match key.code {
            KeyCode::Enter => Message::Submit,
            KeyCode::Esc => Message::Cancel,
            KeyCode::Backspace => Message::Backspace,
            KeyCode::Char(c) => Message::Input(c),
            _ => Message::None,
        };
    }

    // Global keybindings
    match key.code {
        KeyCode::Char('q') => return Message::Quit,
        KeyCode::Tab | KeyCode::Esc => return Message::SwitchMode,
        KeyCode::Char(':') => return Message::OpenGoto,
        _ => {}
    }

    // Mode-specific keybindings
    match mode {
        Mode::Books => match key.code {
            KeyCode::Char('j') | KeyCode::Down => Message::NextItem,
            KeyCode::Char('k') | KeyCode::Up => Message::PrevItem,
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => Message::SelectBook,
            KeyCode::Char('g') | KeyCode::Home => Message::GoToTop,
            KeyCode::Char('G') | KeyCode::End => Message::GoToBottom,
            _ => Message::None,
        },
        Mode::Reader | Mode::Goto => match key.code {
            KeyCode::Char('j') | KeyCode::Down => Message::ScrollDown,
            KeyCode::Char('k') | KeyCode::Up => Message::ScrollUp,
            KeyCode::Char('d') if ctrl => Message::PageDown,
            KeyCode::Char('u') if ctrl => Message::PageUp,
            KeyCode::PageDown | KeyCode::Char(' ') => Message::PageDown,
            KeyCode::PageUp => Message::PageUp,
            KeyCode::Char('n') | KeyCode::Right => Message::NextChapter,
            KeyCode::Char('p') | KeyCode::Left => Message::PrevChapter,
            KeyCode::Char('g') | KeyCode::Home => Message::GoToTop,
            KeyCode::Char('G') | KeyCode::End => Message::GoToBottom,
            KeyCode::Char('h') => Message::SwitchMode,
            _ => Message::None,
        },
    }
}
