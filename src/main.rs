use std::path::{Path, PathBuf};

use clap::Parser;

use delete_rest_lib::{Action, AppConfig, FileSource, KeepFileError, SelectedFiles};

/// Deletes files that match the filter
///
/// Deletes files that match the filter. If `dry_run` is true, the files will not be deleted.
/// If `verbose` is true, the files will be printed before being deleted.
///
/// # Arguments
/// - `matching_files` - an iterator over the files to be deleted
/// - `dry_run` - if true, the files will not be deleted
/// - `verbose` - if true, the files will be printed before being deleted
fn handle_delete(app_config: AppConfig, matching_files: impl FileSource) {
    let options = app_config.options();
    let mut errors = 0;

    if options.dry_run {
        if options.verbose {
            matching_files
                .iter()
                .for_each(|file| println!("Deleting: {}", file.display()));
        }
        return;
    }

    for file in matching_files.iter() {
        if options.verbose {
            println!("Deleting: {}", file.display());
        }
        if let Err(e) = std::fs::remove_file(file) {
            eprintln!("Error: {}", e);
            errors += 1;
        }
    }

    if errors > 0 {
        eprintln!("{} errors occurred", errors);
    }
}

/// Moves files that match the filter to the specified directory
///
/// Moves files that match the filter to the specified directory. If `dry_run` is true, the files will not be moved.
/// If `verbose` is true, the files will be printed before being moved.
///
/// # Arguments
/// - `matching_files` - an iterator over the files to be moved
/// - `dir` - the directory to move the files to
/// - `dry_run` - if true, the files will not be moved
/// - `verbose` - if true, the files will be printed before being moved
fn handle_move_to(app_config: AppConfig, matching_files: impl FileSource, dest_dir: PathBuf) {
    let options = app_config.options();
    let mut errors = 0;

    if options.dry_run {
        if options.verbose {
            matching_files
                .iter()
                .for_each(|file| println!("Moving: {}", file.display()));
        }
        return;
    }

    let src_dir = matching_files.dir();
    for src in matching_files.iter() {
        let Ok(dest) = src.strip_prefix(src_dir).map(|p| dest_dir.join(p)) else {
            continue;
        };

        if options.verbose {
            println!("Moving from {} to {}", src.display(), dest.display());
        }
        
        match dest.parent() {
            Some(parent) => {
                // Create the parent directories if they don't exist
                std::fs::create_dir_all(parent).expect("Failed to create directory");
                if let Err(e) = std::fs::rename(src, dest) {
                    eprintln!("Error: {}", e);
                    errors += 1;
                }
            }
            None => {
                eprintln!("Error: Failed to get parent directory");
                errors += 1;
            }
        }
    }
    if errors > 0 {
        eprintln!("{} errors occurred", errors);
    }
}

/// Copies files that match the filter to the specified directory
///
/// Copies files that match the filter to the specified directory. If `dry_run` is true, the files will not be copied.
/// If `verbose` is true, the files will be printed before being copied.
///
/// # Arguments
/// - `matching_files` - an iterator over the files to be copied
/// - `dir` - the directory to copy the files to
/// - `dry_run` - if true, the files will not be copied
/// - `verbose` - if true, the files will be printed before being copied
fn handle_copy_to(app_config: AppConfig, matching_files: impl FileSource, dest_dir: PathBuf) {
    let options = app_config.options();
    let mut errors = 0;

    if options.dry_run {
        if options.verbose {
            matching_files
                .iter()
                .for_each(|file| println!("Copying: {}", file.display()));
        }
        return;
    }

    let src_dir = matching_files.dir();
    for src in matching_files.iter() {
        let Ok(dest) = src.strip_prefix(src_dir).map(|p| dest_dir.join(p)) else {
            continue;
        };

        if options.verbose {
            println!("Copying from {} to {}", src.display(), dest.display());
        }
        match dest.parent() {
            Some(parent) => {
                // Create the parent directories if they don't exist
                std::fs::create_dir_all(parent).expect("Failed to create directory");
                if let Err(e) = std::fs::copy(src, dest) {
                    eprintln!("Error: {}", e);
                    errors += 1;
                }
            }
            None => {
                eprintln!("Error: Failed to get parent directory");
                errors += 1;
            }
        }
    }
    if errors > 0 {
        eprintln!("{} errors occurred", errors);
    }
}

/// The main function
///
/// The main function parses the command line arguments, reads the configuration file, and processes the files.
///
/// # Steps
/// 1. Parse the command line arguments
///     - If there is no arguments, or the `--help` flag is set, print the help message and return
///     - If the `--print-config` flag is set, print the configuration and return
/// 2. Read the configuration file
///     - If the configuration file is not found, print an error message and return
/// 3. Get the files that match the filter
///     1. Get all the files in the specified path </li>
///     2. Filter the files that match the filter </li>
/// 4. Get the file names from the keep file
/// 5. Process the files ( separate files to keep and files to delete )
/// 6. Execute the action
fn main() {
    // Step 1
    let app_cfg = AppConfig::parse();

    if app_cfg.print_config {
        println!("{}", app_cfg.filter_config());
        return;
    }

    // Step 2
    let filter = app_cfg.filter_config();

    // Step 3.1
    let directory = match app_cfg.directory() {
        Ok(directory) => directory,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    let files: SelectedFiles = match directory.try_into() {
        Ok(files) => files,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    // Step 3.2
    let total_count = files.len();
    let matching_files = files.filter_by(filter.into_matcher());
    let matching_count = matching_files.count();

    if app_cfg.verbose() {
        println!("Matching files: {matching_count}/{total_count}");
    }

    // Step 4
    let keep = match app_cfg.read_to_keep() {
        Ok(keep) => keep,
        Err(error) => {
            match error {
                KeepFileError::Io(e) => match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        eprintln!("Keep file not found: {}", app_cfg.keepfile())
                    }
                    _ => eprintln!("I/O error: {}", e),
                },
                _ => eprintln!("Error: {}", error),
            }
            return;
        }
    };

    // Step 5
    let action = app_cfg.action();
    let matching_files = matching_files.filter_by(keep.into_matcher(action.matcher_type()));

    let kept_count = matching_files.clone().count();

    if app_cfg.verbose() {
        println!("Keeping files: {kept_count}/{matching_count}");
    }

    // Step 6
    match action {
        Action::Delete => {
            handle_delete(app_cfg, matching_files);
        }
        Action::MoveTo(dir) => {
            handle_move_to(app_cfg, matching_files, dir);
        }
        Action::CopyTo(dir) => {
            handle_copy_to(app_cfg, matching_files, dir);
        }
    }
}
