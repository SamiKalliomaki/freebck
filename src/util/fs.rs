use std::ffi::OsString;

use tokio::io;

use crate::storage::{Collection, Storage};

pub fn sanitize_os_string(os_string: OsString) -> io::Result<String> {
    match os_string.into_string() {
        Ok(string) => Ok(string),
        Err(os_string) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid UTF-8 in path: {:?}", os_string),
        )),
    }
}

pub async fn read_file_from_storage(
    storage: &dyn Storage,
    collection: Collection,
    key: &str,
) -> io::Result<Vec<u8>> {
    let mut file = storage.read(collection, key).await?;
    let mut buffer = Vec::new();
    io::copy(&mut file, &mut buffer).await?;
    return Ok(buffer);
}
