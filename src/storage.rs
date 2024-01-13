use std::{io, pin::Pin};

use async_trait::async_trait;
use futures::Stream;
use tokio::io::{AsyncRead, AsyncWrite};

#[macro_use]
mod test;

pub mod file;
mod util;

pub enum Collection {
    Snapshot,
    Blob,
}

type ResultPinBox<T> = io::Result<Pin<Box<T>>>;

#[async_trait]
pub trait SafeAsyncWrite: AsyncWrite {
    async fn finish(self: Pin<Box<Self>>) -> io::Result<()>;
}

pub type StorageWrite = ResultPinBox<dyn SafeAsyncWrite + Send>;
pub type StorageRead = ResultPinBox<dyn AsyncRead + Send>;
pub type StorageItems = ResultPinBox<dyn Stream<Item = io::Result<String>>>;

#[async_trait]
pub trait Storage: Sync + Send {
    // Write a new item to the collection. Collection and key should be alphanumeric.
    async fn write(&self, collection: Collection, key: &str)
        -> StorageWrite;

    // Read an item from the collection. Collection and key should be alphanumeric.
    async fn read(&self, collection: Collection, key: &str) -> StorageRead;

    // Get an iterator over all items in the collection. Collection should be alphanumeric.
    async fn get_collection_items(
        &self,
        collection: Collection,
    ) -> StorageItems;
}
