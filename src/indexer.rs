use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

use crate::database::Database;
use crate::ocr;

/// Supported image extensions for OCR
pub const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "tiff", "webp"];

#[derive(Debug, Default)]
pub struct IndexStats {
    /// Files OCRed (new or changed since last run).
    pub processed: usize,
    /// Files whose OCR failed.
    pub failed: usize,
    /// Cached entries removed because the file no longer exists on disk.
    pub pruned: usize,
}

/// Recursively index all supported images under `directory`: OCR new/changed
/// files in parallel, cache the results, and prune cache entries for files
/// that no longer exist. Hidden files and ignore rules (.gitignore etc.) are
/// respected during the walk.
pub fn index_directory(
    db: &Database,
    directory: &Path,
    num_threads: Option<usize>,
) -> Result<IndexStats> {
    let root = directory
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize directory: {}", directory.display()))?;
    let root_str = root
        .to_str()
        .context("Failed to convert directory path to string")?;

    let mut stats = IndexStats::default();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut candidates: Vec<PathBuf> = Vec::new();

    for entry in WalkBuilder::new(&root).build() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to read directory entry: {}", e);
                continue;
            }
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.into_path();

        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !SUPPORTED_EXTENSIONS.contains(&extension.to_lowercase().as_str()) {
            continue;
        }

        let (Some(filename), Some(parent)) = (
            path.file_name().and_then(|n| n.to_str()),
            path.parent().and_then(|p| p.to_str()),
        ) else {
            debug!("Skipping non-UTF-8 path: {}", path.display());
            continue;
        };
        seen.insert((parent.to_string(), filename.to_string()));

        match needs_ocr(db, &path) {
            Ok(true) => candidates.push(path),
            Ok(false) => {}
            Err(e) => error!("Failed to check cache for {}: {}", path.display(), e),
        }
    }

    if !candidates.is_empty() {
        info!("OCRing {} file(s)", candidates.len());
    }

    // OCR in parallel, then write results serially.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads.unwrap_or(0))
        .build()
        .context("Failed to build thread pool")?;
    let results: Vec<(PathBuf, Result<ocr::OcrOutcome>)> = pool.install(|| {
        candidates
            .into_par_iter()
            .map(|path| {
                info!("Processing file: \"{}\"", path.display());
                let outcome = ocr::extract_text(&path);
                (path, outcome)
            })
            .collect()
    });

    for (path, outcome) in results {
        match outcome {
            Ok(outcome) => {
                let text = outcome.text.trim();
                if text.is_empty() {
                    debug!("No text found in \"{}\"", path.display());
                }
                if let Err(e) = store_result(db, &path, text, outcome.engine) {
                    error!("Failed to store result for \"{}\": {}", path.display(), e);
                    stats.failed += 1;
                } else {
                    stats.processed += 1;
                }
            }
            Err(e) => {
                error!("Error processing \"{}\": {:#}", path.display(), e);
                stats.failed += 1;
            }
        }
    }

    // Prune cache entries for files that are gone from disk (or now ignored).
    for (dir, filename) in db.files_under(root_str)? {
        if seen.contains(&(dir.clone(), filename.clone())) {
            continue;
        }
        match db.delete(&filename, &dir) {
            Ok(()) => {
                debug!("Pruned missing file from cache: \"{}/{}\"", dir, filename);
                stats.pruned += 1;
            }
            Err(e) => error!("Failed to prune \"{}/{}\": {}", dir, filename, e),
        }
    }
    if stats.pruned > 0 {
        info!("Pruned {} stale cache entries", stats.pruned);
    }

    Ok(stats)
}

fn store_result(db: &Database, path: &Path, text: &str, engine: &str) -> Result<()> {
    let filename = path
        .file_name()
        .context("Failed to get filename from path")?
        .to_string_lossy();
    let parent = path
        .parent()
        .context("Failed to get parent path")?
        .to_string_lossy();
    db.store(&filename, &parent, text, engine)
}

/// Returns true if the file has never been OCRed, or its OCR result is older
/// than the file's last modification.
pub fn needs_ocr(db: &Database, path: &Path) -> Result<bool> {
    let filename = path
        .file_name()
        .context("Failed to get filename from path")?
        .to_string_lossy();
    let parent = path
        .parent()
        .context("Failed to get parent path")?
        .to_string_lossy();

    let Some(ocr_date_str) = db.ocr_date(&filename, &parent)? else {
        return Ok(true);
    };
    let ocr_date = chrono::DateTime::parse_from_rfc3339(&ocr_date_str)
        .context("Failed to parse OCR date")?
        .with_timezone(&chrono::Utc);

    let last_modified = path
        .metadata()
        .and_then(|m| m.modified())
        .context("Failed to get last modified date of file")?;
    let last_modified = chrono::DateTime::<chrono::Utc>::from(last_modified);

    Ok(ocr_date < last_modified)
}
