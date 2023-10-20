use std::{io, pin::Pin};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt};

pub async fn read_hash(mut file: Pin<&mut (dyn AsyncRead + Send)>) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = Box::new([0; 1024 * 1024]);
    loop {
        let bytes_read = file.read(buffer.as_mut()).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}
