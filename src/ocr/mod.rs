#[doc(hidden)] // public so integration tests can exercise the fallback engine
pub mod ocrs_engine;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use anyhow::{Context, Result};
use image::ImageReader;
use std::path::Path;
use tracing::debug;

/// The text extracted from an image and the engine that produced it.
#[derive(Debug)]
pub struct OcrOutcome {
    pub text: String,
    pub engine: &'static str,
}

/// Extract text from an image file.
///
/// Tries the platform's native OCR first (Apple Vision on macOS, Windows OCR
/// on Windows) and falls back to the embedded ocrs engine if native OCR is
/// unavailable or fails (always the case on Linux).
pub fn extract_text(path: &Path) -> Result<OcrOutcome> {
    if let Some(outcome) = native_ocr(path) {
        return Ok(outcome);
    }

    let image = ImageReader::open(path)
        .context("Failed to open image")?
        .decode()
        .context("Failed to decode image")?;
    let text = ocrs_engine::extract_text(&image).context("Failed to extract text with ocrs")?;
    Ok(OcrOutcome {
        text,
        engine: "ocrs",
    })
}

#[cfg(target_os = "macos")]
fn native_ocr(path: &Path) -> Option<OcrOutcome> {
    match macos::extract_text(path) {
        Ok(text) => Some(OcrOutcome {
            text,
            engine: "apple-vision",
        }),
        Err(e) => {
            debug!(
                "Native OCR failed for \"{}\", falling back to ocrs: {:#}",
                path.display(),
                e
            );
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn native_ocr(path: &Path) -> Option<OcrOutcome> {
    match windows::extract_text(path) {
        Ok(text) => Some(OcrOutcome {
            text,
            engine: "windows-ocr",
        }),
        Err(e) => {
            debug!(
                "Native OCR failed for \"{}\", falling back to ocrs: {:#}",
                path.display(),
                e
            );
            None
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_ocr(path: &Path) -> Option<OcrOutcome> {
    debug!(
        "No native OCR on this platform, using ocrs for \"{}\"",
        path.display()
    );
    None
}
