use log::debug;
use std::{error::Error, path::PathBuf};
use test_log::{self, test};

use freebck::{
    cmd::{
        backup::{backup, BackupArgs},
        common::ProgramContext,
    },
    storage::file::FileStorage,
};

#[test(tokio::test)]
async fn test_backup() -> Result<(), Box<dyn Error>> {
    let content_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test_data/test_backup_content")
        .canonicalize()?;

    let temp_dir = tempfile::tempdir()?;
    debug!("Test backup output: {:}", temp_dir.path().display());

    let storage = Box::new(FileStorage::new(temp_dir.path().to_owned()).await.unwrap());

    backup(
        &ProgramContext {
            storage,
            backup_target: content_path,
        },
        BackupArgs {},
    )
    .await?;

    Ok(())
}
