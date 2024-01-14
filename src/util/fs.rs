use std::ffi::OsString;

use crate::cmd::common::{CommandError, CommandErrorKind, CommandResult};

pub fn sanitize_os_string(os_string: OsString) -> CommandResult<String> {
    match os_string.into_string() {
        Ok(string) => Ok(string),
        Err(os_string) => Err(CommandError::new(
            CommandErrorKind::System,
            format!("Invalid UTF-8 in path: {:?}", os_string),
        )),
    }
}
