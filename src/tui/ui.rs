use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::app::{App, Mode};

pub fn render(frame: &mut Frame, app: &mut App) {
    let size = frame.area();

    // Main layout: split horizontally
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20), // Book list
            Constraint::Min(40),    // Content
        ])
        .split(size);

    // Left panel: Books and chapters
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),   // Book list
            Constraint::Length(3), // Chapter indicator
        ])
        .split(main_chunks[0]);

    // Right panel: Content and status bar
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // Verse content
            Constraint::Length(1), // Status bar
        ])
        .split(main_chunks[1]);

    // Update content height for scroll calculation
    app.set_content_height(right_chunks[0].height.saturating_sub(2));

    render_book_list(frame, app, left_chunks[0]);
    render_chapter_indicator(frame, app, left_chunks[1]);
    render_verses(frame, app, right_chunks[0]);
    render_status_bar(frame, app, right_chunks[1]);
}

fn render_book_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let highlight_style = if app.mode == Mode::Books {
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    };

    let items: Vec<ListItem> = app
        .book_names
        .iter()
        .map(|name| {
            let style = if *name == app.current_book {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            ListItem::new(Span::styled(*name, style))
        })
        .collect();

    let border_style = if app.mode == Mode::Books {
        Style::default().fg(Color::Blue)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Books ")
                .border_style(border_style),
        )
        .highlight_style(highlight_style)
        .highlight_symbol("> ");

    // The real list state, so the list keeps its scroll position between frames.
    frame.render_stateful_widget(list, area, &mut app.books);
}

fn render_chapter_indicator(frame: &mut Frame, app: &App, area: Rect) {
    let chapter_text = format!("Ch {}/{}", app.current_chapter, app.max_chapter);

    let nav_hint = if app.max_chapter > 1 { " [n/p]" } else { "" };

    let paragraph = Paragraph::new(Line::from(vec![
        Span::styled(&chapter_text, Style::default().fg(Color::Cyan)),
        Span::styled(nav_hint, Style::default().fg(Color::DarkGray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(paragraph, area);
}

/// One verse as a line: a dim verse number, then the text. Highlighted verses
/// get a yellow number and bold text.
fn verse_line(number: u16, text: &str, highlighted: bool) -> Line<'_> {
    let (number_style, text_style) = if highlighted {
        (
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            Style::default().add_modifier(Modifier::BOLD),
        )
    } else {
        (Style::default().fg(Color::DarkGray), Style::default())
    };
    Line::from(vec![
        Span::styled(format!("{:>3} ", number), number_style),
        Span::styled(text, text_style),
    ])
}

fn render_verses(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = format!(
        " {} {} · {} ",
        app.current_book,
        app.current_chapter,
        app.translation.to_uppercase()
    );
    let border_style = if app.mode == Mode::Books {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Blue)
    };
    let inner_width = area.width.saturating_sub(2);

    let mut lines: Vec<Line> = Vec::new();
    let mut target_line: Option<usize> = None;
    for verse in &app.chapter_verses {
        // Where the pending scroll target starts: the wrapped height of
        // everything above it, measured with the renderer's own wrapping.
        if app.pending_scroll_to == Some(verse.verse) {
            let above = Paragraph::new(lines.clone()).wrap(Wrap { trim: false });
            target_line = Some(if lines.is_empty() {
                0
            } else {
                above.line_count(inner_width)
            });
        }
        let highlighted = app.highlight.contains(&verse.verse);
        lines.push(verse_line(verse.verse, &verse.text, highlighted));
        // Blank line between verses for readability.
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    app.content_lines = paragraph.line_count(inner_width).min(u16::MAX as usize) as u16;
    if app.pending_scroll_to.take().is_some() {
        let target = target_line.unwrap_or(0).min(u16::MAX as usize) as u16;
        app.scroll_offset = target.min(app.max_scroll());
    }
    app.scroll_offset = app.scroll_offset.min(app.max_scroll());

    let paragraph = paragraph
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .scroll((app.scroll_offset, 0));

    frame.render_widget(paragraph, area);
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let line = if app.mode == Mode::Goto {
        Line::from(vec![
            Span::styled(
                "Go to: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(app.input.as_str()),
            Span::styled("█", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "  Enter:go  Esc:cancel",
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else {
        let mode_indicator = match app.mode {
            Mode::Books => "[BOOKS]",
            _ => "[READER]",
        };
        let hint = match (&app.status, app.mode) {
            (Some(message), _) => {
                Span::styled(message.as_str(), Style::default().fg(Color::Yellow))
            }
            (None, Mode::Books) => Span::styled(
                "j/k:nav  g/G:first/last  Enter:select  ::go to  Tab:switch  q:quit",
                Style::default().fg(Color::DarkGray),
            ),
            (None, _) => Span::styled(
                "j/k:scroll  n/p:chapter  ::go to  g/G:top/bottom  Tab:books  q:quit",
                Style::default().fg(Color::DarkGray),
            ),
        };
        Line::from(vec![
            Span::styled(
                mode_indicator,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            hint,
        ])
    };

    let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Black));

    frame.render_widget(paragraph, area);
}
