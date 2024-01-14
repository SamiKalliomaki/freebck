use std::io;

use async_trait::async_trait;

#[macro_use]
mod test;

pub mod file;
mod util;

pub enum Collection {
    Snapshot,
    Blob,
}

pub type StorageWrite = io::Result<()>;
pub type StorageRead = io::Result<()>;
pub type StorageItems = io::Result<Vec<String>>;

#[async_trait]
pub trait Storage: Sync + Send {
    // Write a new item to the collection. Collection and key should be alphanumeric.
    async fn write(&self, collection: Collection, key: &str, data: &[u8]) -> io::Result<()>;

    // Read an item from the collection. Collection and key should be alphanumeric.
    async fn read(&self, collection: Collection, key: &str, buffer: &mut Vec<u8>) -> StorageRead;

    // Get an iterator over all items in the collection. Collection should be alphanumeric.
    async fn get_collection_items(&self, collection: Collection) -> StorageItems;
}
