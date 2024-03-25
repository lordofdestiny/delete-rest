//! ## Main library crate
//!
//! Most of the logic is implemented here, or in the
//! one of the child modules.

use std::clone::Clone;
use std::fmt::Debug;
use std::path::{Path, PathBuf};

use clap::Parser;

use action::Action;
use keepfile::{KeepFile, KeepFileError};

use crate::config::{ConfigFile, ConfigFileError};

pub mod action;
pub mod config;
pub mod file_source;
pub mod keepfile;
#[cfg(test)]
#[doc(hidden)]
pub mod test_utils;

/// Selected source directory to seek files from
#[derive(Debug, Clone)]
pub struct SelectedDirectory(PathBuf);

impl TryFrom<PathBuf> for SelectedDirectory {
    type Error = std::io::Error;
    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        if path.is_dir() {
            path.canonicalize().map(Self)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Not a directory"))
        }
    }
}

impl AsRef<Path> for SelectedDirectory {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

impl SelectedDirectory {
    /// Get the path of all matching files
    ///
    /// This method returns a vector of all the matching files in the specified directory.
    /// It uses the `path` field of the `AppConfig` struct to search for files.
    ///
    /// Directories are searched recursively.
    ///
    /// # Errors
    ///
    /// Errors are returned in the following cases, but not limited to:
    ///
    /// - If the specified directory does not exist
    /// - If the specified directory is not readable
    /// - If an I/O error occurs while reading the directory
    /// - Path canonicalization fails
    fn read_recursive_path(&self) -> std::io::Result<Vec<PathBuf>> {
        let path = Path::new(&self.0);
        // All found files
        let mut files = Vec::new();
        // Stack for recursive search
        let mut stack: Vec<_> = path.read_dir()?.flat_map(Result::ok).collect();

        // Iterate over the stack until it's empty
        while let Some(entry) = stack.pop() {
            if entry.path().is_dir() {
                // If the entry is a directory, add its contents to the stack
                stack.extend(entry.path().read_dir()?.flat_map(Result::ok));
            } else {
                // Else, add the file to the list of found files
                files.push(entry.path().canonicalize()?);
            }
        }

        Ok(files)
    }
}

/// Command line arguments for the delete-rest app
///
/// This struct is used to parse command line arguments using the `clap` crate.
///
/// By default, if no flags are provided, the help message will be printed.
///
/// # Operations
/// - Copy matching files to the specified directory (default)
/// - Move matching files to the specified directory
/// - Delete non-matching files
///
/// ## Options:
/// - `path`: The directory to search for files
/// - `keep`: The file to use as the keep file
/// - `config`: The configuration file to use
/// - `move_to`: Move matching files to the specified directory
/// - `copy_to`: Copy matching files to the specified directory
/// - `delete`: Delete non-matching files
/// - `dry_run`: Only print what would be done, don't actually do anything.
/// - `verbose`: Print detailed information about what's happening
/// - `print_config`: Print parsed configuration and exit
#[derive(Parser, Debug, Clone)]
#[clap(
    name = "delete-rest",
    author = "lordofdestiny",
    about = "A CLI app to delete files based on a configuration file"
)]
#[command(arg_required_else_help(true))]
pub struct Args {
    /// The directory to search for files
    #[clap(short, long, default_value = ".", value_name = "DIR")]
    path: Option<String>,

    /// The file to use as the keep file
    #[clap(short, long)]
    keep: Option<String>,

    /// The configuration file to use
    #[clap(long, visible_alias = "cfg", visible_short_alias = 'Y')]
    config: Option<String>,

    /// Move matching files to the specified directory.
    /// Mutually exclusive with `delete` and `copy-to`
    #[clap(
        short,
        conflicts_with_all = &["copy_to", "delete"],
        group = "action",
        value_name = "DIR"
    )]
    move_to: Option<String>,

    /// Copy matching files to the specified directory.
    /// Mutually exclusive with `move-to` and `delete`
    #[clap(
        short,
        conflicts_with_all = &["move_to", "delete"],
        group = "action",
        value_name = "DIR"
    )]
    copy_to: Option<String>,

    /// Delete non-matching files.
    /// Mutually exclusive with `move-to` and `copy-to`
    #[clap(
        short,
        conflicts_with_all = &["move_to", "copy_to"],
        group = "action",
    )]
    delete: bool,

    /// Only print what would be done, don't actually do anything.
    #[clap(long, default_value = "false")]
    dry_run: bool,

    /// Print detailed information about what's happening
    #[clap(short, long)]
    verbose: bool,

    /// Print parsed configuration and exit
    #[clap(long)]
    pub print_config: bool,
}

/// Parsed configuration
///
/// This struct contains the data needed to execute the program.
/// It is parsed from [Args].
#[derive(Debug)]
pub struct AppConfig {
    /// Directory the script will be executed from
    pub path: SelectedDirectory,
    /// Configuration describing what files to look up in `path` field
    pub config_file: ConfigFile,
    /// A parsed keepfile
    pub keepfile: KeepFile,
    /// Action to perform once the files are filtered
    pub action: Action,
    /// Additional options
    pub options: ExecutionOptions,
}

/// Options for executing the action
#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    /// Should the action be performed in dry-run mode?
    pub dry_run: bool,
    /// Should the detailed information be printed?
    pub verbose: bool,
    /// Should the parsed configuration be printed?
    pub print: bool,
}

/// An error that occurs when parsing the [Args]
#[derive(thiserror::Error, Debug)]
pub enum AppConfigError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Config(#[from] ConfigFileError),
    #[error("{0}")]
    KeepFile(#[from] KeepFileError),
}

impl TryFrom<Args> for AppConfig {
    type Error = AppConfigError;
    fn try_from(args: Args) -> Result<Self, Self::Error> {
        use std::io::{Error, ErrorKind::InvalidInput};
        #[rustfmt::skip]
        let Args {
            path, config,  keep,
            copy_to, move_to, delete,
            dry_run, verbose, print_config: print,
        } = args;

        let path = path
            .as_deref()
            .or(Some("."))
            .map(PathBuf::from)
            .filter(|p| p.exists() && p.is_dir())
            .ok_or_else(|| Error::new(InvalidInput, "Invalid directory"))
            .and_then(SelectedDirectory::try_from)?;

        let config_file = match config.map(PathBuf::from).map(ConfigFile::try_load) {
            Some(file) => file?,
            None => ConfigFile::load(path.as_ref().join("config.yaml")),
        };

        let keepfile = match keep.map(PathBuf::from).map(KeepFile::try_load) {
            Some(file) => file?,
            None => KeepFile::try_load(path.as_ref().join("keep.txt"))?,
        };

        let action = Action::new(copy_to, move_to, delete);

        Ok(AppConfig {
            path,
            config_file,
            keepfile,
            action,
            options: ExecutionOptions {
                dry_run,
                verbose,
                print,
            },
        })
    }
}
