use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Get the resource directory for testing
///
/// # Panics
///
/// - Panics if the resource directory does not exist
pub fn resource_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resource_dir = manifest_dir.join("resources/test");
    assert!(resource_dir.exists(), "Resource directory does not exist: {:?}", resource_dir);
    resource_dir.canonicalize().unwrap()
}

/// Visit all files in a directory and its subdirectories
/// 
/// This function visits all files in a directory and its subdirectories, calling the callback function for each file.
/// 
/// # Arguments
/// - `dir` - the directory to visit
/// - `cb` - the callback function to call for each file
pub fn visit_dirs(dir: &Path, cb: &mut impl FnMut(&Path)) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, cb)?;
            } else {
                cb(&path);
            }
        }
    }
    Ok(())
}

/// Visit all files in a directory and its subdirectories
/// 
/// This function visits all files in a directory and its subdirectories, returning a vector of all the files.
/// Files are sorted alphabetically.
/// 
/// # Arguments
/// - `dir` - the directory to visit
pub fn visit_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit_dirs(dir, &mut |path| {
        files.push(path.to_path_buf());
    })?;

    files.sort();
    
    Ok(files)
}

/// Get the set of filenames to test against, as a static reference to a set of paths
///
/// # Panics
///
/// - Panics if the resource directory does not exist
pub fn test_filenames() -> &'static HashSet<PathBuf> {
    static SET: OnceLock<HashSet<PathBuf>> = OnceLock::new();
    SET.get_or_init(|| {
        visit_files(&resource_dir()).unwrap().into_iter().collect()
    })
}


/// Get the extension of a file
pub fn get_extension<P: AsRef<Path>>(file: P) -> Option<String> {
    file.as_ref().extension().and_then(|ext| ext.to_str())
        .map(|ext| ext.to_string())
}