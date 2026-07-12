//! pksp edge CLI — serve / migrate

use anyhow::Result;
use clap::{Parser, Subcommand};
use pksp_db::{connect_pool, Settings};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "pksp", about = "PKSP Check-In edge runtime (Rust)")]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run API + vision + media supervisor
    Serve,
    /// Apply migrations and upsert cameras
    Migrate,
}

/// Load `.env` from cwd, then walk parents (so monorepo root `.env` works from `apps/edge`).
fn load_dotenv() {
    // Prefer nearest .env first
    if dotenvy::dotenv().is_ok() {
        return;
    }
    // Walk up a few levels for monorepo root
    let mut dir = std::env::current_dir().ok();
    for _ in 0..5 {
        let Some(d) = dir else { break };
        let candidate = d.join(".env");
        if candidate.is_file() {
            let _ = dotenvy::from_path(&candidate);
            return;
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Commands::Serve => {
            let settings = Settings::from_env();
            tracing::info!(
                data_dir = %settings.data_dir.display(),
                database_url = %settings.database_url,
                "starting pksp serve"
            );
            pksp_api::serve(settings).await?;
        }
        Commands::Migrate => {
            let settings = Settings::from_env();
            let _pool = connect_pool(&settings).await?;
            println!(
                "migrations applied; cameras upserted; data_dir={} database_url={}",
                settings.data_dir.display(),
                settings.database_url
            );
        }
    }
    Ok(())
}
