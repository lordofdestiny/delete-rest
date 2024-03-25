//! Module with declarations related to [FileSource] trait

use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::SelectedDirectory;

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

impl<F: FileSource> FilteredFiles<F> {
    pub fn source(&self) -> &F {
        &self.source
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::*;
    
    #[test]
    fn test_selected_directory() -> TestResult {
        let selected = SelectedDirectory::try_from(resource_dir())?;
        assert_eq!(selected.0, resource_dir());

        Ok(())
    }

    #[test]
    fn test_selected_files() -> TestResult {
        let selected = SelectedDirectory::try_from(resource_dir())?;
        let files = SelectedFiles::try_from(selected)?;
        assert_eq!(files.dir.0, resource_dir());
        assert!(!files.files.is_empty());

        for file in files.files.iter() {
            assert!(test_filenames().contains(file), "File not found: {:?}", file);
        }

        Ok(())
    }

    #[test]
    fn test_filtered_files() -> TestResult {
        let selected = SelectedDirectory::try_from(resource_dir()).unwrap();
        let files = SelectedFiles::try_from(selected).unwrap();
        let filtered = files.filter_by(Rc::new(|f| get_extension(f).unwrap() == "txt"));
        assert_eq!(filtered.source().dir.0, resource_dir());
        assert!(!filtered.source().files.is_empty());
        assert_eq!(filtered.iter().count(), filtered.source().files.len() - 1);

        for file in filtered.iter() {
            assert!(get_extension(file).unwrap().ends_with("txt"));
        }

        for file in test_filenames() {
            if get_extension(file).unwrap().ends_with("txt") {
                assert!(filtered.iter().any(|f| f == file));
            } else {
                assert!(!filtered.iter().any(|f| f == file));
            }
        }

        Ok(())
    }
}
