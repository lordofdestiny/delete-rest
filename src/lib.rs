use std::clone::Clone;
use std::fmt::{Debug, Display};
use std::io::BufRead;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use clap::Parser;
use itertools::Itertools;
use regex::Regex;
use regex_macro::regex;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct SelectedDirectory(PathBuf);

impl TryFrom<PathBuf> for SelectedDirectory {
    type Error = std::io::Error;
    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        if path.is_dir() {
            path.canonicalize().map(Self)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Not a directory",
            ))
        }
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

#[derive(Clone)]
pub struct SelectedFiles {
    pub dir: SelectedDirectory,
    pub files: Vec<PathBuf>,
}

impl TryFrom<SelectedDirectory> for SelectedFiles {
    type Error = std::io::Error;
    fn try_from(selected: SelectedDirectory) -> Result<Self, Self::Error> {
        let files = selected.read_recursive_path()?;
        Ok(SelectedFiles {
            dir: selected,
            files,
        })
    }
}

pub trait FileSource {
    fn dir(&self) -> &Path;

    fn iter(&self) -> impl Iterator<Item = &PathBuf> + Clone;
    fn count(&self) -> usize {
        self.iter().count()
    }
    
    fn filter_by(self, matcher: Rc<dyn Fn(&&PathBuf) -> bool>) -> FilteredFiles<Self>
    where
        Self: Sized,
    {
        FilteredFiles {
            source: self,
            matcher,
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

#[derive(Clone)]
pub struct FilteredFiles<F: FileSource> {
    source: F,
    matcher: Rc<dyn Fn(&&PathBuf) -> bool>,
}

impl<F: FileSource> FilteredFiles<F> {
    pub fn iter(&self) -> impl Iterator<Item = &PathBuf> + Clone {
        self.source.iter().filter(self.matcher.deref())
    }
}

impl<F: FileSource> FileSource for FilteredFiles<F> {
    fn dir(&self) -> &Path {
        self.source.dir()
    }
    fn iter(&self) -> impl Iterator<Item = &PathBuf> + Clone {
        self.source.iter()
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
#[derive(Parser, Debug)]
#[clap(
    name = "delete-rest",
    author = "lordofdestiny",
    about = "A CLI app to delete files based on a configuration file"
)]
#[command(arg_required_else_help(true))]
pub struct AppConfig {
    /// The directory to search for files
    #[clap(short, long, default_value = ".", value_name = "DIR")]
    path: String,

    /// The file to use as the keep file
    #[clap(short, long, default_value = "keep.txt")]
    keep: String,

    /// The configuration file to use
    #[clap(long)]
    config: Option<String>,

    /// Move matching files to the specified directory
    /// This option is mutually exclusive with `delete` and `copy-to`
    #[clap(
        short,
        long,
        conflicts_with_all = &["copy_to", "delete"],
        group = "action",
        value_name = "DIR"
    )]
    move_to: Option<String>,

    /// Copy matching files to the specified directory
    /// This option is mutually exclusive with `move-to` and `delete`
    #[clap(
        short,
        long,
        conflicts_with_all = &["move_to", "delete"],
        group = "action",
        value_name = "DIR"
    )]
    copy_to: Option<String>,

    /// Delete non-matching files
    /// This option is mutually exclusive with `move-to` and `copy-to`
    #[clap(
        short,
        long,
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
    #[clap(long, exclusive = true)]
    pub print_config: bool,
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

#[derive(Debug, Clone)]
pub enum MoveOrCopy {
    Move,
    Copy,
}

impl MoveOrCopy {
    pub fn description(&self) -> &str {
        match self {
            MoveOrCopy::Move => "moved",
            MoveOrCopy::Copy => "copied",
        }
    }

    pub fn move_or_copy<P: AsRef<Path>, Q: AsRef<Path>>(
        &self,
        from: P,
        to: Q,
    ) -> Result<(), std::io::Error> {
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

#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    pub dry_run: bool,
    pub verbose: bool,
}

impl AppConfig {
    /// Get the path of the keep file
    pub fn keepfile(&self) -> &str {
        &self.keep
    }

    /// Should the detailed information be printed?
    pub fn verbose(&self) -> bool {
        self.verbose
    }

    /// Should the action be performed in dry-run mode?
    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    /// Get the directory to search for files
    pub fn directory(&self) -> Result<SelectedDirectory, std::io::Error> {
        SelectedDirectory::try_from(PathBuf::from(&self.path))
    }

    /// Options for executin the action
    ///
    /// This method returns a tuple of booleans indicating whether the action should be performed in dry-run mode
    /// and whether detailed information should be printed.
    ///
    /// The first element is the dry-run mode, and the second element is the verbose mode.
    pub fn options(&self) -> ExecutionOptions {
        ExecutionOptions {
            dry_run: self.dry_run,
            verbose: self.verbose,
        }
    }

    /// Derive the action to perform on matching files
    ///
    /// This method returns the action to perform on matching files, depending on the command line arguments.
    /// It also returns a boolean indicating whether the action should be performed in dry-run mode.
    ///
    /// The actions are prioritized as follows:
    /// - If `copy_to` is specified, the action is `CopyTo`.
    /// - If `move_to` is specified, the action is `MoveTo`.
    /// - If no action is specified, the action is `CopyTo`, with the default directory being `./selected`.
    /// - If `delete` is specified, the action is `Delete`.
    pub fn action(&self) -> Action {
        let Self {
            delete,
            move_to,
            copy_to,
            ..
        } = self;

        use Action::*;
        use MoveOrCopy::*;
        match (move_to, copy_to, delete) {
            (_, Some(path), _) => MoveOrCopyTo(Copy, PathBuf::from(path)),
            (Some(path), _, _) => MoveOrCopyTo(Move, PathBuf::from(path)),
            (None, None, false) => MoveOrCopyTo(Copy, PathBuf::from("selected")),
            (_, _, true) => Delete,
        }
    }

    /// Read the keep file
    ///
    /// This method reads the keep file and returns a list of numbers to keep.
    ///
    /// # Errors
    /// Possible errors include:
    /// - If the keep file does not exist
    /// - If the keep file is not readable
    /// - If an I/O error occurs while reading the keep file
    /// - If the keep file contains invalid lines
    pub fn read_to_keep(&self) -> Result<KeepFile, KeepFileError> {
        let path = Path::new(&self.keep).canonicalize()?;
        let file = std::fs::File::open(path.clone())?;
        let reader = std::io::BufReader::new(file);
        // Split the lines into valid and invalid lines
        let (valid, invalid): (Vec<_>, Vec<_>) = reader
            .lines()
            .enumerate()
            // Filter out invalid lines
            .filter_map(|(num, line)| line.ok().map(|line| (num, line)))
            // Parse the lines into numbers, or return an error
            .map(|(num, line)| match line.parse() {
                Ok(ord) => Ok(KeepFileLine(ord)),
                Err(_) => Err(KeepFileLineError(num, line)),
            })
            .partition_result();

        if invalid.is_empty() {
            Ok(KeepFile { lines: valid })
        } else {
            Err(KeepFileError::Format {
                filename: self.keep.clone(),
                lines: KeepFileLineErrors(invalid),
            })
        }
    }

    /// Get the file filter configuration
    ///
    /// This method returns the file filter configuration to use.
    ///
    /// If the `config` field is `None`, the default configuration is used.
    /// Else, the configuration is loaded from the specified file.
    pub fn filter_config(&self) -> FileFilter {
        self.config
            .as_ref()
            .map_or_else(FileFilter::default, |config| {
                FileFilter::load(Path::new(config))
            })
    }
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
        std::env::current_exe()
            .ok()
            .and_then(|mut path| {
                // Attempt to read the config.yaml file from the same directory as the executable
                path.pop();
                let exec_dir = path.clone();
                path.push("config.yaml");

                match FileFilter::try_load(&path) {
                    Some(config) => Some(config),
                    None => {
                        // Attempt to read the config.yaml file from the parent directory of the executable
                        let mut path = exec_dir.clone();
                        path.pop();
                        path.push("config.yaml");
                        FileFilter::try_load(&path)
                    }
                }
            })
            .or_else(|| {
                // Fallback to the default embedded config
                let config_str = include_str!("default_config.yaml");
                serde_yaml::from_str(config_str).ok()?
            })
            .unwrap_or_else(|| FileFilter {
                // Fallback to the hardcoded default config
                name: Some("default_all".to_owned()),
                extensions: ["jpg", "png", "cr2"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                formats: vec![regex!(r#".+\d+"#).clone().into()],
            })
    }
}

impl FileFilter {
    /// Try to load a file filter configuration from the specified path
    ///
    /// This method attempts to load a file filter configuration from the specified path.
    ///
    /// If the file does not exist, or if an error occurs while reading the file, `None` is returned.
    fn try_load(config_path: &Path) -> Option<Self> {
        let config_str = std::fs::read_to_string(config_path).ok()?;
        serde_yaml::from_str(&config_str).ok()?
    }

    /// Load a file filter configuration from the specified path
    ///
    /// Load a file filter configuration from the specified path, or return the default configuration if the file does not exist.
    fn load(config_path: &Path) -> Self {
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
            .filter_map(|f| f.matches(&self.extensions, path.as_ref()))
            .any(|x| x)
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

impl IntoIterator for KeepFile {
    type Item = KeepFileLine;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.lines.into_iter()
    }
}

impl KeepFile {
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
            self.lines
                .iter()
                .any(|KeepFileLine(num)| Self::matches_number(filename, *num))
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
            self.lines
                .iter()
                .all(|KeepFileLine(num)| !Self::matches_number(filename, *num))
        })
    }
}

/// Wrapper around a number to keep
#[derive(Debug)]
pub struct KeepFileLine(u32);

/// Number and content of a line in keep file that doesn't contain a number
#[derive(Debug)]
pub struct KeepFileLineError(usize, String);

/// List of lines in the keep file that don't contain a number
#[derive(Debug)]
pub struct KeepFileLineErrors(pub Vec<KeepFileLineError>);

impl Display for KeepFileLineErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for err in self.0.iter() {
            writeln!(f, "Line {}: {}", err.0, err.1)?;
        }
        Ok(())
    }
}

/// Error type for keep file loading
///
/// This type represents the possible errors that can occur while loading the keep file.
#[derive(thiserror::Error, Debug)]
pub enum KeepFileError {
    /// The keep file contains invalid lines
    #[error("One or more lines in the keepfile \"{}\" are invalid:\n{}", .filename, .lines)]
    Format {
        filename: String,
        lines: KeepFileLineErrors,
    },
    /// An I/O error occurred while reading the keep file
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
