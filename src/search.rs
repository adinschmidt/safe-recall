use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::path::Path;

use crate::database::OcrRecord;

/// A fuzzy-search match against a cached OCR result.
#[derive(Debug)]
pub struct SearchHit {
    /// Full path to the matched image file.
    pub path: String,
    /// The best-matching line of OCRed text.
    pub line: String,
    pub score: u32,
}

/// Fuzzy-search OCR records fzf-style: each line of a file's text is scored
/// against the query and the file's best line wins. Files with no match are
/// excluded entirely, so an empty result means "no matches".
pub fn fuzzy_search(records: &[OcrRecord], query: &str, limit: usize) -> Vec<SearchHit> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();

    let mut hits: Vec<SearchHit> = Vec::new();
    for record in records {
        let mut best: Option<(u32, &str)> = None;
        for line in record.text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let haystack = Utf32Str::new(line, &mut buf);
            if let Some(score) = pattern.score(haystack, &mut matcher) {
                if best.is_none_or(|(best_score, _)| score > best_score) {
                    best = Some((score, line));
                }
            }
        }
        if let Some((score, line)) = best {
            hits.push(SearchHit {
                path: Path::new(&record.path)
                    .join(&record.filename)
                    .to_string_lossy()
                    .into_owned(),
                line: line.to_string(),
                score,
            });
        }
    }

    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    hits.truncate(limit);
    hits
}
