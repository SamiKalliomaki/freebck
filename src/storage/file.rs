use std::mem::ManuallyDrop;
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use async_recursion::async_recursion;
use log::warn;
use tokio::{
    fs::{self, read_dir, File, OpenOptions},
    io::{self, AsyncReadExt, AsyncWrite, AsyncWriteExt},
};

use async_trait::async_trait;

use rand::distributions::{Alphanumeric, DistString};

use crate::util::fs::sanitize_os_string;

use super::util::xor_byte_hash;
use super::{Collection, Storage, StorageItems, StorageRead, StorageWrite};

pub struct FileStorage {
    root: PathBuf,
    tmp_dir: PathBuf,
}

impl FileStorage {
    pub async fn new(root: PathBuf) -> io::Result<Self> {
        let tmp_dir = root.join("tmp");
        fs::create_dir_all(&tmp_dir).await?;

        Ok(Self { root, tmp_dir })
    }
}

fn get_collection_path(root: &Path, collection: Collection) -> PathBuf {
    let name = match collection {
        Collection::Snapshot => "snapshot",
        Collection::Blob => "blob",
    };

    root.join(name)
}

// Get the path to an item in a collection.
// The path is: <collection>/<key[0..2]>/<key[2..]>
fn get_item_path(root: &Path, collection: Collection, key: &str) -> io::Result<PathBuf> {
    if key.len() <= 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Key must be at least 3 characters long",
        ));
    }

    Ok(get_collection_path(root, collection)
        .join(xor_byte_hash(key.as_bytes()))
        .join(&key))
}

struct RenameOnFinishFile {
    file: ManuallyDrop<File>,
    tmp_path: PathBuf,
    new_path: PathBuf,
    cleaned_up: bool,
}

impl RenameOnFinishFile {
    async fn new(tmp_path: PathBuf, new_path: PathBuf) -> io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .await?;

        Ok(Self {
            file: ManuallyDrop::new(file),
            tmp_path,
            new_path,
            cleaned_up: false,
        })
    }

    /// Structural pin projection. This is safe because we never move the
    /// `File` out of the `ManuallyDrop`.
    fn pin_get_file(self: Pin<&mut Self>) -> Pin<&mut File> {
        Pin::new(self.get_mut().file.deref_mut())
    }
}

impl AsyncWrite for RenameOnFinishFile {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        self.pin_get_file().poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.pin_get_file().poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.pin_get_file().poll_shutdown(cx)
    }
}

#[async_trait]
pub trait SafeAsyncWrite: AsyncWrite {
    async fn finish(self: Self) -> io::Result<()>;
}

#[async_trait]
impl SafeAsyncWrite for RenameOnFinishFile {
    async fn finish(mut self: Self) -> io::Result<()> {
        self.file.sync_all().await?;
        unsafe {
            ManuallyDrop::drop(&mut self.file);
        }

        self.cleaned_up = true;
        match fs::rename(&self.tmp_path, &self.new_path).await {
            Ok(()) => {}
            Err(e) => {
                if let Err(e) = fs::remove_file(&self.tmp_path).await {
                    warn!("Failed to remove temp file: {}", e);
                }

                return Err(e);
            }
        }

        Ok(())
    }
}

impl Drop for RenameOnFinishFile {
    fn drop(&mut self) {
        if !self.cleaned_up {
            if let Err(e) = std::fs::remove_file(&self.tmp_path) {
                warn!("Failed to remove temp file: {}", e);
            }
        }

        assert!(
            self.cleaned_up,
            "File was dropped without finish being called"
        )
    }
}

#[async_trait]
impl Storage for FileStorage {
    async fn write(&self, collection: Collection, key: &str, data: &[u8]) -> StorageWrite {
        let path = get_item_path(&self.root, collection, key)?;
        match fs::metadata(&path).await {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("File already exists: {}", path.display()),
                ));
            }
            Err(_) => {}
        };

        let random_name = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
        let tmp_path = self.tmp_dir.join(random_name);

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }

        let mut file = RenameOnFinishFile::new(tmp_path, path).await?;
        file.write_all(data).await?;
        file.finish().await?;

        Ok(())
    }

    async fn read(&self, collection: Collection, key: &str, buffer: &mut Vec<u8>) -> StorageRead {
        let path = get_item_path(&self.root, collection, key)?;
        let mut file = File::open(path).await?;
        buffer.clear();
        file.read_to_end(buffer).await?;
        Ok(())
    }

    /// Iterate over directory structure like
    ///
    /// collection:
    ///  - foo:
    ///    - bar
    ///    - baz
    ///  - qux:
    ///    - quux
    ///    - quuz
    ///
    /// And produces a list like ["foobar", "foobaz", "quxquux", "quxquuz"].
    async fn get_collection_items(&self, collection: Collection) -> StorageItems {
        let path = get_collection_path(&self.root, collection);
        if !path.exists() {
            return Ok(Vec::new());
        }

        #[async_recursion]
        async fn iterate_dir(items: &mut Vec<String>, path: PathBuf) -> io::Result<()> {
            let mut dir_entries = read_dir(path).await?;
            while let Some(dir_entry) = dir_entries.next_entry().await? {
                let file_type = dir_entry.file_type().await?;

                if file_type.is_dir() {
                    iterate_dir(items, dir_entry.path()).await?;
                } else if file_type.is_file() {
                    items.push(sanitize_os_string(dir_entry.file_name())?);
                }
            }
            Ok(())
        }

        let mut items: Vec<String> = Vec::new();
        iterate_dir(&mut items, path).await?;
        Ok(items)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    struct FileStorageTestState {
        _tmp_dir: tempfile::TempDir,
        storage: FileStorage,
    }

    impl FileStorageTestState {
        async fn new() -> Self {
            let _tmp_dir = tempfile::tempdir().unwrap();
            let storage = FileStorage::new(_tmp_dir.path().to_owned()).await.unwrap();

            Self { _tmp_dir, storage }
        }
    }

    storage_tests!(FileStorageTestState);
}
