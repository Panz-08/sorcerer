//! File formats known to Sorcerer, detection and conversion targets.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::error::{Error, Result};

/// Every file format Sorcerer understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    // ---- raster images -------------------------------------------------
    Png,
    Jpeg,
    Webp,
    Gif,
    Bmp,
    Tiff,
    Ico,
    Tga,
    Pnm,
    Qoi,
    Hdr,
    Exr,
    Farbfeld,
    Avif,
    Heic,
    Jxl,
    // ---- vector & documents -------------------------------------------
    Svg,
    Pdf,
    Doc,
    Docx,
    Odt,
    Rtf,
    Txt,
    Markdown,
    Html,
    Xlsx,
    Xls,
    Ods,
    Csv,
    Pptx,
    Ppt,
    Odp,
    // ---- media ---------------------------------------------------------
    Mp4,
    Webm,
    Mkv,
    Mov,
    Avi,
    Mp3,
    Wav,
    Ogg,
    Flac,
    Aac,
    M4a,
    Opus,
}

/// Broad family used for menus and routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    Image,
    Vector,
    Document,
    Text,
    Media,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Image => "image",
            Category::Vector => "vector",
            Category::Document => "document",
            Category::Text => "text",
            Category::Media => "media",
        }
    }
}

impl Format {
    /// Canonical file extension (without the dot).
    pub const fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
            Format::Gif => "gif",
            Format::Bmp => "bmp",
            Format::Tiff => "tiff",
            Format::Ico => "ico",
            Format::Tga => "tga",
            Format::Pnm => "pnm",
            Format::Qoi => "qoi",
            Format::Hdr => "hdr",
            Format::Exr => "exr",
            Format::Farbfeld => "ff",
            Format::Avif => "avif",
            Format::Heic => "heic",
            Format::Jxl => "jxl",
            Format::Svg => "svg",
            Format::Pdf => "pdf",
            Format::Doc => "doc",
            Format::Docx => "docx",
            Format::Odt => "odt",
            Format::Rtf => "rtf",
            Format::Txt => "txt",
            Format::Markdown => "md",
            Format::Html => "html",
            Format::Xlsx => "xlsx",
            Format::Xls => "xls",
            Format::Ods => "ods",
            Format::Csv => "csv",
            Format::Pptx => "pptx",
            Format::Ppt => "ppt",
            Format::Odp => "odp",
            Format::Mp4 => "mp4",
            Format::Webm => "webm",
            Format::Mkv => "mkv",
            Format::Mov => "mov",
            Format::Avi => "avi",
            Format::Mp3 => "mp3",
            Format::Wav => "wav",
            Format::Ogg => "ogg",
            Format::Flac => "flac",
            Format::Aac => "aac",
            Format::M4a => "m4a",
            Format::Opus => "opus",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Webp => "WEBP",
            Format::Gif => "GIF",
            Format::Bmp => "BMP",
            Format::Tiff => "TIFF",
            Format::Ico => "ICO",
            Format::Tga => "TGA",
            Format::Pnm => "PNM",
            Format::Qoi => "QOI",
            Format::Hdr => "HDR",
            Format::Exr => "EXR",
            Format::Farbfeld => "FARBFELD",
            Format::Avif => "AVIF",
            Format::Heic => "HEIC",
            Format::Jxl => "JXL",
            Format::Svg => "SVG",
            Format::Pdf => "PDF",
            Format::Doc => "DOC",
            Format::Docx => "DOCX",
            Format::Odt => "ODT",
            Format::Rtf => "RTF",
            Format::Txt => "TXT",
            Format::Markdown => "MD",
            Format::Html => "HTML",
            Format::Xlsx => "XLSX",
            Format::Xls => "XLS",
            Format::Ods => "ODS",
            Format::Csv => "CSV",
            Format::Pptx => "PPTX",
            Format::Ppt => "PPT",
            Format::Odp => "ODP",
            Format::Mp4 => "MP4",
            Format::Webm => "WEBM",
            Format::Mkv => "MKV",
            Format::Mov => "MOV",
            Format::Avi => "AVI",
            Format::Mp3 => "MP3",
            Format::Wav => "WAV",
            Format::Ogg => "OGG",
            Format::Flac => "FLAC",
            Format::Aac => "AAC",
            Format::M4a => "M4A",
            Format::Opus => "OPUS",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Format::Png => "Portable Network Graphics",
            Format::Jpeg => "JPEG image",
            Format::Webp => "WebP image",
            Format::Gif => "GIF image / animation",
            Format::Bmp => "Windows bitmap",
            Format::Tiff => "TIFF image",
            Format::Ico => "Windows icon",
            Format::Tga => "Targa image",
            Format::Pnm => "Portable anymap",
            Format::Qoi => "Quite OK Image",
            Format::Hdr => "Radiance HDR",
            Format::Exr => "OpenEXR",
            Format::Farbfeld => "Farbfeld image",
            Format::Avif => "AV1 image",
            Format::Heic => "HEIF/HEIC image (via ffmpeg)",
            Format::Jxl => "JPEG XL (via ffmpeg)",
            Format::Svg => "Scalable Vector Graphics",
            Format::Pdf => "Portable Document Format",
            Format::Doc => "Word 97 document",
            Format::Docx => "Word document",
            Format::Odt => "OpenDocument text",
            Format::Rtf => "Rich Text Format",
            Format::Txt => "Plain text",
            Format::Markdown => "Markdown",
            Format::Html => "HTML document",
            Format::Xlsx => "Excel workbook",
            Format::Xls => "Excel 97 workbook",
            Format::Ods => "OpenDocument spreadsheet",
            Format::Csv => "Comma separated values",
            Format::Pptx => "PowerPoint presentation",
            Format::Ppt => "PowerPoint 97 presentation",
            Format::Odp => "OpenDocument presentation",
            Format::Mp4 => "MPEG-4 video",
            Format::Webm => "WebM video",
            Format::Mkv => "Matroska video",
            Format::Mov => "QuickTime movie",
            Format::Avi => "AVI video",
            Format::Mp3 => "MPEG audio",
            Format::Wav => "Wave audio",
            Format::Ogg => "Ogg audio",
            Format::Flac => "FLAC audio",
            Format::Aac => "AAC audio",
            Format::M4a => "MPEG-4 audio",
            Format::Opus => "Opus audio",
        }
    }

    pub const fn category(self) -> Category {
        match self {
            Format::Png
            | Format::Jpeg
            | Format::Webp
            | Format::Gif
            | Format::Bmp
            | Format::Tiff
            | Format::Ico
            | Format::Tga
            | Format::Pnm
            | Format::Qoi
            | Format::Hdr
            | Format::Exr
            | Format::Farbfeld
            | Format::Avif
            | Format::Heic
            | Format::Jxl => Category::Image,
            Format::Svg => Category::Vector,
            Format::Pdf
            | Format::Doc
            | Format::Docx
            | Format::Odt
            | Format::Rtf
            | Format::Xlsx
            | Format::Xls
            | Format::Ods
            | Format::Csv
            | Format::Pptx
            | Format::Ppt
            | Format::Odp => Category::Document,
            Format::Txt | Format::Markdown | Format::Html => Category::Text,
            Format::Mp4
            | Format::Webm
            | Format::Mkv
            | Format::Mov
            | Format::Avi
            | Format::Mp3
            | Format::Wav
            | Format::Ogg
            | Format::Flac
            | Format::Aac
            | Format::M4a
            | Format::Opus => Category::Media,
        }
    }

    pub const fn is_raster(self) -> bool {
        matches!(self.category(), Category::Image)
    }

    pub const fn is_vector(self) -> bool {
        matches!(self, Format::Svg)
    }

    pub const fn is_pdf(self) -> bool {
        matches!(self, Format::Pdf)
    }

    pub const fn is_document(self) -> bool {
        matches!(self.category(), Category::Document)
    }

    pub const fn is_text(self) -> bool {
        matches!(self.category(), Category::Text)
    }

    pub const fn is_media(self) -> bool {
        matches!(self.category(), Category::Media)
    }

    pub const fn is_audio(self) -> bool {
        matches!(
            self,
            Format::Mp3
                | Format::Wav
                | Format::Ogg
                | Format::Flac
                | Format::Aac
                | Format::M4a
                | Format::Opus
        )
    }

    pub const fn is_video(self) -> bool {
        matches!(
            self,
            Format::Mp4 | Format::Webm | Format::Mkv | Format::Mov | Format::Avi
        )
    }

    pub const fn is_office(self) -> bool {
        matches!(
            self,
            Format::Doc
                | Format::Docx
                | Format::Odt
                | Format::Rtf
                | Format::Xlsx
                | Format::Xls
                | Format::Ods
                | Format::Csv
                | Format::Pptx
                | Format::Ppt
                | Format::Odp
        )
    }

    /// Whether a lossy quality knob makes sense.
    pub const fn supports_quality(self) -> bool {
        matches!(
            self,
            Format::Jpeg
                | Format::Webp
                | Format::Avif
                | Format::Jxl
                | Format::Heic
                | Format::Mp4
                | Format::Webm
                | Format::Mkv
                | Format::Mov
                | Format::Avi
                | Format::Mp3
                | Format::Ogg
                | Format::Flac
                | Format::Aac
                | Format::M4a
                | Format::Opus
                | Format::Gif
        )
    }

    /// Whether the format keeps an alpha channel.
    pub const fn supports_alpha(self) -> bool {
        matches!(
            self,
            Format::Png
                | Format::Webp
                | Format::Gif
                | Format::Tiff
                | Format::Ico
                | Format::Tga
                | Format::Qoi
                | Format::Avif
                | Format::Exr
                | Format::Farbfeld
        )
    }

    /// Sensible default quality for lossy targets.
    pub const fn default_quality(self) -> u8 {
        match self {
            Format::Jpeg => 90,
            Format::Webp => 88,
            Format::Avif | Format::Jxl => 80,
            Format::Gif => 90,
            Format::Mp3 | Format::Ogg | Format::Opus => 90,
            _ => 90,
        }
    }

    /// Formats offered by the format picker for this source format.
    pub fn targets_from(self) -> Vec<Format> {
        use Format::*;
        let images = vec![
            Png, Jpeg, Webp, Avif, Gif, Bmp, Tiff, Ico, Qoi, Tga, Pnm, Hdr, Exr, Farbfeld, Jxl,
        ];
        let mut out = match self {
            f if f.is_raster() => images,
            Svg => images,
            Pdf => vec![Png, Jpeg, Webp, Tiff, Bmp, Avif, Txt, Svg],
            f if f.is_office() => vec![Pdf, Png, Jpeg, Webp, Txt, Docx, Odt, Html, Csv],
            Txt => vec![Pdf, Png, Jpeg, Webp, Txt, Html, Markdown, Docx, Odt],
            Markdown => vec![Pdf, Png, Jpeg, Webp, Html, Txt, Docx, Odt],
            Html => vec![Pdf, Png, Jpeg, Webp, Txt, Docx, Odt, Markdown],
            f if f.is_video() => vec![
                Mp4, Webm, Mkv, Mov, Avi, Gif, Mp3, Wav, Ogg, Flac, Aac, M4a, Opus, Png,
            ],
            f if f.is_audio() => vec![Mp3, Wav, Ogg, Flac, Aac, M4a, Opus],
            _ => images,
        };
        // always allow keeping the current format (copy / re-encode)
        if !out.contains(&self) {
            out.push(self);
        }
        out.sort();
        out.dedup();
        out
    }

    /// MIME type used by `inspect --json`.
    pub const fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Webp => "image/webp",
            Format::Gif => "image/gif",
            Format::Bmp => "image/bmp",
            Format::Tiff => "image/tiff",
            Format::Ico => "image/x-icon",
            Format::Tga => "image/x-tga",
            Format::Pnm => "image/x-portable-anymap",
            Format::Qoi => "image/qoi",
            Format::Hdr => "image/vnd.radiance",
            Format::Exr => "image/x-exr",
            Format::Farbfeld => "image/x-farbfeld",
            Format::Avif => "image/avif",
            Format::Heic => "image/heic",
            Format::Jxl => "image/jxl",
            Format::Svg => "image/svg+xml",
            Format::Pdf => "application/pdf",
            Format::Doc => "application/msword",
            Format::Docx => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            Format::Odt => "application/vnd.oasis.opendocument.text",
            Format::Rtf => "application/rtf",
            Format::Txt => "text/plain",
            Format::Markdown => "text/markdown",
            Format::Html => "text/html",
            Format::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Format::Xls => "application/vnd.ms-excel",
            Format::Ods => "application/vnd.oasis.opendocument.spreadsheet",
            Format::Csv => "text/csv",
            Format::Pptx => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
            Format::Ppt => "application/vnd.ms-powerpoint",
            Format::Odp => "application/vnd.oasis.opendocument.presentation",
            Format::Mp4 => "video/mp4",
            Format::Webm => "video/webm",
            Format::Mkv => "video/x-matroska",
            Format::Mov => "video/quicktime",
            Format::Avi => "video/x-msvideo",
            Format::Mp3 => "audio/mpeg",
            Format::Wav => "audio/wav",
            Format::Ogg => "audio/ogg",
            Format::Flac => "audio/flac",
            Format::Aac => "audio/aac",
            Format::M4a => "audio/mp4",
            Format::Opus => "audio/opus",
        }
    }

    /// Best-effort conversion target used when the user gives no `--to` /
    /// output extension.
    pub const fn fallback_target(self) -> Format {
        match self.category() {
            Category::Image | Category::Vector => Format::Png,
            Category::Document => Format::Pdf,
            Category::Text => Format::Pdf,
            Category::Media => {
                if self.is_audio() {
                    Format::Mp3
                } else {
                    Format::Mp4
                }
            }
        }
    }

    /// Look up a format from an extension (with common aliases).
    pub fn from_extension(ext: &str) -> Option<Format> {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        Some(match ext.as_str() {
            "png" | "apng" => Format::Png,
            "jpg" | "jpeg" | "jpe" | "jfif" => Format::Jpeg,
            "webp" => Format::Webp,
            "gif" => Format::Gif,
            "bmp" | "dib" => Format::Bmp,
            "tif" | "tiff" => Format::Tiff,
            "ico" => Format::Ico,
            "tga" | "icb" | "vda" | "vst" => Format::Tga,
            "pnm" | "pbm" | "pgm" | "ppm" | "pam" => Format::Pnm,
            "qoi" => Format::Qoi,
            "hdr" | "rgbe" => Format::Hdr,
            "exr" => Format::Exr,
            "ff" | "farbfeld" => Format::Farbfeld,
            "avif" => Format::Avif,
            "heic" | "heif" => Format::Heic,
            "jxl" => Format::Jxl,
            "svg" | "svgz" => Format::Svg,
            "pdf" => Format::Pdf,
            "doc" => Format::Doc,
            "docx" => Format::Docx,
            "odt" => Format::Odt,
            "rtf" => Format::Rtf,
            "txt" | "text" | "log" | "nfo" => Format::Txt,
            "md" | "markdown" | "mdown" | "mkd" => Format::Markdown,
            "html" | "htm" | "xhtml" => Format::Html,
            "xlsx" => Format::Xlsx,
            "xls" => Format::Xls,
            "ods" => Format::Ods,
            "csv" | "tsv" => Format::Csv,
            "pptx" => Format::Pptx,
            "ppt" => Format::Ppt,
            "odp" => Format::Odp,
            "mp4" | "m4v" => Format::Mp4,
            "webm" => Format::Webm,
            "mkv" => Format::Mkv,
            "mov" | "qt" => Format::Mov,
            "avi" => Format::Avi,
            "mp3" => Format::Mp3,
            "wav" | "wave" => Format::Wav,
            "ogg" | "oga" => Format::Ogg,
            "flac" => Format::Flac,
            "aac" | "m4b" => Format::Aac,
            "m4a" => Format::M4a,
            "opus" => Format::Opus,
            _ => return None,
        })
    }

    /// Detect a format from a path. The extension wins; if it is missing or
    /// unknown the file header is inspected.
    pub fn from_path(path: &Path) -> Result<Format> {
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && let Some(fmt) = Format::from_extension(ext)
        {
            return Ok(fmt);
        }
        detect_magic(path).ok_or_else(|| Error::UnknownFormat(path.display().to_string()))
    }

    /// Human friendly list for `sorce formats`.
    pub fn all() -> Vec<Format> {
        use Format::*;
        vec![
            Png, Jpeg, Webp, Gif, Bmp, Tiff, Ico, Tga, Pnm, Qoi, Hdr, Exr, Farbfeld, Avif, Heic,
            Jxl, Svg, Pdf, Doc, Docx, Odt, Rtf, Txt, Markdown, Html, Xlsx, Xls, Ods, Csv, Pptx,
            Ppt, Odp, Mp4, Webm, Mkv, Mov, Avi, Mp3, Wav, Ogg, Flac, Aac, M4a, Opus,
        ]
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl FromStr for Format {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let cleaned = s.trim().trim_start_matches('.').to_ascii_lowercase();
        Format::from_extension(&cleaned)
            .or_else(|| {
                // also accept display names like "jpeg", "markdown"
                Format::all()
                    .into_iter()
                    .find(|f| f.label().eq_ignore_ascii_case(&cleaned))
            })
            .ok_or_else(|| Error::InvalidOption(format!("unknown format `{s}`")))
    }
}

/// Sniff a file header. Returns `None` when the format is not recognised.
pub fn detect_magic(path: &Path) -> Option<Format> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; 8192];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    detect_bytes(&buf)
}

/// Header sniffing on a byte slice.
pub fn detect_bytes(b: &[u8]) -> Option<Format> {
    if b.len() >= 8 && b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Format::Png);
    }
    if b.len() >= 3 && b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF {
        return Some(Format::Jpeg);
    }
    if b.len() >= 6 && (b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")) {
        return Some(Format::Gif);
    }
    if b.len() >= 12 && b.starts_with(b"RIFF") && &b[8..12] == b"WEBP" {
        return Some(Format::Webp);
    }
    if b.len() >= 12 && b.starts_with(b"RIFF") && &b[8..12] == b"WAVE" {
        return Some(Format::Wav);
    }
    if b.len() >= 12 && b.starts_with(b"RIFF") && &b[8..12] == b"AVI " {
        return Some(Format::Avi);
    }
    if b.len() >= 4 && b.starts_with(b"%PDF") {
        return Some(Format::Pdf);
    }
    if b.len() >= 2 && b.starts_with(b"BM") {
        return Some(Format::Bmp);
    }
    if b.len() >= 4 && (b.starts_with(b"II*\0") || b.starts_with(b"MM\0*")) {
        return Some(Format::Tiff);
    }
    if b.len() >= 4 && b.starts_with(&[0, 0, 1, 0]) {
        return Some(Format::Ico);
    }
    if b.len() >= 4 && b.starts_with(b"qoif") {
        return Some(Format::Qoi);
    }
    if b.len() >= 4 && b.starts_with(&[0x76, 0x2F, 0x31, 0x01]) {
        return Some(Format::Exr);
    }
    if b.len() >= 10 && (b.starts_with(b"#?RADIANCE") || b.starts_with(b"#?RGBE")) {
        return Some(Format::Hdr);
    }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        let brand = &b[8..12];
        return Some(match brand {
            b"avif" | b"avis" => Format::Avif,
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1" => Format::Heic,
            b"qt  " => Format::Mov,
            b"M4A " | b"M4B " => Format::M4a,
            _ => Format::Mp4,
        });
    }
    if b.len() >= 12 && b.starts_with(&[0x00, 0x00, 0x00, 0x0C]) && &b[4..12] == b"JXL \r\n\x87\n" {
        return Some(Format::Jxl);
    }
    if b.len() >= 2 && b.starts_with(&[0xFF, 0x0A]) {
        return Some(Format::Jxl);
    }
    if b.len() >= 4 && b.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        let hay = String::from_utf8_lossy(&b[..b.len().min(256)]);
        if hay.contains("webm") {
            return Some(Format::Webm);
        }
        return Some(Format::Mkv);
    }
    if b.len() >= 4 && b.starts_with(b"OggS") {
        let hay = &b[..b.len().min(128)];
        if hay.windows(8).any(|w| w == b"OpusHead") {
            return Some(Format::Opus);
        }
        return Some(Format::Ogg);
    }
    if b.len() >= 4 && b.starts_with(b"fLaC") {
        return Some(Format::Flac);
    }
    if b.len() >= 3 && b.starts_with(b"ID3") {
        return Some(Format::Mp3);
    }
    if b.len() >= 2 && b[0] == 0xFF && (b[1] & 0xE0) == 0xE0 {
        return Some(Format::Mp3);
    }
    if b.len() >= 2 && b[0] == 0xFF && (b[1] == 0xF1 || b[1] == 0xF9) {
        return Some(Format::Aac);
    }
    if b.len() >= 5 && b.starts_with(b"{\\rtf") {
        return Some(Format::Rtf);
    }

    // Zip based office formats. Extensions normally win, this is a fallback.
    if b.len() >= 4 && b.starts_with(b"PK\x03\x04") {
        let hay = String::from_utf8_lossy(b).to_ascii_lowercase();
        if hay.contains("word/document.xml") {
            return Some(Format::Docx);
        }
        if hay.contains("xl/workbook.xml") {
            return Some(Format::Xlsx);
        }
        if hay.contains("ppt/presentation.xml") {
            return Some(Format::Pptx);
        }
        if hay.contains("application/vnd.oasis.opendocument.text") {
            return Some(Format::Odt);
        }
        if hay.contains("application/vnd.oasis.opendocument.spreadsheet") {
            return Some(Format::Ods);
        }
        if hay.contains("application/vnd.oasis.opendocument.presentation") {
            return Some(Format::Odp);
        }
        return None;
    }

    // Text based formats.
    let text = String::from_utf8_lossy(&b[..b.len().min(4096)]);
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("<!doctype html") || lower.starts_with("<html") {
        return Some(Format::Html);
    }
    if lower.starts_with("<?xml") && lower.contains("<svg") || lower.starts_with("<svg") {
        return Some(Format::Svg);
    }
    if lower.starts_with("<svg") {
        return Some(Format::Svg);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_roundtrip() {
        for fmt in Format::all() {
            assert_eq!(Format::from_extension(fmt.extension()), Some(fmt));
        }
    }

    #[test]
    fn aliases_work() {
        assert_eq!(Format::from_extension("jpeg"), Some(Format::Jpeg));
        assert_eq!(Format::from_extension(".tif"), Some(Format::Tiff));
        assert_eq!(Format::from_extension("MD"), Some(Format::Markdown));
        assert_eq!(Format::from_str("jpg").unwrap(), Format::Jpeg);
        assert_eq!(Format::from_str("JPEG").unwrap(), Format::Jpeg);
        assert!(Format::from_str("nope").is_err());
    }

    #[test]
    fn magic_png() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(detect_bytes(&png), Some(Format::Png));
    }

    #[test]
    fn targets_include_self() {
        for fmt in Format::all() {
            assert!(fmt.targets_from().contains(&fmt), "{fmt} missing");
        }
    }
}
