//! Colours, icons and styles for the terminal interface.

use ratatui::style::{Color, Modifier, Style};

use crate::core::format::Format;

pub const BG: Color = Color::Rgb(9, 11, 20);
pub const PANEL: Color = Color::Rgb(16, 20, 34);
pub const PANEL_ALT: Color = Color::Rgb(22, 27, 45);
pub const BORDER: Color = Color::Rgb(58, 66, 100);
pub const BORDER_ACTIVE: Color = Color::Rgb(178, 126, 255);
pub const ACCENT: Color = Color::Rgb(178, 126, 255);
pub const ACCENT_SOFT: Color = Color::Rgb(120, 94, 190);
pub const CYAN: Color = Color::Rgb(92, 206, 255);
pub const GREEN: Color = Color::Rgb(98, 224, 158);
pub const YELLOW: Color = Color::Rgb(255, 202, 92);
pub const RED: Color = Color::Rgb(255, 106, 128);
pub const TEXT: Color = Color::Rgb(226, 230, 244);
pub const MUTED: Color = Color::Rgb(124, 134, 166);
pub const DIM: Color = Color::Rgb(82, 90, 118);

pub fn base() -> Style {
    Style::default().bg(BG).fg(TEXT)
}

pub fn panel() -> Style {
    Style::default().bg(PANEL).fg(TEXT)
}

pub fn border(active: bool) -> Style {
    Style::default()
        .fg(if active { BORDER_ACTIVE } else { BORDER })
        .bg(BG)
}

pub fn title(active: bool) -> Style {
    Style::default()
        .fg(if active { ACCENT } else { MUTED })
        .bg(BG)
        .add_modifier(Modifier::BOLD)
}

pub fn highlight() -> Style {
    Style::default()
        .bg(ACCENT_SOFT)
        .fg(Color::Rgb(250, 250, 255))
        .add_modifier(Modifier::BOLD)
}

pub fn selected(active: bool) -> Style {
    if active {
        Style::default()
            .bg(ACCENT_SOFT)
            .fg(Color::Rgb(250, 250, 255))
    } else {
        Style::default().bg(PANEL_ALT).fg(TEXT)
    }
}

/// Small unicode icon for a file type.
pub fn icon(format: Option<Format>, is_dir: bool) -> &'static str {
    if is_dir {
        return "▸";
    }
    match format.map(|f| f.category()) {
        Some(crate::core::format::Category::Image) => "▣",
        Some(crate::core::format::Category::Vector) => "◇",
        Some(crate::core::format::Category::Document) => "▤",
        Some(crate::core::format::Category::Text) => "≡",
        Some(crate::core::format::Category::Media) => "▶",
        None => "·",
    }
}

pub fn icon_color(format: Option<Format>) -> Color {
    match format.map(|f| f.category()) {
        Some(crate::core::format::Category::Image) => GREEN,
        Some(crate::core::format::Category::Vector) => CYAN,
        Some(crate::core::format::Category::Document) => ACCENT,
        Some(crate::core::format::Category::Text) => YELLOW,
        Some(crate::core::format::Category::Media) => RED,
        None => MUTED,
    }
}

/// A 10-cell textual progress bar.
pub fn mini_bar(fraction: f32, width: usize) -> String {
    let filled = ((fraction.clamp(0.0, 1.0)) * width as f32).round() as usize;
    let mut out = String::new();
    for i in 0..width {
        out.push(if i < filled { '█' } else { '░' });
    }
    out
}
