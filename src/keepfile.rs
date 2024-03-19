//! Module containing declarations related to [KeepFile] struct

use itertools::Itertools;
use regex_macro::regex;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::rc::Rc;

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

impl KeepFile {
    /// Load the keepfile from the provided path
    ///
    /// This method loads the keepfile from the provided path, and returns a `KeepFile` if successful.
    ///
    /// If the file is not found, or if the file is not valid, an error is returned.
    ///
    /// # Errors
    /// - If the file is not found
    /// - If the file is not valid
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
    /// Filter will allow files that were found in the keepfile
    ///
    /// The filter function takes a reference to a `PathBuf` and returns a boolean indicating whether the file should be kept.
    ///
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
    /// Filter will allow files that were **not** found in the keep file
    ///
    /// The filter function takes a reference to a `PathBuf` and returns a boolean indicating whether the file should be kept.
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
