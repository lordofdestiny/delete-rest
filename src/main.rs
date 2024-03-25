//! # `delete-rest` application
//!
//! This application is a small utility written as an aid to photographers.
//! Photographers often need to keep only some of the images they took, and get rid of the rest.
//! This is often a manual labor, as they have to select images their clients want
//! by going through a folder of possibly thousands of images. This app tries to ease this process.
//!
//! The idea is to provide a configuration file `config.yaml` which describes a format ( a `regex`)
//! the images have, and a "keepfile" `keep.txt`, that enumerates images that are to be kept.
//!
//! This design decision regarding the `config.yaml` file was made because usually
//! all the files produced by a camera follow a single format, something like `VLA_xxxx.cr2`
//! which can easily be described with regular expressions. Additionally,
//! config file is able to only look for files with a particular extension, in case you are keeping your
//! `.cr2` and `.jpg` files near each other.
//!
//! Keepfiles follow a simple formatting:
//! ```text
//! 1
//! 16
//! 167
//! 33
//! ```
//! The program keeps/extracts files that contain this number instead of the `xxxx`.
//!
//! ## How to use
//! To get the detailed options descriptions, you can just run the program without any flags, or with the `--help` flag
//! ```
//! delete-rest
//! ```
//! ```
//! delete-rest --help
//! ```
//! This brings up the follwing explanation
//! ```text
//! A CLI app to delete files based on a configuration file
//!
//! Usage: delete-rest.exe [OPTIONS]
//!
//! Options:
//!   -p, --path <DIR>       The directory to search for files [default: .]
//!   -k, --keep <KEEP>      The file to use as the keep file
//!       --config <CONFIG>  The configuration file to use [aliases: cfg] [short aliases: Y]
//!   -m <DIR>               Move matching files to the specified directory. Mutually exclusive with `delete` and `copy-to`
//!   -c <DIR>               Copy matching files to the specified directory. Mutually exclusive with `move-to` and `delete`
//!   -d                     Delete non-matching files. Mutually exclusive with `move-to` and `copy-to`
//!       --dry-run          Only print what would be done, don't actually do anything
//!   -v, --verbose          Print detailed information about what's happening
//!       --print-config     Print parsed configuration and exit
//!   -h, --help             Print help (see more with '--help')
//! ```
//!
//! Minimal configuration includes providing one of the `-c`, `-m` or `-d` options,
//! which correspond to the copy, move, and delete operations. If multiple operations are supplied,
//! copy is always preferred, then move, and then delete. ***If none of these three flags is provided,
//! but other flags where, the default behaviour is to copy***.
//!
//! Files are looked up by default in the current working directory (directory the script
//! was run from). This is also the case for the keepfile and configuration file.
//! You can override this behaviour with the `-p` flag, which specifies the working directory.
//! If no `-k` or `--config` flags are provided, keepfile are config file are also looked up
//! from the same directory.
//!
//! When config file is not explicitly provided, it will look up several places,
//! or default to builtin configuration. Lookup order is the following:
//! - In the working directory
//! - Next to the executable itself
//! - In the parent folder of the executable
//! - Hardcoded configuration
//!     ```yaml
//!     name: default_builtin
//!     formats:
//!       - IMG_\d{4}\.\w+
//!     extensions:
//!       - jpg
//!       - png
//!       - cr2
//!     ```
//!
//! To provide a custom path to the keepfile use `-k` file. This path can be both relative
//! and absolute. If the provided keepfile has errors, program exits.
//!
//! To provide a custom path to the config file use `--config`, `--cfg` or `-Y` flags.
//! They are just aliases. This option also accepts relative and absolute paths.  
//! If the detected keepfile has errors, program exits.
//!
//! You can use `-v-` ( `--verobse` ) flag to print a detailed list of all files that are
//! being moved/copied/deleted.
//!
//! You can use `--dry-run` if you want to run the command without making any changes.
//! This is highly recommended, in combination with `--verbose`, before actually running the command.
//! This enables you to make sure that the right files are being selected ( for copy/move/delete),
//! or that they are being moved/copied to the right location.
//!
//! If you are providing a custom configuration (with `--config`), you can verify that it is being properly loaded
//! by using the `--print-config` flag. This will print the configuration and exit.
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
