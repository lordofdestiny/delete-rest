//! Module containing declarations related to [KeepFile] struct

use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use itertools::Itertools;
use regex_macro::regex;

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
                Err(_) => Err(KeepFileBadLine(num + 1, line)),
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


#[cfg(test)]
mod test {
    use crate::test_utils::*;

    use super::*;

    #[test]
    pub fn test_load_keepfile() -> TestResult {
        KeepFile::try_load(resource_dir().join("keep.txt"))?;
        Ok(())
    }

    #[test]
    pub fn test_load_keepfile_error() -> TestResult {
        let result = KeepFile::try_load(resource_dir().join("keep_bad.txt"));
        assert!(result.is_err());

        let error = result.unwrap_err();

        match error {
            KeepFileError::Format { file, lines } => {
                assert_eq!(file, resource_dir().join("keep_bad.txt"));
                assert_eq!(lines.0.len(), 2);

                let mut lines = lines.0.iter();
                let error = lines.next().unwrap();
                assert_eq!(error.0, 1);
                assert_eq!(error.1, "daf");
                let error = lines.next().unwrap();
                assert_eq!(error.0, 2);
                assert_eq!(error.1, "hello");

                assert!(lines.next().is_none(), "No more errors");
            }
            _ => panic!("Unexpected error: {:?}", error),
        }


        Ok(())
    }

    #[test]
    pub fn test_keepfile_properly_loaded() -> TestResult {
        let keepfile = KeepFile::try_load(resource_dir().join("keep.txt"))?;
        assert_eq!(keepfile.lines.len(), 2);
        // Keep TXT_1
        assert_eq!(keepfile.lines[0].0, 1);
        // Keep TXT_4
        assert_eq!(keepfile.lines[1].0, 4);

        Ok(())
    }
    
    #[test]
    pub fn test_keepfile_inclusion_matcher() -> TestResult {
        let keepfile = KeepFile::try_load(resource_dir().join("keep.txt"))?;
        let matcher = keepfile.into_inclusion_matcher();
        
        // In the keepfile
        assert!(matcher(&&PathBuf::from("TXT_1")));
        assert!(matcher(&&PathBuf::from("TXT_4")));
        
        // Not in the keepfile
        assert!(!matcher(&&PathBuf::from("TXT_2")));
        assert!(!matcher(&&PathBuf::from("TXT_3")));
        assert!(!matcher(&&PathBuf::from("TXT_5")));
        
        // Without a number
        assert!(!matcher(&&PathBuf::from("TXT")));
        
        Ok(())
    }
}
