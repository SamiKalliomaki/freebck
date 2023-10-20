use std::{io, pin::Pin};

use async_trait::async_trait;
use futures::Stream;
use tokio::io::{AsyncRead, AsyncWrite};

#[macro_use]
mod test;
pub mod file;

type ResultPinBox<T> = io::Result<Pin<Box<T>>>;

pub enum Collection {
    Snapshot,
    Blob,
}

#[async_trait]
pub trait Storage: Sync + Send {
    // Write a new item to the collection. Collection and key should be alphanumeric.
    async fn write(&self, collection: Collection, key: &str)
        -> ResultPinBox<dyn AsyncWrite + Send>;

    // Read an item from the collection. Collection and key should be alphanumeric.
    async fn read(&self, collection: Collection, key: &str) -> ResultPinBox<dyn AsyncRead + Send>;

    // Get an iterator over all items in the collection. Collection should be alphanumeric.
    async fn get_collection_items(
        &self,
        collection: Collection,
    ) -> ResultPinBox<dyn Stream<Item = io::Result<String>>>;
}
