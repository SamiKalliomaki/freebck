use clap::{Parser, Subcommand};

use freebck::cmd::{backup::BackupArgs, restore::RestoreArgs};

/// freebck - The free backup tool
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the config file to use.
    #[arg(long, default_value = ".freebck/config.toml")]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create a new snapshot.
    Backup(BackupArgs),
    /// Restore from a snapshot.
    Restore(RestoreArgs),
}

#[tokio::main]
async fn main() {
    let _args = Cli::parse();
}
