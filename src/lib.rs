use std::cell::RefCell;
use std::clone::Clone;
use std::fmt::{Debug, Display};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use clap::Parser;
use itertools::Itertools;
use regex::Regex;
use regex_macro::regex;
use serde::{Deserialize, Serialize};

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

    #[clap(long, default_value = "false")]
    /// Only print what would be done, don't actually do anything.
    dry_run: bool,

    /// Move matching files to the specified directory
    /// This option is mutually exclusive with `delete`
    #[clap(
        short,
        long,
        conflicts_with_all = &["copy_to", "delete"],
        group = "action",
        value_name = "DIR"
    )]
    move_to: Option<String>,

    /// Copy matching files to the specified directory
    #[clap(
        short,
        long,
        conflicts_with_all = &["move_to", "delete"],
        group = "action",
        value_name = "DIR"
    )]
    copy_to: Option<String>,

    /// Delete non-matching files
    /// This option is mutually exclusive with `move-to`
    #[clap(
        short,
        long,
        conflicts_with_all = &["move_to", "copy_to"],
        group = "action",
    )]
    delete: bool,

    /// Print detailed information about what's happening
    #[clap(short, long)]
    verbose: bool,

    // Print parsed configuration and exit
    #[clap(long, exclusive = true)]
    pub print_config: bool,
}

#[derive(Debug, Clone)]
pub enum Action {
    MoveTo(String),
    CopyTo(String),
    Delete,
}

impl AppConfig {
    pub fn keepfile(&self) -> &str {
        &self.keep
    }

    pub fn verbose(&self) -> bool {
        self.verbose
    }

    pub fn action(&self) -> (Action, bool) {
        let Self {
            dry_run,
            delete,
            move_to,
            copy_to,
            ..
        } = self;
        (
            match (move_to, copy_to, delete) {
                (_, Some(path), _) => Action::CopyTo(path.clone()),
                (Some(path), _, _) => Action::MoveTo(path.clone()),
                (None, None, false) => Action::CopyTo("./selected".to_string()),
                (_, _, true) => Action::Delete,
            },
            *dry_run,
        )
    }

    pub fn matching_files(&self) -> std::io::Result<Vec<PathBuf>> {
        let path = Path::new(&self.path);
        let mut files = Vec::new();
        let mut stack: Vec<_> = path.read_dir()?.flat_map(Result::ok).collect();

        while let Some(entry) = stack.pop() {
            if entry.path().is_dir() {
                stack.extend(entry.path().read_dir()?.flat_map(Result::ok));
            } else {
                files.push(entry.path().canonicalize()?);
            }
        }

        Ok(files)
    }

    pub fn read_to_keep(&self) -> Result<KeepFile, KeepFileError> {
        let path = Path::new(&self.keep);
        let path = path.canonicalize()?;
        let file = std::fs::File::open(path.clone())?;
        let reader = std::io::BufReader::new(file);
        let (valid, invalid): (Vec<_>, Vec<_>) = reader
            .lines()
            .enumerate()
            .filter_map(|(num, line)| line.ok().map(|line| (num, line)))
            .map(|(num, line)| match line.parse() {
                Ok(ord) => Ok(KeepFileLine(ord)),
                Err(_) => Err(KeepFileLineError(num, line)),
            })
            .partition_result();

        if invalid.is_empty() {
            Ok(KeepFile { lines: valid })
        } else {
            Err(KeepFileError::Format {
                filename: path.file_name().unwrap().to_str().unwrap().to_owned(),
                lines: invalid.into(),
            })
        }
    }

    pub fn filter_config(&self) -> FileFilter {
        self.config
            .as_ref()
            .map_or_else(FileFilter::default, |config| {
                FileFilter::load(Path::new(config))
            })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileFilter {
    name: Option<String>,
    extensions: Vec<String>,
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

impl Default for FileFilter {
    fn default() -> Self {
        std::env::current_exe()
            .ok()
            .and_then(|mut path| {
                // Attempt to read the config file from the same directory as the executable
                path.pop();
                let exec_dir = path.clone();

                path.push("config.yaml");
                println!(
                    "Attempting to load config from {:?}",
                    path.display()
                );

                match FileFilter::try_load(&path) {
                    Some(config) => Some(config),
                    None => {
                        let mut path = exec_dir.clone();
                        path.pop();
                        path.push("config.yaml");
                        println!(
                            "Attempting to load config from {:?}",
                            path.canonicalize().unwrap().display()
                        );
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
    fn try_load(config_path: &Path) -> Option<Self> {
        let config_str = std::fs::read_to_string(config_path).ok()?;
        serde_yaml::from_str(&config_str).ok()?
    }

    fn load(config_path: &Path) -> Self {
        FileFilter::try_load(config_path).unwrap_or_default()
    }

    fn has_extension_impl<P: AsRef<Path>>(extensions: &[String], path: P) -> bool {
        let path = path.as_ref();
        match path.extension() {
            Some(ext) => match ext.to_str() {
                Some(ext) => extensions.contains(&ext.to_string()),
                None => false,
            },
            None => false,
        }
    }

    pub fn has_extension<P: AsRef<Path>>(&self, path: P) -> bool {
        Self::has_extension_impl(&self.extensions, path)
    }

    fn has_format_impl<P: AsRef<Path>>(formats: &[Format], extensions: &[String], path: P) -> bool {
        formats
            .iter()
            .filter_map(|f| f.matches(extensions, path.as_ref()))
            .any(|x| x)
    }

    pub fn has_format<P: AsRef<Path>>(&self, path: P) -> bool {
        Self::has_format_impl(&self.formats, &self.extensions, path)
    }

    fn matches_impl<P: AsRef<Path>>(formats: &[Format], extensions: &[String], path: P) -> bool {
        let path = path.as_ref();
        Self::has_extension_impl(extensions, path)
            && Self::has_format_impl(formats, extensions, path)
    }

    pub fn matches<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();
        self.has_format(path) && self.has_extension(path)
    }

    pub fn into_matcher(self) -> impl Fn(&&PathBuf) -> bool + Clone {
        let Self {
            extensions,
            formats,
            ..
        } = self;
        struct Inner {
            extensions: Rc<RefCell<Vec<String>>>,
            format: Rc<RefCell<Vec<Format>>>,
        }
        impl Clone for Inner {
            fn clone(&self) -> Self {
                Self {
                    extensions: self.extensions.clone(),
                    format: self.format.clone(),
                }
            }
        }

        let inner = Inner {
            extensions: Rc::new(RefCell::new(extensions)),
            format: Rc::new(RefCell::new(formats)),
        };
        move |path| {
            let extensions = inner.extensions.borrow();
            let format = inner.format.borrow();
            Self::matches_impl(&format, &extensions, path)
        }
    }
}

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

#[derive(Debug, Copy, Clone)]
pub enum KeepFileMatcherType {
    Include,
    Exclude,
}

impl KeepFile {
    pub fn iter(&self) -> std::slice::Iter<KeepFileLine> {
        self.lines.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<KeepFileLine> {
        self.lines.iter_mut()
    }

    pub fn matches_number(filename: &str, num: u32) -> bool {
        regex!(r#"(\d+)"#)
            .captures(filename)
            .and_then(|cap| cap.iter().last()?)
            .and_then(|m| m.as_str().parse().ok())
            .map_or(false, |m: u32| m == num)
    }

    pub fn into_matcher(self, mtype: KeepFileMatcherType) -> impl Fn(&&PathBuf) -> bool + Clone {
        let lines = Rc::new(RefCell::new(self.lines));
        move |path| {
            let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
                return false;
            };
            use KeepFileMatcherType::*;
            match mtype {
                Include => lines
                    .borrow()
                    .iter()
                    .any(|KeepFileLine(num)| Self::matches_number(filename, *num)),
                Exclude => lines
                    .borrow()
                    .iter()
                    .all(|KeepFileLine(num)| !Self::matches_number(filename, *num)),
            }
        }
    }
}

#[derive(Debug)]
pub struct KeepFileLine(u32);

#[derive(Debug)]
pub struct KeepFileLineError(usize, String);

#[derive(Debug)]
pub struct KeepFileLineErrors(pub Vec<KeepFileLineError>);

impl From<Vec<KeepFileLineError>> for KeepFileLineErrors {
    fn from(errors: Vec<KeepFileLineError>) -> Self {
        KeepFileLineErrors(errors)
    }
}

impl Display for KeepFileLineErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for err in self.0.iter() {
            writeln!(f, "Line {}: {}", err.0, err.1)?;
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum KeepFileError {
    #[error("One or more lines in the keepfile \"{}\" are invalid:\n{}", .filename, .lines)]
    Format {
        filename: String,
        lines: KeepFileLineErrors,
    },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
