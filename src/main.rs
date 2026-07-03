use anyhow::{Context, Result};
use clap::Parser;
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use recall::database::Database;
use recall::{indexer, search};

#[derive(Parser)]
#[command(author, version, about = "Recall is a CLI tool to OCR and search for text in your photos.", long_about = None)]
struct Cli {
    /// Text to search for in OCR results
    #[arg(index = 1)]
    search_text: Option<String>,

    /// The directory to search for photos, defaults to the current directory
    #[arg(index = 2, default_value = ".")]
    directory: PathBuf,

    /// Enable debug output
    #[arg(short, long)]
    debug: bool,

    /// Perform a search across all previously OCRed files
    #[arg(short, long)]
    global_search: bool,

    /// Number of images to process in parallel, defaults to number of CPUs
    #[arg(short, long)]
    num_threads: Option<usize>,

    /// Search the existing cache only, without scanning for new or changed files
    #[arg(long)]
    cached: bool,

    /// Delete all cached OCR results and exit
    #[arg(long)]
    wipe: bool,

    /// Maximum number of search results to show
    #[arg(short, long, default_value_t = 10)]
    limit: usize,

    /// Show credits and license information
    #[arg(long)]
    credits: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.credits {
        println!("Recall - OCR and search for text in your photos.");
        println!("Native OCR by Apple Vision (macOS) / Windows OCR (Windows) where available.");
        println!("Fallback OCR powered by ocrs models (CC-BY-SA-4.0 by Robert Knight; see https://huggingface.co/robertknight/ocrs)");
        return Ok(());
    }

    let console_layer = tracing_subscriber::fmt::layer()
        .pretty()
        .with_file(false)
        .with_level(true);
    let log_level = if cli.debug { "debug" } else { "info" };
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .parse_lossy(format!("recall={log_level}"));
    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .try_init()?;

    let Some(proj_dirs) = ProjectDirs::from("com", "adinschmidt", "recall") else {
        anyhow::bail!("Failed to get data path");
    };
    let data_path = proj_dirs.data_dir();
    fs::create_dir_all(data_path).context("Failed to create data directory")?;
    let db_path = data_path.join("data.sqlite");
    debug!("Data file path: {:?}", db_path);

    let db = Database::open(&db_path)?;

    if cli.wipe {
        db.wipe()?;
        info!("Cleared all cached OCR results");
        return Ok(());
    }

    if !cli.cached {
        if !cli.directory.is_dir() {
            anyhow::bail!("Not a directory: {}", cli.directory.display());
        }
        indexer::index_directory(&db, &cli.directory, cli.num_threads)
            .context("Error indexing photos")?;
    }

    if let Some(search_text) = cli.search_text {
        let directory_filter = if cli.global_search {
            None
        } else {
            let canonical = cli.directory.canonicalize().with_context(|| {
                format!(
                    "Failed to canonicalize directory: {}",
                    cli.directory.display()
                )
            })?;
            Some(canonical.to_string_lossy().into_owned())
        };

        let records = db.records_under(directory_filter.as_deref())?;
        let hits = search::fuzzy_search(&records, &search_text, cli.limit);

        if hits.is_empty() {
            eprintln!("No matches.");
            std::process::exit(1);
        }
        for hit in hits {
            debug!("Score {} for \"{}\"", hit.score, hit.path);
            println!("{}: {}", hit.path, truncate(&hit.line, 120));
        }
    }

    Ok(())
}

/// Truncate to at most `max_chars` characters, appending an ellipsis.
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}...")
}
