use log::debug;
use std::{error::Error, path::PathBuf};
use test_log::{self, test};
use tokio::fs;
use walkdir::WalkDir;

use freebck::{
    cmd::{
        backup::{backup, BackupArgs},
        common::ProgramContext,
        restore::{restore, RestoreArgs},
    },
    storage::file::FileStorage,
};

#[test(tokio::test)]
async fn test_backup_and_restore() -> Result<(), Box<dyn Error>> {
    let content_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test_data/test_backup_content")
        .canonicalize()?;

    let backup_dir = tempfile::tempdir()?;
    debug!("Test backup output: {:}", backup_dir.path().display());

    let storage = Box::new(FileStorage::new(backup_dir.path().into()).await.unwrap());
    let mut context = ProgramContext {
        storage,
        backup_target: content_path.clone(),
    };

    backup(&context, &BackupArgs {}).await?;

    let restore_dir = tempfile::tempdir()?;
    context.backup_target = restore_dir.path().into();

    restore(
        &context,
        &RestoreArgs {
            snapshot: "latest".to_owned(),
            keep_going: false,
            no_override_files: true,
        },
    )
    .await?;

    for entry in WalkDir::new(&content_path) {
        let entry = entry?;
        let relative_path = entry.path().strip_prefix(&content_path)?;
        let restore_path = restore_dir.path().join(relative_path);

        if entry.file_type().is_dir() {
            assert!(
                restore_path.is_dir(),
                "Expected {:?} to be a directory in the restored directory",
                relative_path
            );
            continue;
        }

        assert!(
            entry.file_type().is_file(),
            "{:?} is neither a file or a directory?",
            relative_path
        );
        assert!(
            restore_path.is_file(),
            "Expected {:?} to be a file in the restored directory",
            relative_path
        );

        let orig_content = fs::read(entry.path()).await?;
        let restore_content = fs::read(restore_path).await?;

        assert_eq!(
            orig_content, restore_content,
            "Expected restored file contents to match for {:?}",
            relative_path
        );
    }

    Ok(())
}
