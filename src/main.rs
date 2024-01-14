use std::{
    error::Error,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};

use freebck::{
    cmd::{
        backup::{backup, BackupArgs},
        common::{
            CommandError, CommandErrorKind, CommandResult, IntoCommandResult, ProgramContext,
        },
        restore::{restore, RestoreArgs},
    },
    data::config::{ArchiveConfig, StorageConfig},
    storage::{file::FileStorage, Storage},
};
use log::error;
use tokio::fs;

/// freebck - The free backup tool
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the config file to use.
    #[arg(long, default_value = ".freebck/config.toml")]
    config: String,

    /// Enable verbose logging.
    #[arg(long, short)]
    verbose: bool,

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

async fn parse_archive_config(path: &Path) -> CommandResult<ArchiveConfig> {
    let raw_toml = fs::read_to_string(path).await.map_err(|e| {
        CommandError::with_source(
            CommandErrorKind::User,
            format!("Error reading archive config file: {}", path.display()),
            Box::new(e),
        )
    })?;
    return toml::from_str(&raw_toml).map_err(|e| {
        CommandError::with_source(
            CommandErrorKind::User,
            format!("Error parsing archive config"),
            Box::new(e),
        )
    });
}

async fn create_storage(
    config_path: &Path,
    config: &ArchiveConfig,
) -> CommandResult<Box<dyn Storage>> {
    let storage = match config.storage {
        StorageConfig::File(ref file_config) => Box::new(
            FileStorage::from_config(config_path, &file_config)
                .await
                .into_command_result(
                    CommandErrorKind::System,
                    "Failed to initialize file storage",
                )?,
        ),
    };

    Ok(storage)
}

async fn run(args: Cli) -> CommandResult {
    let config_path = PathBuf::from(&args.config);

    let archive_config = parse_archive_config(&config_path).await?;
    let backup_target = config_path.parent().unwrap().join(&archive_config.path);
    let storage = create_storage(&config_path, &archive_config).await?;

    let context = ProgramContext {
        archive_name: archive_config.name,
        storage,
        backup_target,
    };

    match args.command {
        Commands::Backup(backup_args) => backup(&context, &backup_args).await,
        Commands::Restore(restore_args) => restore(&context, &restore_args).await,
    }
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    let log_env = env_logger::Env::default()
        .filter_or(
            "FREEBCK_LOG_LEVEL",
            if args.verbose { "debug" } else { "info" },
        )
        .write_style("FREEBCK_LOG_STYLE");
    env_logger::Builder::from_env(log_env)
        .format_level(false)
        .format_target(false)
        .format_indent(None)
        .format_timestamp(None)
        .init();

    if let Err(e) = run(args).await {
        error!("{}", e);

        let mut source = e.source();
        while source.is_some() {
            let e = source.unwrap();

            error!("\nCaused by: {}", e);
            source = e.source();
        }
    }
}
