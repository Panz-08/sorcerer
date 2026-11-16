//! A small, dependency-free PDF writer.
//!
//! Sorcerer ships its own PDF writer so that converting images and plain text
//! to PDF works out of the box, without LaTeX, LibreOffice or a browser.  It
//! supports:
//!
//! * one image per page (JPEG passthrough or lossless Flate compression)
//! * transparent images through a PDF soft mask (`/SMask`)
//! * A4 / Letter / image-sized pages with margins and automatic orientation
//! * multi-page plain text documents using the standard Helvetica font
//! * document metadata (`/Title`, `/Producer`, `/Creator`)

use std::io::Write;
use std::path::Path;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use image::{DynamicImage, ExtendedColorType, ImageEncoder, RgbaImage};
use tempfile::NamedTempFile;

use super::error::{Error, Result};
use super::image_ops;
use super::options::PageSize;
use super::progress::{Progress, ProgressFn};

/// Options for image → PDF.
#[derive(Debug, Clone)]
pub struct PdfOptions {
    pub page_size: PageSize,
    pub margin_mm: f32,
    pub dpi: u32,
    pub quality: u8,
    pub background: [u8; 4],
    pub title: String,
    pub overwrite: bool,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            page_size: PageSize::Fit,
            margin_mm: 12.0,
            dpi: 96,
            quality: 90,
            background: [255, 255, 255, 255],
            title: "Sorcerer document".to_string(),
            overwrite: false,
        }
    }
}

/// One image destined for a PDF page.
pub struct PdfImage {
    pub image: DynamicImage,
    /// Use JPEG (DCT) compression instead of Flate, useful for photos.
    pub jpeg_source: bool,
}

impl PdfImage {
    pub fn new(image: DynamicImage, jpeg_source: bool) -> Self {
        Self { image, jpeg_source }
    }
}

/// Write several images as a multi-page PDF.
pub fn images_to_pdf(
    images: &[PdfImage],
    output: &Path,
    opts: &PdfOptions,
    progress: &ProgressFn,
) -> Result<()> {
    if images.is_empty() {
        return Err(Error::message("no images to write into the PDF"));
    }
    if output.exists() && !opts.overwrite {
        return Err(Error::Exists(output.to_path_buf()));
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    let mut pages = Vec::with_capacity(images.len());
    let total = images.len() as u32;
    for (idx, input) in images.iter().enumerate() {
        progress(Progress::stage(
            format!("preparing page {}", idx + 1),
            0.05 + 0.75 * (idx as f32 / total as f32),
        ));
        pages.push(prepare_image_page(input, opts)?);
        progress(Progress::page(idx as u32 + 1, total));
    }

    progress(Progress::stage("writing PDF", 0.85));
    let bytes = build_pdf(&pages, &opts.title, None);
    write_atomic(output, &bytes, opts.overwrite)?;
    progress(Progress::stage("done", 1.0));
    Ok(())
}

/// Write plain text as an A4 PDF with word wrapping.
pub fn text_to_pdf(
    text: &str,
    output: &Path,
    title: &str,
    overwrite: bool,
    progress: &ProgressFn,
) -> Result<()> {
    if output.exists() && !overwrite {
        return Err(Error::Exists(output.to_path_buf()));
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    progress(Progress::stage("laying out text", 0.3));
    let pages = layout_text(text);
    progress(Progress::stage("writing PDF", 0.8));
    let bytes = build_pdf(&pages, title, Some((10.5, 14.5)));
    write_atomic(output, &bytes, overwrite)?;
    progress(Progress::stage("done", 1.0));
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8], overwrite: bool) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = NamedTempFile::new_in(parent).map_err(|e| Error::io(parent, e))?;
    temp.as_file_mut()
        .write_all(bytes)
        .map_err(|e| Error::io(path, e))?;
    temp.as_file_mut()
        .sync_all()
        .map_err(|e| Error::io(path, e))?;
    let persisted = if overwrite {
        temp.persist(path)
    } else {
        temp.persist_noclobber(path)
    };
    persisted.map(|_| ()).map_err(|e| Error::io(path, e.error))
}

// ---------------------------------------------------------------------------
// Page construction
// ---------------------------------------------------------------------------

struct ImagePage {
    data: Vec<u8>,
    filter: &'static str,
    width: u32,
    height: u32,
    smask: Option<Smask>,
    page_width: f32,
    page_height: f32,
    draw_width: f32,
    draw_height: f32,
    x: f32,
    y: f32,
    fill: Option<[u8; 3]>,
}

struct Smask {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

fn prepare_image_page(input: &PdfImage, opts: &PdfOptions) -> Result<ImagePage> {
    let image = &input.image;
    let (page_w, page_h, draw_w, draw_h, x, y) = page_geometry(
        image.width(),
        image.height(),
        opts.page_size,
        opts.dpi.max(1),
        opts.margin_mm,
    );

    let rgba = image.to_rgba8();
    let has_alpha = rgba.pixels().any(|p| p[3] < 255);

    let (data, filter, smask) = if input.jpeg_source && !has_alpha {
        let rgb = image.to_rgb8();
        let mut jpeg = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut jpeg,
            opts.quality.clamp(1, 100),
        );
        encoder
            .write_image(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                ExtendedColorType::Rgb8,
            )
            .map_err(Error::Image)?;
        (jpeg, "DCTDecode", None)
    } else {
        let rgb = image.to_rgb8();
        let compressed = deflate(rgb.as_raw())?;
        let mask = if has_alpha {
            let alpha = alpha_channel(&rgba);
            Some(Smask {
                data: deflate(&alpha)?,
                width: rgba.width(),
                height: rgba.height(),
            })
        } else {
            None
        };
        (compressed, "FlateDecode", mask)
    };

    let bg = opts.background;
    let fill =
        (bg[3] > 0 && (bg[0], bg[1], bg[2]) != (255, 255, 255)).then_some([bg[0], bg[1], bg[2]]);

    Ok(ImagePage {
        data,
        filter,
        width: image.width().max(1),
        height: image.height().max(1),
        smask,
        page_width: page_w,
        page_height: page_h,
        draw_width: draw_w,
        draw_height: draw_h,
        x,
        y,
        fill,
    })
}

fn page_geometry(
    img_w: u32,
    img_h: u32,
    page_size: PageSize,
    dpi: u32,
    margin_mm: f32,
) -> (f32, f32, f32, f32, f32, f32) {
    let img_w_f = img_w.max(1) as f32;
    let img_h_f = img_h.max(1) as f32;

    match page_size.points() {
        None => {
            let w = img_w_f * 72.0 / dpi as f32;
            let h = img_h_f * 72.0 / dpi as f32;
            (w, h, w, h, 0.0, 0.0)
        }
        Some((pw, ph)) => {
            let (pw, ph) = if page_size == PageSize::A4Auto && img_w_f > img_h_f {
                (ph, pw)
            } else {
                (pw, ph)
            };
            let margin = (margin_mm.max(0.0) * 72.0 / 25.4).min(pw.min(ph) / 3.0);
            let avail_w = (pw - 2.0 * margin).max(1.0);
            let avail_h = (ph - 2.0 * margin).max(1.0);
            let scale = (avail_w / img_w_f).min(avail_h / img_h_f);
            let draw_w = (img_w_f * scale).min(avail_w);
            let draw_h = (img_h_f * scale).min(avail_h);
            let x = (pw - draw_w) / 2.0;
            let y = (ph - draw_h) / 2.0;
            (pw, ph, draw_w, draw_h, x, y)
        }
    }
}

fn deflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| Error::message(format!("PDF compression failed: {e}")))?;
    encoder
        .finish()
        .map_err(|e| Error::message(format!("PDF compression failed: {e}")))
}

fn alpha_channel(rgba: &RgbaImage) -> Vec<u8> {
    rgba.pixels().map(|p| p[3]).collect()
}

// ---------------------------------------------------------------------------
// Text layout
// ---------------------------------------------------------------------------

fn layout_text(text: &str) -> Vec<ImagePage> {
    // A4, 20mm margins, Helvetica 10.5pt, 14.5pt leading.
    let page_w = 595.276_f32;
    let page_h = 841.890_f32;
    let margin = 20.0 * 72.0 / 25.4;
    let font_size = 10.5_f32;
    let leading = 14.5_f32;
    let usable_w = page_w - 2.0 * margin;
    let usable_h = page_h - 2.0 * margin;
    let max_chars = (usable_w / (font_size * 0.5)).floor() as usize;
    let max_lines = (usable_h / leading).floor() as usize;

    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        let expanded = raw.replace('\t', "    ").replace('\r', "");
        if expanded.trim().is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(wrap_line(&expanded, max_chars.max(10)));
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }

    let mut pages = Vec::new();
    for chunk in lines.chunks(max_lines.max(1)) {
        let mut content = String::new();
        content.push_str("BT\n");
        content.push_str(&format!("/F1 {font_size} Tf\n"));
        content.push_str(&format!("{leading} TL\n"));
        let first_y = page_h - margin - font_size;
        content.push_str(&format!("1 0 0 1 {margin:.3} {first_y:.3} Tm\n"));
        for line in chunk {
            if line.is_empty() {
                content.push_str("T*\n");
            } else {
                content.push_str(&format!("({}) Tj\nT*\n", pdf_escape(line)));
            }
        }
        content.push_str("ET\n");
        pages.push(ImagePage {
            data: content.into_bytes(),
            filter: "",
            width: 0,
            height: 0,
            smask: None,
            page_width: page_w,
            page_height: page_h,
            draw_width: 0.0,
            draw_height: 0.0,
            x: 0.0,
            y: 0.0,
            fill: None,
        });
    }
    pages
}

fn wrap_line(line: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for word in line.split_whitespace() {
        if word.chars().count() > max_chars {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            let mut chunk = String::new();
            for ch in word.chars() {
                chunk.push(ch);
                if chunk.chars().count() >= max_chars {
                    out.push(std::mem::take(&mut chunk));
                }
            }
            if !chunk.is_empty() {
                current = chunk;
            }
            continue;
        }
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
            current.push(' ');
            current.push_str(word);
        } else {
            out.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn pdf_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        let code = ch as u32;
        match ch {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            _ if (32..=126).contains(&code) => out.push(ch),
            _ if code <= 255 => out.push_str(&format!("\\{code:03o}")),
            _ => out.push('?'),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// PDF object writer
// ---------------------------------------------------------------------------

fn build_pdf(pages: &[ImagePage], title: &str, text_font: Option<(f32, f32)>) -> Vec<u8> {
    let mut writer = PdfBuilder::new();

    let catalog_id = writer.alloc();
    let pages_id = writer.alloc();
    let font_id = writer.alloc();
    let info_id = writer.alloc();

    struct Ids {
        page: u32,
        content: u32,
        image: Option<u32>,
        smask: Option<u32>,
    }

    let mut ids = Vec::with_capacity(pages.len());
    let mut is_text = Vec::with_capacity(pages.len());
    for page in pages {
        let page_id = writer.alloc();
        let content_id = writer.alloc();
        let (image_id, smask_id) = if page.width == 0 {
            (None, None)
        } else {
            let image_id = writer.alloc();
            let smask_id = page.smask.as_ref().map(|_| writer.alloc());
            (Some(image_id), smask_id)
        };
        is_text.push(page.width == 0);
        ids.push(Ids {
            page: page_id,
            content: content_id,
            image: image_id,
            smask: smask_id,
        });
    }

    // Images and masks.
    for (page, id) in pages.iter().zip(ids.iter()) {
        if let Some(image_id) = id.image {
            let dict = format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /{} /Length {}{} >>",
                page.width,
                page.height,
                page.filter,
                page.data.len(),
                id.smask
                    .map(|s| format!(" /SMask {s} 0 R"))
                    .unwrap_or_default()
            );
            writer.stream(image_id, &dict, &page.data);
        }
        if let (Some(smask), Some(smask_id)) = (&page.smask, id.smask) {
            let dict = format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>",
                smask.width,
                smask.height,
                smask.data.len()
            );
            writer.stream(smask_id, &dict, &smask.data);
        }
    }

    // Content streams.
    for (page, id) in pages.iter().zip(ids.iter()) {
        let content = if page.width == 0 {
            page.data.clone()
        } else {
            let mut content = Vec::new();
            if let Some(fill) = page.fill {
                let color = format!(
                    "{:.3} {:.3} {:.3} rg\n0 0 {:.3} {:.3} re f\n",
                    fill[0] as f32 / 255.0,
                    fill[1] as f32 / 255.0,
                    fill[2] as f32 / 255.0,
                    page.page_width,
                    page.page_height
                );
                content.extend_from_slice(color.as_bytes());
            }
            let draw = format!(
                "q\n{:.3} 0 0 {:.3} {:.3} {:.3} cm\n/Im0 Do\nQ\n",
                page.draw_width, page.draw_height, page.x, page.y
            );
            content.extend_from_slice(draw.as_bytes());
            content
        };
        let dict = format!("<< /Length {} >>", content.len());
        writer.stream(id.content, &dict, &content);
    }

    // Page objects.
    for (page, id) in pages.iter().zip(ids.iter()) {
        let resources = if page.width == 0 {
            format!("/Font << /F1 {font_id} 0 R >>")
        } else {
            format!(
                "/XObject << /Im0 {} 0 R >> /ProcSet [/PDF /ImageC]",
                id.image.unwrap()
            )
        };
        let body = format!(
            "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {:.3} {:.3}] /Resources << {} >> /Contents {} 0 R >>",
            page.page_width, page.page_height, resources, id.content
        );
        writer.object(id.page, body.as_bytes());
    }

    let kids = ids
        .iter()
        .map(|id| format!("{} 0 R", id.page))
        .collect::<Vec<_>>()
        .join(" ");
    writer.object(
        pages_id,
        format!("<< /Type /Pages /Count {} /Kids [{}] >>", ids.len(), kids).as_bytes(),
    );
    writer.object(
        catalog_id,
        format!("<< /Type /Catalog /Pages {pages_id} 0 R >>").as_bytes(),
    );

    // Standard font. Helvetica is good enough for plain text documents.
    let (font_size, leading) = text_font.unwrap_or((10.5, 14.5));
    let _ = (font_size, leading);
    writer.object(
        font_id,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );

    let producer = format!("Sorcerer {} by Panz-08", crate::VERSION);
    writer.object(
        info_id,
        format!(
            "<< /Title ({}) /Producer ({}) /Creator (Sorcerer · Panz-08) >>",
            pdf_escape(title),
            pdf_escape(&producer)
        )
        .as_bytes(),
    );

    writer.finish(catalog_id, info_id)
}

struct PdfBuilder {
    output: Vec<u8>,
    offsets: Vec<usize>,
    next_id: u32,
}

impl PdfBuilder {
    fn new() -> Self {
        let mut output = Vec::new();
        output.extend_from_slice(b"%PDF-1.7\n");
        output.extend_from_slice(&[b'%', 0xE2, 0xE3, 0xCF, 0xD3, b'\n']);
        Self {
            output,
            offsets: vec![0],
            next_id: 1,
        }
    }

    fn alloc(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.offsets.push(0);
        id
    }

    fn object(&mut self, id: u32, body: &[u8]) {
        self.offsets[id as usize] = self.output.len();
        self.output
            .extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        self.output.extend_from_slice(body);
        self.output.extend_from_slice(b"\nendobj\n");
    }

    fn stream(&mut self, id: u32, dict: &str, data: &[u8]) {
        self.offsets[id as usize] = self.output.len();
        self.output
            .extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        self.output.extend_from_slice(dict.as_bytes());
        self.output.extend_from_slice(b"\nstream\n");
        self.output.extend_from_slice(data);
        self.output.extend_from_slice(b"\nendstream\nendobj\n");
    }

    fn finish(self, root: u32, info: u32) -> Vec<u8> {
        let mut out = self.output;
        let xref_start = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", self.next_id).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for id in 1..self.next_id {
            out.extend_from_slice(
                format!("{:010} 00000 n \n", self.offsets[id as usize]).as_bytes(),
            );
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root {root} 0 R /Info {info} 0 R >>\nstartxref\n{xref_start}\n%%EOF\n",
                self.next_id
            )
            .as_bytes(),
        );
        out
    }
}

/// Convert any image to an RGB PDF-compatible page (helper for tests / TUI).
pub fn image_page_size(img: &DynamicImage, dpi: u32) -> (f32, f32) {
    (
        img.width() as f32 * 72.0 / dpi.max(1) as f32,
        img.height() as f32 * 72.0 / dpi.max(1) as f32,
    )
}

/// Render an image with a chosen background for PDF embedding.
pub fn prepare_image(img: DynamicImage, background: [u8; 4]) -> DynamicImage {
    if img.color().has_alpha() {
        image_ops::flatten(img, background)
    } else {
        img
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_escape_handles_specials() {
        assert_eq!(pdf_escape("a(b)c\\d"), "a\\(b\\)c\\\\d");
        assert_eq!(pdf_escape("é"), "\\351");
    }

    #[test]
    fn wrap_breaks_long_lines() {
        let lines = wrap_line("hello wonderful world", 10);
        assert!(lines.len() >= 2);
        assert!(lines.iter().all(|l| l.chars().count() <= 10));
    }

    #[test]
    fn builds_a_valid_header() {
        let pages = layout_text("hello");
        let bytes = build_pdf(&pages, "test", None);
        assert!(bytes.starts_with(b"%PDF-1.7"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        assert!(bytes.windows(5).any(|w| w == b"/Type"));
    }
}
