use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    path::PathBuf,
    time::SystemTimeError,
};

use clap::Args;
use log::warn;
use tokio::io;

use crate::storage::Storage;

pub struct ProgramContext {
    pub storage: Box<dyn Storage>,
    pub backup_target: PathBuf,
}

#[derive(Debug, Args)]
pub struct CommonArgs {
    /// Dry run, don't perform any writes.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug)]
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
        source: Option<Box<dyn Error + Send>>,
    ) -> Self {
        CommandError {
            error_type,
            message,
            source,
        }
    }

    pub fn new(error_type: CommandErrorKind, message: String) -> Self {
        CommandError {
            error_type,
            message,
            source: None,
        }
    }
}

impl Display for CommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.error_type, self.message)
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

impl From<io::Error> for CommandError {
    fn from(error: io::Error) -> Self {
        CommandError {
            error_type: CommandErrorKind::System,
            message: error.to_string(),
            source: Some(Box::new(error)),
        }
    }
}

impl From<SystemTimeError> for CommandError {
    fn from(error: SystemTimeError) -> Self {
        CommandError {
            error_type: CommandErrorKind::System,
            message: error.to_string(),
            source: Some(Box::new(error)),
        }
    }
}

pub type CommandResult<T = ()> = Result<T, CommandError>;

pub trait IntoCommandResult<T> {
    fn into_command_result(self) -> CommandResult<T>;
}

impl<T, E> IntoCommandResult<T> for Result<T, E>
where
    E: Into<CommandError>,
{
    fn into_command_result(self) -> CommandResult<T> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.into()),
        }
    }
}

pub trait KeepGoingOrErr<E> {
    fn keep_going_or_err<F>(self, keep_going: bool, f: F) -> Result<(), E>
    where
        F: FnOnce(E) -> String;
}

impl<T, E> KeepGoingOrErr<E> for Result<T, E>
where
    E: std::error::Error,
{
    fn keep_going_or_err<F>(self, keep_going: bool, f: F) -> Result<(), E>
    where
        F: FnOnce(E) -> String,
    {
        match self {
            Ok(_) => Ok(()),
            Err(e) => {
                if keep_going {
                    warn!("{}", f(e));
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }
}
