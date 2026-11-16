//! Document conversions: office files, HTML, Markdown and plain text.
//!
//! LibreOffice handles office formats, HTML → PDF tries the best available
//! browser/HTML engine first, and plain text / Markdown always work thanks to
//! Sorcerer's native PDF writer.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use pulldown_cmark::{Event, Options, Parser, TagEnd};

use super::backends;
use super::error::{Error, Result};
use super::format::Format;
use super::options::ConvertOptions;
use super::pdf_write::{self, PdfOptions};
use super::progress::{Progress, ProgressFn};

/// Read a text file leniently (UTF-8 with BOM, otherwise lossy).
pub fn read_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.trim_start_matches('\u{feff}').to_string())
}

/// Write text atomically.
pub fn write_text(path: &Path, text: &str, overwrite: bool) -> Result<()> {
    use std::io::Write;
    if path.exists() && !overwrite {
        return Err(Error::Exists(path.to_path_buf()));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| Error::io(parent, e))?;
    temp.as_file_mut()
        .write_all(text.as_bytes())
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

/// Convert Markdown to a standalone HTML document.
pub fn markdown_to_html(markdown: &str, title: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, parser);
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{}</title>\n\
         <style>body{{max-width:52rem;margin:3rem auto;padding:0 1.2rem;font-family:system-ui,sans-serif;line-height:1.6;color:#1a1a1a}}\
         pre{{background:#f4f4f4;padding:1rem;overflow:auto;border-radius:6px}}\
         code{{background:#f4f4f4;padding:.1rem .3rem;border-radius:4px}}\
         blockquote{{border-left:4px solid #ddd;margin-left:0;padding-left:1rem;color:#555}}\
         table{{border-collapse:collapse}}td,th{{border:1px solid #ccc;padding:.4rem .7rem}}\
         img{{max-width:100%}}</style></head><body>{}</body></html>",
        escape_html(title),
        body
    )
}

/// Markdown to plain text (headings, lists and code are kept readable).
pub fn markdown_to_text(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut out = String::new();
    for event in parser {
        match event {
            Event::Text(text) | Event::Code(text) => out.push_str(&text),
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            Event::Rule => out.push_str("\n----------------------------------------\n"),
            Event::End(tag) => match tag {
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::CodeBlock => {
                    out.push_str("\n\n")
                }
                TagEnd::BlockQuote(_) => out.push('\n'),
                _ => {}
            },
            Event::TaskListMarker(checked) => out.push_str(if checked { "[x] " } else { "[ ] " }),
            _ => {}
        }
    }
    collapse_blank_lines(&out)
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank <= 1 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(line);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// Strip HTML tags and decode the most common entities.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut chars = html.chars().peekable();
    let mut skip_until: Option<String> = None;

    while let Some(ch) = chars.next() {
        if let Some(end) = &skip_until {
            // Skip script/style content.
            if ch == '<' {
                let mut candidate = String::from("<");
                for _ in 0..end.len() + 1 {
                    if let Some(next) = chars.next() {
                        candidate.push(next);
                    }
                }
                if candidate.to_ascii_lowercase().starts_with(end) {
                    skip_until = None;
                }
            }
            continue;
        }

        if ch == '<' {
            let mut tag = String::from("<");
            for next in chars.by_ref() {
                tag.push(next);
                if next == '>' {
                    break;
                }
            }
            let lower = tag.to_ascii_lowercase();
            if lower.starts_with("<script") {
                skip_until = Some("</script".to_string());
            } else if lower.starts_with("<style") {
                skip_until = Some("</style".to_string());
            } else if lower.starts_with("<br")
                || lower.starts_with("</p")
                || lower.starts_with("</div")
                || lower.starts_with("</h")
                || lower.starts_with("<li")
            {
                out.push('\n');
            }
            continue;
        }
        if ch == '&' {
            let mut entity = String::new();
            for _ in 0..12 {
                if let Some(&next) = chars.peek() {
                    if next == ';' {
                        chars.next();
                        break;
                    }
                    entity.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            let decoded = match entity.as_str() {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                "mdash" => Some('—'),
                "ndash" => Some('–'),
                "hellip" => Some('…'),
                "copy" => Some('©'),
                "reg" => Some('®'),
                "trade" => Some('™'),
                other if other.starts_with("#x") || other.starts_with("#X") => {
                    u32::from_str_radix(&other[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                }
                other if other.starts_with('#') => {
                    other[1..].parse::<u32>().ok().and_then(char::from_u32)
                }
                _ => None,
            };
            if let Some(ch) = decoded {
                out.push(ch);
            }
            continue;
        }
        out.push(ch);
    }

    collapse_blank_lines(&out)
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Plain text as a very small HTML page.
pub fn text_to_html(text: &str, title: &str) -> String {
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{}</title>\
         <style>body{{max-width:60rem;margin:2rem auto;padding:0 1rem}}pre{{white-space:pre-wrap;font-family:monospace;line-height:1.5}}</style>\
         </head><body><pre>{}</pre></body></html>",
        escape_html(title),
        escape_html(text)
    )
}

/// Convert any text-ish source to a text target natively.
pub fn convert_text(
    input: &Path,
    output: &Path,
    source: Format,
    target: Format,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    progress(Progress::stage("reading text", 0.2));
    let raw = read_text(input)?;
    let title = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Document".to_string());

    match target {
        Format::Txt => {
            let text = match source {
                Format::Html => html_to_text(&raw),
                Format::Markdown => markdown_to_text(&raw),
                _ => raw,
            };
            write_text(output, &text, opts.overwrite)?;
        }
        Format::Markdown => {
            let text = match source {
                Format::Html => html_to_text(&raw),
                _ => raw,
            };
            write_text(output, &text, opts.overwrite)?;
        }
        Format::Html => {
            let html = match source {
                Format::Markdown => markdown_to_html(&raw, &title),
                Format::Html => raw,
                _ => text_to_html(&raw, &title),
            };
            write_text(output, &html, opts.overwrite)?;
        }
        Format::Pdf => {
            let text = match source {
                Format::Markdown => markdown_to_text(&raw),
                Format::Html => html_to_text(&raw),
                _ => raw,
            };
            pdf_write::text_to_pdf(&text, output, &title, opts.overwrite, progress)?;
        }
        _ => {
            return Err(Error::unsupported(source.label(), target.label()));
        }
    }
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

/// HTML → PDF using a browser engine when possible.
pub fn html_to_pdf(
    html_path: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    if output.exists() && !opts.overwrite {
        return Err(Error::Exists(output.to_path_buf()));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    progress(Progress::stage("rendering HTML", 0.3));

    if let Some((_, wkhtmltopdf)) = backends::first_available(&["wkhtmltopdf"]) {
        let result = backends::run_checked(
            &wkhtmltopdf,
            [
                OsString::from("--enable-local-file-access"),
                OsString::from("--quiet"),
                html_path.as_os_str().to_os_string(),
                output.as_os_str().to_os_string(),
            ],
            "wkhtmltopdf",
        );
        if result.is_ok() {
            progress(Progress::stage("done", 1.0));
            return Ok(vec![output.to_path_buf()]);
        }
    }

    for browser in [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
    ] {
        if let Some(chrome) = backends::which(browser) {
            let url = file_url(html_path);
            let out_arg = format!("--print-to-pdf={}", output.display());
            let result = backends::run_checked(
                &chrome,
                [
                    OsString::from("--headless"),
                    OsString::from("--disable-gpu"),
                    OsString::from("--no-sandbox"),
                    OsString::from("--no-pdf-header-footer"),
                    OsString::from(out_arg),
                    OsString::from(url),
                ],
                browser,
            );
            if result.is_ok() && output.exists() {
                progress(Progress::stage("done", 1.0));
                return Ok(vec![output.to_path_buf()]);
            }
            let _ = std::fs::remove_file(output);
        }
    }

    // LibreOffice can render simple HTML too.
    if let Ok(lo_pdf) = office_to_pdf(html_path) {
        move_file(&lo_pdf, output, opts.overwrite)?;
        progress(Progress::stage("done", 1.0));
        return Ok(vec![output.to_path_buf()]);
    }

    // Last resort: extract the text and use the native writer.
    let text = html_to_text(&read_text(html_path)?);
    let title = html_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Document".to_string());
    pdf_write::text_to_pdf(&text, output, &title, opts.overwrite, progress)?;
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

/// Markdown → PDF: render HTML, then use HTML → PDF, falling back to text.
pub fn markdown_to_pdf(
    input: &Path,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    let raw = read_text(input)?;
    let title = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Document".to_string());
    let html = markdown_to_html(&raw, &title);
    let temp = tempfile::Builder::new()
        .prefix("sorcerer-md-")
        .suffix(".html")
        .tempfile()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    std::fs::write(temp.path(), html).map_err(|e| Error::io(temp.path(), e))?;
    html_to_pdf(temp.path(), output, opts, progress)
}

/// Convert a Markdown/HTML/TXT file to an office format via LibreOffice.
pub fn text_to_office(
    input: &Path,
    source: Format,
    target: Format,
    output: &Path,
    opts: &ConvertOptions,
    progress: &ProgressFn,
) -> Result<Vec<PathBuf>> {
    let workdir = tempfile::Builder::new()
        .prefix("sorcerer-text-")
        .tempdir()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;

    let prepared = match source {
        Format::Markdown => {
            let html = markdown_to_html(&read_text(input)?, "Document");
            let path = workdir.path().join("document.html");
            std::fs::write(&path, html).map_err(|e| Error::io(&path, e))?;
            path
        }
        Format::Html => input.to_path_buf(),
        _ => {
            let path = workdir.path().join("document.txt");
            std::fs::write(&path, read_text(input)?).map_err(|e| Error::io(&path, e))?;
            path
        }
    };

    progress(Progress::stage("converting with LibreOffice", 0.4));
    let produced = office_convert(&prepared, target, workdir.path())?;
    move_file(&produced, output, opts.overwrite)?;
    progress(Progress::stage("done", 1.0));
    Ok(vec![output.to_path_buf()])
}

/// Convert an office document to PDF in a temporary directory.
pub fn office_to_pdf(input: &Path) -> Result<PathBuf> {
    let workdir = tempfile::Builder::new()
        .prefix("sorcerer-office-")
        .tempdir()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let produced = office_convert(input, Format::Pdf, workdir.path())?;
    // Keep the temp directory alive by leaking its path into a persistent temp
    // file copy, then return the copy.
    let kept = tempfile::Builder::new()
        .prefix("sorcerer-doc-")
        .suffix(".pdf")
        .tempfile()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;
    let kept_path = kept.path().to_path_buf();
    std::fs::copy(&produced, &kept_path).map_err(|e| Error::io(&kept_path, e))?;
    let (_, path) = kept
        .keep()
        .map_err(|e| Error::io(std::env::temp_dir(), e.error))?;
    Ok(path)
}

/// Convert an office document to any supported office/text format.
pub fn office_convert(input: &Path, target: Format, outdir: &Path) -> Result<PathBuf> {
    let (_, soffice) = backends::require(
        &["soffice", "libreoffice"],
        "install LibreOffice: sudo apt install libreoffice (or libreoffice-core)",
    )?;

    let filter = libreoffice_filter(target)?;
    std::fs::create_dir_all(outdir).map_err(|e| Error::io(outdir, e))?;

    let profile = tempfile::Builder::new()
        .prefix("sorcerer-lo-profile-")
        .tempdir()
        .map_err(|e| Error::io(std::env::temp_dir(), e))?;

    let mut args: Vec<OsString> = vec![
        OsString::from("--headless"),
        OsString::from("--norestore"),
        OsString::from("--invisible"),
        OsString::from("--nolockcheck"),
        OsString::from("--nodefault"),
        OsString::from("--nofirststartwizard"),
        OsString::from(format!(
            "-env:UserInstallation={}",
            file_url(profile.path())
        )),
        OsString::from("--convert-to"),
        OsString::from(filter),
        OsString::from("--outdir"),
        outdir.as_os_str().to_os_string(),
        input.as_os_str().to_os_string(),
    ];
    if let Some(parent) = input.parent() {
        // LibreOffice resolves relative resources from the current directory.
        let _ = parent;
    }
    // LibreOffice sometimes needs the working directory to exist.
    let _ = &mut args;

    let output = backends::run_checked(&soffice, args, "libreoffice")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = (stdout, stderr);

    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".to_string());
    let expected = outdir.join(format!("{stem}.{}", target.extension()));
    if expected.exists() {
        return Ok(expected);
    }
    // Some filters (CSV) may choose a different name; pick the first matching
    // extension in the output directory.
    let entries = std::fs::read_dir(outdir).map_err(|e| Error::io(outdir, e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some(target.extension()) {
            return Ok(path);
        }
    }
    let _ = args;
    Err(Error::message(format!(
        "LibreOffice did not produce a .{} file for {}",
        target.extension(),
        input.display()
    )))
}

/// Map a target format to a LibreOffice `--convert-to` filter name.
pub fn libreoffice_filter(target: Format) -> Result<&'static str> {
    Ok(match target {
        Format::Pdf => "pdf",
        Format::Docx => "docx:MS Word 2007 XML",
        Format::Doc => "doc:MS Word 97",
        Format::Odt => "odt",
        Format::Rtf => "rtf",
        Format::Txt => "txt:Text",
        Format::Html => "html:HTML (StarWriter)",
        Format::Xlsx => "xlsx:Calc MS Excel 2007 XML",
        Format::Xls => "xls:MS Excel 97",
        Format::Ods => "ods",
        Format::Csv => "csv:Text - txt - csv (StarCalc):44,34,76,1",
        Format::Pptx => "pptx:Impress MS PowerPoint 2007 XML",
        Format::Ppt => "ppt:MS PowerPoint 97",
        Format::Odp => "odp",
        other => {
            return Err(Error::unsupported(
                "document",
                format!("{} (LibreOffice filter unavailable)", other.label()),
            ));
        }
    })
}

/// Copy or rename `from` to `to`, creating parents and respecting overwrite.
pub fn move_file(from: &Path, to: &Path, overwrite: bool) -> Result<()> {
    if to.exists() && !overwrite {
        return Err(Error::Exists(to.to_path_buf()));
    }
    if let Some(parent) = to.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(from, to).map_err(|e| Error::io(to, e))?;
            let _ = std::fs::remove_file(from);
            Ok(())
        }
    }
}

fn file_url(path: &Path) -> String {
    let mut url = String::from("file://");
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    for byte in absolute.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                url.push(byte as char)
            }
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    url
}

/// Options for native text → PDF (kept for API symmetry).
pub fn default_pdf_options() -> PdfOptions {
    PdfOptions::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_renders_html() {
        let html = markdown_to_html("# Hi\n\n**bold** and `code`", "Test");
        assert!(html.contains("<h1>Hi</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn markdown_plain_text() {
        let text = markdown_to_text("# Title\n\nHello *world*.\n\n- one\n- two");
        assert!(text.contains("Title"));
        assert!(text.contains("Hello world"));
        assert!(text.contains("one"));
    }

    #[test]
    fn html_strips_tags() {
        let text = html_to_text("<p>Hello <b>world</b> &amp; friends</p>");
        assert!(text.contains("Hello world & friends"));
    }

    #[test]
    fn file_url_encodes_spaces() {
        let url = file_url(Path::new("/tmp/a b"));
        assert!(url.contains("a%20b"));
        assert!(url.starts_with("file://"));
    }
}
