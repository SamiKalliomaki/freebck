use async_recursion::async_recursion;
use clap::Args;
use log::warn;
use prost::Message;
use sha2::Digest;
use sha2::Sha256;
use std::time::SystemTime;
use std::{collections::HashMap, path::Path, pin::pin};
use tokio::sync::Semaphore;

use futures::future::{try_join, try_join_all, BoxFuture};
use tokio::{
    fs::{self, read_dir, File},
    io::{self, AsyncReadExt, AsyncSeekExt},
};

use crate::constants::CHUNK_SIZE;
use crate::{
    data::backup::{sub_dir_entry::Content, DirEntry, FileEntry, Snapshot, SubDirEntry},
    storage::Collection,
    util::{fs::sanitize_os_string, hash::read_hash, time::as_unix_timestamp},
};
use log::{debug, info};

use super::common::*;

#[derive(Debug, Args)]
pub struct BackupArgs {}

trait IgnoreAlreadyExists {
    fn ignore_already_exists(self) -> io::Result<()>;
}

impl<T> IgnoreAlreadyExists for io::Result<T> {
    fn ignore_already_exists(self) -> io::Result<()> {
        match self {
            Ok(_) => Ok(()),
            Err(e) => {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }
}

pub async fn backup(context: &ProgramContext, args: &BackupArgs) -> CommandResult {
    info!("Backup starting");
    let started = as_unix_timestamp(SystemTime::now());

    let previous_snapshot_number = get_highest_snapshot_number(context).await?;
    let mut previous_snapshot_root: Option<DirEntry> = None;
    if previous_snapshot_number > 0 {
        let previous_snapshot_name =
            format!("{}/{}", context.archive_name, previous_snapshot_number);
        let mut previous_snapshot_buffer: Vec<u8> = Vec::new();
        context
            .storage
            .read(
                Collection::Snapshot,
                &previous_snapshot_name,
                &mut previous_snapshot_buffer,
            )
            .await
            .into_command_result(CommandErrorKind::System, "Failed to get previous snapshot")?;

        let previous_snapshot = Snapshot::decode(previous_snapshot_buffer.as_slice())
            .into_command_result(CommandErrorKind::System, "Failed to decode snapshot")?;

        let mut previous_root_buffer: Vec<u8> = Vec::new();
        context
            .storage
            .read(
                Collection::Blob,
                &previous_snapshot.root_hash,
                &mut previous_root_buffer,
            )
            .await
            .into_command_result(
                CommandErrorKind::System,
                "Failed to get previous root entry",
            )?;

        previous_snapshot_root = Some(
            DirEntry::decode(previous_root_buffer.as_slice())
                .into_command_result(CommandErrorKind::System, "Failed to decode root entry")?,
        );
    }

    // Create a backup entry and write it to the storage.
    let backup_root_entry = backup_dir(
        context,
        &args,
        &context.backup_target,
        previous_snapshot_root.as_ref(),
    )
    .await?
    .encode_to_vec();
    let root_hash = format!("{:x}", Sha256::digest(&backup_root_entry));

    context
        .storage
        .write(Collection::Blob, &root_hash, backup_root_entry.as_slice())
        .await
        .ignore_already_exists()
        .into_command_result(
            CommandErrorKind::System,
            "Failed to upload backup root entry",
        )?;

    let finished = as_unix_timestamp(SystemTime::now());
    let snapshot = Snapshot {
        root_hash,
        started,
        finished,
    };

    const MAX_LOOP_ITERATIONS: u32 = 100;
    for _ in 0..MAX_LOOP_ITERATIONS {
        let highest_snapshot = get_highest_snapshot_number(context).await?;

        // Create a snapshot entry and write it to the storage.
        let snapshot_name = format!("{}/{}", context.archive_name, highest_snapshot + 1);
        match context
            .storage
            .write(
                Collection::Snapshot,
                snapshot_name.as_str(),
                snapshot.encode_to_vec().as_slice(),
            )
            .await
        {
            Ok(_) => {
                // Backup complete.
                info!("Backup complete. Wrote snapshot: {}", snapshot_name);
                return Ok(());
            }
            Err(e) => {
                if e.kind() != io::ErrorKind::AlreadyExists {
                    return Err(e.into_command_error(
                        CommandErrorKind::System,
                        "Failed to upload snapshot",
                    ));
                }
            }
        }
    }

    Err(CommandError::new(
        CommandErrorKind::Program,
        format!(
            "Failed to create snapshot after {} iterations",
            MAX_LOOP_ITERATIONS
        ),
    ))
}

async fn get_highest_snapshot_number(context: &ProgramContext) -> CommandResult<u32> {
    // Find the highest snapshot number.
    let snapshots = context
        .storage
        .get_collection_items(Collection::Snapshot)
        .await
        .into_command_result(CommandErrorKind::System, "Failed to get snapshots")?;
    let mut highest_snapshot: u32 = 0;
    for snapshot_name in snapshots {
        let parts: Vec<_> = snapshot_name.split('/').collect();
        if parts.len() != 2 {
            warn!("Invalid snapshot name: {}", snapshot_name);
            continue;
        }

        if parts[0] != context.archive_name {
            continue;
        }
        if let Ok(snapshot_number) = parts[1].parse::<u32>() {
            highest_snapshot = highest_snapshot.max(snapshot_number);
        }
    }
    Ok(highest_snapshot)
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

    let mut dir_entries = read_dir(path).await.into_command_result(
        CommandErrorKind::System,
        format!("Failed to list directory entries in: {}", path.display()).as_str(),
    )?;
    while let Some(dir_entry) = dir_entries.next_entry().await.into_command_result(
        CommandErrorKind::System,
        format!("Failed to iterate directory entries in: {}", path.display()).as_str(),
    )? {
        let name = sanitize_os_string(dir_entry.file_name())?;
        let path = dir_entry.path();
        let file_type = dir_entry.file_type().await.into_command_result(
            CommandErrorKind::System,
            format!("Failed to get file type: {}", path.display()).as_str(),
        )?;

        if file_type.is_file() {
            let file_entry = previous_files.get(&name).copied();
            file_futures.push(Box::pin(async move {
                backup_file(context, name, &args, &path, file_entry).await
            }));
        } else if file_type.is_dir() {
            let previous_sub_dirs = &previous_sub_dirs;
            sub_dir_futures.push(Box::pin(async move {
                let fetched_sub_dir: DirEntry;

                let sub_dir_entry: Option<&DirEntry> = match previous_sub_dirs.get(&name) {
                    Some(ref previous_sub_dir) => match previous_sub_dir.content {
                        Some(Content::Inline(ref dir_entry)) => Some(dir_entry),
                        Some(Content::Hash(ref hash)) => {
                            fetched_sub_dir = get_dir_entry(context, &hash).await?;
                            Some(&fetched_sub_dir)
                        }
                        None => None,
                    },
                    None => None,
                };

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
    let size = sub_dir_tasks.iter().map(|i| i.size).sum::<u64>()
        + file.iter().map(|i| i.size).sum::<u64>();
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

static BACKUP_FILE_OPENS: Semaphore = Semaphore::const_new(16);

async fn backup_file(
    context: &ProgramContext,
    name: String,
    _args: &BackupArgs,
    path: &Path,
    previous_snapshot: Option<&FileEntry>,
) -> CommandResult<FileEntry> {
    let metadata = fs::metadata(path).await.into_command_result(
        CommandErrorKind::System,
        format!("Failed to get file metadata: {}", path.display()).as_str(),
    )?;
    let modified = as_unix_timestamp(
        metadata
            .modified()
            .into_command_result(CommandErrorKind::System, "Failed to get file modified time")?,
    );
    let size = metadata.len();

    if let Some(previous_snapshot) = previous_snapshot {
        if previous_snapshot.modified == modified && previous_snapshot.size == size {
            return Ok(previous_snapshot.clone());
        }
    }

    let _permit = BACKUP_FILE_OPENS.acquire().await.into_command_result(
        CommandErrorKind::System,
        "Failed to acquire file open permit",
    )?;
    debug!("Backing up file: {:}", path.display());

    let mut file = pin!(File::open(path).await.into_command_result(
        CommandErrorKind::System,
        format!("Failed to open file: {}", path.display()).as_str()
    )?);
    let content_hash = read_hash(file.as_mut())
        .await
        .into_command_result(CommandErrorKind::System, "Failed to calculate file hash")?;

    if let Some(previous_snapshot) = previous_snapshot {
        if previous_snapshot.content_hash == content_hash {
            return Ok(FileEntry {
                name,
                content_hash,
                chunk_hash: previous_snapshot.chunk_hash.clone(),
                size,
                modified,
            });
        }
    }

    file.seek(io::SeekFrom::Start(0))
        .await
        .into_command_result(CommandErrorKind::System, "Failed to seek file")?;
    let mut buffer: Vec<u8> = vec![0; CHUNK_SIZE];
    let mut chunk_hashes = Vec::new();

    loop {
        let mut chunk = file.as_mut().take(CHUNK_SIZE as u64);

        buffer.clear();
        io::copy(&mut chunk, &mut buffer)
            .await
            .into_command_result(CommandErrorKind::System, "Failed to read file chunk")?;
        if buffer.len() == 0 {
            break;
        }

        let hash = format!("{:x}", Sha256::digest(&buffer));
        context
            .storage
            .write(Collection::Blob, &hash, &buffer)
            .await
            .ignore_already_exists()
            .into_command_result(CommandErrorKind::System, "Failed to upload file chunk")?;
        chunk_hashes.push(hash);
    }

    Ok(FileEntry {
        name,
        content_hash,
        chunk_hash: chunk_hashes,
        size,
        modified,
    })
}
