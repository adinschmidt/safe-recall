use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

/// A cached OCR result row.
#[derive(Debug, Clone)]
pub struct OcrRecord {
    pub filename: String,
    /// Absolute path of the directory containing the file.
    pub path: String,
    pub text: String,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path).context("Failed to open database connection")?;
        Self::init(&conn)?;
        Ok(Self { conn })
    }

    #[doc(hidden)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("Failed to open in-memory database")?;
        Self::init(&conn)?;
        Ok(Self { conn })
    }

    fn init(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ocr_results (
                filename TEXT NOT NULL,
                path TEXT NOT NULL,
                text TEXT NOT NULL,
                ocr_date TEXT NOT NULL,
                ocr_success BOOLEAN NOT NULL,
                ocr_engine TEXT NOT NULL,
                PRIMARY KEY (filename, path)
            )",
            [],
        )
        .context("Failed to create table ocr_results")?;
        Ok(())
    }

    /// Returns the stored OCR date (RFC 3339) for a file, or `None` if it has
    /// never been OCRed.
    pub fn ocr_date(&self, filename: &str, directory: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT ocr_date FROM ocr_results WHERE filename = ?1 AND path = ?2")
            .context("Failed to prepare ocr_date statement")?;
        let date = stmt
            .query_row(params![filename, directory], |row| row.get(0))
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .context("Failed to query OCR date")?;
        Ok(date)
    }

    /// Insert or replace the OCR result for a file. `text` may be empty, which
    /// records that the file was processed but contained no text.
    pub fn store(&self, filename: &str, directory: &str, text: &str, engine: &str) -> Result<()> {
        let ocr_date = chrono::Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT OR REPLACE INTO ocr_results
                 (filename, path, text, ocr_date, ocr_success, ocr_engine)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![filename, directory, text, ocr_date, true, engine],
            )
            .context("Failed to store OCR result")?;
        Ok(())
    }

    /// All (directory, filename) pairs cached under `directory` (inclusive,
    /// recursive).
    pub fn files_under(&self, directory: &str) -> Result<Vec<(String, String)>> {
        let (condition, prefix) = Self::dir_scope(directory);
        let mut stmt = self
            .conn
            .prepare_cached(&format!(
                "SELECT path, filename FROM ocr_results WHERE {condition}"
            ))
            .context("Failed to prepare files_under statement")?;
        let rows = stmt
            .query_map(params![directory, prefix], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .context("Failed to query files under directory")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to read files under directory")
    }

    /// All records with non-empty text, optionally scoped to a directory
    /// (inclusive, recursive). `None` searches the whole cache.
    pub fn records_under(&self, directory: Option<&str>) -> Result<Vec<OcrRecord>> {
        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<OcrRecord> {
            Ok(OcrRecord {
                path: row.get(0)?,
                filename: row.get(1)?,
                text: row.get(2)?,
            })
        };
        let records = match directory {
            Some(dir) => {
                let (condition, prefix) = Self::dir_scope(dir);
                let mut stmt = self
                    .conn
                    .prepare_cached(&format!(
                        "SELECT path, filename, text FROM ocr_results
                         WHERE text != '' AND ({condition})"
                    ))
                    .context("Failed to prepare scoped records statement")?;
                let rows = stmt.query_map(params![dir, prefix], map_row)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            }
            None => {
                let mut stmt = self
                    .conn
                    .prepare_cached("SELECT path, filename, text FROM ocr_results WHERE text != ''")
                    .context("Failed to prepare global records statement")?;
                let rows = stmt.query_map([], map_row)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            }
        };
        records.context("Failed to read OCR records")
    }

    pub fn delete(&self, filename: &str, directory: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM ocr_results WHERE filename = ?1 AND path = ?2",
                params![filename, directory],
            )
            .context("Failed to delete record")?;
        Ok(())
    }

    /// Remove every cached OCR result.
    pub fn wipe(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM ocr_results", [])
            .context("Failed to wipe database")?;
        Ok(())
    }

    /// SQL condition + LIKE prefix matching a directory itself (?1) and
    /// everything below it (?2). The separator is appended before escaping so
    /// that on Windows (where the separator is also the ESCAPE character) it
    /// gets escaped too, instead of accidentally escaping the `%` wildcard.
    fn dir_scope(directory: &str) -> (&'static str, String) {
        let escaped = format!("{directory}{}", std::path::MAIN_SEPARATOR)
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let prefix = format!("{escaped}%");
        ("path = ?1 OR path LIKE ?2 ESCAPE '\\'", prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::Database;

    #[test]
    fn dir_scope_keeps_wildcard_unescaped() {
        let sep = std::path::MAIN_SEPARATOR;
        let (_, prefix) = Database::dir_scope(&format!("{sep}photos"));
        // The trailing % must be a bare wildcard: not preceded by the escape
        // character (on Windows the separator IS the escape character, so it
        // must itself be escaped as \\).
        assert!(prefix.ends_with('%'));
        assert!(
            !prefix.ends_with("\\%") || prefix.ends_with("\\\\%"),
            "wildcard is escaped away in {prefix:?}"
        );
    }

    #[test]
    fn dir_scope_escapes_like_wildcards_in_path() {
        let (_, prefix) = Database::dir_scope("/pho_tos/100%");
        assert!(prefix.contains("pho\\_tos"));
        assert!(prefix.contains("100\\%"));
    }
}
