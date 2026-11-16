//! Batch planning and parallel execution.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use super::convert::convert_file;
use super::error::{Error, Result};
use super::format::Format;
use super::merge;
use super::options::ConvertOptions;
use super::progress::{Progress, ProgressFn};
use crate::util::paths;

/// How the user asked the batch to be written.
#[derive(Debug, Clone)]
pub struct BatchSpec {
    pub target: Format,
    pub output: Option<PathBuf>,
    /// Merge all inputs into a single PDF (only valid for `--to pdf`).
    pub merge_pdf: bool,
    pub overwrite: bool,
    pub preserve_tree: bool,
}

/// One input → one output.
#[derive(Debug, Clone)]
pub struct Job {
    pub input: PathBuf,
    pub output: PathBuf,
}

/// A complete plan.
#[derive(Debug, Clone)]
pub enum Plan {
    Individual(Vec<Job>),
    Merge {
        inputs: Vec<PathBuf>,
        output: PathBuf,
    },
}

impl Plan {
    pub fn len(&self) -> usize {
        match self {
            Plan::Individual(jobs) => jobs.len(),
            Plan::Merge { inputs, .. } => inputs.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Build a conversion plan from already-expanded input files.
pub fn build_plan(inputs: &[PathBuf], spec: &BatchSpec) -> Result<Plan> {
    if inputs.is_empty() {
        return Err(Error::message("no input files"));
    }

    let output_is_pdf = spec
        .output
        .as_ref()
        .map(|p| {
            !p.is_dir()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("pdf"))
                    .unwrap_or(false)
        })
        .unwrap_or(false);

    let merge = spec.merge_pdf || (spec.target == Format::Pdf && inputs.len() > 1 && output_is_pdf);
    if merge {
        if spec.target != Format::Pdf {
            return Err(Error::InvalidOption(
                "merging is only available for PDF output".to_string(),
            ));
        }
        let output = match &spec.output {
            Some(output) => {
                if output.is_dir() {
                    return Err(Error::InvalidOption(
                        "merging needs a PDF file name, not a directory".to_string(),
                    ));
                }
                paths::with_extension(output, Format::Pdf)
            }
            None => {
                let parent = inputs[0].parent().unwrap_or(Path::new("."));
                parent.join("sorcerer-merged.pdf")
            }
        };
        if output.exists() && !spec.overwrite {
            return Err(Error::Exists(output));
        }
        return Ok(Plan::Merge {
            inputs: inputs.to_vec(),
            output,
        });
    }

    let mode = OutputMode::resolve(spec.output.as_deref(), inputs.len(), spec.target)?;
    let common = common_root(inputs);
    let mut used = HashSet::new();
    let mut jobs = Vec::new();

    for input in inputs {
        let output = match &mode {
            OutputMode::Alongside => {
                let parent = input.parent().unwrap_or(Path::new("."));
                parent.join(with_target_name(input, spec.target))
            }
            OutputMode::File(path) => {
                if inputs.len() != 1 {
                    return Err(Error::InvalidOption(
                        "a single output file was given for several inputs; pass a directory instead"
                            .to_string(),
                    ));
                }
                paths::with_extension(path, spec.target)
            }
            OutputMode::Directory(dir) => {
                let relative = input.strip_prefix(&common).unwrap_or(input);
                let name = with_target_name(relative, spec.target);
                if spec.preserve_tree {
                    dir.join(name)
                } else {
                    dir.join(
                        name.file_name()
                            .map(PathBuf::from)
                            .unwrap_or_else(|| PathBuf::from("output")),
                    )
                }
            }
        };

        if paths::same_path(input, &output) && !spec.overwrite {
            return Err(Error::message(format!(
                "{} would overwrite its own input; pass --overwrite or choose another output",
                input.display()
            )));
        }
        if output.exists() && !spec.overwrite {
            return Err(Error::Exists(output));
        }

        let output = unique_in_set(output, &mut used);
        jobs.push(Job {
            input: input.clone(),
            output,
        });
    }

    Ok(Plan::Individual(jobs))
}

fn with_target_name(path: &Path, target: Format) -> PathBuf {
    let mut out = path.to_path_buf();
    out.set_extension(target.extension());
    out
}

fn unique_in_set(path: PathBuf, used: &mut HashSet<PathBuf>) -> PathBuf {
    if used.insert(path.clone()) {
        return path;
    }
    let stem = paths::file_stem(&path);
    let ext = path.extension().map(|e| e.to_string_lossy().to_string());
    for n in 2..10_000 {
        let name = match &ext {
            Some(ext) => format!("{stem}-{n}.{ext}"),
            None => format!("{stem}-{n}"),
        };
        let candidate = path.with_file_name(name);
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    path
}

fn common_root(inputs: &[PathBuf]) -> PathBuf {
    let mut iter = inputs.iter().filter_map(|p| p.parent());
    let Some(first) = iter.next() else {
        return PathBuf::from(".");
    };
    let mut common: Vec<_> = first.components().collect();
    for parent in iter {
        let components: Vec<_> = parent.components().collect();
        let keep = common
            .iter()
            .zip(components.iter())
            .take_while(|(a, b)| a == b)
            .count();
        common.truncate(keep);
        if common.is_empty() {
            return PathBuf::from(".");
        }
    }
    common.iter().collect()
}

enum OutputMode {
    Alongside,
    File(PathBuf),
    Directory(PathBuf),
}

impl OutputMode {
    fn resolve(output: Option<&Path>, input_count: usize, target: Format) -> Result<Self> {
        let Some(output) = output else {
            return Ok(OutputMode::Alongside);
        };
        if output.is_dir() {
            return Ok(OutputMode::Directory(output.to_path_buf()));
        }
        if output.exists() {
            if input_count == 1 {
                return Ok(OutputMode::File(output.to_path_buf()));
            }
            return Err(Error::InvalidOption(format!(
                "{} is a file but there are {input_count} inputs; pass a directory",
                output.display()
            )));
        }
        // Non-existing path: a dot in the file name means "file", otherwise a
        // directory (`-o out` creates `out/`, `-o out.png` creates a file).
        if output.extension().is_some() {
            if input_count > 1 {
                return Err(Error::InvalidOption(format!(
                    "{} looks like a file but there are {input_count} inputs; pass a directory",
                    output.display()
                )));
            }
            // Make sure the extension matches the requested target.
            let mut path = output.to_path_buf();
            path.set_extension(target.extension());
            Ok(OutputMode::File(path))
        } else {
            Ok(OutputMode::Directory(output.to_path_buf()))
        }
    }
}

/// A callback receiving `(job index, progress event)`.
pub type JobProgressFn = Arc<dyn Fn(usize, Progress) + Send + Sync>;

/// One finished job.
#[derive(Debug, Clone)]
pub struct JobResult {
    pub input: PathBuf,
    pub outputs: Vec<PathBuf>,
    pub error: Option<String>,
    pub duration: Duration,
    pub input_bytes: u64,
}

impl JobResult {
    pub fn ok(&self) -> bool {
        self.error.is_none()
    }

    pub fn output_bytes(&self) -> u64 {
        self.outputs
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum()
    }
}

/// Aggregated batch results.
#[derive(Debug, Clone)]
pub struct Summary {
    pub results: Vec<JobResult>,
    pub duration: Duration,
}

impl Summary {
    pub fn succeeded(&self) -> usize {
        self.results.iter().filter(|r| r.ok()).count()
    }

    pub fn failed(&self) -> usize {
        self.results.len() - self.succeeded()
    }

    pub fn input_bytes(&self) -> u64 {
        self.results.iter().map(|r| r.input_bytes).sum()
    }

    pub fn output_bytes(&self) -> u64 {
        self.results.iter().map(|r| r.output_bytes()).sum()
    }
}

/// Execute a plan, optionally in parallel.
pub fn execute_plan(
    plan: &Plan,
    opts: &ConvertOptions,
    threads: usize,
    on_progress: &JobProgressFn,
) -> Summary {
    let started = Instant::now();
    let results = match plan {
        Plan::Individual(jobs) => {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads.max(1))
                .build();
            let run = || {
                jobs.par_iter()
                    .enumerate()
                    .map(|(index, job)| {
                        let callback = on_progress.clone();
                        let sink: ProgressFn = Arc::new(move |event| callback(index, event));
                        let input_bytes =
                            std::fs::metadata(&job.input).map(|m| m.len()).unwrap_or(0);
                        let start = Instant::now();
                        let result = convert_file(&job.input, &job.output, opts, &sink);
                        let (outputs, error) = match result {
                            Ok(outputs) => (outputs, None),
                            Err(err) => (Vec::new(), Some(err.to_string())),
                        };
                        JobResult {
                            input: job.input.clone(),
                            outputs,
                            error,
                            duration: start.elapsed(),
                            input_bytes,
                        }
                    })
                    .collect::<Vec<_>>()
            };
            match pool {
                Ok(pool) => pool.install(run),
                Err(_) => run(),
            }
        }
        Plan::Merge { inputs, output } => {
            let callback = on_progress.clone();
            let sink: ProgressFn = Arc::new(move |event| callback(0, event));
            let start = Instant::now();
            let input_bytes = inputs
                .iter()
                .filter_map(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .sum();
            let result = merge::merge_to_pdf(inputs, output, opts, &sink);
            let (outputs, error) = match result {
                Ok(outputs) => (outputs, None),
                Err(err) => (Vec::new(), Some(err.to_string())),
            };
            vec![JobResult {
                input: inputs.first().cloned().unwrap_or_default(),
                outputs,
                error,
                duration: start.elapsed(),
                input_bytes,
            }]
        }
    };

    Summary {
        results,
        duration: started.elapsed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_alongside() {
        let inputs = vec![PathBuf::from("/tmp/a.jpg")];
        let spec = BatchSpec {
            target: Format::Png,
            output: None,
            merge_pdf: false,
            overwrite: false,
            preserve_tree: false,
        };
        let plan = build_plan(&inputs, &spec).unwrap();
        match plan {
            Plan::Individual(jobs) => {
                assert_eq!(jobs[0].output, PathBuf::from("/tmp/a.png"));
            }
            _ => panic!("expected individual plan"),
        }
    }

    #[test]
    fn plans_directory() {
        let inputs = vec![PathBuf::from("/tmp/a.jpg"), PathBuf::from("/tmp/b.jpg")];
        let spec = BatchSpec {
            target: Format::Png,
            output: Some(PathBuf::from("/tmp/out")),
            merge_pdf: false,
            overwrite: false,
            preserve_tree: false,
        };
        let plan = build_plan(&inputs, &spec).unwrap();
        match plan {
            Plan::Individual(jobs) => {
                assert_eq!(jobs[0].output, PathBuf::from("/tmp/out/a.png"));
                assert_eq!(jobs[1].output, PathBuf::from("/tmp/out/b.png"));
            }
            _ => panic!("expected individual plan"),
        }
    }

    #[test]
    fn merges_pdf_output() {
        let inputs = vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")];
        let spec = BatchSpec {
            target: Format::Pdf,
            output: Some(PathBuf::from("/tmp/album.pdf")),
            merge_pdf: true,
            overwrite: true,
            preserve_tree: false,
        };
        let plan = build_plan(&inputs, &spec).unwrap();
        assert!(matches!(plan, Plan::Merge { .. }));
    }
}
