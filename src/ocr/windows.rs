//! Native OCR via the Windows.Media.Ocr WinRT API.

use anyhow::{bail, Context, Result};
use std::path::Path;
use windows::Graphics::Imaging::{BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

pub fn extract_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).context("Failed to read image file")?;

    let stream = InMemoryRandomAccessStream::new().context("Failed to create stream")?;
    let writer = DataWriter::CreateDataWriter(&stream).context("Failed to create data writer")?;
    writer
        .WriteBytes(&bytes)
        .context("Failed to write image bytes")?;
    writer
        .StoreAsync()
        .context("Failed to store stream")?
        .join()?;
    writer
        .FlushAsync()
        .context("Failed to flush stream")?
        .join()?;
    writer.DetachStream().context("Failed to detach stream")?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .context("Failed to start decoding image")?
        .join()
        .context("Failed to decode image")?;
    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .context("Failed to start bitmap conversion")?
        .join()
        .context("Failed to get software bitmap")?;
    // Windows OCR requires Bgra8 or Gray8 pixel data.
    let bitmap = SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Bgra8)
        .context("Failed to convert bitmap to Bgra8")?;

    let max_dimension = OcrEngine::MaxImageDimension().unwrap_or(0) as i32;
    if bitmap.PixelWidth()? > max_dimension || bitmap.PixelHeight()? > max_dimension {
        bail!(
            "Image exceeds Windows OCR max dimension of {} pixels",
            max_dimension
        );
    }

    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .context("No Windows OCR engine available for user profile languages")?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .context("Failed to start OCR")?
        .join()
        .context("OCR recognition failed")?;

    let mut lines = Vec::new();
    for line in result.Lines().context("Failed to read OCR lines")? {
        lines.push(
            line.Text()
                .context("Failed to read OCR line text")?
                .to_string(),
        );
    }

    Ok(lines.join("\n"))
}
