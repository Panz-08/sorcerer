//! Metadata inspection used by `sorce inspect` and the TUI preview.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use super::error::{Error, Result};
use super::format::Format;
use super::{image_ops, media, pdf_render, svg};

/// Everything Sorcerer can tell you about a file.
#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub format: String,
    pub category: String,
    pub mime: String,
    pub size: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub color: Option<String>,
    pub pages: Option<u32>,
    pub duration_secs: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub fps: Option<f64>,
    pub lines: Option<usize>,
    pub words: Option<usize>,
    pub modified: Option<u64>,
}

impl FileInfo {
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        Some((self.width?, self.height?))
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let (Some(w), Some(h)) = (self.width, self.height) {
            parts.push(format!("{w}×{h}"));
        }
        if let Some(pages) = self.pages {
            parts.push(format!("{pages} page{}", if pages == 1 { "" } else { "s" }));
        }
        if let Some(duration) = self.duration_secs {
            parts.push(format_duration(duration));
        }
        if let Some(color) = &self.color {
            parts.push(color.clone());
        }
        parts.join(" · ")
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Inspect a single file.
pub fn inspect(path: &Path) -> Result<FileInfo> {
    let metadata = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    let format = Format::from_path(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());

    let mut info = FileInfo {
        path: path.to_path_buf(),
        name,
        format: format.label().to_string(),
        category: format.category().label().to_string(),
        mime: format.mime().to_string(),
        size: metadata.len(),
        width: None,
        height: None,
        color: None,
        pages: None,
        duration_secs: None,
        video_codec: None,
        audio_codec: None,
        fps: None,
        lines: None,
        words: None,
        modified: metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
    };

    match format {
        Format::Pdf => {
            let pdf = pdf_render::info(path);
            if pdf.pages > 0 {
                info.pages = Some(pdf.pages);
            }
            if let Some(size) = pdf.page_size {
                // Try to pull dimensions like "595.276 x 841.89 pts".
                if let Some((w, h)) = parse_points(&size) {
                    info.width = Some(w);
                    info.height = Some(h);
                }
            }
        }
        Format::Svg => {
            if let Ok((w, h)) = svg::svg_size(path) {
                info.width = Some(w);
                info.height = Some(h);
            }
            info.color = Some("vector".to_string());
        }
        f if f.is_media() => {
            if let Some(media) = media::info(path) {
                info.width = media.width;
                info.height = media.height;
                info.duration_secs = media.duration_secs;
                info.video_codec = media.video_codec;
                info.audio_codec = media.audio_codec;
                info.fps = media.fps;
            }
        }
        Format::Txt | Format::Markdown | Format::Html => {
            if let Ok(text) = std::fs::read_to_string(path) {
                info.lines = Some(text.lines().count());
                info.words = Some(text.split_whitespace().count());
            }
        }
        f if f.is_raster() => match image_ops::load_image(path) {
            Ok(img) => {
                info.width = Some(img.width());
                info.height = Some(img.height());
                info.color = Some(image_ops::color_label(&img));
            }
            Err(_) => {
                // Dimensions unknown but the file itself is still valid metadata.
            }
        },
        _ => {}
    }

    Ok(info)
}

fn parse_points(size: &str) -> Option<(u32, u32)> {
    let x = size.find('x')?;
    let w: f32 = size[..x].trim().parse().ok()?;
    let rest = size[x + 1..].trim();
    let h: f32 = rest.split_whitespace().next()?.parse().ok()?;
    Some((w.round() as u32, h.round() as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pdf_page_size() {
        assert_eq!(parse_points("595.276 x 841.89 pts (A4)"), Some((595, 842)));
    }
}
