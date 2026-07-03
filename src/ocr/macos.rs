//! Native OCR via the macOS Vision framework (VNRecognizeTextRequest).

use anyhow::{anyhow, Context, Result};
use objc2::AnyThread;
use objc2_foundation::{NSArray, NSData, NSDictionary};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
};
use std::path::Path;

pub fn extract_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).context("Failed to read image file")?;
    let data = NSData::with_bytes(&bytes);

    let handler = VNImageRequestHandler::initWithData_options(
        VNImageRequestHandler::alloc(),
        &data,
        &NSDictionary::new(),
    );

    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    request.setUsesLanguageCorrection(true);

    let base_request: &VNRequest = &request;
    let requests = NSArray::from_slice(&[base_request]);
    handler
        .performRequests_error(&requests)
        .map_err(|e| anyhow!("Vision request failed: {}", e.localizedDescription()))?;

    let mut lines = Vec::new();
    if let Some(observations) = request.results() {
        for observation in observations.iter() {
            let candidates = observation.topCandidates(1);
            if let Some(candidate) = candidates.iter().next() {
                lines.push(candidate.string().to_string());
            }
        }
    }

    Ok(lines.join("\n"))
}
