//! Drawing code for the ratatui interface.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::core::format::Format;
use crate::util::{human, paths};

use super::app::{App, Entry, Focus, InputPrompt, JobState, Overlay, queue_summary};
use super::preview::display_path;
use super::theme;

/// Render the whole interface.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(theme::base()), area);

    let queue_height = if app.queue.is_empty() {
        5
    } else {
        (app.queue.len() as u16 + 2).clamp(5, 12)
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(queue_height),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(frame, rows[0]);
    draw_body(frame, rows[1], app);
    draw_queue(frame, rows[2], app);
    draw_footer(frame, rows[3], app);

    match &app.overlay {
        Overlay::None => {}
        Overlay::Help => draw_help(frame, area),
        Overlay::Formats { formats, index } => draw_formats(frame, area, formats, *index),
        Overlay::Input(prompt) => draw_input(frame, area, prompt),
    }
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);

    let left = Line::from(vec![
        Span::styled(
            " ◆ SORCERER",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  all-in-one file & document converter",
            Style::default().fg(theme::MUTED),
        ),
    ]);
    frame.render_widget(Paragraph::new(left).style(theme::base()), columns[0]);

    let right = Line::from(Span::styled(
        format!("crafted by Panz-08 · v{} ", crate::VERSION),
        Style::default().fg(theme::DIM),
    ));
    frame.render_widget(
        Paragraph::new(right)
            .alignment(Alignment::Right)
            .style(theme::base()),
        columns[1],
    );
}

fn draw_body(frame: &mut Frame, area: Rect, app: &mut App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(area);

    draw_browser(frame, columns[0], app);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(columns[1]);
    app.preview.draw(frame, right[0]);
    draw_settings(frame, right[1], app);
}

fn draw_browser(frame: &mut Frame, area: Rect, app: &App) {
    let active = app.focus == Focus::Browser;
    let mut title = format!(" Files · {} ", paths::pretty(&app.cwd));
    if !app.filter.is_empty() {
        title.push_str(&format!("· filter: {} ", app.filter));
    } else if !app.marked.is_empty() {
        title.push_str(&format!("· {} marked ", app.marked.len()));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border(active))
        .style(theme::panel())
        .title(Span::styled(title, theme::title(active)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.entries.is_empty() {
        let message = if app.filter.is_empty() {
            "  This folder is empty."
        } else {
            "  No files match the filter."
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                message,
                Style::default().fg(theme::MUTED),
            )))
            .style(theme::panel()),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .entries
        .iter()
        .map(|entry| ListItem::new(entry_line(entry, app.marked.contains(&entry.path))))
        .collect();
    let mut state = ListState::default()
        .with_selected(Some(app.selected.min(app.entries.len().saturating_sub(1))));
    let list = List::new(items)
        .style(theme::panel())
        .highlight_style(if active {
            theme::highlight()
        } else {
            theme::selected(false)
        });
    frame.render_stateful_widget(list, inner, &mut state);
}

fn entry_line(entry: &Entry, marked: bool) -> Line<'static> {
    let mut spans = Vec::new();
    spans.push(Span::styled(
        if marked { "●" } else { " " },
        Style::default().fg(theme::GREEN),
    ));
    spans.push(Span::raw(" "));

    let icon = if entry.parent {
        "↑"
    } else {
        theme::icon(entry.format, entry.is_dir)
    };
    spans.push(Span::styled(
        icon,
        Style::default().fg(theme::icon_color(entry.format)),
    ));
    spans.push(Span::raw(" "));

    let name = if entry.is_dir && !entry.parent {
        format!("{}/", entry.name)
    } else {
        entry.name.clone()
    };
    let name_style = if entry.is_dir {
        Style::default()
            .fg(theme::CYAN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT)
    };
    spans.push(Span::styled(name, name_style));

    if !entry.is_dir {
        if let Some(format) = entry.format {
            spans.push(Span::styled(
                format!("  {}", format.label()),
                Style::default().fg(theme::DIM),
            ));
        }
        spans.push(Span::styled(
            format!("  {}", human::bytes(entry.size)),
            Style::default().fg(theme::DIM),
        ));
    }

    Line::from(spans)
}

fn draw_settings(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border(false))
        .style(theme::panel())
        .title(Span::styled(" Settings ", theme::title(false)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width.saturating_sub(14) as usize;
    let mut lines: Vec<Line> = Vec::new();
    setting(
        &mut lines,
        "Target",
        format!("{}  .{}", app.target.label(), app.target.extension()),
        theme::ACCENT,
    );
    setting(
        &mut lines,
        "Output",
        human::truncate(&display_path(app.output_dir.as_deref()), width),
        theme::TEXT,
    );
    setting(
        &mut lines,
        "Quality",
        if app.target.supports_quality() {
            format!("{}", app.quality)
        } else {
            format!("{} (not used)", app.quality)
        },
        theme::TEXT,
    );
    setting(&mut lines, "DPI", app.dpi.to_string(), theme::TEXT);
    setting(
        &mut lines,
        "Overwrite",
        if app.overwrite { "on" } else { "off" }.to_string(),
        if app.overwrite {
            theme::YELLOW
        } else {
            theme::MUTED
        },
    );
    setting(
        &mut lines,
        "Grayscale",
        if app.grayscale { "on" } else { "off" }.to_string(),
        if app.grayscale {
            theme::YELLOW
        } else {
            theme::MUTED
        },
    );
    setting(
        &mut lines,
        "Marks",
        format!("{} file(s)", app.marked.len()),
        theme::GREEN,
    );
    setting(&mut lines, "Queue", queue_summary(app), theme::CYAN);

    if let Some(info) = &app.preview.info {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(" {}", info.format),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )));
        let summary = info.summary();
        if !summary.is_empty() {
            setting(
                &mut lines,
                "Info",
                human::truncate(&summary, width),
                theme::TEXT,
            );
        }
        setting(&mut lines, "Size", human::bytes(info.size), theme::MUTED);
        if let Some(modified) = info.modified {
            setting(
                &mut lines,
                "Modified",
                human::unix_datetime(modified),
                theme::MUTED,
            );
        }
    } else if let Some(entry) = app.entries.get(app.selected)
        && !entry.is_dir
    {
        lines.push(Line::from(""));
        setting(
            &mut lines,
            "File",
            human::truncate(&entry.name, width),
            theme::TEXT,
        );
        setting(
            &mut lines,
            "Kind",
            entry
                .format
                .map(|format| format.label().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            theme::MUTED,
        );
        setting(&mut lines, "Size", human::bytes(entry.size), theme::MUTED);
    }

    frame.render_widget(Paragraph::new(lines).style(theme::panel()), inner);
}

fn setting(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    value: String,
    color: ratatui::style::Color,
) {
    lines.push(Line::from(vec![
        Span::styled(format!(" {label:<11}"), Style::default().fg(theme::MUTED)),
        Span::styled(value, Style::default().fg(color)),
    ]));
}

fn draw_queue(frame: &mut Frame, area: Rect, app: &App) {
    let active = app.focus == Focus::Queue;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border(active))
        .style(theme::panel())
        .title(Span::styled(
            format!(" Queue · {} ", queue_summary(app)),
            theme::title(active),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.queue.is_empty() {
        let lines = vec![
            Line::from(Span::styled(
                "  No jobs queued yet.",
                Style::default().fg(theme::MUTED),
            )),
            Line::from(Span::styled(
                "  Mark files with Space, then press a to add and c to convert.",
                Style::default().fg(theme::DIM),
            )),
        ];
        frame.render_widget(Paragraph::new(lines).style(theme::panel()), inner);
        return;
    }

    let items: Vec<ListItem> = app
        .queue
        .iter()
        .map(|item| ListItem::new(queue_line(item)))
        .collect();
    let mut state = ListState::default().with_selected(Some(
        app.queue_selected.min(app.queue.len().saturating_sub(1)),
    ));
    let list = List::new(items)
        .style(theme::panel())
        .highlight_style(if active {
            theme::highlight()
        } else {
            theme::selected(false)
        });
    frame.render_stateful_widget(list, inner, &mut state);
}

fn queue_line(item: &super::app::QueueItem) -> Line<'static> {
    let (symbol, color) = match item.state {
        JobState::Pending => ("○", theme::MUTED),
        JobState::Running => ("◐", theme::YELLOW),
        JobState::Done => ("✔", theme::GREEN),
        JobState::Failed => ("✖", theme::RED),
    };
    let name = item
        .input
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| item.input.display().to_string());
    let mut spans = vec![
        Span::styled(format!(" {symbol} "), Style::default().fg(color)),
        Span::styled(
            format!("{:<28}", human::truncate(&name, 28)),
            Style::default().fg(if item.state == JobState::Failed {
                theme::RED
            } else {
                theme::TEXT
            }),
        ),
        Span::styled(" → ", Style::default().fg(theme::DIM)),
        Span::styled(
            format!("{:<4}", item.target.extension().to_uppercase()),
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            theme::mini_bar(item.progress, 10),
            Style::default().fg(color),
        ),
        Span::styled(
            format!(
                " {:>3}% ",
                (item.progress.clamp(0.0, 1.0) * 100.0).round() as u32
            ),
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(
            human::truncate(&item.stage, 24),
            Style::default().fg(theme::DIM),
        ),
    ];
    if let Some(error) = &item.error {
        spans.push(Span::styled(
            format!(" · {}", human::truncate(error, 48)),
            Style::default().fg(theme::RED),
        ));
    }
    Line::from(spans)
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let keys = Line::from(vec![
        key("q"),
        text(" quit  "),
        key("Tab"),
        text(" focus  "),
        key("↑↓"),
        text(" move  "),
        key("Space"),
        text(" mark  "),
        key("a"),
        text(" add  "),
        key("c"),
        text(" convert  "),
        key("t"),
        text(" target  "),
        key("o"),
        text(" output  "),
        key("?"),
        text(" help"),
    ]);
    let status_color = if app.status_error {
        theme::RED
    } else {
        theme::MUTED
    };
    let status = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(app.status.clone(), Style::default().fg(status_color)),
    ]);
    frame.render_widget(
        Paragraph::new(vec![keys, status]).style(theme::base()),
        area,
    );
}

fn key(label: &str) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default()
            .fg(theme::ACCENT)
            .bg(theme::PANEL_ALT)
            .add_modifier(Modifier::BOLD),
    )
}

fn text(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), Style::default().fg(theme::DIM))
}

// ---------------------------------------------------------------------------
// Overlays
// ---------------------------------------------------------------------------

fn modal_area(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    )
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let modal = modal_area(area, 74, 29);
    frame.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .style(theme::panel())
        .title(Span::styled(
            " Help · keyboard shortcuts ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let shortcuts = [
        ("q / Ctrl+C", "quit"),
        ("Tab", "switch between the browser and the queue"),
        ("↑ ↓ / k j", "move the selection"),
        ("PgUp / PgDn", "jump ten rows"),
        ("Home / End", "first / last row"),
        ("Enter", "open a folder, preview a file or job output"),
        ("← / Backspace", "go to the parent folder"),
        ("Space", "mark or unmark a file"),
        ("a", "add marked files (or the current folder) to the queue"),
        ("c", "convert everything pending in the queue"),
        ("d", "remove the selected queue item"),
        ("x", "clear marks, or clear the queue when it is focused"),
        ("t", "open the target format picker"),
        ("o", "choose the output folder"),
        ("f", "filter file names"),
        ("g", "go to a folder"),
        ("h", "show / hide dotfiles"),
        ("r", "refresh the browser"),
        ("p / i", "edit quality / DPI"),
        ("w / n", "toggle overwrite / grayscale"),
        ("[ ] , .", "quality −5/+5 · DPI −25/+25"),
        ("1 2 3 4", "quick targets: PNG / JPEG / WebP / PDF"),
        ("?", "show this help"),
    ];

    let mut lines = vec![Line::from("")];
    for (keys, description) in shortcuts {
        lines.push(Line::from(vec![
            Span::styled(format!("  {keys:<14}"), Style::default().fg(theme::ACCENT)),
            Span::styled(description.to_string(), Style::default().fg(theme::TEXT)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Press any key to close",
        Style::default().fg(theme::DIM),
    )));
    frame.render_widget(Paragraph::new(lines).style(theme::panel()), inner);
}

fn draw_formats(frame: &mut Frame, area: Rect, formats: &[Format], index: usize) {
    let modal = modal_area(area, 54, 20);
    frame.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .style(theme::panel())
        .title(Span::styled(
            " Choose target format · Enter to apply ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    if formats.is_empty() {
        frame.render_widget(
            Paragraph::new("No target formats available.")
                .style(theme::panel())
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = formats
        .iter()
        .map(|format| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<9}", format.label()),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(".{:<7}", format.extension()),
                    Style::default().fg(theme::CYAN),
                ),
                Span::styled(
                    format.category().label().to_string(),
                    Style::default().fg(theme::MUTED),
                ),
                Span::styled(
                    format!("  {}", format.description()),
                    Style::default().fg(theme::DIM),
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(index.min(formats.len() - 1)));
    let list = List::new(items)
        .style(theme::panel())
        .highlight_style(theme::highlight());
    frame.render_stateful_widget(list, inner, &mut state);
}

fn draw_input(frame: &mut Frame, area: Rect, prompt: &InputPrompt) {
    let modal = modal_area(area, 70, 8);
    frame.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .style(theme::panel())
        .title(Span::styled(
            format!(" {} ", prompt.title),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let byte = prompt
        .value
        .char_indices()
        .nth(prompt.cursor)
        .map(|(byte, _)| byte)
        .unwrap_or(prompt.value.len());
    let (before, after) = prompt.value.split_at(byte);
    let value_line = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(before.to_string(), Style::default().fg(theme::TEXT)),
        Span::styled(
            "▏",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
        Span::styled(after.to_string(), Style::default().fg(theme::TEXT)),
    ]);
    let hint = Line::from(Span::styled(
        format!(" {}", prompt.hint),
        Style::default().fg(theme::MUTED),
    ));
    let apply = Line::from(Span::styled(
        " Enter to apply · Esc to cancel",
        Style::default().fg(theme::DIM),
    ));
    let lines = vec![value_line, Line::from(""), hint, Line::from(""), apply];
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme::panel())
            .wrap(Wrap { trim: false }),
        inner,
    );
}
