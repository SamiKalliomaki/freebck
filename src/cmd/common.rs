use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io::Cursor,
    path::PathBuf,
};

use log::{error, warn};
use prost::Message;

use crate::{
    data::backup::DirEntry,
    storage::{Collection, Storage},
};

pub struct ProgramContext {
    pub archive_name: String,
    pub storage: Box<dyn Storage>,
    pub backup_target: PathBuf,
}

#[derive(Debug, Clone)]
pub enum CommandErrorKind {
    /// An error that is caused by the user.
    User,
    /// Operation was aborted due to conflicting files or directories.
    FileSystemConflict,
    /// An error that is caused by invalid backup data.
    Corrupt,
    /// An error that is caused by the program.
    Program,
    /// An error that is caused by the system.
    System,
}

#[derive(Debug)]
pub struct CommandError {
    error_type: CommandErrorKind,
    message: String,
    source: Option<Box<dyn Error + Send>>,
}

impl CommandError {
    pub fn with_source(
        error_type: CommandErrorKind,
        message: String,
        source: Box<dyn Error + Send>,
    ) -> Self {
        CommandError {
            error_type,
            message,
            source: Some(source),
        }
    }

    pub fn new(error_type: CommandErrorKind, message: String) -> Self {
        CommandError {
            error_type,
            message,
            source: None,
        }
    }

    pub fn with_message(self: Self, message: String) -> Self {
        CommandError::with_source(self.error_type.clone(), message, Box::new(self))
    }
}

impl Display for CommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for CommandError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.source {
            Some(ref source) => Some(source.as_ref()),
            None => None,
        }
    }
}

pub trait IntoCommandError {
    fn into_command_error(self, kind: CommandErrorKind, message: &str) -> CommandError;
}

impl<T> IntoCommandError for T
where
    T: std::error::Error + Send + 'static,
{
    fn into_command_error(self, kind: CommandErrorKind, message: &str) -> CommandError {
        CommandError::with_source(kind, message.to_string(), Box::new(self))
    }
}

pub type CommandResult<T = ()> = Result<T, CommandError>;

pub trait IntoCommandResult<T> {
    fn into_command_result(self, kind: CommandErrorKind, message: &str) -> CommandResult<T>;
}

impl<T, E> IntoCommandResult<T> for Result<T, E>
where
    E: std::error::Error + Send + 'static,
{
    fn into_command_result(self, kind: CommandErrorKind, message: &str) -> CommandResult<T> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(CommandError::with_source(
                kind,
                message.to_string(),
                Box::new(e),
            )),
        }
    }
}

pub trait KeepGoingOrErr<E> {
    fn keep_going_or_err<F>(self, keep_going: bool, f: F) -> CommandResult
    where
        F: FnOnce(E) -> CommandError;
}

impl<T, E> KeepGoingOrErr<E> for Result<T, E>
where
    E: std::error::Error,
{
    fn keep_going_or_err<F>(self, keep_going: bool, f: F) -> CommandResult
    where
        F: FnOnce(E) -> CommandError,
    {
        match self {
            Ok(_) => Ok(()),
            Err(e) => {
                if keep_going {
                    warn!("{}", f(e));
                    Ok(())
                } else {
                    error!("{}", f(e));
                    Err(CommandError::new(
                        CommandErrorKind::Program,
                        "Aborted".to_string(),
                    ))
                }
            }
        }
    }
}

pub async fn get_dir_entry(context: &ProgramContext, hash: &str) -> CommandResult<DirEntry> {
    let mut dir_entry_buf = Vec::new(); // TODO: Setup a pool of buffers.
    context
        .storage
        .read(Collection::Blob, hash, &mut dir_entry_buf)
        .await
        .into_command_result(CommandErrorKind::System, "Failed to download dir entry")?;
    let dir_entry = DirEntry::decode(Cursor::new(dir_entry_buf)).map_err(|e| {
        CommandError::with_source(
            CommandErrorKind::Corrupt,
            "Error decoding dir entry".to_string(),
            Box::new(e),
        )
    })?;

    return Ok(dir_entry);
}
