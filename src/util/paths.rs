//! Path, glob and output-name helpers.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::core::format::Format;

/// Expand a list of raw CLI arguments into concrete input files.
///
/// * glob patterns (`*.jpg`, `docs/**/*.pdf`) are expanded with the `glob` crate
/// * directories are scanned (recursively when `recursive` is true)
/// * only files whose format is supported by Sorcerer are kept, unless an
///   explicit `--from` filter is supplied
pub fn expand_inputs(
    raw: &[String],
    recursive: bool,
    from_filter: &[Format],
) -> Result<Vec<PathBuf>> {
    let mut files: BTreeSet<PathBuf> = BTreeSet::new();
    let mut unmatched: Vec<String> = Vec::new();

    for raw_item in raw {
        let patterns = expand_braces(raw_item);
        let mut matched_any = false;

        for pattern in patterns {
            if has_glob_meta(&pattern) {
                let mut glob_match = false;
                for entry in glob::glob(&pattern)
                    .with_context(|| format!("invalid glob pattern `{pattern}`"))?
                {
                    let path = entry.with_context(|| format!("could not read glob `{pattern}`"))?;
                    glob_match = true;
                    if path.is_dir() {
                        collect_dir(&path, recursive, from_filter, &mut files)?;
                    } else if path.is_file() {
                        push_if_supported(path, from_filter, &mut files);
                    }
                }
                matched_any |= glob_match;
            } else {
                let path = PathBuf::from(&pattern);
                if path.is_dir() {
                    matched_any = true;
                    collect_dir(&path, recursive, from_filter, &mut files)?;
                } else if path.is_file() {
                    matched_any = true;
                    push_if_supported(path, from_filter, &mut files);
                }
            }
        }

        if !matched_any {
            unmatched.push(raw_item.clone());
        }
    }

    if files.is_empty() {
        if unmatched.is_empty() {
            anyhow::bail!("no input files found");
        }
        anyhow::bail!("no matching files found for: {}", unmatched.join(", "));
    }

    Ok(files.into_iter().collect())
}

fn expand_braces(pattern: &str) -> Vec<String> {
    // Minimal brace expansion: `img.{jpg,png}` -> img.jpg, img.png
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let Some(close_rel) = pattern[open..].find('}') else {
        return vec![pattern.to_string()];
    };
    let close = open + close_rel;
    let inner = &pattern[open + 1..close];
    let prefix = &pattern[..open];
    let suffix = &pattern[close + 1..];
    inner
        .split(',')
        .map(|part| format!("{prefix}{part}{suffix}"))
        .collect()
}

fn has_glob_meta(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

fn collect_dir(
    dir: &Path,
    recursive: bool,
    filter: &[Format],
    out: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    if recursive {
        for entry in WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                push_if_supported(entry.into_path(), filter, out);
            }
        }
    } else {
        for entry in std::fs::read_dir(dir)
            .with_context(|| format!("cannot read directory {}", dir.display()))?
        {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                push_if_supported(entry.path(), filter, out);
            }
        }
    }
    Ok(())
}

fn push_if_supported(path: PathBuf, filter: &[Format], out: &mut BTreeSet<PathBuf>) {
    if !filter.is_empty() {
        if let Ok(fmt) = Format::from_path(&path)
            && filter.contains(&fmt)
        {
            out.insert(path);
        }
    } else if Format::from_path(&path).is_ok() {
        out.insert(path);
    }
}

/// Build the output path for `input` inside `out_dir`, keeping the file stem
/// and replacing the extension with the target format.
pub fn output_in_dir(input: &Path, out_dir: &Path, target: Format) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "output".to_string());
    out_dir.join(format!("{stem}.{}", target.extension()))
}

/// Replace the extension of `path` with the target format's extension.
pub fn with_extension(path: &Path, target: Format) -> PathBuf {
    path.with_extension(target.extension())
}

/// Name used when a single input produces several files (PDF pages, etc.):
/// `photo-001.png`, `photo-002.png` …
pub fn sequence_path(base: &Path, page: u32, total: u32) -> PathBuf {
    if total <= 1 {
        return base.to_path_buf();
    }
    let width = total.max(1).to_string().len().max(3);
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "page".to_string());
    let ext = base
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "png".to_string());
    base.with_file_name(format!("{stem}-{page:0width$}.{ext}"))
}

/// If `path` exists, append `-1`, `-2`, … until a free name is found.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let ext = path.extension().map(|s| s.to_string_lossy().to_string());
    for n in 1..10_000 {
        let name = match &ext {
            Some(ext) => format!("{stem}-{n}.{ext}"),
            None => format!("{stem}-{n}"),
        };
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

/// Create the parent directory of `path` if needed.
pub fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create directory {}", parent.display()))?;
    }
    Ok(())
}

/// Whether two paths point to the same file. Falls back to lexical comparison
/// when the files do not exist yet.
pub fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => {
            let a = a.components().collect::<Vec<_>>();
            let b = b.components().collect::<Vec<_>>();
            a == b
        }
    }
}

/// Show a path with `$HOME` replaced by `~`.
pub fn pretty(path: &Path) -> String {
    if let Some(home) = dirs::home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_string();
        }
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

/// `report.pdf` -> `report`, without the extension.
pub fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_names_are_padded() {
        let base = Path::new("/tmp/photo.png");
        assert_eq!(
            sequence_path(base, 2, 12),
            PathBuf::from("/tmp/photo-002.png")
        );
        assert_eq!(sequence_path(base, 1, 1), PathBuf::from("/tmp/photo.png"));
    }

    #[test]
    fn brace_expansion() {
        let got = expand_braces("img.{jpg,png}");
        assert_eq!(got, vec!["img.jpg".to_string(), "img.png".to_string()]);
    }

    #[test]
    fn pretty_path_does_not_panic() {
        let p = pretty(Path::new("/definitely/not/home"));
        assert!(p.contains("definitely"));
    }
}
