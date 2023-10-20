use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    path::PathBuf,
    time::SystemTimeError,
};

use clap::Args;
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
