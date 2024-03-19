//! Module containing declaration related to [Action] struct

use std::path::{Path, PathBuf};

/// The action to perform on matching files
///
/// # Variants
/// - `MoveOrCopyTo` - Move or copy matching files to the specified directory
/// - `Delete` - Delete non-matching files
#[derive(Debug, Clone)]
pub enum Action {
    /// Copy or move matching files to the specified directory
    MoveOrCopyTo(MoveOrCopy, PathBuf),
    /// Delete non-matching files
    Delete,
}

impl Action {
    /// Construct a new action
    ///
    /// Constructs an action to perform on matching files, depending on the command line arguments.
    ///
    /// The actions are prioritized as follows:
    /// - If `copy_to` is specified, the action is `CopyTo`.
    /// - If `move_to` is specified, the action is `MoveTo`.
    /// - If no action is specified, the action is `CopyTo`, with the default directory being `./selected`.
    /// - If `delete` is specified, the action is `Delete`.
    pub fn new(copy_to: Option<String>, move_to: Option<String>, delete: bool) -> Action {
        use Action::*;
        use MoveOrCopy::*;
        match (move_to, copy_to, delete) {
            (_, Some(path), _) => MoveOrCopyTo(Copy, PathBuf::from(path)),
            (Some(path), _, _) => MoveOrCopyTo(Move, PathBuf::from(path)),
            (None, None, false) => MoveOrCopyTo(Copy, PathBuf::from("selected")),
            (_, _, true) => Delete,
        }
    }
}

/// The action to perform on matching files, as a move or copy operation
#[derive(Debug, Clone)]
pub enum MoveOrCopy {
    /// Files will be moved
    Move,
    /// Files will be copied
    Copy,
}

impl MoveOrCopy {
    /// Get a description of the operation
    pub fn description(&self) -> &str {
        match self {
            MoveOrCopy::Move => "moved",
            MoveOrCopy::Copy => "copied",
        }
    }

    /// Perform the move or copy operation
    ///
    /// This method moves or copies a file from the `from` path to the `to` path.
    ///
    /// # Arguments
    /// - `from` - the source path
    /// - `to` - the destination path
    ///
    /// # Errors
    /// Possible errors include:
    /// - If the parent directory of the destination path does not exist
    /// - If the parent directory of the destination path is not writable
    pub fn move_or_copy<P: AsRef<Path>, Q: AsRef<Path>>(&self, from: P, to: Q) -> Result<(), std::io::Error> {
        match to.as_ref().parent() {
            Some(parent) => {
                // Create the parent directories if they don't exist
                std::fs::create_dir_all(parent)?;
                match self {
                    MoveOrCopy::Move => std::fs::rename(from, to),
                    MoveOrCopy::Copy => std::fs::copy(from, to).map(|_| ()),
                }
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to get parent directory",
            )),
        }
    }
}
