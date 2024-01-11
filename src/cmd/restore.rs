use std::{
    io::{self, Cursor},
    path::PathBuf,
};

use crate::{
    data::backup::{sub_dir_entry, DirEntry, FileEntry, Snapshot, SubDirEntry},
    storage::Collection,
    util::fs::read_file_from_storage,
};

use super::common::{CommandError, CommandErrorKind, CommandResult, ProgramContext};
use async_recursion::async_recursion;
use futures::future::BoxFuture;
use log::{debug, info, warn};
use prost::Message;
use tokio::fs;

pub struct RestoreArgs {
    pub snapshot: String,
    pub target: PathBuf,
    pub override_files: bool,
    pub keep_going: bool,
}

pub async fn restore(context: &ProgramContext, args: &RestoreArgs) -> CommandResult {
    let snapshot_buf = read_file_from_storage(
        context.storage.as_ref(),
        Collection::Snapshot,
        &args.snapshot,
    )
    .await?;
    let snapshot = Snapshot::decode(Cursor::new(snapshot_buf)).map_err(|e| {
        CommandError::with_source(
            CommandErrorKind::Corrupt,
            format!("Error decoding snapshot: {}", e),
            Some(Box::new(e)),
        )
    })?;
    let root_dir_entry = get_dir_entry(context, snapshot.root_hash.as_str()).await?;

    restore_dir(context, args, root_dir_entry, &args.target).await?;
    Ok(())
}

trait KeepGoingOrErr<E> {
    fn keep_going_or_err(self, args: &RestoreArgs, target: &PathBuf) -> Result<(), E>;
}

impl<T, E> KeepGoingOrErr<E> for Result<T, E>
where
    E: std::error::Error,
{
    fn keep_going_or_err(self, args: &RestoreArgs, target: &PathBuf) -> Result<(), E> {
        match self {
            Ok(_) => Ok(()),
            Err(e) => {
                if args.keep_going {
                    warn!("Failed to restore {}: {}", target.to_string_lossy(), e);
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }
}

#[async_recursion]
async fn restore_dir(
    context: &ProgramContext,
    args: &RestoreArgs,
    root_dir_entry: DirEntry,
    target: &PathBuf,
) -> CommandResult {
    match fs::metadata(target).await {
        Ok(v) => Ok(v),
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                fs::create_dir(target).await?;
                fs::metadata(target).await
            }
            _ => Err(e),
        },
    }?;

    let DirEntry {
        file: files,
        sub_dir: sub_dirs,
        ..
    } = root_dir_entry;

    let mut results: Vec<BoxFuture<CommandResult>> = Vec::new();
    for SubDirEntry { name, content, .. } in sub_dirs.into_iter() {
        let content = content.ok_or_else(|| {
            CommandError::new(
                CommandErrorKind::Corrupt,
                format!("Sub dir entry without content {}", target.to_string_lossy()),
            )
        })?;

        results.push(Box::pin(async move {
            let dir_entry = match content {
                sub_dir_entry::Content::Inline(dir_entry) => Ok(dir_entry),
                sub_dir_entry::Content::Hash(hash) => get_dir_entry(context, hash.as_ref()).await,
            }?;

            let dir_target = target.join(&name);
            restore_dir(context, args, dir_entry, &dir_target)
                .await
                .keep_going_or_err(args, &dir_target)?;

            Ok(())
        }));
    }

    for file_entry in files.into_iter() {
        results.push(Box::pin(async move {
            let file_target = target.join(&file_entry.name);
            restore_file(context, args, file_entry, &file_target)
                .await
                .keep_going_or_err(args, &file_target)?;

            Ok(())
        }));
    }

    Ok(())
}

async fn restore_file(
    context: &ProgramContext,
    args: &RestoreArgs,
    file_entry: FileEntry,
    target: &PathBuf,
) -> CommandResult {

    Ok(())
}

pub async fn get_dir_entry(context: &ProgramContext, hash: &str) -> CommandResult<DirEntry> {
    let dir_entry_buf =
        read_file_from_storage(context.storage.as_ref(), Collection::Blob, hash).await?;
    let dir_entry = DirEntry::decode(Cursor::new(dir_entry_buf)).map_err(|e| {
        CommandError::with_source(
            CommandErrorKind::Corrupt,
            format!("Error decoding dir entry: {}", e),
            Some(Box::new(e)),
        )
    })?;

    return Ok(dir_entry);
}
