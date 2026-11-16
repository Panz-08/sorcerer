//! Conversion routing and orchestration.

use std::path::{Path, PathBuf};

use super::docs;
use super::error::{Error, Result};
use super::format::{Category, Format};
use super::image_ops;
use super::media;
use super::options::ConvertOptions;
use super::pdf_render;
use super::pdf_write::{self, PdfImage, PdfOptions};
use super::progress::{Progress, ProgressFn};
use super::svg;
use crate::util::paths;

/// The strategy chosen for a source/target pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    RasterToRaster,
    RasterToPdf,
    SvgToRaster,
    SvgToPdf,
    SvgCopy,
    PdfToRaster,
    PdfToText,
    PdfToSvg,
    PdfToPdf,
    DocToPdf,
    DocToRaster,
    DocToText,
    DocToDoc,
    TextToPdf,
    TextToRaster,
    TextToText,
    TextToOffice,
    MediaToMedia,
    MediaToImage,
}

impl Route {
    pub fn label(self) -> &'static str {
        match self {
            Route::RasterToRaster => "raster → raster",
            Route::RasterToPdf => "image → PDF",
            Route::SvgToRaster => "SVG → raster",
            Route::SvgToPdf => "SVG → PDF",
            Route::SvgCopy => "SVG → SVG (copy)",
            Route::PdfToRaster => "PDF → raster",
            Route::PdfToText => "PDF → text",
            Route::PdfToSvg => "PDF → SVG",
            Route::PdfToPdf => "PDF → PDF",
            Route::DocToPdf => "document → PDF",
            Route::DocToRaster => "document → raster",
            Route::DocToText => "document → text",
            Route::DocToDoc => "document → document",
            Route::TextToPdf => "text → PDF",
            Route::TextToRaster => "text → raster",
            Route::TextToText => "text → text",
            Route::TextToOffice => "text → office document",
            Route::MediaToMedia => "media → media",
            Route::MediaToImage => "video → image",
        }
    }
}

/// Decide how to convert `source` into `target` (or explain why we cannot).
pub fn route(source: Format, target: Format) -> Result<Route> {
    if source == Format::Svg {
        return match target {
            Format::Svg => Ok(Route::SvgCopy),
            Format::Pdf => Ok(Route::SvgToPdf),
            t if t.is_raster() => Ok(Route::SvgToRaster),
            _ => Err(unsupported(source, target)),
        };
    }
    if source == Format::Pdf {
        return match target {
            Format::Pdf => Ok(Route::PdfToPdf),
            Format::Txt => Ok(Route::PdfToText),
            Format::Svg => Ok(Route::PdfToSvg),
            t if t.is_raster() => Ok(Route::PdfToRaster),
            _ => Err(unsupported_hint(
                source,
                target,
                "PDF → office is not supported; convert to images or text first",
            )),
        };
    }
    if source.is_raster() {
        return match target {
            t if t.is_raster() => Ok(Route::RasterToRaster),
            Format::Pdf => Ok(Route::RasterToPdf),
            Format::Svg => Err(unsupported_hint(
                source,
                target,
                "raster → vector tracing is not built in; use a PNG/JPEG output instead",
            )),
            _ => Err(unsupported(source, target)),
        };
    }
    if source.is_office() {
        return match target {
            Format::Pdf => Ok(Route::DocToPdf),
            t if t.is_raster() => Ok(Route::DocToRaster),
            Format::Txt | Format::Csv => Ok(Route::DocToText),
            t if t.is_office() || t == Format::Html => Ok(Route::DocToDoc),
            _ => Err(unsupported(source, target)),
        };
    }
    if source.is_text() {
        return match target {
            Format::Pdf => Ok(Route::TextToPdf),
            t if t.is_raster() => Ok(Route::TextToRaster),
            Format::Txt | Format::Markdown | Format::Html => Ok(Route::TextToText),
            t if t.is_office() => Ok(Route::TextToOffice),
            _ => Err(unsupported(source, target)),
        };
    }
    if source.is_media() {
        return match target {
            Format::Gif => Ok(Route::MediaToMedia),
            t if t.is_raster() => Ok(Route::MediaToImage),
            t if t.is_media() => Ok(Route::MediaToMedia),
            _ => Err(unsupported_hint(
                source,
                target,
                "extract a frame (PNG/JPEG) first, then convert that image",
            )),
        };
    }
    Err(unsupported(source, target))
}

fn unsupported(source: Format, target: Format) -> Error {
    Error::unsupported(source.label(), target.label())
}

fn unsupported_hint(source: Format, target: Format, hint: &str) -> Error {
    Error::unsupported_hint(source.label(), target.label(), hint)
}

/// Whether the source can reach the target at all.
pub fn can_convert(source: Format, target: Format) -> bool {
    route(source, target).is_ok()
}

/// Convert one file. `output` is the exact base path for the result; PDFs with
/// several pages expand to `name-001.ext`, `name-002.ext`, …
pub fn convert_file(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    opts.validate()?;
    if !input.exists() {
        return Err(Error::io(
            input,
            std::io::Error::new(std::io::ErrorKind::NotFound, "input file not found"),
        ));
    }
    let source = Format::from_path(input)?;
    let route = route(source, opts.target)?;

    // Allow `-o some-directory` from library users.
    let output = if output.is_dir() {
        paths::output_in_dir(input, output, opts.target)
    } else if output.extension().is_none() {
        paths::with_extension(output, opts.target)
    } else {
        output.to_path_buf()
    };

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    progress(Progress::stage(
        format!("{} → {}", source.label(), opts.target.label()),
        0.01,
    ));

    match route {
        Route::RasterToRaster => raster_to_raster(input, &output, source, opts, progress),
        Route::RasterToPdf => raster_to_pdf(input, &output, source, opts, progress),
        Route::SvgToRaster => svg_to_raster(input, &output, opts, progress),
        Route::SvgToPdf => svg_to_pdf(input, &output, opts, progress),
        Route::SvgCopy => copy_file(input, &output, opts.overwrite),
        Route::PdfToRaster | Route::PdfToText | Route::PdfToSvg => {
            pdf_render::render_pdf(input, &output, opts, progress)
        }
        Route::PdfToPdf => pdf_render::subset(input, &output, opts, progress),
        Route::DocToPdf => doc_to_pdf(input, &output, opts, progress),
        Route::DocToRaster => doc_to_raster(input, &output, opts, progress),
        Route::DocToText => doc_to_text(input, &output, opts, progress),
        Route::DocToDoc => doc_to_doc(input, &output, opts, progress),
        Route::TextToPdf => text_to_pdf(input, &output, source, opts, progress),
        Route::TextToRaster => text_to_raster(input, &output, source, opts, progress),
        Route::TextToText => {
            docs::convert_text(input, &output, source, opts.target, opts, progress)
        }
        Route::TextToOffice => {
            docs::text_to_office(input, source, opts.target, &output, opts, progress)
        }
        Route::MediaToMedia => media::convert_media(input, &output, source, opts, progress),
        Route::MediaToImage => media::convert_media(input, &output, source, opts, progress),
    }
}

fn load_transformed(
    input: &Path,
    source: Format,
    opts: &ConvertOptions,
) -> Result<image::DynamicImage> {
    let img = match source {
        Format::Svg => svg::render_svg(input, opts.dpi)?,
        _ => image_ops::load_image(input)?,
    };
    Ok(image_ops::transform(img, opts))
}

fn raster_to_raster(
    input: &Path,
    output: &Path,
    source: Format,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    progress(Progress::stage("decoding image", 0.15));
    let img = load_transformed(input, source, opts)?;
    progress(Progress::stage("encoding image", 0.65));
    image_ops::encode(&img, opts.target, output, opts, progress)?;
    Ok(vec![output.to_path_buf()])
}

fn raster_to_pdf(
    input: &Path,
    output: &Path,
    source: Format,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    progress(Progress::stage("decoding image", 0.1));
    let img = load_transformed(input, source, opts)?;
    let title = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Sorcerer document".to_string());
    let pdf_opts = PdfOptions {
        page_size: opts.page_size,
        margin_mm: opts.margin_mm,
        dpi: opts.dpi,
        quality: opts.quality,
        background: opts.background,
        title,
        overwrite: opts.overwrite,
    };
    pdf_write::images_to_pdf(
        &[PdfImage::new(img, source == Format::Jpeg)],
        output,
        &pdf_opts,
        progress,
    )?;
    Ok(vec![output.to_path_buf()])
}

fn svg_to_raster(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    progress(Progress::stage("rendering SVG", 0.25));
    let img = svg::render_svg(input, opts.dpi)?;
    let img = image_ops::transform(img, opts);
    image_ops::encode(&img, opts.target, output, opts, progress)?;
    Ok(vec![output.to_path_buf()])
}

fn svg_to_pdf(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    progress(Progress::stage("rendering SVG", 0.25));
    let img = svg::render_svg(input, opts.dpi)?;
    let img = image_ops::transform(img, opts);
    let title = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Sorcerer document".to_string());
    let pdf_opts = PdfOptions {
        page_size: opts.page_size,
        margin_mm: opts.margin_mm,
        dpi: opts.dpi,
        quality: opts.quality,
        background: opts.background,
        title,
        overwrite: opts.overwrite,
    };
    pdf_write::images_to_pdf(&[PdfImage::new(img, false)], output, &pdf_opts, progress)?;
    Ok(vec![output.to_path_buf()])
}

fn doc_to_pdf(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    progress(Progress::stage("converting with LibreOffice", 0.3));
    let pdf = docs::office_to_pdf(input)?;
    docs::move_file(&pdf, output, opts.overwrite)?;
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

fn doc_to_raster(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    let pdf = docs::office_to_pdf(input)?;
    let result = pdf_render::render_pdf(&pdf, output, opts, progress);
    let _ = std::fs::remove_file(&pdf);
    result
}

fn doc_to_text(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    progress(Progress::stage("converting with LibreOffice", 0.3));
    let workdir = tempfile::Builder::new()
        .prefix("sorcerer-doc-text-")
        .tempdir()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let produced = docs::office_convert(input, Format::Txt, workdir.path())?;
    docs::move_file(&produced, output, opts.overwrite)?;
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

fn doc_to_doc(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    progress(Progress::stage("converting with LibreOffice", 0.3));
    let workdir = tempfile::Builder::new()
        .prefix("sorcerer-doc-")
        .tempdir()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let produced = docs::office_convert(input, opts.target, workdir.path())?;
    docs::move_file(&produced, output, opts.overwrite)?;
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

fn text_to_pdf(
    input: &Path,
    output: &Path,
    source: Format,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    match source {
        Format::Html => docs::html_to_pdf(input, output, opts, progress),
        Format::Markdown => docs::markdown_to_pdf(input, output, opts, progress),
        _ => {
            progress(Progress::stage("laying out text", 0.3));
            let text = docs::read_text(input)?;
            let title = input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Document".to_string());
            pdf_write::text_to_pdf(&text, output, &title, opts.overwrite, progress)?;
            Ok(vec![output.to_path_buf()])
        }
    }
}

fn text_to_raster(
    input: &Path,
    output: &Path,
    source: Format,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    let tempdir = tempfile::Builder::new()
        .prefix("sorcerer-text-pdf-")
        .tempdir()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let pdf = tempdir.path().join("text.pdf");
    text_to_pdf(input, &pdf, source, opts, progress)?;
    let result = pdf_render::render_pdf(&pdf, output, opts, progress);
    drop(tempdir);
    result
}

fn copy_file(input: &Path, output: &Path, overwrite: bool) -> Result<Vec<PathBuf>> {
    if paths::same_path(input, output) {
        return Ok(vec![output.to_path_buf()]);
    }
    if output.exists() && !overwrite {
        return Err(Error::Exists(output.to_path_buf()));
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    std::fs::copy(input, output).map_err(|e| Error::io(output, e))?;
    Ok(vec![output.to_path_buf()])
}

/// Short description used by `--dry-run` and the TUI queue.
pub fn describe(source: Format, target: Format) -> String {
    match route(source, target) {
        Ok(route) => route.label().to_string(),
        Err(_) => format!("{} → {} (unsupported)", source.label(), target.label()),
    }
}

/// Category helper used by the TUI filter.
pub fn category_of(path: &Path) -> Option<Category> {
    Format::from_path(path).ok().map(|f| f.category())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_common_conversions() {
        assert_eq!(
            route(Format::Jpeg, Format::Png).unwrap(),
            Route::RasterToRaster
        );
        assert_eq!(route(Format::Png, Format::Pdf).unwrap(), Route::RasterToPdf);
        assert_eq!(route(Format::Pdf, Format::Png).unwrap(), Route::PdfToRaster);
        assert_eq!(route(Format::Pdf, Format::Txt).unwrap(), Route::PdfToText);
        assert_eq!(route(Format::Svg, Format::Png).unwrap(), Route::SvgToRaster);
        assert_eq!(route(Format::Docx, Format::Pdf).unwrap(), Route::DocToPdf);
        assert_eq!(
            route(Format::Mp4, Format::Gif).unwrap(),
            Route::MediaToMedia
        );
        assert_eq!(
            route(Format::Mp4, Format::Mp3).unwrap(),
            Route::MediaToMedia
        );
        assert!(route(Format::Png, Format::Docx).is_err());
    }
}
