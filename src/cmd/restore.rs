use std::{io::Cursor, os::unix::prelude::MetadataExt, path::PathBuf};

use crate::{
    cmd::common::{get_dir_entry, IntoCommandError, IntoCommandResult},
    data::backup::{sub_dir_entry, DirEntry, FileEntry, Snapshot, SubDirEntry},
    storage::Collection,
    util::time::{as_unix_timestamp, system_time_from_unix_timestamp},
};

use super::common::{
    CommandError, CommandErrorKind, CommandResult, KeepGoingOrErr, ProgramContext,
};
use async_recursion::async_recursion;
use clap::Args;
use futures::future::{try_join_all, BoxFuture};
use log::{debug, info};
use prost::Message;
use tokio::{
    fs::{self, File, OpenOptions},
    io::AsyncWriteExt,
};

#[derive(Debug, Args)]
pub struct RestoreArgs {
    pub snapshot: String,
    /// Keep going on errors.
    #[arg(long)]
    pub keep_going: bool,
    /// Don't override existing files.
    #[arg(long)]
    pub no_override_files: bool,
}

pub async fn restore(context: &ProgramContext, args: &RestoreArgs) -> CommandResult {
    info!("Restore starting");

    let snapshot_name = format!("{}/{}", context.archive_name, args.snapshot);

    let mut snapshot_buf = Vec::new();
    if let Err(e) = context
        .storage
        .read(Collection::Snapshot, &snapshot_name, &mut snapshot_buf)
        .await
    {
        if e.kind() == std::io::ErrorKind::NotFound {
            return Err(CommandError::new(
                CommandErrorKind::User,
                format!("Snapshot not found {}", snapshot_name),
            ));
        } else {
            return Err(
                e.into_command_error(CommandErrorKind::System, "Failed to download snapshot")
            );
        }
    }
    let snapshot = Snapshot::decode(Cursor::new(snapshot_buf)).map_err(|e| {
        CommandError::with_source(
            CommandErrorKind::Corrupt,
            format!("Error decoding snapshot"),
            Box::new(e),
        )
    })?;
    let root_dir_entry = get_dir_entry(context, &snapshot.root_hash).await?;

    restore_dir(context, args, root_dir_entry, &context.backup_target).await?;

    info!("Restore complete");
    Ok(())
}

#[async_recursion]
async fn restore_dir(
    context: &ProgramContext,
    args: &RestoreArgs,
    root_dir_entry: DirEntry,
    target: &PathBuf,
) -> CommandResult {
    debug!("Restoring dir {}", target.display());

    match fs::metadata(target).await {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() {
                Err(CommandError::new(
                    CommandErrorKind::FileSystemConflict,
                    "Target exists and is not a directory".to_string(),
                ))
            } else {
                Ok(())
            }
        }
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {
                fs::create_dir(target)
                    .await
                    .into_command_result(CommandErrorKind::System, "Failed to create directory")?;
                Ok(())
            }
            _ => Err(
                e.into_command_error(CommandErrorKind::System, "Failed to get directory metadata")
            ),
        },
    }?;

    let DirEntry {
        file: files,
        sub_dir: sub_dirs,
        ..
    } = root_dir_entry;

    let mut results: Vec<BoxFuture<CommandResult>> = Vec::new();
    for SubDirEntry { name, content, .. } in sub_dirs.into_iter() {
        let dir_target = target.join(&name);
        let content = content.ok_or_else(|| {
            CommandError::new(
                CommandErrorKind::Corrupt,
                format!("Sub dir entry without content {}", dir_target.display()),
            )
        })?;

        results.push(Box::pin(async move {
            let dir_entry = match content {
                sub_dir_entry::Content::Inline(dir_entry) => Ok(dir_entry),
                sub_dir_entry::Content::Hash(hash) => get_dir_entry(context, &hash).await,
            }?;

            restore_dir(context, args, dir_entry, &dir_target)
                .await
                .keep_going_or_err(args.keep_going, |e| {
                    e.with_message(format!("Failed to restore dir {}", dir_target.display()))
                })?;
            Ok(())
        }));
    }

    for file_entry in files.into_iter() {
        results.push(Box::pin(async move {
            let file_target = target.join(&file_entry.name);
            restore_file(context, args, file_entry, &file_target)
                .await
                .keep_going_or_err(args.keep_going, |e| {
                    e.with_message(format!("Failed to restore file {}", file_target.display()))
                })?;
            Ok(())
        }));
    }

    try_join_all(results).await?;

    Ok(())
}

async fn restore_file(
    context: &ProgramContext,
    args: &RestoreArgs,
    file_entry: FileEntry,
    target_path: &PathBuf,
) -> CommandResult {
    let FileEntry {
        name: _,
        chunk_hash: chunk_hashes,
        size,
        modified,
        ..
    } = file_entry;

    debug!("Restoring file {}", target_path.display());

    enum Matches {
        DoesNotExist,
        Matches,
        DoesNotMatch,
    }
    let existing_matches = match fs::metadata(target_path).await {
        Ok(metadata) => 'matches: {
            let existing_size = metadata.size();
            let existing_modified = match metadata.modified() {
                Ok(m) => as_unix_timestamp(m),
                Err(e) => {
                    debug!(
                        "Failed to get modified time for {}: {}",
                        target_path.display(),
                        e
                    );
                    break 'matches Matches::DoesNotMatch;
                }
            };

            if existing_size != size && existing_modified != modified {
                break 'matches Matches::DoesNotMatch;
            }

            // TODO: Check hash if requested.
            Matches::Matches
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                debug!(
                    "Failed to get metadata for {}: {}",
                    target_path.display(),
                    e
                );
            }
            Matches::DoesNotExist
        }
    };
    match existing_matches {
        Matches::Matches => return Ok(()),
        Matches::DoesNotMatch => {
            if args.no_override_files {
                return Err(CommandError::new(
                    CommandErrorKind::FileSystemConflict,
                    format!("{} already exists", target_path.display()),
                ));
            }
        }
        Matches::DoesNotExist => {}
    }

    let mut open_options = OpenOptions::new();

    open_options.write(true);
    if args.no_override_files {
        open_options.create(true).truncate(true);
    } else {
        open_options.create_new(true);
    }

    let mut target_file = open_options
        .open(target_path)
        .await
        .into_command_result(CommandErrorKind::System, "Failed to open file for writing")?;

    let mut buffer = Vec::new(); // TODO: Setup a pool of buffers.
    for chunk_hash in chunk_hashes.into_iter() {
        context
            .storage
            .read(Collection::Blob, &chunk_hash, &mut buffer)
            .await
            .keep_going_or_err(args.keep_going, |e| {
                CommandError::with_source(
                    CommandErrorKind::Corrupt,
                    format!("Failed to read chunk {}", chunk_hash),
                    Box::new(e),
                )
            })?;
        target_file
            .write_all(&buffer)
            .await
            .into_command_result(CommandErrorKind::System, "Failed to write chunk")?;
    }

    let target_file = target_file.into_std().await;
    target_file
        .set_modified(system_time_from_unix_timestamp(modified)?)
        .into_command_result(CommandErrorKind::System, "Failed to set modified time")?;

    let target_file = File::from_std(target_file);
    target_file
        .sync_all()
        .await
        .into_command_result(CommandErrorKind::System, "Failed to sync changes")?;

    Ok(())
}
