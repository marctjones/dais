//! Pure rendering: `&App -> frame` (CLIENT_REDESIGN.md §2). No state mutation here.

use dais_client::api::relative_time;
use dais_client::model::Post;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Mode, View};

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(0),    // body
        Constraint::Length(1), // footer
    ])
    .split(f.area());

    render_header(f, app, chunks[0]);
    match app.mode {
        Mode::Thread => render_thread(f, app, chunks[1]),
        _ => match app.view {
            View::Requests => render_requests(f, app, chunks[1]),
            View::Dms | View::Notifs => render_placeholder(f, app, chunks[1]),
            _ => render_posts(f, app, chunks[1]),
        },
    }
    render_footer(f, app, chunks[2]);

    // Overlays
    match app.mode {
        Mode::Composer => render_composer(f, app),
        Mode::Palette => render_palette(f, app),
        _ => {}
    }
    if app.show_help {
        render_help(f);
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = vec![Span::styled(
        " dais ",
        Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
    )];
    spans.push(Span::raw(" "));
    for v in View::all() {
        let unread = app.unread(v);
        let label = if unread > 0 {
            format!(" {}({}) ", v.title(), unread)
        } else {
            format!(" {} ", v.title())
        };
        let style = if v == app.view {
            Style::default().fg(Color::Cyan).bold().underlined()
        } else {
            Style::default().fg(Color::Gray)
        };
        spans.push(Span::styled(label, style));
    }
    let handle = app.client.config.handle.clone().unwrap_or_default();
    let line = Line::from(spans);
    let p = Paragraph::new(line);
    f.render_widget(p, area);

    // Right-aligned handle.
    if !handle.is_empty() {
        let w = handle.chars().count() as u16 + 2;
        if area.width > w {
            let r = Rect::new(area.x + area.width - w, area.y, w, 1);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("{handle} "),
                    Style::default().fg(Color::DarkGray),
                )))
                .alignment(Alignment::Right),
                r,
            );
        }
    }
}

fn post_item(p: &Post, width: usize) -> ListItem<'static> {
    let dot = if p.unread { "●" } else { "○" };
    let star = if p.is_friend { "★ " } else { "  " };
    let head = Line::from(vec![
        Span::styled(format!("{dot} "), Style::default().fg(Color::Cyan)),
        Span::styled(star.to_string(), Style::default().fg(Color::Yellow)),
        Span::styled(
            p.display_name().to_string(),
            Style::default().fg(Color::White).bold(),
        ),
        Span::raw("  "),
        Span::styled(
            p.author_handle.clone(),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} {}", p.visibility.glyph(), p.visibility.label()),
            Style::default().fg(visibility_color(p)),
        ),
        if p.encrypted {
            Span::styled("  🔒", Style::default().fg(Color::Magenta))
        } else {
            Span::raw("")
        },
        Span::raw("  "),
        Span::styled(relative_time(p.published), Style::default().fg(Color::DarkGray)),
    ]);

    let body_text = truncate(&p.content, width.saturating_sub(4).max(8));
    let body = Line::from(vec![
        Span::raw("    "),
        Span::styled(body_text, Style::default().fg(Color::Gray)),
    ]);

    let meta = Line::from(vec![
        Span::raw("       "),
        Span::styled(
            format!("↳ {}  ♥ {}  ↗ {}", p.reply_count, p.like_count, p.boost_count),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    ListItem::new(vec![head, body, meta, Line::raw("")])
}

fn visibility_color(p: &Post) -> Color {
    use dais_client::model::Visibility::*;
    match p.visibility {
        Public => Color::Blue,
        Followers => Color::Green,
        Direct => Color::Magenta,
    }
}

fn render_posts(f: &mut Frame, app: &App, area: Rect) {
    if app.posts.is_empty() {
        render_empty(f, area, "No posts yet. Run `dais init --demo` or refresh.");
        return;
    }
    let width = area.width as usize;
    let items: Vec<ListItem> = app.posts.iter().map(|p| post_item(p, width)).collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE))
        .highlight_style(Style::default().bg(Color::Rgb(30, 30, 40)))
        .highlight_symbol("");
    let mut state = ListState::default();
    state.select(Some(app.selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_requests(f: &mut Frame, app: &App, area: Rect) {
    if app.requests.is_empty() {
        render_empty(f, area, "No pending follow requests.");
        return;
    }
    let items: Vec<ListItem> = app
        .requests
        .iter()
        .map(|r| {
            let dot = if r.unread { "●" } else { "○" };
            let head = Line::from(vec![
                Span::styled(format!("{dot} "), Style::default().fg(Color::Cyan)),
                Span::styled(
                    r.name.clone().unwrap_or_else(|| r.handle.clone()),
                    Style::default().fg(Color::White).bold(),
                ),
                Span::raw("  "),
                Span::styled(r.handle.clone(), Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::styled(
                    format!("asked {} ago", relative_time(r.asked_at)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            let msg = Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    r.message.clone().unwrap_or_default(),
                    Style::default().fg(Color::Gray).italic(),
                ),
            ]);
            let meta = Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!(
                        "{} mutuals · {} · {} posts",
                        r.mutuals,
                        r.account_age_days
                            .map(|d| format!("{}y", d / 365))
                            .unwrap_or_else(|| "?".into()),
                        r.post_count.map(|c| c.to_string()).unwrap_or_else(|| "?".into()),
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("     "),
                Span::styled(
                    "[ A approve   X reject ]",
                    Style::default().fg(Color::Yellow),
                ),
            ]);
            ListItem::new(vec![head, msg, meta, Line::raw("")])
        })
        .collect();
    let list = List::new(items)
        .highlight_style(Style::default().bg(Color::Rgb(30, 30, 40)));
    let mut state = ListState::default();
    state.select(Some(app.selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_thread(f: &mut Frame, app: &App, area: Rect) {
    let Some(root) = app.selected_post() else {
        render_empty(f, area, "No post selected.");
        return;
    };
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            root.display_name().to_string(),
            Style::default().fg(Color::White).bold(),
        ),
        Span::raw("  "),
        Span::styled(root.author_handle.clone(), Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(
            format!("{} {}", root.visibility.glyph(), root.visibility.label()),
            Style::default().fg(visibility_color(root)),
        ),
    ]));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::raw(root.content.clone())));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        format!("— {} replies —", app.thread_replies.len()),
        Style::default().fg(Color::DarkGray),
    )));
    for r in &app.thread_replies {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::raw("  ↳ "),
            Span::styled(
                r.display_name().to_string(),
                Style::default().fg(Color::White).bold(),
            ),
            Span::raw("  "),
            Span::styled(
                relative_time(r.published),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        lines.push(Line::from(Span::raw(format!("    {}", r.content))));
    }
    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Thread "));
    f.render_widget(p, area);
}

fn render_placeholder(f: &mut Frame, app: &App, area: Rect) {
    render_empty(
        f,
        area,
        &format!("{} — view coming in a later phase.", app.view.title()),
    );
}

fn render_empty(f: &mut Frame, area: Rect, msg: &str) {
    let p = Paragraph::new(Line::from(Span::styled(
        msg.to_string(),
        Style::default().fg(Color::DarkGray).italic(),
    )))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::NONE));
    // vertically center-ish
    let inner = Rect::new(area.x, area.y + area.height / 3, area.width, 1);
    f.render_widget(p, inner);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let hints = match app.mode {
        Mode::Composer => "⏎ send · ^V audience · ^X encrypt · esc cancel",
        Mode::Palette => "type to filter · ⏎ run · esc close",
        Mode::Thread => "r reply · esc/q back",
        Mode::Normal => match app.view {
            View::Requests => "j/k move · A approve · X reject · g go-to · : palette · ? help · q quit",
            _ => "j/k move · ⏎ open · c compose · r reply · m read · g go-to · : palette · ? help · q quit",
        },
    };
    let left = Paragraph::new(Line::from(Span::styled(
        hints,
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(left, area);

    if !app.status.is_empty() {
        let w = (app.status.chars().count() as u16 + 2).min(area.width);
        let r = Rect::new(area.x + area.width.saturating_sub(w), area.y, w, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{} ", app.status),
                Style::default().fg(Color::Cyan),
            )))
            .alignment(Alignment::Right),
            r,
        );
    }
}

fn render_composer(f: &mut Frame, app: &App) {
    let area = centered(70, 40, f.area());
    f.render_widget(Clear, area);

    let c = &app.composer;
    let mut lines: Vec<Line> = Vec::new();
    if let Some(h) = &c.reply_handle {
        lines.push(Line::from(Span::styled(
            format!("Replying to {h}"),
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::raw(if c.text.is_empty() {
        "…".to_string()
    } else {
        c.text.clone()
    })));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        format!("{} chars left", c.remaining()),
        Style::default().fg(if c.remaining() < 0 { Color::Red } else { Color::DarkGray }),
    )));
    lines.push(Line::from(vec![
        Span::raw("Audience: "),
        Span::styled(
            format!("‹ {} {} ›", c.visibility.glyph(), c.visibility.label()),
            Style::default().fg(Color::Green).bold(),
        ),
        Span::raw("    Encrypt: "),
        Span::styled(
            if c.encrypt { "[ on ]" } else { "[ off ]" },
            Style::default().fg(if c.encrypt { Color::Magenta } else { Color::DarkGray }),
        ),
    ]));
    if c.visibility == dais_client::model::Visibility::Public {
        lines.push(Line::from(Span::styled(
            "⚠ Public posts federate to the whole fediverse.",
            Style::default().fg(Color::Yellow),
        )));
    }
    if c.encrypt {
        lines.push(Line::from(Span::styled(
            "🔒 Friends on dais read it; others see “encrypted — open in dais”.",
            Style::default().fg(Color::Magenta),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Compose ")
        .border_style(Style::default().fg(Color::Cyan));
    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn render_palette(f: &mut Frame, app: &App) {
    let area = centered(60, 50, f.area());
    f.render_widget(Clear, area);
    let items = app.palette_items();
    let mut lines: Vec<Line> = vec![Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Cyan)),
        Span::raw(app.palette.query.clone()),
    ])];
    lines.push(Line::raw(""));
    for (i, item) in items.iter().enumerate() {
        let selected = i == app.palette.selected;
        let style = if selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::Gray)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<28}", item.label), style),
            Span::styled(format!(" {}", item.hint), Style::default().fg(Color::DarkGray)),
        ]));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Command palette ")
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_help(f: &mut Frame) {
    let area = centered(60, 70, f.area());
    f.render_widget(Clear, area);
    let lines = vec![
        Line::from(Span::styled("dais — keybindings", Style::default().fg(Color::Cyan).bold())),
        Line::raw(""),
        Line::raw("Move        j/k or ↓/↑ · ^d/^u page · g g top · G bottom"),
        Line::raw("Switch view g h Home · g m Mentions · g r Requests"),
        Line::raw("            g d DMs · g n Notifications · g s Sent"),
        Line::raw("Act         ⏎ open · c compose · r reply · m mark read"),
        Line::raw("Requests    A approve · X reject"),
        Line::raw("Global      / search · : palette · ? help · q quit"),
        Line::raw(""),
        Line::from(Span::styled("press any key to close", Style::default().fg(Color::DarkGray))),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help ")
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// A centered rect, `pw`/`ph` percent of `area`.
fn centered(pw: u16, ph: u16, area: Rect) -> Rect {
    let v = Layout::vertical([
        Constraint::Percentage((100 - ph) / 2),
        Constraint::Percentage(ph),
        Constraint::Percentage((100 - ph) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - pw) / 2),
        Constraint::Percentage(pw),
        Constraint::Percentage((100 - pw) / 2),
    ])
    .split(v[1])[1]
}

fn truncate(s: &str, max: usize) -> String {
    let one_line = s.replace('\n', " ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        let kept: String = one_line.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}
