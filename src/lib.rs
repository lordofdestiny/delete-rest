use std::clone::Clone;
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use clap::Parser;
use itertools::Itertools;
use regex::Regex;
use regex_macro::regex;
use serde::{Deserialize, Serialize};

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

/// Files selected from a directory
#[derive(Debug, Clone)]
pub struct SelectedFiles {
    /// Directory the files where selected from
    pub dir: SelectedDirectory,
    /// Selected files' paths
    pub files: Vec<PathBuf>,
}

impl TryFrom<SelectedDirectory> for SelectedFiles {
    type Error = std::io::Error;
    fn try_from(selected: SelectedDirectory) -> Result<Self, Self::Error> {
        let files = selected.read_recursive_path()?;
        Ok(SelectedFiles { dir: selected, files })
    }
}

pub trait FileSource: Debug {
    /// Get the path of the directory files are located in
    fn dir(&self) -> &Path;

    /// Get an iterator over the files in the source
    fn iter(&self) -> impl Iterator<Item = &PathBuf> + Clone;

    /// Get the number of files in the source
    ///
    /// This method is linear in time complexity
    fn count(&self) -> usize {
        self.iter().count()
    }

    /// Filter the files in the source, using the specified filter
    ///
    /// This method returns a new `FilteredFiles` struct that contains the files that match the specified filter
    fn filter_by(self, filter: Rc<dyn Fn(&&PathBuf) -> bool>) -> FilteredFiles<Self>
    where
        Self: Sized,
    {
        FilteredFiles {
            source: self,
            matcher: filter,
        }
    }
}

impl FileSource for SelectedFiles {
    fn dir(&self) -> &Path {
        &self.dir.0
    }

    fn iter(&self) -> impl Iterator<Item = &PathBuf> + Clone {
        self.files.iter()
    }
}

/// Files filtered by a matcher function
///
/// This struct represents files that have been filtered by a matcher function.
///
/// It is used to chain multiple filters together
///
/// Files are filter on demand, so the filter is not applied until the files are iterated over
#[derive(Clone)]
pub struct FilteredFiles<F: FileSource> {
    source: F,
    matcher: Rc<dyn Fn(&&PathBuf) -> bool>,
}

impl<F: FileSource> Debug for FilteredFiles<F> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilteredFiles")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl<F: FileSource> FileSource for FilteredFiles<F> {
    fn dir(&self) -> &Path {
        self.source.dir()
    }
    fn iter(&self) -> impl Iterator<Item = &PathBuf> + Clone {
        self.source.iter().filter(self.matcher.deref())
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

    /// Move matching files to the specified directory
    /// This option is mutually exclusive with `delete` and `copy-to`
    #[clap(
        short,
        conflicts_with_all = &["copy_to", "delete"],
        group = "action",
        value_name = "DIR"
    )]
    move_to: Option<String>,

    /// Copy matching files to the specified directory
    /// This option is mutually exclusive with `move-to` and `delete`
    #[clap(
        short,
        conflicts_with_all = &["move_to", "delete"],
        group = "action",
        value_name = "DIR"
    )]
    copy_to: Option<String>,

    /// Delete non-matching files
    /// This option is mutually exclusive with `move-to` and `copy-to`
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

#[derive(Debug)]
pub struct AppConfig {
    pub path: SelectedDirectory,
    pub filter: FileFilter,
    pub keep: KeepFile,
    pub action: Action,
    pub options: ExecutionOptions,
    pub print: bool,
}

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
        let Args {
            path,
            config,
            keep,
            copy_to,
            move_to,
            delete,
            dry_run,
            verbose,
            print_config,
        } = args;

        let path = path
            .as_deref()
            .or(Some("."))
            .map(PathBuf::from)
            .filter(|p| p.exists() && p.is_dir())
            .ok_or_else(|| Error::new(InvalidInput, "Invalid directory"))
            .and_then(SelectedDirectory::try_from)?;

        let cfg = match config.map(PathBuf::from).map(FileFilter::try_load) {
            Some(file) => file?,
            None => FileFilter::load(path.as_ref().join("config.yaml")),
        };

        let keepfile = match keep.map(PathBuf::from).map(KeepFile::try_load) {
            Some(file) => file?,
            None => KeepFile::try_load(path.as_ref().join("keep.txt"))?,
        };

        let action = Action::new(copy_to, move_to, delete);

        Ok(AppConfig {
            path,
            filter: cfg,
            keep: keepfile,
            action,
            options: ExecutionOptions { dry_run, verbose },
            print: print_config,
        })
    }
}

/// The action to perform on matching files
///
/// This enum represents the action to perform on matching files.
/// It is calculated from the command line arguments.
#[derive(Debug, Clone)]
pub enum Action {
    /// Copy or move matching files to the specified directory
    MoveOrCopyTo(MoveOrCopy, PathBuf),
    /// Delete non-matching files
    Delete,
}

impl Action {
    /// Construct a new action
    ///
    /// This method returns the action to perform on matching files, depending on the command line arguments.
    /// It also returns a boolean indicating whether the action should be performed in dry-run mode.
    ///
    /// The actions are prioritized as follows:
    /// - If `copy_to` is specified, the action is `CopyTo`.
    /// - If `move_to` is specified, the action is `MoveTo`.
    /// - If no action is specified, the action is `CopyTo`, with the default directory being `./selected`.
    /// - If `delete` is specified, the action is `Delete`.
    pub fn new(copy_to: Option<String>, move_to: Option<String>, delete: bool) -> Action {
        use Action::*;
        use MoveOrCopy::*;
        match (move_to, copy_to, delete) {
            (_, Some(path), _) => MoveOrCopyTo(Copy, PathBuf::from(path)),
            (Some(path), _, _) => MoveOrCopyTo(Move, PathBuf::from(path)),
            (None, None, false) => MoveOrCopyTo(Copy, PathBuf::from("selected")),
            (_, _, true) => Delete,
        }
    }
}

/// The action to perform on matching files, as a move or copy operation
#[derive(Debug, Clone)]
pub enum MoveOrCopy {
    Move,
    Copy,
}

impl MoveOrCopy {
    /// Get a description of the operation
    pub fn description(&self) -> &str {
        match self {
            MoveOrCopy::Move => "moved",
            MoveOrCopy::Copy => "copied",
        }
    }

    /// Perform the move or copy operation
    ///
    /// This method moves or copies a file from the `from` path to the `to` path.
    ///
    /// # Arguments
    /// - `from` - the source path
    /// - `to` - the destination path
    ///
    /// # Errors
    /// Possible errors include:
    /// - If the parent directory of the destination path does not exist
    /// - If the parent directory of the destination path is not writable
    pub fn move_or_copy<P: AsRef<Path>, Q: AsRef<Path>>(&self, from: P, to: Q) -> Result<(), std::io::Error> {
        match to.as_ref().parent() {
            Some(parent) => {
                // Create the parent directories if they don't exist
                std::fs::create_dir_all(parent)?;
                match self {
                    MoveOrCopy::Move => std::fs::rename(from, to),
                    MoveOrCopy::Copy => std::fs::copy(from, to).map(|_| ()),
                }
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to get parent directory",
            )),
        }
    }
}

/// Options for executing the action
#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    /// Should the action be performed in dry-run mode?
    pub dry_run: bool,
    /// Should the detailed information be printed?
    pub verbose: bool,
}

/// A file filter configuration
///
/// This type describes how to filter files based on their names and extensions.
///
/// # Default values
/// Default configuration is resolved in the following order:
/// 1. Look for a file named `config.yaml` in the same directory as the executable
/// 2. Look for a file named `config.yaml` in the parent directory of the executable
/// 3. Use the default embedded configuration
/// 4. Use the hardcoded default configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct FileFilter {
    /// The name of the filter configuration
    name: Option<String>,
    /// The list of file extensions to match
    extensions: Vec<String>,
    /// The list of file formats to match
    formats: Vec<Format>,
}

impl Display for FileFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Filter {{")?;
        if let Some(name) = &self.name {
            writeln!(f, "    Name: {:?},", name)?;
        }
        writeln!(f, "    Extensions: {:?},", self.extensions)?;
        writeln!(f, "    Formats: [{}],", self.formats.iter().join(", "))?;
        writeln!(f, "}}")?;

        Ok(())
    }
}

impl Default for FileFilter {
    fn default() -> Self {
        // Get the path of the executable
        let install_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_owned()))
            .filter(|p| p.exists());

        // Look for a file named `config.yaml` in the same directory as the executable
        if let Some(filter) = install_dir
            .as_ref()
            .map(|p| p.join("config.yaml"))
            .filter(|p| p.exists())
            .and_then(|p| FileFilter::try_load(p).ok())
        {
            return filter;
        }

        // Look for a file named `config.yaml` in the parent directory of the executable
        if let Some(filter) = install_dir
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.join("config.yaml")))
            .filter(|p| p.exists())
            .and_then(|p| FileFilter::try_load(p).ok())
        {
            return filter;
        }

        // Try to load the default configuration from the embedded file
        if let Ok(config) = serde_yaml::from_str(include_str!("default_config.yaml")) {
            return config;
        }

        // Fallback to the hardcoded default config
        FileFilter {
            // Fallback to the hardcoded default config
            name: Some("default_all".to_owned()),
            extensions: vec![], // All extensions
            formats: vec![regex!(r#".+\d+"#).clone().into()],
        }
    }
}

impl FileFilter {
    /// Try to load a file filter configuration from the specified path
    ///
    /// This method attempts to load a file filter configuration from the specified path.
    ///Ya
    /// If the file does not exist, or if an error occurs while reading the file, `None` is returned.
    fn try_load<P: AsRef<Path>>(config_path: P) -> Result<Self, ConfigFileError> {
        let config_file = File::open(config_path)?;
        let reader = BufReader::new(config_file);
        let filter = serde_yaml::from_reader(reader)?;
        Ok(filter)
    }

    /// Load a file filter configuration from the specified path
    ///
    /// Load a file filter configuration from the specified path, or return the default configuration if the file does not exist.
    fn load<P: AsRef<Path>>(config_path: P) -> Self {
        FileFilter::try_load(config_path).unwrap_or_default()
    }

    /// Check if a file name has one of the configured extensions
    pub fn has_extension<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();

        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_string())
            .map(|ext| self.extensions.contains(&ext))
            .unwrap_or(false)
    }

    /// Check if a file name has one of the configured formats
    pub fn has_format<P: AsRef<Path>>(&self, path: P) -> bool {
        self.formats
            .iter()
            .any(|f| f.matches(&self.extensions, path.as_ref()).unwrap_or(false))
    }

    /// Check if a file name matches one of the configured formats and has one of the configured extensions
    pub fn matches<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();
        self.has_format(path) && self.has_extension(path)
    }

    /// Convert the file filter configuration into a matcher function
    ///
    /// This method converts the file filter configuration into a matcher function that can be used to filter files.
    ///
    /// The matcher function takes a reference to a `PathBuf` and returns a boolean indicating whether the file should be kept.
    ///
    /// The matcher function is a closure that captures the `extensions` and `formats` fields of the `FileFilter` struct,
    /// and is cloneable. This allows the matcher function to be used in multiple places without cloning the `FileFilter` struct,
    /// and allows cloning of the iterators where the matcher function is used.
    pub fn into_matcher(self) -> Rc<dyn Fn(&&PathBuf) -> bool> {
        Rc::new(move |path| self.matches(path))
    }
}

/// A file name format
///
/// This is a wrapper around a regular expression that describes a file name format.
///
/// It provides Display and utility methods to check if a file name matches the format, given a list of extensions.
#[derive(Debug, Serialize, Deserialize)]
pub struct Format(#[serde(with = "serde_regex")] Regex);

impl Display for Format {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\"", self.0.as_str())
    }
}

impl From<Regex> for Format {
    fn from(re: Regex) -> Self {
        Format(re)
    }
}

impl Format {
    /// Check if a file name matches the format, and has one of the specified extensions
    pub fn matches<P: AsRef<Path>>(&self, extensions: &[String], path: P) -> Option<bool> {
        let path = path.as_ref();
        let file_name = path.file_name()?.to_str()?;
        let file_extension = path.extension()?.to_str()?;
        if !extensions.contains(&file_extension.to_string()) {
            return Some(false);
        }

        Some(self.0.is_match(file_name))
    }
}

/// A list of numbers to keep
///
/// This type represents a list of numbers to keep from the matching files.
#[derive(Debug)]
pub struct KeepFile {
    pub lines: Vec<KeepFileLine>,
}

/// Wrapper around a number to keep
#[derive(Debug)]
pub struct KeepFileLine(u32);

/// Number and content of a line in keep file that doesn't contain a number
#[derive(Debug)]
pub struct KeepFileBadLine(usize, String);

/// List of lines in the keep file that don't contain a number
#[derive(thiserror::Error, Debug)]
pub struct KeepFileFormatError(pub Vec<KeepFileBadLine>);

impl IntoIterator for KeepFile {
    type Item = KeepFileLine;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.lines.into_iter()
    }
}

impl KeepFile {
    pub fn try_load<P: AsRef<Path>>(path: P) -> Result<KeepFile, KeepFileError> {
        let file = File::open(path.as_ref())?;
        let reader = BufReader::new(file);
        // Split the lines into valid and invalid lines
        let (valid, invalid): (Vec<_>, Vec<_>) = reader
            .lines()
            .enumerate()
            // Filter out invalid lines
            .filter_map(|(num, line)| line.ok().map(|line| (num, line)))
            // Parse the lines into numbers, or return an error
            .map(|(num, line)| match line.parse() {
                Ok(ord) => Ok(KeepFileLine(ord)),
                Err(_) => Err(KeepFileBadLine(num, line)),
            })
            .partition_result();

        if invalid.is_empty() {
            Ok(KeepFile { lines: valid })
        } else {
            Err(KeepFileError::Format {
                file: path.as_ref().to_path_buf(),
                lines: KeepFileFormatError(invalid),
            })
        }
    }

    /// Get an iterator over the list of numbers to keep
    pub fn iter(&self) -> std::slice::Iter<KeepFileLine> {
        self.lines.iter()
    }

    /// Get a mutable iterator over the list of numbers to keep
    pub fn iter_mut(&mut self) -> std::slice::IterMut<KeepFileLine> {
        self.lines.iter_mut()
    }

    /// Check if a file name matches contains a number
    ///
    /// This method checks if a file name contains a number that matches the specified number.
    pub fn matches_number(filename: &str, num: u32) -> bool {
        regex!(r#"(\d+)"#)
            .captures(filename)
            .and_then(|cap| cap.iter().last()?)
            .and_then(|m| m.as_str().parse().ok())
            .map_or(false, |m: u32| m == num)
    }

    /// Convert the keep file into an inclusive filter
    ///
    /// This method converts the keep file into a matcher function that can be used to filter files.
    /// Filter will allow files that were found in the original match file
    ///
    /// The filter function takes a reference to a `PathBuf` and returns a boolean indicating whether the file should be kept.
    ///
    /// The filter is a closure that captures the `lines` field of the `KeepFile` struct,
    /// and is cloneable. This allows the filter to be used in multiple places without cloning the `KeepFile` struct,
    /// and allows cloning of the iterators where the matcher function is used.
    pub fn into_inclusion_matcher(self) -> Rc<dyn Fn(&&PathBuf) -> bool> {
        Rc::new(move |path| {
            let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
                return false;
            };
            self.lines.iter().any(|KeepFileLine(num)| Self::matches_number(filename, *num))
        })
    }

    /// Convert the keep file into an inclusive filter
    ///
    /// This method converts the keep file into a matcher function that can be used to filter files.
    /// Filter will allow files that were not found in the original match file
    ///
    /// The filter function takes a reference to a `PathBuf` and returns a boolean indicating whether the file should be kept.
    ///
    /// The filter is a closure that captures the `lines` field of the `KeepFile` struct,
    /// and is cloneable. This allows the filter to be used in multiple places without cloning the `KeepFile` struct,
    /// and allows cloning of the iterators where the matcher function is used.
    pub fn into_exclusion_matcher(self) -> Rc<dyn Fn(&&PathBuf) -> bool> {
        Rc::new(move |path| {
            let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
                return false;
            };
            self.lines.iter().all(|KeepFileLine(num)| !Self::matches_number(filename, *num))
        })
    }
}

impl Display for KeepFileFormatError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for KeepFileBadLine(line, content) in self.0.iter() {
            writeln!(f, "Line {line}: {content}")?;
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigFileError {
    #[error("Config I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

/// Error type for keep file loading
///
/// This type represents the possible errors that can occur while loading the keep file.
#[derive(thiserror::Error, Debug)]
pub enum KeepFileError {
    /// The keep file contains invalid lines
    #[error("One or more lines in the keepfile \"{}\" are invalid:\n{}", .file.display(), .lines)]
    Format { file: PathBuf, lines: KeepFileFormatError },
    /// An I/O error occurred while reading the keep file
    #[error("Keepfile I/O error: {0}")]
    Io(#[from] std::io::Error),
}
