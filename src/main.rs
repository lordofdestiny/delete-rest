use std::path::{Path, PathBuf};

use clap::Parser;

use delete_rest::{Action, AppConfig, KeepFileError};
use delete_rest::KeepFileMatcherType::{Exclude, Include};

#[derive(thiserror::Error, Debug)]
enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid cfg format: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("One or more lines in the keep file are invalid")]
    BadKeepFile(#[from] KeepFileError),
}

fn handle_delete<'a>(matching_files: impl Iterator<Item = &'a Path>, dry_run: bool, verbose: bool) {
    let mut errors = 0;
    for file in matching_files {
        if verbose {
            println!("Deleting: {}", file.display());
        }
        if !dry_run {
            if let Err(e) = std::fs::remove_file(file) {
                eprintln!("Error: {}", e);
                errors += 1;
            }
        }
    }
    if errors > 0 {
        eprintln!("{} errors occurred", errors);
    }
}

pub fn make_from_to<'a>(file: &'a Path, dir: &str) -> Option<(&'a Path, PathBuf)> {
    let path = Path::new(&dir);
    let filename = file.file_name()?.to_str()?.to_string();
    Some((file, path.join(filename)))
}

fn handle_move_to<'a>(
    matching_files: impl Iterator<Item = &'a Path>,
    dir: &str,
    dry_run: bool,
    verbose: bool,
) {
    let mut errors = 0;
    for (from, to) in matching_files.filter_map(|file| make_from_to(file, dir)) {
        if verbose {
            println!("Moving from {} to {}", from.display(), to.display());
        }
        if dry_run {
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create directory");
            if let Err(e) = std::fs::rename(from, to) {
                eprintln!("Error: {}", e);
                errors += 1;
            }
        }
    }
    if errors > 0 {
        eprintln!("{} errors occurred", errors);
    }
}

fn handle_copy_to<'a>(
    matching_files: impl Iterator<Item = &'a Path>,
    dir: &str,
    dry_run: bool,
    verbose: bool,
) {
    let mut errors = 0;
    for (from, to) in matching_files.filter_map(|file| make_from_to(file, dir)) {
        if verbose {
            println!("Copying from {} to {}", from.display(), to.display());
        }
        if dry_run {
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create directory");
            if let Err(e) = std::fs::copy(from, to) {
                eprintln!("Error: {}", e);
                errors += 1;
            }
        }
    }
    if errors > 0 {
        eprintln!("{} errors occurred", errors);
    }
}

fn main() {
    let app_cfg = AppConfig::parse();

    if app_cfg.print_config {
        println!("{}", app_cfg.filter_config());
        return;
    }

    let filter = app_cfg.filter_config();

    let files = match app_cfg.matching_files() {
        Ok(files) => files,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    let matching_files = files.iter().filter(filter.into_matcher());
    let total_files_cnt = files.len();
    let matching_files_cnt = matching_files.clone().count();

    if app_cfg.verbose() {
        println!("Matching files: {matching_files_cnt}/{total_files_cnt}");
    }

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

    let action = app_cfg.action();
    let keep_type = match action.0 {
        Action::Delete => Exclude,
        _ => Include,
    };

    let matching_files = matching_files
        .filter(keep.into_matcher(keep_type))
        .map(|path| path.as_path());
    let should_keep_cnt = matching_files.clone().count();

    println!("Keeping files: {should_keep_cnt}/{matching_files_cnt}");

    match action {
        (Action::Delete, dry_run) => {
            handle_delete(matching_files, dry_run, app_cfg.verbose());
        }
        (Action::MoveTo(dir), dry_run) => {
            handle_move_to(matching_files, &dir, dry_run, app_cfg.verbose());
        }
        (Action::CopyTo(dir), dry_run) => {
            handle_copy_to(matching_files, &dir, dry_run, app_cfg.verbose());
        }
    }
}
