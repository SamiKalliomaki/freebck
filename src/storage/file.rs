use std::mem::ManuallyDrop;
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use log::warn;
use tokio::{
    fs::{self, read_dir, DirEntry, File, OpenOptions},
    io::{self, AsyncRead, AsyncWrite},
};

use async_trait::async_trait;

use futures::{
    channel::mpsc::{self, Sender},
    stream::{self, Stream},
    SinkExt,
};
use rand::distributions::{Alphanumeric, DistString};

use crate::util::fs::sanitize_os_string;

use super::{Collection, ResultPinBox, Storage};

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
        .join(&key[0..2])
        .join(&key[2..]))
}

struct RenameOnDropFile {
    file: ManuallyDrop<File>,
    tmp_path: PathBuf,
    new_path: PathBuf,
}

impl RenameOnDropFile {
    /// Structural pin projection. This is safe because we never move the
    /// `File` out of the `ManuallyDrop`.
    fn pin_get_file(self: Pin<&mut Self>) -> Pin<&mut File> {
        unsafe { Pin::new_unchecked(self.get_unchecked_mut().file.deref_mut()) }
    }
}

impl AsyncWrite for RenameOnDropFile {
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

impl Drop for RenameOnDropFile {
    fn drop(&mut self) {
        inner_drop(unsafe { Pin::new_unchecked(self) });

        fn inner_drop(mut this: Pin<&mut RenameOnDropFile>) {
            unsafe {
                ManuallyDrop::drop(&mut this.file);
            }

            match std::fs::rename(&this.tmp_path, &this.new_path) {
                Ok(()) => {}
                Err(e) => match e.kind() {
                    std::io::ErrorKind::AlreadyExists => {
                        // The most likely reason someone else already wrote the file.
                        // The filename is based on the hash of the contents so this should be
                        // safe.
                        warn!(
                            "Cannot finish writing, file already exists: {}",
                            this.new_path.display()
                        );
                    }
                    _ => {
                        panic!("Failed to rename file: {}", e);
                    }
                },
            }
        }
    }
}

#[async_trait]
impl Storage for FileStorage {
    async fn write(
        &self,
        collection: Collection,
        key: &str,
    ) -> ResultPinBox<dyn AsyncWrite + Send> {
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

        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .await?;
        Ok(Box::pin(RenameOnDropFile {
            file: ManuallyDrop::new(file),
            tmp_path,
            new_path: path,
        }))
    }

    async fn read(&self, collection: Collection, key: &str) -> ResultPinBox<dyn AsyncRead + Send> {
        let path = get_item_path(&self.root, collection, key)?;
        let file = File::open(path).await?;
        Ok(Box::pin(file))
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
    async fn get_collection_items(
        &self,
        collection: Collection,
    ) -> ResultPinBox<dyn Stream<Item = io::Result<String>>> {
        let path = get_collection_path(&self.root, collection);
        if !path.exists() {
            return Ok(Box::pin(stream::empty()));
        }

        let mut dir_entries = read_dir(path).await?;
        let (mut tx, rx) = mpsc::channel::<io::Result<String>>(16);

        async fn iterate_sub_dir(
            tx: &mut Sender<io::Result<String>>,
            sub_dir: DirEntry,
        ) -> io::Result<()> {
            let sub_dir_name = sanitize_os_string(sub_dir.file_name())?;
            let mut sub_dir_entries = read_dir(sub_dir.path()).await?;

            while let Some(dir_entry) = sub_dir_entries.next_entry().await? {
                let file_name = sanitize_os_string(dir_entry.file_name())?;
                tx.feed(Ok(sub_dir_name.to_owned() + &file_name))
                    .await
                    .unwrap();
            }

            Ok(())
        }

        tokio::spawn(async move {
            loop {
                match dir_entries.next_entry().await {
                    Err(e) => {
                        tx.feed(Err(e)).await.unwrap();
                        return;
                    }
                    Ok(None) => {
                        return;
                    }
                    Ok(Some(dir_entry)) => match iterate_sub_dir(&mut tx, dir_entry).await {
                        Err(e) => {
                            tx.feed(Err(e)).await.unwrap();
                            return;
                        }
                        Ok(()) => {}
                    },
                }
            }
        });

        Ok(Box::pin(rx))
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
