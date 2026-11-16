//! Conversion options shared by the CLI, the engine and the TUI.

use std::fmt;
use std::str::FromStr;

use super::error::{Error, Result};
use super::format::Format;

/// Page selection for PDFs (and animations).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PageSelection {
    #[default]
    All,
    Ranges(Vec<(u32, Option<u32>)>),
}

impl PageSelection {
    /// Parse `all`, `1`, `2-5`, `1,3,7-9`, `10-` or `-3`.
    pub fn parse(spec: &str) -> Result<Self> {
        let spec = spec.trim();
        if spec.is_empty() || spec.eq_ignore_ascii_case("all") || spec == "*" {
            return Ok(PageSelection::All);
        }
        let mut ranges = Vec::new();
        for part in spec.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((a, b)) = part.split_once('-') {
                let start = if a.trim().is_empty() {
                    1
                } else {
                    a.trim()
                        .parse::<u32>()
                        .map_err(|_| Error::InvalidPages(spec.to_string()))?
                };
                let end = if b.trim().is_empty() {
                    None
                } else {
                    let end = b
                        .trim()
                        .parse::<u32>()
                        .map_err(|_| Error::InvalidPages(spec.to_string()))?;
                    if end < start {
                        return Err(Error::InvalidPages(spec.to_string()));
                    }
                    Some(end)
                };
                if start == 0 {
                    return Err(Error::InvalidPages(spec.to_string()));
                }
                ranges.push((start, end));
            } else {
                let page = part
                    .parse::<u32>()
                    .map_err(|_| Error::InvalidPages(spec.to_string()))?;
                if page == 0 {
                    return Err(Error::InvalidPages(spec.to_string()));
                }
                ranges.push((page, Some(page)));
            }
        }
        if ranges.is_empty() {
            return Err(Error::InvalidPages(spec.to_string()));
        }
        Ok(PageSelection::Ranges(ranges))
    }

    pub fn is_all(&self) -> bool {
        matches!(self, PageSelection::All)
    }

    /// Expand to concrete 1-based page numbers for a document with `total`
    /// pages. Out-of-range numbers are ignored.
    pub fn selected(&self, total: u32) -> Vec<u32> {
        match self {
            PageSelection::All => (1..=total).collect(),
            PageSelection::Ranges(ranges) => {
                let mut pages = Vec::new();
                for &(start, end) in ranges {
                    let end = end.unwrap_or(total).min(total);
                    if start > total {
                        continue;
                    }
                    pages.extend(start..=end);
                }
                pages.sort_unstable();
                pages.dedup();
                pages
            }
        }
    }

    /// First/last requested page, useful for backend render windows.
    pub fn bounds(&self) -> Option<(u32, Option<u32>)> {
        match self {
            PageSelection::All => None,
            PageSelection::Ranges(ranges) => {
                let first = ranges.iter().map(|(a, _)| *a).min()?;
                let last = ranges.iter().filter_map(|(_, b)| *b).max();
                Some((first, last))
            }
        }
    }
}

impl fmt::Display for PageSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageSelection::All => f.write_str("all"),
            PageSelection::Ranges(ranges) => {
                let parts: Vec<String> = ranges
                    .iter()
                    .map(|(a, b)| match b {
                        Some(b) if a == b => a.to_string(),
                        Some(b) => format!("{a}-{b}"),
                        None => format!("{a}-"),
                    })
                    .collect();
                f.write_str(&parts.join(","))
            }
        }
    }
}

/// How an image is fitted into a requested box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Keep aspect ratio, fit completely inside the box.
    Contain,
    /// Keep aspect ratio, fill the box and crop the overflow.
    Cover,
    /// Stretch to exactly the requested size.
    Fill,
}

impl Fit {
    pub fn label(self) -> &'static str {
        match self {
            Fit::Contain => "contain",
            Fit::Cover => "cover",
            Fit::Fill => "fill",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Fit::Contain => Fit::Cover,
            Fit::Cover => Fit::Fill,
            Fit::Fill => Fit::Contain,
        }
    }
}

impl FromStr for Fit {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "contain" | "inside" | "fit" => Ok(Fit::Contain),
            "cover" | "crop" => Ok(Fit::Cover),
            "fill" | "stretch" | "exact" => Ok(Fit::Fill),
            other => Err(Error::InvalidOption(format!(
                "unknown fit mode `{other}` (use contain, cover or fill)"
            ))),
        }
    }
}

/// Optional resize request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resize {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fit: Fit,
}

impl Resize {
    pub fn new(width: Option<u32>, height: Option<u32>, fit: Fit) -> Self {
        Self { width, height, fit }
    }

    /// Parse `800x600`, `800x`, `x600`.
    pub fn parse(spec: &str) -> Result<Self> {
        let spec = spec.trim();
        let (w, h) = spec
            .split_once(['x', 'X'])
            .ok_or_else(|| Error::InvalidOption(format!("invalid size `{spec}` (use WxH)")))?;
        let width = if w.trim().is_empty() {
            None
        } else {
            Some(
                w.trim()
                    .parse::<u32>()
                    .map_err(|_| Error::InvalidOption(format!("invalid width in `{spec}`")))?,
            )
        };
        let height = if h.trim().is_empty() {
            None
        } else {
            Some(
                h.trim()
                    .parse::<u32>()
                    .map_err(|_| Error::InvalidOption(format!("invalid height in `{spec}`")))?,
            )
        };
        if width.is_none() && height.is_none() {
            return Err(Error::InvalidOption(format!(
                "empty size `{spec}` (use WxH)"
            )));
        }
        Ok(Resize {
            width,
            height,
            fit: Fit::Contain,
        })
    }

    /// Compute the concrete output dimensions for a source of `w`×`h`.
    pub fn dimensions(self, w: u32, h: u32) -> (u32, u32) {
        let w = w.max(1) as f64;
        let h = h.max(1) as f64;
        match self.fit {
            Fit::Fill => (
                self.width.unwrap_or(w as u32).max(1),
                self.height.unwrap_or(h as u32).max(1),
            ),
            Fit::Contain => {
                let mut scale = f64::INFINITY;
                if let Some(width) = self.width {
                    scale = scale.min(width as f64 / w);
                }
                if let Some(height) = self.height {
                    scale = scale.min(height as f64 / h);
                }
                if !scale.is_finite() {
                    return (w as u32, h as u32);
                }
                (
                    ((w * scale).round() as u32).max(1),
                    ((h * scale).round() as u32).max(1),
                )
            }
            Fit::Cover => {
                let mut scale = 0.0_f64;
                if let Some(width) = self.width {
                    scale = scale.max(width as f64 / w);
                }
                if let Some(height) = self.height {
                    scale = scale.max(height as f64 / h);
                }
                if scale <= 0.0 {
                    return (w as u32, h as u32);
                }
                (
                    ((w * scale).round() as u32).max(1),
                    ((h * scale).round() as u32).max(1),
                )
            }
        }
    }

    pub fn describe(self) -> String {
        match (self.width, self.height) {
            (Some(w), Some(h)) => format!("{w}x{h} {}", self.fit.label()),
            (Some(w), None) => format!("{w}px wide"),
            (None, Some(h)) => format!("{h}px high"),
            (None, None) => "none".to_string(),
        }
    }
}

/// Clockwise rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    None,
    Deg90,
    Deg180,
    Deg270,
}

impl Rotation {
    pub fn degrees(self) -> u16 {
        match self {
            Rotation::None => 0,
            Rotation::Deg90 => 90,
            Rotation::Deg180 => 180,
            Rotation::Deg270 => 270,
        }
    }

    pub fn from_degrees(deg: u16) -> Result<Self> {
        match deg % 360 {
            0 => Ok(Rotation::None),
            90 => Ok(Rotation::Deg90),
            180 => Ok(Rotation::Deg180),
            270 => Ok(Rotation::Deg270),
            _ => Err(Error::InvalidOption(format!(
                "rotation must be 0, 90, 180 or 270 (got {deg})"
            ))),
        }
    }

    pub fn next(self) -> Self {
        match self {
            Rotation::None => Rotation::Deg90,
            Rotation::Deg90 => Rotation::Deg180,
            Rotation::Deg180 => Rotation::Deg270,
            Rotation::Deg270 => Rotation::None,
        }
    }
}

/// Mirroring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Flip {
    #[default]
    None,
    Horizontal,
    Vertical,
    Both,
}

impl Flip {
    pub fn label(self) -> &'static str {
        match self {
            Flip::None => "none",
            Flip::Horizontal => "horizontal",
            Flip::Vertical => "vertical",
            Flip::Both => "both",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Flip::None => Flip::Horizontal,
            Flip::Horizontal => Flip::Vertical,
            Flip::Vertical => Flip::Both,
            Flip::Both => Flip::None,
        }
    }
}

impl FromStr for Flip {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "none" | "no" => Ok(Flip::None),
            "h" | "horizontal" | "hflip" => Ok(Flip::Horizontal),
            "v" | "vertical" | "vflip" => Ok(Flip::Vertical),
            "both" | "hv" => Ok(Flip::Both),
            other => Err(Error::InvalidOption(format!(
                "unknown flip `{other}` (use none, h, v or both)"
            ))),
        }
    }
}

/// Paper size when creating a PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageSize {
    /// Page matches the image dimensions at the requested DPI.
    #[default]
    Fit,
    A4,
    Letter,
    /// A4 in portrait or landscape depending on the image.
    A4Auto,
}

impl PageSize {
    pub fn label(self) -> &'static str {
        match self {
            PageSize::Fit => "fit image",
            PageSize::A4 => "A4",
            PageSize::Letter => "Letter",
            PageSize::A4Auto => "A4 auto",
        }
    }

    pub fn next(self) -> Self {
        match self {
            PageSize::Fit => PageSize::A4,
            PageSize::A4 => PageSize::Letter,
            PageSize::Letter => PageSize::A4Auto,
            PageSize::A4Auto => PageSize::Fit,
        }
    }

    /// Portrait dimensions in PDF points.
    pub fn points(self) -> Option<(f32, f32)> {
        match self {
            PageSize::Fit => None,
            PageSize::A4 | PageSize::A4Auto => Some((595.276, 841.890)),
            PageSize::Letter => Some((612.0, 792.0)),
        }
    }
}

impl FromStr for PageSize {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "fit" | "image" | "auto" => Ok(PageSize::Fit),
            "a4" => Ok(PageSize::A4),
            "a4-auto" | "a4auto" => Ok(PageSize::A4Auto),
            "letter" | "us-letter" => Ok(PageSize::Letter),
            other => Err(Error::InvalidOption(format!(
                "unknown page size `{other}` (use fit, a4, a4-auto or letter)"
            ))),
        }
    }
}

/// Everything needed to convert one file.
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub target: Format,
    pub quality: u8,
    pub dpi: u32,
    pub pages: PageSelection,
    pub resize: Option<Resize>,
    pub rotate: Rotation,
    pub flip: Flip,
    pub grayscale: bool,
    pub background: [u8; 4],
    pub overwrite: bool,
    pub page_size: PageSize,
    pub margin_mm: f32,
    pub fps: f32,
    pub seek: f64,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            target: Format::Png,
            quality: 90,
            dpi: 150,
            pages: PageSelection::All,
            resize: None,
            rotate: Rotation::None,
            flip: Flip::None,
            grayscale: false,
            background: [255, 255, 255, 255],
            overwrite: false,
            page_size: PageSize::Fit,
            margin_mm: 12.0,
            fps: 12.0,
            seek: 0.0,
        }
    }
}

impl ConvertOptions {
    pub fn for_target(target: Format) -> Self {
        Self {
            target,
            quality: target.default_quality(),
            ..Self::default()
        }
    }

    pub fn has_image_ops(&self) -> bool {
        self.resize.is_some()
            || self.rotate != Rotation::None
            || self.flip != Flip::None
            || self.grayscale
    }

    /// Quality as a fraction (0.0 – 1.0), handy for ffmpeg / encoders.
    pub fn quality_fraction(&self) -> f32 {
        (self.quality.clamp(1, 100) as f32) / 100.0
    }

    /// Ensure quality is inside the accepted range.
    pub fn validate(&self) -> Result<()> {
        if self.quality == 0 || self.quality > 100 {
            return Err(Error::InvalidOption(format!(
                "quality must be between 1 and 100 (got {})",
                self.quality
            )));
        }
        if self.dpi == 0 || self.dpi > 1200 {
            return Err(Error::InvalidOption(format!(
                "dpi must be between 1 and 1200 (got {})",
                self.dpi
            )));
        }
        if self.fps <= 0.0 || self.fps > 120.0 {
            return Err(Error::InvalidOption(format!(
                "fps must be between 0.1 and 120 (got {})",
                self.fps
            )));
        }
        Ok(())
    }
}

/// Parse colours such as `white`, `black`, `transparent`, `#fff`,
/// `#ffffff` or `#ffffff80`.
pub fn parse_color(s: &str) -> Result<[u8; 4]> {
    let s = s.trim().to_ascii_lowercase();
    match s.as_str() {
        "white" => return Ok([255, 255, 255, 255]),
        "black" => return Ok([0, 0, 0, 255]),
        "transparent" | "none" => return Ok([0, 0, 0, 0]),
        "red" => return Ok([255, 0, 0, 255]),
        "green" => return Ok([0, 128, 0, 255]),
        "blue" => return Ok([0, 0, 255, 255]),
        "gray" | "grey" => return Ok([128, 128, 128, 255]),
        _ => {}
    }
    let hex = s.strip_prefix('#').unwrap_or(&s);
    let parse = |h: &str| {
        u8::from_str_radix(h, 16).map_err(|_| Error::InvalidOption(format!("invalid colour `{s}`")))
    };
    match hex.len() {
        3 => {
            let r = parse(&hex[0..1])?;
            let g = parse(&hex[1..2])?;
            let b = parse(&hex[2..3])?;
            Ok([r * 17, g * 17, b * 17, 255])
        }
        6 => Ok([
            parse(&hex[0..2])?,
            parse(&hex[2..4])?,
            parse(&hex[4..6])?,
            255,
        ]),
        8 => Ok([
            parse(&hex[0..2])?,
            parse(&hex[2..4])?,
            parse(&hex[4..6])?,
            parse(&hex[6..8])?,
        ]),
        _ => Err(Error::InvalidOption(format!(
            "invalid colour `{s}` (use #rrggbb, #rrggbbaa, white, black…)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_selection_parses() {
        assert!(PageSelection::parse("all").unwrap().is_all());
        assert_eq!(
            PageSelection::parse("1-3,5,8-9").unwrap(),
            PageSelection::Ranges(vec![(1, Some(3)), (5, Some(5)), (8, Some(9))])
        );
        let sel = PageSelection::parse("2-").unwrap();
        assert_eq!(sel.selected(4), vec![2, 3, 4]);
        assert_eq!(PageSelection::parse("-2").unwrap().selected(4), vec![1, 2]);
        assert!(PageSelection::parse("0").is_err());
        assert!(PageSelection::parse("5-3").is_err());
    }

    #[test]
    fn resize_contain() {
        let r = Resize::parse("100x100").unwrap();
        assert_eq!(r.dimensions(200, 100), (100, 50));
        assert_eq!(
            Resize::parse("x100").unwrap().dimensions(200, 100),
            (200, 100)
        );
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#fff").unwrap(), [255, 255, 255, 255]);
        assert_eq!(parse_color("#ff0000").unwrap(), [255, 0, 0, 255]);
        assert_eq!(parse_color("transparent").unwrap()[3], 0);
        assert!(parse_color("#12").is_err());
    }

    #[test]
    fn rotation_parses() {
        assert_eq!(Rotation::from_degrees(180).unwrap(), Rotation::Deg180);
        assert!(Rotation::from_degrees(45).is_err());
    }
}
