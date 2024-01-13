use std::ffi::OsString;

use tokio::io;

pub fn sanitize_os_string(os_string: OsString) -> io::Result<String> {
    match os_string.into_string() {
        Ok(string) => Ok(string),
        Err(os_string) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid UTF-8 in path: {:?}", os_string),
        )),
    }
}
