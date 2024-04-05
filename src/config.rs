//! Module containing declarations related to [ConfigFile] struct

use std::convert::identity;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use itertools::Itertools;
use regex::Regex;
use regex_macro::regex;
use serde::{Deserialize, Serialize};

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
pub struct ConfigFile {
    /// The name of the filter configuration
    name: Option<String>,
    /// The list of file extensions to match
    extensions: Vec<String>,
    /// The list of file formats to match
    formats: Vec<Format>,
}

impl Display for ConfigFile {
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

impl Default for ConfigFile {
    fn default() -> Self {
        // Get the path of the executable
        let install_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_owned()))
            .filter(|p| p.exists());

        let install_dir = install_dir.as_ref();

        // Look for a file named `config.yaml` in the same directory as the executable
        if let Some(filter) = install_dir
            .map(|p| p.join("config.yaml"))
            .filter(|p| p.exists() && p.is_file())
            .and_then(|p| ConfigFile::try_load(p).ok())
        {
            return filter;
        }

        // Look for a file named `config.yaml` in the parent directory of the executable
        if let Some(filter) = install_dir
            .and_then(|p| p.parent().map(|p| p.join("config.yaml")))
            .filter(|p| p.exists() && p.is_file())
            .and_then(|p| ConfigFile::try_load(p).ok())
        {
            return filter;
        }

        // Try to load the default configuration from the embedded file
        if let Ok(config) = serde_yaml::from_str(include_str!("default_config.yaml")) {
            return config;
        }

        // Fallback to the hardcoded default config
        ConfigFile {
            // Fallback to the hardcoded default config
            name: Some("default_all".to_owned()),
            extensions: vec![], // All extensions
            formats: vec![regex!(r#".+\d+"#).clone().into()],
        }
    }
}

impl ConfigFile {
    /// Try to load a file filter configuration from the specified path
    ///
    /// This method attempts to load a file filter configuration from the specified path.
    ///Ya
    /// If the file does not exist, or if an error occurs while reading the file, `None` is returned.
    pub(crate) fn try_load<P: AsRef<Path>>(config_path: P) -> Result<Self, ConfigFileError> {
        let config_file = File::open(config_path)?;
        let reader = BufReader::new(config_file);
        let filter = serde_yaml::from_reader(reader)?;
        Ok(filter)
    }

    /// Load a file filter configuration from the specified path
    ///
    /// Load a file filter configuration from the specified path, or return the default configuration if the file does not exist.
    pub(crate) fn load<P: AsRef<Path>>(config_path: P) -> Self {
        ConfigFile::try_load(config_path).unwrap_or_default()
    }

    /// Check if a file name has one of the configured extensions
    pub fn has_extension<P: AsRef<Path>>(&self, path: P) -> bool {
        path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .map_or(false, |ext| self.extensions.contains(&ext))
    }

    /// Check if a file name has one of the configured formats
    pub fn has_format<P: AsRef<Path>>(&self, path: P) -> bool {
        self.formats.iter().filter_map(|f| f.matches(&path)).any(identity)
    }

    /// Check if a file name matches one of the configured formats and has one of the configured extensions
    pub fn matches<P: AsRef<Path>>(&self, path: P) -> bool {
        self.has_extension(&path) && self.has_format(&path)
    }

    /// Convert the  configuration into a filter function
    ///
    /// Files are filtered based on the configured extensions and formats.
    ///
    /// Returned function takes a reference to a `PathBuf` and returns a boolean indicating whether the file should be kept.
    pub fn into_filter(self) -> Rc<dyn Fn(&&PathBuf) -> bool> {
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
    pub fn matches<P: AsRef<Path>>(&self, path: P) -> Option<bool> {
        let path = path.as_ref();
        let file_name = path.file_name()?.to_str()?;

        Some(self.0.is_match(file_name))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigFileError {
    #[error("Config I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

#[cfg(test)]
mod test {
    use crate::test_utils::resource_dir;

    use super::*;

    #[test]
    fn load_config_file() {
        let config = ConfigFile::load(resource_dir().join("cfg.yaml"));
        assert_eq!(config.name, Some("test_cfg".to_owned()));
        assert_eq!(config.extensions, vec!["txt".to_owned(), "csv".to_owned()]);
        assert_eq!(config.formats.len(), 1);
    }

    #[test]
    fn default_config_file() {
        let _: ConfigFile = serde_yaml::from_str(include_str!("default_config.yaml")).unwrap();
    }

    #[test]
    fn has_extension() {
        let config = ConfigFile {
            name: None,
            extensions: vec!["txt".to_owned(), "csv".to_owned()],
            formats: vec![],
        };

        assert!(config.has_extension("test.txt"));
        assert!(config.has_extension("test.csv"));
        assert!(!config.has_extension("test.md"));
    }

    #[test]
    fn has_format_no_ext() {
        let config = ConfigFile {
            name: None,
            extensions: vec![],
            formats: vec![regex!(r#".+\d+"#).clone().into()],
        };

        assert!(config.has_format("test1"));
        assert!(config.has_format("test2"));
        assert!(!config.has_format("test"));
    }

    #[test]
    fn into_filter() {
        let config = ConfigFile {
            name: None,
            extensions: vec!["txt".to_owned()],
            formats: vec![regex!(r#".+\d+"#).clone().into()],
        };

        let filter = config.into_filter();

        assert!(filter(&&PathBuf::from("test1.txt")));
        assert!(filter(&&PathBuf::from("test2.txt")));
        assert!(!filter(&&PathBuf::from("test.txt")));

        assert!(!filter(&&PathBuf::from("test1.md")));
        assert!(!filter(&&PathBuf::from("test1.md")));
        assert!(!filter(&&PathBuf::from("test.md")));
    }
}
