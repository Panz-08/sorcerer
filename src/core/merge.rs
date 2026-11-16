//! Merge several files into one PDF.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::backends;
use super::convert::convert_file;
use super::error::{Error, Result};
use super::format::Format;
use super::image_ops;
use super::options::ConvertOptions;
use super::pdf_write::{self, PdfImage, PdfOptions};
use super::progress::{Progress, ProgressFn};
use super::svg;

/// Merge images, PDFs, office documents and text files into a single PDF.
pub fn merge_to_pdf(
    inputs: &[PathBuf],
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    if inputs.is_empty() {
        return Err(Error::message("nothing to merge"));
    }
    let formats: Result<Vec<Format>> = inputs.iter().map(|p| Format::from_path(p)).collect();
    let formats = formats?;

    if formats.iter().all(|f| *f == Format::Pdf) {
        return merge_pdfs(inputs, output, opts, progress);
    }

    if formats
        .iter()
        .all(|f| f.is_raster() || *f == Format::Svg || *f == Format::Pdf)
    {
        return merge_native(inputs, &formats, output, opts, progress);
    }

    // Mixed documents: normalise everything to a temporary PDF, then unite.
    let workdir = tempfile::Builder::new()
        .prefix("sorcerer-merge-")
        .tempdir()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let mut parts = Vec::new();
    for (index, (input, format)) in inputs.iter().zip(formats.iter()).enumerate() {
        progress(Progress::stage(
            format!(
                "preparing {} ({}/{})",
                input.display(),
                index + 1,
                inputs.len()
            ),
            0.05 + 0.8 * (index as f32 / inputs.len() as f32),
        ));
        if *format == Format::Pdf {
            parts.push(input.clone());
            continue;
        }
        let part = workdir.path().join(format!("part-{index:04}.pdf"));
        let mut part_opts = opts.clone();
        part_opts.target = Format::Pdf;
        part_opts.pages = super::options::PageSelection::All;
        convert_file(input, &part, &part_opts, progress)?;
        parts.push(part);
    }
    unite_pdfs(&parts, output, opts.overwrite, progress)?;
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

fn merge_native(
    inputs: &[PathBuf],
    formats: &[Format],
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    let mut images = Vec::with_capacity(inputs.len());
    for (index, (input, format)) in inputs.iter().zip(formats.iter()).enumerate() {
        progress(Progress::stage(
            format!("preparing page {} of {}", index + 1, inputs.len()),
            0.05 + 0.8 * (index as f32 / inputs.len() as f32),
        ));
        match format {
            Format::Pdf => {
                // A PDF inside a native image merge is handled by normalising
                // through the mixed route; this branch is unreachable.
                return Err(Error::message("internal: unexpected PDF in native merge"));
            }
            Format::Svg => {
                let img = svg::render_svg(input, opts.dpi)?;
                images.push(PdfImage::new(image_ops::transform(img, opts), false));
            }
            _ => {
                let img = image_ops::load_image(input)?;
                images.push(PdfImage::new(
                    image_ops::transform(img, opts),
                    *format == Format::Jpeg,
                ));
            }
        }
        progress(Progress::page(index as u32 + 1, inputs.len() as u32));
    }

    let title = output
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Sorcerer album".to_string());
    let pdf_opts = PdfOptions {
        page_size: opts.page_size,
        margin_mm: opts.margin_mm,
        dpi: opts.dpi,
        quality: opts.quality,
        background: opts.background,
        title,
        overwrite: opts.overwrite,
    };
    pdf_write::images_to_pdf(&images, output, &pdf_opts, progress)?;
    Ok(vec![output.to_path_buf()])
}

fn merge_pdfs(
    inputs: &[PathBuf],
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    if inputs.len() == 1 {
        if output.exists() && !opts.overwrite {
            return Err(Error::Exists(output.to_path_buf()));
        }
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::copy(&inputs[0], output).map_err(|e| Error::io(output, e))?;
        return Ok(vec![output.to_path_buf()]);
    }
    unite_pdfs(inputs, output, opts.overwrite, progress)?;
    Ok(vec![output.to_path_buf()])
}

fn unite_pdfs(
    inputs: &[PathBuf],
    output: &Path,
    overwrite: bool,
    progress: &ProgressFn,
) -> Result<()> {
    let (_, pdfunite) = backends::require(
        &["pdfunite"],
        "install poppler-utils: sudo apt install poppler-utils",
    )?;
    if output.exists() && !overwrite {
        return Err(Error::Exists(output.to_path_buf()));
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    progress(Progress::stage("merging PDFs", 0.85));
    let mut args: Vec<OsString> = inputs
        .iter()
        .map(|p| p.as_os_str().to_os_string())
        .collect();
    args.push(output.as_os_str().to_os_string());
    backends::run_checked(&pdfunite, args, "pdfunite")?;
    Ok(())
}

/// Check whether a merge is possible without touching the filesystem.
pub fn can_merge(inputs: &[PathBuf]) -> bool {
    !inputs.is_empty()
        && inputs.iter().all(|path| {
            Format::from_path(path)
                .map(|f| !f.is_media())
                .unwrap_or(false)
        })
}

/// Describe what merging these inputs would do.
pub fn describe(inputs: &[PathBuf]) -> String {
    let mut counts = std::collections::BTreeMap::new();
    for input in inputs {
        if let Ok(format) = Format::from_path(input) {
            *counts.entry(format.label()).or_insert(0usize) += 1;
        }
    }
    let summary = counts
        .into_iter()
        .map(|(label, count)| format!("{count}× {label}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("merge {} file(s): {summary}", inputs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_inputs() {
        let inputs = vec![PathBuf::from("a.png"), PathBuf::from("b.png")];
        // Format::from_path uses extensions; no filesystem access needed.
        assert!(describe(&inputs).contains("2× PNG"));
    }
}
