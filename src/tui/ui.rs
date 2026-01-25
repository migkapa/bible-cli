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

fn render_book_list(frame: &mut Frame, app: &App, area: Rect) {
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

    frame.render_stateful_widget(list, area, &mut app.books.clone());
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

fn render_verses(frame: &mut Frame, app: &App, area: Rect) {
    let title = format!(" {} {} ", app.current_book, app.current_chapter);

    let border_style = if app.mode == Mode::Reader {
        Style::default().fg(Color::Blue)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut lines: Vec<Line> = Vec::new();

    for verse in &app.chapter_verses {
        let verse_num = format!("{:>3} ", verse.verse);
        lines.push(Line::from(vec![
            Span::styled(verse_num, Style::default().fg(Color::DarkGray)),
            Span::raw(&verse.text),
        ]));
        // Add empty line between verses for readability
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));

    frame.render_widget(paragraph, area);
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mode_indicator = match app.mode {
        Mode::Books => "[BOOKS]",
        Mode::Reader => "[READER]",
    };

    let keybindings = match app.mode {
        Mode::Books => "j/k:nav  Enter:select  Tab:switch  q:quit",
        Mode::Reader => "j/k:scroll  n/p:chapter  Tab:books  g/G:top/bottom  q:quit",
    };

    let line = Line::from(vec![
        Span::styled(
            mode_indicator,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(keybindings, Style::default().fg(Color::DarkGray)),
    ]);

    let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Black));

    frame.render_widget(paragraph, area);
}
