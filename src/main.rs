use clap::{Parser, Subcommand};

use freebck::cmd::common::CommonArgs;

/// freebck - The free backup tool
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the config file to use.
    #[arg(long, default_value = ".freebck/config.toml")]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,

    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create a new snapshot.
    Backup {},
    /// Restore from a snapshot.
    Restore {},
}

#[tokio::main]
async fn main() {
    let _args = Cli::parse();
}
