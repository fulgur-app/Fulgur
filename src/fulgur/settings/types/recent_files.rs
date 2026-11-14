use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentFiles {
    pub(in crate::fulgur::settings) files: Vec<PathBuf>,
    pub(in crate::fulgur::settings) max_files: usize,
}

impl RecentFiles {
    /// Create a new recent files instance
    ///
    /// ### Arguments
    /// - `max_files`: The maximum number of files to store
    ///
    /// ### Returns
    /// - `RecentFiles`: The new recent files instance
    #[must_use]
    pub fn new(max_files: usize) -> Self {
        Self {
            files: Vec::new(),
            max_files,
        }
    }

    /// Add a file to the recent files
    ///
    /// ### Arguments
    /// - `file`: The file to add
    pub fn add_file(&mut self, file: PathBuf) {
        self.files.push(file);
        if self.files.len() > self.max_files {
            self.files.remove(0);
        }
    }

    /// Remove a file from the recent files
    ///
    /// ### Arguments
    /// - `file`: The file to remove
    pub fn remove_file(&mut self, file: &PathBuf) {
        self.files.retain(|f| f != file);
    }

    /// Get the recent files
    ///
    /// ### Returns
    /// - `&Vec<PathBuf>`: The recent files
    #[must_use]
    pub fn get_files(&self) -> &Vec<PathBuf> {
        &self.files
    }

    /// Clear the recent files
    pub fn clear(&mut self) {
        self.files.clear();
    }
}
