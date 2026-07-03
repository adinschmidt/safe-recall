# AGENTS.md (Repo Guide for Coding Agents)

This repository is a Rust CLI called **Recall / SafeRecall**. It OCRs images (native OS OCR with an embedded ocrs fallback), caches results in SQLite, and fuzzy-searches the cached text fzf-style.

This file is written for agentic coding tools (Codex/Cursor/etc.). Keep changes small, follow existing patterns, and run at least `cargo fmt` + `cargo test` before finishing.

## Quick Commands

### Setup

- Install Rust toolchain (stable) and components:
  - `rustup update stable`
  - `rustup component add rustfmt clippy`
- Notes:
  - ocrs models are embedded from `models/` using `include_bytes!` (see `src/ocr/ocrs_engine.rs`).
  - SQLite is compiled in via rusqlite's `bundled` feature; no system SQLite needed.
  - Everything runs offline; there are no runtime downloads.

### Build

- Debug build:
  - `cargo build`
- Release build:
  - `cargo build --release`
- Run the CLI:
  - `cargo run -- --help`
  - `cargo run -- "needle" .`

### Format

- Auto-format:
  - `cargo fmt`
- Check formatting in CI-like mode:
  - `cargo fmt -- --check`

### Lint (Clippy)

- Lint default targets:
  - `cargo clippy`
- Lint the full crate surface (recommended for PRs):
  - `cargo clippy --all-targets --all-features -- -D warnings`

Tip: there is a `bacon.toml` for running clippy continuously:
- Install bacon: `cargo install bacon`
- Run: `bacon clippy-all`

### Test

Integration tests live in `tests/integration.rs` and use the sample image in `test/data/`:
- `cargo test`

#### Run a single test

- Single test by substring:
  - `cargo test test_name_substring`
- Integration test file:
  - `cargo test --test integration`
- Exact match (avoids running similarly-named tests):
  - `cargo test test_name -- --exact`

### CI

GitHub Actions builds release binaries for Windows (MSVC), macOS (x86_64 + ARM64), and Linux:
- `.github/workflows/build.yml`

Local equivalent of the main CI step:
- `cargo build --release`

The Windows OCR module cannot be compiled on other hosts; to type-check it, copy `src/ocr/windows.rs` into a scratch crate with the `windows` dependency and run `cargo check --target x86_64-pc-windows-msvc`.

## Project Layout

- `src/main.rs`: CLI entrypoint (clap) — wires indexing and search together.
- `src/lib.rs`: library root exposing the modules below (so integration tests can use them).
- `src/indexer.rs`: recursive directory walk (`ignore` crate), parallel OCR (rayon), cache freshness (mtime vs OCR date), pruning of deleted files.
- `src/ocr/mod.rs`: OCR dispatch — native engine first, ocrs fallback.
- `src/ocr/macos.rs`: Apple Vision OCR (`objc2-vision`), macOS only.
- `src/ocr/windows.rs`: Windows.Media.Ocr (`windows` crate), Windows only.
- `src/ocr/ocrs_engine.rs`: embedded ocrs engine (models compiled into the binary).
- `src/database.rs`: SQLite cache (`ocr_results` table) — store/lookup/prune/wipe.
- `src/search.rs`: fzf-style fuzzy search over cached text (`nucleo-matcher`), scored per line.
- `models/`: `.rten` models embedded into the binary.
- `test/data/`: sample images used by integration tests (the sample PNG contains the text "tech.lol").
- `tests/integration.rs`: integration tests for database, indexer, search, and both OCR paths.

## Code Style Guidelines (Rust)

### Formatting

- Use rustfmt defaults (`cargo fmt`). Do not hand-format.
- Keep line widths rustfmt-friendly; prefer wrapping method chains as rustfmt chooses.

### Imports

Follow standard rustfmt ordering, matching existing files:
- External crates first (`anyhow`, `clap`, `image`, `rusqlite`, etc.)
- Then `std::*`
- Then `crate::*`

Guidelines:
- Prefer grouped imports like `use anyhow::{Context, Result};`.
- Avoid wildcard imports.

### Types, Results, and Error Handling

Error handling conventions are consistent across the codebase:
- Prefer `anyhow::Result<T>` in application code.
- Add context on fallible operations with `Context`:
  - `foo().context("what failed")?`
  - `foo().with_context(|| format!("...") )?` when you need dynamic context.
- Prefer early returns and `let Some(x) = ... else { ... };` to avoid deep nesting.
- Avoid `unwrap()`/`expect()` in request-path code.
  - The embedded OCR engine init is deliberately fallible (`LazyLock<Result<OcrEngine>>` in `src/ocr/ocrs_engine.rs`) so model-load failures surface as CLI errors, not panics. Keep it that way.
- Native OCR failures must not abort processing: they are logged at `debug!` and fall back to ocrs (see `src/ocr/mod.rs`).

### Logging

Logging uses `tracing`:
- Prefer `info!` for high-level progress, `debug!` for diagnostics, `error!` for recoverable failures.
- Keep log messages actionable and include paths/ids.
- When continuing after an error (e.g., skipping a file), log at `error!` and `continue`.

### Naming

- Modules/files: `snake_case`.
- Types/traits: `PascalCase`.
- Functions/variables: `snake_case`.
- Constants: `SCREAMING_SNAKE_CASE`.
- Prefer descriptive names over abbreviations (`directory_filter`, `absolute_path`).

### Concurrency

- The CLI is synchronous; parallelism happens only inside `indexer::index_directory` via a rayon thread pool sized by the `--num-threads` flag (0 = rayon default).
- OCR runs in parallel; database writes happen serially afterwards on the main thread. Keep it that way — `rusqlite::Connection` is not `Sync`.

### Strings and Paths

- Prefer `Path`/`PathBuf` as long as possible; convert to string at boundaries.
- The walk root is canonicalized once in `index_directory`; walked paths inherit that prefix. Anything stored to or queried from the database must go through the same canonicalization.
- Files are stored as (`path` = parent directory, `filename`) pairs.

### Database patterns (SQLite via rusqlite)

- Table name is `ocr_results`, primary key (`filename`, `path`); see `src/database.rs`.
- All SQL uses positional parameters (`?1`, `?2`) — never interpolate values into SQL strings.
- Directory scoping (recursive) uses `dir_scope()`: `path = ?1 OR path LIKE ?2 ESCAPE '\'` with LIKE wildcards escaped. Reuse it for any new scoped query.
- Empty OCR text is stored (so files without text are not re-OCRed every run) but excluded from search by `records_under`.
- If you add a column, update `init()`, `store()`, and the queries that read rows, and consider existing user databases (the schema has no migrations yet).

### CLI (clap)

- CLI is defined in `src/main.rs` via `#[derive(Parser)]`.
- Keep args backward-compatible unless explicitly requested.
- Prefer `Option<T>` for optional args and provide sensible defaults via `#[arg(default_value = ...)]`.
- Exit code 1 means "no matches" (grep convention).

## Repo-Specific Rules (Cursor/Copilot)

- No `.cursor/rules/` or `.cursorrules` files were found.
- No `.github/copilot-instructions.md` file was found.

If you add such rule files in the future, update this section so agents can follow them.
