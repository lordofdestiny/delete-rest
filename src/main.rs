#[doc = include_str!("../README.md")]
use std::path::PathBuf;

use clap::Parser;

use delete_rest_lib::action::{Action, MoveOrCopy};
use delete_rest_lib::file_source::{FileSource, SelectedFiles};
use delete_rest_lib::{AppConfig, Args, ExecutionOptions};

/// Deletes files that from the provided source
///
/// If `options.dry_run` is true, the files will not be deleted.
/// If `options.verbose` is true, the files will be printed before being deleted.
///
/// # Arguments
/// options - the execution options
/// matching_files - files that should be deleted
fn handle_delete(options: ExecutionOptions, matching_files: impl FileSource) {
    let mut errors = 0;

    if options.dry_run {
        if options.verbose {
            matching_files.iter().for_each(|file| println!("Deleted: {}", file.display()));
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
/// If `options.dry_run` is true, the files will not be moved.
/// If `options.verbose` is true, the files will be printed before being moved.
///
/// # Arguments
/// op - the move or copy operation
/// options - the execution options
/// matching_files - files that should be moved or copied
/// dest_dir - the destination directory
fn handle_move_or_copy(op: MoveOrCopy, options: ExecutionOptions, matching_files: impl FileSource, dest_dir: PathBuf) {
    let ExecutionOptions { dry_run, verbose, .. } = options;
    let mut errors = 0;

    let src_dir = matching_files.dir();
    for src in matching_files.iter() {
        let Ok(dest) = src.strip_prefix(src_dir).map(|p| dest_dir.join(p)) else {
            continue;
        };
        if !dry_run {
            if let Err(e) = op.move_or_copy(src, &dest) {
                eprintln!("Error: {}", e);
                errors += 1;
            }
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
fn main() {
    let args = Args::parse();

    let config = match AppConfig::try_from(args) {
        Ok(config) => config,
        Err(e) => return eprintln!("{e}"),
    };

    if config.options.print {
        return println!("{}", config.config_file);
    }

    let files = match SelectedFiles::try_from(config.path) {
        Ok(files) => files,
        Err(e) => return eprintln!("{e}"),
    };

    let matching_files = files.filter_by(config.config_file.into_filter());

    if config.options.verbose {
        println!(
            "Matching files: {}/{}",
            matching_files.count(),
            matching_files.source().count()
        );
    }

    let matching_files = matching_files.filter_by(match config.action {
        Action::Delete => config.keepfile.into_exclusion_matcher(),
        Action::MoveOrCopyTo(_, _) => config.keepfile.into_inclusion_matcher(),
    });

    if config.options.verbose {
        let mut kept_count = matching_files.count();
        let matching_count = matching_files.source().count();

        if let Action::Delete = config.action {
            kept_count = matching_count - kept_count;
        }
        println!("Keeping files: {kept_count}/{matching_count}")
    }

    // Step 6
    match config.action {
        Action::Delete => handle_delete(config.options, matching_files),
        Action::MoveOrCopyTo(op, dir) => handle_move_or_copy(op, config.options, matching_files, dir),
    }
}
