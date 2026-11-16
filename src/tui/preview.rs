//! File previews for the terminal interface.
//!
//! Raster images are drawn with Unicode half blocks (`▀`), which double the
//! vertical resolution of a terminal cell.  PDFs are rendered to a temporary
//! PNG through Poppler or Ghostscript, videos through ffmpeg, and text files
//! are simply shown line by line.

use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::core::format::{Category, Format};
use crate::core::inspect::FileInfo;
use crate::core::{backends, image_ops, inspect_file, svg};
use crate::util::paths;

use super::theme;

/// What a preview is currently showing.
pub enum Body {
    Empty,
    Image(ImageState),
    Text(Vec<String>),
    Message(String),
}

/// A decoded raster plus a small cache of the resized version.
pub struct ImageState {
    source: DynamicImage,
    cache: Option<CachedImage>,
}

struct CachedImage {
    width: u16,
    height: u16,
    pixels: RgbaImage,
}

/// A loaded preview.  Keeping the temporary directory alive is important:
/// PDF and video frames live in it until the next preview replaces this one.
pub struct Preview {
    pub path: Option<PathBuf>,
    pub info: Option<FileInfo>,
    pub note: Option<String>,
    body: Body,
    _temp: Option<tempfile::TempDir>,
}

impl Preview {
    pub fn empty() -> Self {
        Self {
            path: None,
            info: None,
            note: None,
            body: Body::Empty,
            _temp: None,
        }
    }

    pub fn directory(path: &Path, count: usize) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            info: None,
            note: None,
            body: Body::Message(format!(
                "Directory\n\n{count} entries\n\nPress Enter to open it.",
            )),
            _temp: None,
        }
    }

    pub fn load(path: &Path) -> Self {
        let format = Format::from_path(path).ok();
        let info = inspect_file(path).ok();
        let mut preview = Self {
            path: Some(path.to_path_buf()),
            info,
            note: None,
            body: Body::Empty,
            _temp: None,
        };

        let Some(format) = format else {
            preview.body = Body::Message(
                "No preview for this file type.\n\nChoose a target format with `t` and convert it with `c`."
                    .to_string(),
            );
            return preview;
        };

        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if format.is_raster() {
            let too_large = size > 96 * 1024 * 1024
                || preview
                    .info
                    .as_ref()
                    .and_then(|info| info.dimensions())
                    .map(|(width, height)| width as u64 * height as u64 > 80_000_000)
                    .unwrap_or(false);
            if too_large {
                preview.body = Body::Message(format!(
                    "This image is {} and is too large to preview.\n\nIt can still be converted.",
                    crate::util::human::bytes(size)
                ));
                return preview;
            }
        }

        match format.category() {
            Category::Image => match image_ops::load_image(path) {
                Ok(image) => preview.body = Body::Image(ImageState::new(image)),
                Err(err) => {
                    preview.body = Body::Message(format!("Could not decode image:\n\n{err}"))
                }
            },
            Category::Vector => {
                // Keep huge SVGs manageable by lowering the render DPI.
                let dpi = match svg::svg_size(path) {
                    Ok((w, h)) => {
                        let longest = w.max(h).max(1);
                        if longest > 1600 {
                            ((96 * 1600) / longest).max(1)
                        } else {
                            96
                        }
                    }
                    Err(_) => 96,
                };
                match svg::render_svg(path, dpi) {
                    Ok(image) => preview.body = Body::Image(ImageState::new(image)),
                    Err(err) => {
                        preview.body = Body::Message(format!("Could not render SVG:\n\n{err}"))
                    }
                }
            }
            Category::Document => {
                if format == Format::Pdf {
                    let temp = match tempfile::Builder::new()
                        .prefix("sorcerer-preview-")
                        .tempdir()
                    {
                        Ok(dir) => dir,
                        Err(err) => {
                            preview.body =
                                Body::Message(format!("Cannot create temp directory:\n\n{err}"));
                            return preview;
                        }
                    };
                    match render_pdf_preview(path, temp.path()) {
                        Ok(image) => {
                            preview.body = Body::Image(ImageState::new(image));
                            preview.note = Some("first page".to_string());
                        }
                        Err(err) => preview.body = Body::Message(err),
                    }
                    preview._temp = Some(temp);
                } else if matches!(format, Format::Csv | Format::Rtf) {
                    preview.body = read_text_body(path);
                } else {
                    preview.body = Body::Message(format!(
                        "No inline preview for {} files.\n\nConvert to PDF or PNG with `t` and `c` to see the result.",
                        format.label()
                    ));
                }
            }
            Category::Text => preview.body = read_text_body(path),
            Category::Media => {
                if format.is_video() {
                    let temp = match tempfile::Builder::new()
                        .prefix("sorcerer-preview-")
                        .tempdir()
                    {
                        Ok(dir) => dir,
                        Err(err) => {
                            preview.body =
                                Body::Message(format!("Cannot create temp directory:\n\n{err}"));
                            return preview;
                        }
                    };
                    match render_video_frame(path, temp.path()) {
                        Ok(image) => {
                            preview.body = Body::Image(ImageState::new(image));
                            preview.note = Some("video frame".to_string());
                        }
                        Err(err) => preview.body = Body::Message(err),
                    }
                    preview._temp = Some(temp);
                } else {
                    let detail = preview
                        .info
                        .as_ref()
                        .and_then(|i| i.duration_secs)
                        .map(|d| {
                            format!(
                                " · {}",
                                crate::util::human::duration(std::time::Duration::from_secs_f64(d))
                            )
                        })
                        .unwrap_or_default();
                    preview.body = Body::Message(format!(
                        "Audio preview{detail}\n\nMetadata is shown on the right. Convert it with `t` and `c`.",
                    ));
                }
            }
        }

        preview
    }

    fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Preview".to_string());
        let mut title = name;
        if let Some(info) = &self.info {
            let summary = info.summary();
            if !summary.is_empty() {
                title.push_str(" · ");
                title.push_str(&summary);
            }
        }
        if let Some(note) = &self.note {
            title.push_str(" · ");
            title.push_str(note);
        }
        title
    }

    /// Draw the preview inside `area`.
    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let title = Line::from(vec![
            ratatui::text::Span::styled(" Preview ", theme::title(false)),
            ratatui::text::Span::styled(
                format!("· {}", self.title()),
                Style::default().fg(theme::MUTED),
            ),
        ]);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme::border(false))
            .style(theme::panel())
            .title(title);

        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        match &mut self.body {
            Body::Empty => {
                let hint = Paragraph::new(vec![
                    Line::from(""),
                    Line::from(ratatui::text::Span::styled(
                        "  Select a file to preview it",
                        Style::default().fg(theme::MUTED),
                    )),
                    Line::from(ratatui::text::Span::styled(
                        "  Images, PDFs, SVGs, text and video frames",
                        Style::default().fg(theme::DIM),
                    )),
                ])
                .style(theme::panel());
                frame.render_widget(hint, inner);
            }
            Body::Image(image) => {
                let buffer = frame.buffer_mut();
                image.draw(buffer, inner);
            }
            Body::Text(lines) => {
                let lines: Vec<Line> = lines
                    .iter()
                    .map(|text| {
                        Line::from(ratatui::text::Span::styled(
                            text.clone(),
                            Style::default().fg(theme::TEXT),
                        ))
                    })
                    .collect();
                let paragraph = Paragraph::new(lines).style(theme::panel());
                frame.render_widget(paragraph, inner);
            }
            Body::Message(text) => {
                let lines: Vec<Line> = text
                    .lines()
                    .map(|line| {
                        Line::from(ratatui::text::Span::styled(
                            line.to_string(),
                            Style::default().fg(theme::MUTED),
                        ))
                    })
                    .collect();
                let paragraph = Paragraph::new(lines)
                    .style(theme::panel())
                    .wrap(Wrap { trim: false });
                frame.render_widget(paragraph, inner);
            }
        }
    }
}

impl ImageState {
    fn new(source: DynamicImage) -> Self {
        Self {
            source,
            cache: None,
        }
    }

    fn ensure_cache(&mut self, max_width: u16, max_height: u16) {
        let source_width = self.source.width().max(1);
        let source_height = self.source.height().max(1);
        let scale =
            (max_width as f32 / source_width as f32).min(max_height as f32 / source_height as f32);
        let width = (source_width as f32 * scale)
            .round()
            .clamp(1.0, max_width.max(1) as f32) as u16;
        let height = (source_height as f32 * scale)
            .round()
            .clamp(1.0, max_height.max(1) as f32) as u16;

        if self
            .cache
            .as_ref()
            .map(|cache| cache.width == width && cache.height == height)
            .unwrap_or(false)
        {
            return;
        }

        let rgba = self.source.to_rgba8();
        let pixels =
            image::imageops::resize(&rgba, width as u32, height as u32, FilterType::Triangle);
        self.cache = Some(CachedImage {
            width,
            height,
            pixels,
        });
    }

    fn draw(&mut self, buffer: &mut Buffer, area: Rect) {
        // Two vertical pixels per terminal cell.
        self.ensure_cache(area.width, area.height.saturating_mul(2));
        let Some(cache) = &self.cache else {
            return;
        };

        let rows = cache.height.div_ceil(2);
        let x_offset = area.width.saturating_sub(cache.width) / 2;
        let y_offset = area.height.saturating_sub(rows) / 2;
        let background = match theme::PANEL {
            ratatui::style::Color::Rgb(r, g, b) => [r, g, b],
            _ => [16, 20, 34],
        };

        for row in 0..rows {
            for column in 0..cache.width {
                let x = area.x + x_offset + column;
                let y = area.y + y_offset + row;
                if x >= area.right() || y >= area.bottom() {
                    continue;
                }
                let top_y = (row as u32 * 2).min(cache.height as u32 - 1);
                let bottom_y = (top_y + 1).min(cache.height as u32 - 1);
                let top = cache.pixels.get_pixel(column as u32, top_y).0;
                let bottom = cache.pixels.get_pixel(column as u32, bottom_y).0;

                let cell = buffer
                    .cell_mut((x, y))
                    .expect("cell is inside the preview area");
                cell.set_symbol("▀")
                    .set_fg(blend(top, background))
                    .set_bg(blend(bottom, background));
            }
        }
    }
}

fn blend(pixel: [u8; 4], background: [u8; 3]) -> ratatui::style::Color {
    let alpha = pixel[3] as u32;
    let inverse = 255 - alpha;
    let mix = |channel: u8, bg: u8| -> u8 {
        ((channel as u32 * alpha + bg as u32 * inverse) / 255) as u8
    };
    ratatui::style::Color::Rgb(
        mix(pixel[0], background[0]),
        mix(pixel[1], background[1]),
        mix(pixel[2], background[2]),
    )
}

fn read_text_body(path: &Path) -> Body {
    let Ok(file) = File::open(path) else {
        return Body::Message("Cannot open this file.".to_string());
    };
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    for line in reader.lines().take(2000) {
        let Ok(line) = line else { break };
        let mut line = line.replace('\t', "    ");
        if line.chars().count() > 4000 {
            line = line.chars().take(4000).collect();
        }
        lines.push(line);
    }
    if lines.is_empty() {
        Body::Message("Empty file.".to_string())
    } else {
        Body::Text(lines)
    }
}

fn render_pdf_preview(path: &Path, temp: &Path) -> Result<DynamicImage, String> {
    let prefix = temp.join("page");
    if let Some((label, program)) = backends::first_available(&["pdftoppm", "pdftocairo"]) {
        let args: Vec<OsString> = vec![
            "-f".into(),
            "1".into(),
            "-l".into(),
            "1".into(),
            "-scale-to".into(),
            "1600".into(),
            "-png".into(),
            "-singlefile".into(),
            path.as_os_str().to_os_string(),
            prefix.as_os_str().to_os_string(),
        ];
        if backends::run_checked(&program, args, &label).is_ok() {
            let output = prefix.with_extension("png");
            if output.exists() {
                return image_ops::load_image(&output).map_err(|e| e.to_string());
            }
        }
    }

    if let Some(gs) = backends::which("gs") {
        let output = temp.join("gs.png");
        let args: Vec<OsString> = vec![
            "-q".into(),
            "-dNOPAUSE".into(),
            "-dBATCH".into(),
            "-dSAFER".into(),
            "-sDEVICE=png16m".into(),
            "-dFirstPage=1".into(),
            "-dLastPage=1".into(),
            "-r120".into(),
            format!("-sOutputFile={}", output.display()).into(),
            path.as_os_str().to_os_string(),
        ];
        if backends::run_checked(&gs, args, "ghostscript").is_ok() && output.exists() {
            return image_ops::load_image(&output).map_err(|e| e.to_string());
        }
    }

    Err("Cannot render this PDF yet.\n\nInstall poppler-utils (`pdftoppm`) or ghostscript (`gs`) to preview PDF pages."
        .to_string())
}

fn render_video_frame(path: &Path, temp: &Path) -> Result<DynamicImage, String> {
    let ffmpeg = backends::which("ffmpeg")
        .ok_or_else(|| "Install ffmpeg to preview video frames.".to_string())?;
    let output = temp.join("frame.png");

    // Aim one second in; very short clips fall back to the first frame.
    for seek in ["1", "0"] {
        let args: Vec<OsString> = vec![
            "-y".into(),
            "-nostdin".into(),
            "-loglevel".into(),
            "error".into(),
            "-ss".into(),
            seek.into(),
            "-i".into(),
            path.as_os_str().to_os_string(),
            "-frames:v".into(),
            "1".into(),
            "-vf".into(),
            "scale=min(1600\\,iw):-2".into(),
            "-update".into(),
            "1".into(),
            output.as_os_str().to_os_string(),
        ];
        if backends::run_checked(&ffmpeg, args, "ffmpeg").is_ok() && output.exists() {
            return image_ops::load_image(&output).map_err(|e| e.to_string());
        }
    }

    Err("Could not extract a video frame.".to_string())
}

/// Human friendly path shown in the settings panel.
pub fn display_path(path: Option<&Path>) -> String {
    match path {
        Some(path) => paths::pretty(path),
        None => "same folder as source".to_string(),
    }
}
