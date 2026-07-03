use std::fs;
use std::path::Path;

use recall::database::{Database, OcrRecord};
use recall::{indexer, ocr, search};

const SAMPLE_IMAGE: &str = "test/data/synthetic_test_ocr.png";

fn record(path: &str, filename: &str, text: &str) -> OcrRecord {
    OcrRecord {
        filename: filename.to_string(),
        path: path.to_string(),
        text: text.to_string(),
    }
}

#[test]
fn database_roundtrip_and_wipe() {
    let db = Database::open_in_memory().unwrap();
    assert_eq!(db.ocr_date("a.png", "/photos").unwrap(), None);

    db.store("a.png", "/photos", "hello world", "test").unwrap();
    assert!(db.ocr_date("a.png", "/photos").unwrap().is_some());

    // Replacing updates rather than duplicating (same primary key).
    db.store("a.png", "/photos", "hello again", "test").unwrap();
    let records = db.records_under(None).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].text, "hello again");

    db.delete("a.png", "/photos").unwrap();
    assert_eq!(db.ocr_date("a.png", "/photos").unwrap(), None);

    db.store("b.png", "/photos", "text", "test").unwrap();
    db.wipe().unwrap();
    assert!(db.records_under(None).unwrap().is_empty());
}

#[test]
fn records_under_scopes_to_directory_recursively() {
    let sep = std::path::MAIN_SEPARATOR;
    let root = format!("{sep}photos");
    let nested = format!("{sep}photos{sep}vacation");
    let sibling = format!("{sep}photos-backup");

    let db = Database::open_in_memory().unwrap();
    db.store("a.png", &root, "in root", "test").unwrap();
    db.store("b.png", &nested, "in nested", "test").unwrap();
    db.store("c.png", &sibling, "in sibling", "test").unwrap();
    // Empty text is cached (to avoid re-OCR) but excluded from search.
    db.store("empty.png", &root, "", "test").unwrap();

    let scoped = db.records_under(Some(&root)).unwrap();
    let mut names: Vec<&str> = scoped.iter().map(|r| r.filename.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["a.png", "b.png"]);

    // files_under includes empty-text entries (they exist on disk).
    let files = db.files_under(&root).unwrap();
    assert_eq!(files.len(), 3);

    let all = db.records_under(None).unwrap();
    assert_eq!(all.len(), 3);
}

#[test]
fn fuzzy_search_ranks_and_filters() {
    let records = vec![
        record("/a", "exact.png", "the quick brown fox\nsecond line"),
        record("/a", "fuzzy.png", "quirky bounce fixture"),
        record("/a", "unrelated.png", "completely different words"),
    ];

    let hits = search::fuzzy_search(&records, "quick brown", 10);
    assert!(!hits.is_empty());
    assert!(hits[0].path.ends_with("exact.png"));
    assert_eq!(hits[0].line, "the quick brown fox");
    assert!(!hits.iter().any(|h| h.path.ends_with("unrelated.png")));

    // Fuzzy: typo'd query still finds the target.
    let hits = search::fuzzy_search(&records, "qck brwn fx", 10);
    assert!(hits.iter().any(|h| h.path.ends_with("exact.png")));

    // No match at all -> empty, not top-N noise.
    let hits = search::fuzzy_search(&records, "zzzqqqxxx", 10);
    assert!(hits.is_empty());

    // Limit is respected.
    let hits = search::fuzzy_search(&records, "e", 1);
    assert_eq!(hits.len(), 1);
}

#[test]
fn ocrs_fallback_engine_reads_sample_image() {
    let image = image::ImageReader::open(SAMPLE_IMAGE)
        .unwrap()
        .decode()
        .unwrap();
    let text = ocr::ocrs_engine::extract_text(&image).unwrap();
    assert!(
        text.to_lowercase().contains("test"),
        "expected sample text, got: {text:?}"
    );
}

#[test]
fn platform_ocr_reads_sample_image() {
    let outcome = ocr::extract_text(Path::new(SAMPLE_IMAGE)).unwrap();
    assert!(
        outcome.text.to_lowercase().contains("test"),
        "expected sample text via {}, got: {:?}",
        outcome.engine,
        outcome.text
    );
}

#[test]
fn index_recurses_caches_and_prunes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let nested = root.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::copy(SAMPLE_IMAGE, root.join("top.png")).unwrap();
    fs::copy(SAMPLE_IMAGE, nested.join("deep.png")).unwrap();
    // Unsupported extension is ignored.
    fs::write(root.join("notes.txt"), "not an image").unwrap();

    let db = Database::open_in_memory().unwrap();

    let stats = indexer::index_directory(&db, root, Some(2)).unwrap();
    assert_eq!(stats.processed, 2, "both images OCRed");
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.pruned, 0);

    let canonical_root = root.canonicalize().unwrap();
    let records = db
        .records_under(Some(canonical_root.to_str().unwrap()))
        .unwrap();
    assert_eq!(records.len(), 2, "recursive walk found the nested image");

    // Second run: everything cached, nothing reprocessed.
    let stats = indexer::index_directory(&db, root, Some(2)).unwrap();
    assert_eq!(stats.processed, 0, "cache hit skips OCR");

    // Touching a file forces re-OCR.
    let future = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    fs::File::options()
        .append(true)
        .open(root.join("top.png"))
        .unwrap()
        .set_modified(future)
        .unwrap();
    let stats = indexer::index_directory(&db, root, Some(2)).unwrap();
    assert_eq!(stats.processed, 1, "modified file re-OCRed");

    // Deleting a file prunes its cache entry.
    fs::remove_file(nested.join("deep.png")).unwrap();
    let stats = indexer::index_directory(&db, root, Some(2)).unwrap();
    assert_eq!(stats.pruned, 1);
    let records = db
        .records_under(Some(canonical_root.to_str().unwrap()))
        .unwrap();
    assert_eq!(records.len(), 1);
}

#[test]
fn needs_ocr_respects_cache_dates() {
    let tmp = tempfile::tempdir().unwrap();
    let image_path = tmp.path().join("img.png");
    fs::copy(SAMPLE_IMAGE, &image_path).unwrap();

    let db = Database::open_in_memory().unwrap();
    assert!(indexer::needs_ocr(&db, &image_path).unwrap());

    let filename = image_path.file_name().unwrap().to_str().unwrap();
    let parent = image_path.parent().unwrap().to_str().unwrap();
    db.store(filename, parent, "text", "test").unwrap();
    assert!(!indexer::needs_ocr(&db, &image_path).unwrap());

    let future = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    fs::File::options()
        .append(true)
        .open(&image_path)
        .unwrap()
        .set_modified(future)
        .unwrap();
    assert!(indexer::needs_ocr(&db, &image_path).unwrap());
}
