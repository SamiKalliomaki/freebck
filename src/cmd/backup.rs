use async_recursion::async_recursion;
use prost::Message;
use sha2::Digest;
use sha2::Sha256;
use std::{collections::HashMap, path::Path, pin::pin};

use futures::future::{try_join, try_join_all, BoxFuture};
use tokio::{
    fs::{self, read_dir, File},
    io::{self, AsyncSeekExt, AsyncWriteExt},
};

use crate::{
    data::backup::{sub_dir_entry::Content, DirEntry, FileEntry, Snapshot, SubDirEntry},
    storage::Collection,
    util::{fs::sanitize_os_string, hash::read_hash, time::as_unix_timestamp},
};
use log::{debug, info};

use super::common::*;

pub struct BackupArgs {}

pub async fn backup(context: &ProgramContext, args: &BackupArgs) -> CommandResult {
    info!("Backup starting");

    let backup_root_entry = backup_dir(context, &args, &context.backup_target, None)
        .await?
        .encode_to_vec();
    let root_hash = format!("{:x}", Sha256::digest(&backup_root_entry));

    let mut dir_entry_file = context.storage.write(Collection::Blob, &root_hash).await?;
    dir_entry_file
        .write_all(backup_root_entry.as_slice())
        .await?;
    drop(dir_entry_file);

    let snapshot = Snapshot { root_hash };
    let mut snapshot_file = context
        .storage
        .write(Collection::Snapshot, "latest")
        .await?;
    snapshot_file
        .write_all(snapshot.encode_to_vec().as_slice())
        .await?;

    info!("Backup complete");

    Ok(())
}

#[async_recursion]
async fn backup_dir(
    context: &ProgramContext,
    args: &BackupArgs,
    path: &Path,
    previous_snapshot: Option<&'async_recursion DirEntry>,
) -> CommandResult<DirEntry> {
    debug!("Backing up directory: {:}", path.display());

    let mut previous_sub_dirs: HashMap<&String, &SubDirEntry> = HashMap::new();
    let mut previous_files: HashMap<&String, &FileEntry> = HashMap::new();

    if let Some(previous_snapshot) = previous_snapshot {
        for entry in &previous_snapshot.sub_dir {
            previous_sub_dirs.insert(&entry.name, entry);
        }
        for entry in &previous_snapshot.file {
            previous_files.insert(&entry.name, entry);
        }
    }

    struct SubDirTaskResult {
        sub_dir: SubDirEntry,
        size: u64,
    }

    let mut file_futures: Vec<BoxFuture<CommandResult<FileEntry>>> = Vec::new();
    let mut sub_dir_futures: Vec<BoxFuture<CommandResult<SubDirTaskResult>>> = Vec::new();

    let mut dir_entries = read_dir(path).await?;
    while let Some(dir_entry) = dir_entries.next_entry().await? {
        let name = sanitize_os_string(dir_entry.file_name())?;
        let path = dir_entry.path();
        let file_type = dir_entry.file_type().await?;

        if file_type.is_file() {
            let file_entry = previous_files.get(&name).copied();
            file_futures.push(Box::pin(async move {
                backup_file(context, name, &args, &path, file_entry).await
            }));
        } else if file_type.is_dir() {
            let sub_dir_entry: Option<&DirEntry> = match previous_sub_dirs.get(&name) {
                Some(previous_sub_dir) => match previous_sub_dir.content {
                    Some(Content::Inline(ref dir_entry)) => Some(dir_entry),
                    Some(Content::Hash(_)) => todo!("fetch sub dir from storage"),
                    None => None,
                },
                None => None,
            };

            sub_dir_futures.push(Box::pin(async move {
                backup_dir(context, &args, &path, sub_dir_entry)
                    .await
                    .map(|dir_entry| SubDirTaskResult {
                        size: dir_entry.size,
                        sub_dir: SubDirEntry {
                            name,
                            content: Some(Content::Inline(dir_entry)),
                        },
                    })
            }));
        } else {
            return Err(CommandError::new(
                CommandErrorKind::Program,
                "Unsupported file type".to_owned(),
            ));
        }
    }

    let (sub_dir_tasks, mut file) =
        try_join(try_join_all(sub_dir_futures), try_join_all(file_futures)).await?;
    let size =
        sub_dir_tasks.iter().map(|i| i.size).sum::<u64>() + file.iter().map(|i| i.size).sum::<u64>();
    let mut sub_dir = sub_dir_tasks
        .into_iter()
        .map(|i| i.sub_dir)
        .collect::<Vec<_>>();

    sub_dir.sort_by(|a, b| a.name.cmp(&b.name));
    file.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(DirEntry {
        sub_dir,
        file,
        size,
    })
}

async fn backup_file(
    context: &ProgramContext,
    name: String,
    _args: &BackupArgs,
    path: &Path,
    previous_snapshot: Option<&FileEntry>,
) -> CommandResult<FileEntry> {
    debug!("Backing up file: {:}", path.display());

    let metadata = fs::metadata(path).await?;
    let modified = as_unix_timestamp(metadata.modified()?);
    let size = metadata.len();

    if let Some(previous_snapshot) = previous_snapshot {
        if previous_snapshot.modified == modified && previous_snapshot.size == size {
            return Ok(previous_snapshot.clone());
        }
    }

    let mut file = pin!(File::open(path).await?);
    let content_hash = read_hash(file.as_mut()).await?;
    file.seek(io::SeekFrom::Start(0)).await?;

    if let Some(previous_snapshot) = previous_snapshot {
        if previous_snapshot.content_hash == content_hash {
            return Ok(FileEntry {
                name,
                content_hash,
                size,
                modified,
            });
        }
    }

    let mut backup = context
        .storage
        .write(Collection::Blob, &content_hash)
        .await?;
    io::copy(&mut file, &mut backup).await?;

    Ok(FileEntry {
        name,
        content_hash,
        size,
        modified,
    })
}
