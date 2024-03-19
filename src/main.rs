use std::path::PathBuf;

use clap::Parser;

use delete_rest_lib::{
    Action, AppConfig, ExecutionOptions, FileSource, KeepFileError, MoveOrCopy, SelectedFiles,
};

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
                .for_each(|file| println!("Deleted: {}", file.display()));
        }
        return;
    }

    for file in matching_files.iter() {
        if let Err(e) = std::fs::remove_file(file) {
            eprintln!("Error: {}", e);
            errors += 1;
        }
        if options.verbose {
            println!("Deleted: {}", file.display());
        }
    }

    if errors > 0 {
        eprintln!("{} errors occurred", errors);
    }
}

/// Moves or copies files to the specified directory.
///
/// If `dry_run` is true, the files will not be moved.
/// If `verbose` is true, the files will be printed before being moved.
///
/// # Arguments
/// - `move_or_copy` - the action to perform (move or copy)
/// - `app_config` - the application configuration
/// - `matching_files` - an iterator over the files to be moved
/// - `dest_dir` - the destination directory
fn handle_move_or_copy(
    op: MoveOrCopy,
    app_config: AppConfig,
    matching_files: impl FileSource,
    dest_dir: PathBuf,
) {
    let ExecutionOptions { dry_run, verbose } = app_config.options();
    let mut errors = 0;

    let src_dir = matching_files.dir();
    for src in matching_files.iter() {
        let Ok(dest) = src.strip_prefix(src_dir).map(|p| dest_dir.join(p)) else {
            continue;
        };
        if dry_run {
            continue;
        }
        if let Err(e) = op.move_or_copy(src, &dest) {
            eprintln!("Error: {}", e);
            errors += 1;
        }
        if verbose {
            println!(
                "{} \"{}\" from to \"{}\"",
                op.description(),
                src.display(),
                dest.display()
            );
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
    let matching_files = matching_files.filter_by(match action {
        Action::Delete => keep.into_exclusion_matcher(),
        Action::MoveOrCopyTo(_, _) => keep.into_inclusion_matcher(),
    });

    let kept_count = matching_files.clone().count();
    if app_cfg.verbose() {
        println!("Keeping files: {kept_count}/{matching_count}");
    }

    // Step 6
    match action {
        Action::Delete => {
            handle_delete(app_cfg, matching_files);
        }
        Action::MoveOrCopyTo(op, dir) => {
            handle_move_or_copy(op, app_cfg, matching_files, dir);
        }
    }
}
