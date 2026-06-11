mod gif;
mod jpeg;
mod png;

use anyhow::{Context, Result, bail};
use clap::Parser;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use walkdir::WalkDir;

const EXAMPLES: &str = "\
Examples:
  iopt assets/                   optimize every PNG/JPEG/GIF under assets/ in place
  iopt --check assets/           CI gate: exit 1 if any file could be smaller
  iopt --strip --zopfli assets/  smallest output, metadata removed

Pre-commit hook (lefthook.yml):
  pre-commit:
    commands:
      iopt:
        glob: \"*.{png,jpg,jpeg,gif}\"
        run: iopt {staged_files}
        stage_fixed: true";

/// Lossless image optimizer for CI pipelines and pre-commit hooks.
///
/// Optimizes PNG, JPEG and GIF files in place without any quality loss
/// (recompression only — pixels stay bit-identical). Files are only
/// rewritten when the result is smaller.
///
/// Exit codes: 0 = success, 1 = --check found optimizable files,
/// 2 = one or more files failed to process.
#[derive(Parser)]
#[command(name = "iopt", version, after_help = EXAMPLES)]
struct Cli {
    /// Files or directories to optimize (directories are scanned recursively)
    #[arg(required = true)]
    paths: Vec<PathBuf>,

    /// Don't write anything; exit 1 if any file could be optimized (CI mode)
    #[arg(long)]
    check: bool,

    /// Strip metadata (JPEG: EXIF/ICC/comments; PNG: non-essential chunks; GIF: comments)
    #[arg(long)]
    strip: bool,

    /// PNG optimization level (0 = fastest, 6 = slowest/smallest)
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(0..=6))]
    level: u8,

    /// Compress PNGs with Zopfli (much slower, usually smaller)
    #[arg(long)]
    zopfli: bool,

    /// Number of parallel workers (default: number of logical CPUs)
    #[arg(long, short = 'j')]
    threads: Option<usize>,

    /// Only print the summary and errors
    #[arg(long, short)]
    quiet: bool,
}

#[derive(Clone, Copy)]
enum Format {
    Png,
    Jpeg,
    Gif,
}

enum Outcome {
    Optimized { before: u64, after: u64 },
    Unchanged,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    jpeg::install_panic_hook();

    if let Some(n) = cli.threads
        && let Err(e) = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
    {
        eprintln!("error: failed to configure thread pool: {e}");
        return ExitCode::from(2);
    }

    let files = match collect_files(&cli.paths) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    if files.is_empty() {
        if !cli.quiet {
            println!("No PNG, JPEG or GIF files found.");
        }
        return ExitCode::SUCCESS;
    }

    let results: Vec<(PathBuf, Result<Outcome>)> = files
        .par_iter()
        .map(|(path, format)| {
            let outcome = process_file(path, *format, &cli);
            (path.clone(), outcome)
        })
        .collect();

    let mut optimized = 0u64;
    let mut unchanged = 0u64;
    let mut errors = 0u64;
    let mut bytes_before = 0u64;
    let mut bytes_after = 0u64;

    for (path, result) in &results {
        match result {
            Ok(Outcome::Optimized { before, after }) => {
                optimized += 1;
                bytes_before += before;
                bytes_after += after;
                if !cli.quiet {
                    let pct = (*before - *after) as f64 / *before as f64 * 100.0;
                    println!(
                        "{}  {} → {}  (-{:.1}%)",
                        path.display(),
                        human_bytes(*before),
                        human_bytes(*after),
                        pct
                    );
                }
            }
            Ok(Outcome::Unchanged) => unchanged += 1,
            Err(e) => {
                errors += 1;
                eprintln!("error: {}: {e:#}", path.display());
            }
        }
    }

    if !cli.quiet || optimized > 0 || errors > 0 {
        let saved = bytes_before.saturating_sub(bytes_after);
        let verb = if cli.check {
            "optimizable"
        } else {
            "optimized"
        };
        println!(
            "{optimized} {verb}, {unchanged} already optimal{}{}",
            if errors > 0 {
                format!(", {errors} failed")
            } else {
                String::new()
            },
            if optimized > 0 {
                format!(
                    ", {} {}",
                    human_bytes(saved),
                    if cli.check {
                        "possible savings"
                    } else {
                        "saved"
                    }
                )
            } else {
                String::new()
            },
        );
    }

    if errors > 0 {
        ExitCode::from(2)
    } else if cli.check && optimized > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Expand CLI paths into a list of image files. Directories are walked
/// recursively, keeping only files with supported image extensions. Files
/// passed explicitly must be supported images.
fn collect_files(paths: &[PathBuf]) -> Result<Vec<(PathBuf, Format)>> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            for entry in WalkDir::new(path) {
                let entry = entry?;
                if entry.file_type().is_file()
                    && let Some(format) = format_from_extension(entry.path())
                {
                    files.push((entry.into_path(), format));
                }
            }
        } else if path.is_file() {
            match format_from_extension(path) {
                Some(format) => files.push((path.clone(), format)),
                None => bail!(
                    "{}: unsupported file type (expected .png, .apng, .jpg, .jpeg or .gif)",
                    path.display()
                ),
            }
        } else {
            bail!("{}: no such file or directory", path.display());
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.dedup_by(|a, b| a.0 == b.0);
    Ok(files)
}

fn format_from_extension(path: &Path) -> Option<Format> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" | "apng" => Some(Format::Png),
        "jpg" | "jpeg" => Some(Format::Jpeg),
        "gif" => Some(Format::Gif),
        _ => None,
    }
}

fn process_file(path: &Path, format: Format, cli: &Cli) -> Result<Outcome> {
    let data = std::fs::read(path).context("failed to read")?;

    // Verify magic bytes so a mislabeled file can't crash the codecs.
    let magic_ok = match format {
        Format::Png => data.starts_with(b"\x89PNG\r\n\x1a\n"),
        Format::Jpeg => data.starts_with(b"\xff\xd8\xff"),
        Format::Gif => data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a"),
    };
    if !magic_ok {
        bail!("file content does not match its extension, skipping");
    }

    let optimized = match format {
        Format::Png => png::optimize(&data, cli.level, cli.zopfli, cli.strip)?,
        Format::Jpeg => jpeg::optimize(&data, cli.strip)?,
        Format::Gif => match gif::optimize(&data, cli.strip)? {
            Some(out) => out,
            None => return Ok(Outcome::Unchanged),
        },
    };

    if optimized.len() >= data.len() {
        return Ok(Outcome::Unchanged);
    }

    if !cli.check {
        write_atomically(path, &optimized).context("failed to write")?;
    }

    Ok(Outcome::Optimized {
        before: data.len() as u64,
        after: optimized.len() as u64,
    })
}

/// Write via a temp file in the same directory + rename, so a crash can
/// never leave a truncated image behind. Preserves the original permissions.
fn write_atomically(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{file_name}.iopt-tmp{}", std::process::id()));

    let result = (|| -> Result<()> {
        std::fs::write(&tmp, data)?;
        let perms = std::fs::metadata(path)?.permissions();
        std::fs::set_permissions(&tmp, perms)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
