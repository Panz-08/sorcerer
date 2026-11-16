//! PDF rendering and text extraction through Poppler, Ghostscript or MuPDF.

use std::path::{Path, PathBuf};

use super::backends;
use super::error::{Error, Result};
use super::format::Format;
use super::image_ops;
use super::options::{ConvertOptions, PageSelection};
use super::progress::{Progress, ProgressFn};
use crate::util::paths::sequence_path;

/// Basic information extracted from `pdfinfo`.
#[derive(Debug, Clone, Default)]
pub struct PdfInfo {
    pub pages: u32,
    pub title: Option<String>,
    pub producer: Option<String>,
    pub page_size: Option<String>,
    pub encrypted: bool,
}

/// Read PDF metadata; missing `pdfinfo` simply yields an empty result.
pub fn info(path: &Path) -> PdfInfo {
    let mut info = PdfInfo::default();
    let Some(pdfinfo) = backends::which("pdfinfo") else {
        return info;
    };
    let Ok(output) = backends::run(pdfinfo, [path.as_os_str()]) else {
        return info;
    };
    if !output.status.success() {
        return info;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Pages:") {
            info.pages = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix("Title:") {
            let value = value.trim();
            if !value.is_empty() {
                info.title = Some(value.to_string());
            }
        } else if let Some(value) = line.strip_prefix("Producer:") {
            let value = value.trim();
            if !value.is_empty() {
                info.producer = Some(value.to_string());
            }
        } else if let Some(value) = line.strip_prefix("Page size:") {
            info.page_size = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Encrypted:") {
            info.encrypted = value.trim().starts_with("yes");
        }
    }
    info
}

/// Page count, or `None` when no tool can tell us.
pub fn page_count(path: &Path) -> Option<u32> {
    let pages = info(path).pages;
    (pages > 0).then_some(pages)
}

/// Copy a PDF, or extract/merge a subset of pages into a new PDF.
pub fn subset(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    if output.exists() && !opts.overwrite {
        return Err(Error::Exists(output.to_path_buf()));
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    let total = page_count(input);
    let selected = total.map(|total| opts.pages.selected(total));
    let is_everything = opts.pages.is_all()
        || selected
            .as_ref()
            .zip(total)
            .map(|(pages, total)| pages.len() as u32 == total)
            .unwrap_or(false);

    if is_everything {
        progress(Progress::stage("copying PDF", 0.5));
        std::fs::copy(input, output).map_err(|e| Error::io(output, e))?;
        progress(Progress::stage("done", 1.0));
        return Ok(vec![output.to_path_buf()]);
    }

    let selected = selected.ok_or_else(|| {
        Error::message("cannot select PDF pages: install poppler-utils (pdfinfo/pdfseparate)")
    })?;
    if selected.is_empty() {
        return Err(Error::message("no pages selected"));
    }

    let (_, pdfseparate) = backends::require(
        &["pdfseparate"],
        "install poppler-utils: sudo apt install poppler-utils",
    )?;
    let (_, pdfunite) = backends::require(
        &["pdfunite"],
        "install poppler-utils: sudo apt install poppler-utils",
    )?;

    let workdir = tempfile::Builder::new()
        .prefix("sorcerer-subset-")
        .tempdir()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let pattern = workdir.path().join("page-%d.pdf");
    progress(Progress::stage("splitting PDF", 0.2));
    backends::run_checked(
        &pdfseparate,
        [
            input.as_os_str().to_os_string(),
            pattern.as_os_str().to_os_string(),
        ],
        "pdfseparate",
    )?;

    let mut parts = Vec::new();
    for page in &selected {
        let candidate = workdir.path().join(format!("page-{page}.pdf"));
        if !candidate.exists() {
            return Err(Error::message(format!(
                "page {page} was not produced by pdfseparate"
            )));
        }
        parts.push(candidate);
    }

    progress(Progress::stage("merging selected pages", 0.6));
    let mut args: Vec<std::ffi::OsString> =
        parts.iter().map(|p| p.as_os_str().to_os_string()).collect();
    args.push(output.as_os_str().to_os_string());
    backends::run_checked(&pdfunite, args, "pdfunite")?;
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

/// Render a PDF to raster images, SVG or text.
pub fn render_pdf(
    input: &Path,
    output_base: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    match opts.target {
        Format::Txt => extract_text(input, output_base, opts, progress),
        Format::Svg => render_svg(input, output_base, opts, progress),
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
        | Format::Jxl => render_raster(input, output_base, opts, progress),
        _ => Err(Error::unsupported("PDF", opts.target.label())),
    }
}

fn render_raster(
    input: &Path,
    output_base: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    progress(Progress::stage("reading PDF", 0.05));
    let total = page_count(input);
    let selected = match total {
        Some(total) => {
            let pages = opts.pages.selected(total);
            if pages.is_empty() {
                return Err(Error::message(format!(
                    "page selection `{}` matches nothing (PDF has {total} pages)",
                    opts.pages
                )));
            }
            pages
        }
        None => match &opts.pages {
            PageSelection::All => Vec::new(), // render everything and discover
            PageSelection::Ranges(_) => Vec::new(),
        },
    };

    let workdir = tempfile::Builder::new()
        .prefix("sorcerer-pdf-")
        .tempdir()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let prefix = workdir.path().join("page");

    let mut rendered = render_to_png(input, &prefix, &selected, opts.dpi, progress)?;
    if rendered.is_empty() {
        return Err(Error::message("PDF renderer produced no pages"));
    }

    let total_pages = total.unwrap_or_else(|| rendered.iter().map(|(p, _)| *p).max().unwrap_or(1));
    // Without `pdfinfo` we rendered everything; apply the requested selection
    // once we know how many pages actually exist.
    if total.is_none() {
        let wanted = opts.pages.selected(total_pages);
        let wanted: std::collections::HashSet<u32> = wanted.into_iter().collect();
        rendered.retain(|(page, _)| wanted.contains(page));
        if rendered.is_empty() {
            return Err(Error::message(format!(
                "page selection `{}` matches nothing (PDF has {total_pages} pages)",
                opts.pages
            )));
        }
    }

    let mut outputs = Vec::new();
    let count = rendered.len();
    for (index, (page_no, png)) in rendered.iter().enumerate() {
        let out = sequence_path(output_base, *page_no, total_pages);
        let label = format!("page {} of {}", page_no, total.unwrap_or(total_pages));
        progress(Progress::stage(
            label,
            0.2 + 0.75 * (index as f32 / count.max(1) as f32),
        ));
        let mut img = image_ops::load_image(png)?;
        img = image_ops::transform(img, opts);
        image_ops::encode(&img, opts.target, &out, opts, progress)?;
        outputs.push(out);
        progress(Progress::page(index as u32 + 1, count as u32));
    }
    progress(Progress::stage("done", 1.0));
    Ok(outputs)
}

/// Render selected pages (empty = all) to PNG files, returning page number and
/// the temporary file path.
fn render_to_png(
    input: &Path,
    prefix: &Path,
    selected: &[u32],
    dpi: u32,
    progress: &ProgressFn,
) -> Result<Vec<(u32, PathBuf)>> {
    if let Some(pdftoppm) = backends::which("pdftoppm") {
        let chunks = chunks_for(selected);
        let mut all = Vec::new();
        for (index, chunk) in chunks.iter().enumerate() {
            progress(Progress::stage(
                format!("rendering PDF at {dpi} dpi"),
                0.1 + 0.1 * (index as f32 / chunks.len().max(1) as f32),
            ));
            let mut args: Vec<std::ffi::OsString> =
                vec!["-r".into(), dpi.to_string().into(), "-png".into()];
            if let Some(start) = chunk.start {
                args.push("-f".into());
                args.push(start.to_string().into());
            }
            if let Some(end) = chunk.end {
                args.push("-l".into());
                args.push(end.to_string().into());
            }
            args.push(input.as_os_str().to_os_string());
            args.push(prefix.as_os_str().to_os_string());
            backends::run_checked(&pdftoppm, args, "pdftoppm (install poppler-utils)")?;
            all.extend(collect_pngs(prefix)?);
        }
        all.sort_by_key(|(page, _)| *page);
        all.dedup_by_key(|(page, _)| *page);
        return Ok(all);
    }

    if let Some(gs) = backends::which("gs") {
        let chunks = chunks_for(selected);
        let mut all = Vec::new();
        for chunk in chunks {
            let output_pattern = format!("{}-%d.png", prefix.display());
            let mut args: Vec<std::ffi::OsString> = vec![
                "-q".into(),
                "-dNOPAUSE".into(),
                "-dBATCH".into(),
                "-dSAFER".into(),
                "-sDEVICE=png16m".into(),
                format!("-r{dpi}").into(),
            ];
            if let Some(start) = chunk.start {
                args.push(format!("-dFirstPage={start}").into());
            }
            if let Some(end) = chunk.end {
                args.push(format!("-dLastPage={end}").into());
            }
            args.push(format!("-sOutputFile={output_pattern}").into());
            args.push(input.as_os_str().to_os_string());
            backends::run_checked(&gs, args, "ghostscript")?;
            all.extend(collect_pngs(prefix)?);
        }
        all.sort_by_key(|(page, _)| *page);
        all.dedup_by_key(|(page, _)| *page);
        if !all.is_empty() {
            return Ok(all);
        }
    }

    if let Some(mutool) = backends::which("mutool") {
        let output_pattern = format!("{}-%d.png", prefix.display());
        let range = pages_to_range(selected);
        let mut args: Vec<std::ffi::OsString> = vec![
            "draw".into(),
            "-q".into(),
            "-r".into(),
            dpi.to_string().into(),
            "-o".into(),
            output_pattern.into(),
            input.as_os_str().to_os_string(),
        ];
        if !range.is_empty() {
            args.push(range.into());
        }
        backends::run_checked(&mutool, args, "mutool")?;
        let mut all = collect_pngs(prefix)?;
        all.sort_by_key(|(page, _)| *page);
        return Ok(all);
    }

    Err(Error::missing(
        "pdftoppm / gs / mutool",
        "install poppler-utils: sudo apt install poppler-utils (or ghostscript / mupdf-tools)",
    ))
}

#[derive(Debug, Clone, Copy)]
struct Chunk {
    start: Option<u32>,
    end: Option<u32>,
}

fn chunks_for(selected: &[u32]) -> Vec<Chunk> {
    if selected.is_empty() {
        return vec![Chunk {
            start: None,
            end: None,
        }];
    }
    let mut chunks = Vec::new();
    let mut start = selected[0];
    let mut prev = selected[0];
    for &page in &selected[1..] {
        if page == prev + 1 {
            prev = page;
        } else {
            chunks.push(Chunk {
                start: Some(start),
                end: Some(prev),
            });
            start = page;
            prev = page;
        }
    }
    chunks.push(Chunk {
        start: Some(start),
        end: Some(prev),
    });
    chunks
}

fn pages_to_range(selected: &[u32]) -> String {
    if selected.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    let mut start = selected[0];
    let mut prev = selected[0];
    for &page in &selected[1..] {
        if page == prev + 1 {
            prev = page;
        } else {
            parts.push(range_part(start, prev));
            start = page;
            prev = page;
        }
    }
    parts.push(range_part(start, prev));
    parts.join(",")
}

fn range_part(start: u32, end: u32) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    }
}

fn collect_pngs(prefix: &Path) -> Result<Vec<(u32, PathBuf)>> {
    let parent = prefix.parent().unwrap_or(Path::new("."));
    let name = prefix
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "page".to_string());
    let mut out = Vec::new();
    let entries = std::fs::read_dir(parent).map_err(|e| Error::io(parent, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::io(parent, e))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        let file = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let Some(rest) = file.strip_prefix(&format!("{name}-")) else {
            continue;
        };
        if let Ok(page) = rest.trim_start_matches('0').parse::<u32>() {
            out.push((page, path));
        } else if rest.chars().all(|c| c.is_ascii_digit()) {
            out.push((0, path));
        }
    }
    Ok(out)
}

fn render_svg(
    input: &Path,
    output_base: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    let (_, pdftocairo) = backends::require(
        &["pdftocairo"],
        "install poppler-utils: sudo apt install poppler-utils",
    )?;
    let total = page_count(input).unwrap_or(1);
    let selected = opts.pages.selected(total);
    if selected.is_empty() {
        return Err(Error::message("no pages selected"));
    }
    let multi = selected.len() > 1;
    let mut outputs = Vec::new();
    for (index, page) in selected.iter().enumerate() {
        let out = if multi {
            sequence_path(output_base, *page, total)
        } else {
            output_base.to_path_buf()
        };
        if out.exists() && !opts.overwrite {
            return Err(Error::Exists(out));
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        progress(Progress::stage(
            format!("converting page {page} to SVG"),
            0.2 + 0.7 * (index as f32 / selected.len() as f32),
        ));
        backends::run_checked(
            &pdftocairo,
            [
                std::ffi::OsString::from("-svg"),
                std::ffi::OsString::from("-f"),
                std::ffi::OsString::from(page.to_string()),
                std::ffi::OsString::from("-l"),
                std::ffi::OsString::from(page.to_string()),
                input.as_os_str().to_os_string(),
                out.as_os_str().to_os_string(),
            ],
            "pdftocairo",
        )?;
        outputs.push(out);
        progress(Progress::page(index as u32 + 1, selected.len() as u32));
    }
    progress(Progress::stage("done", 1.0));
    Ok(outputs)
}

fn extract_text(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    let (_, pdftotext) = backends::require(
        &["pdftotext"],
        "install poppler-utils: sudo apt install poppler-utils",
    )?;
    if output.exists() && !opts.overwrite {
        return Err(Error::Exists(output.to_path_buf()));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    let pages = page_count(input).map(|total| opts.pages.selected(total));
    let mut bytes = Vec::new();
    match pages {
        Some(pages) if pages.len() > 1 && !opts.pages.is_all() => {
            for (index, page) in pages.iter().enumerate() {
                let part = tempfile::Builder::new()
                    .prefix("sorcerer-page-")
                    .suffix(".txt")
                    .tempfile()
                    .map_err(|e| Error::io(std::env::temp_dir(), e))?;
                let part_path = part.path().to_path_buf();
                backends::run_checked(
                    &pdftotext,
                    [
                        std::ffi::OsString::from("-layout"),
                        std::ffi::OsString::from("-f"),
                        std::ffi::OsString::from(page.to_string()),
                        std::ffi::OsString::from("-l"),
                        std::ffi::OsString::from(page.to_string()),
                        input.as_os_str().to_os_string(),
                        part_path.as_os_str().to_os_string(),
                    ],
                    "pdftotext",
                )?;
                if index > 0 {
                    bytes.extend_from_slice(b"\n\n----------------------------------------\n\n");
                }
                bytes.extend_from_slice(&std::fs::read(&part_path).unwrap_or_default());
                progress(Progress::page(index as u32 + 1, pages.len() as u32));
            }
        }
        _ => {
            progress(Progress::stage("extracting text", 0.5));
            let temp = tempfile::Builder::new()
                .prefix("sorcerer-text-")
                .suffix(".txt")
                .tempfile()
                .map_err(|e| Error::io(std::env::temp_dir(), e))?;
            let temp_path = temp.path().to_path_buf();
            let mut args = vec![std::ffi::OsString::from("-layout")];
            if let Some((start, end)) = opts.pages.bounds() {
                args.push("-f".into());
                args.push(start.to_string().into());
                if let Some(end) = end {
                    args.push("-l".into());
                    args.push(end.to_string().into());
                }
            }
            args.push(input.as_os_str().to_os_string());
            args.push(temp_path.as_os_str().to_os_string());
            backends::run_checked(&pdftotext, args, "pdftotext")?;
            bytes = std::fs::read(&temp_path).unwrap_or_default();
        }
    }
    write_atomic(output, &bytes, opts.overwrite)?;
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

fn write_atomic(path: &Path, bytes: &[u8], overwrite: bool) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| Error::io(parent, e))?;
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
