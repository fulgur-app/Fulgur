use crate::fulgur::utils::portable::{PortableLayout, from_persisted_path, to_persisted_path};
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

    /// Copy the recent files with their paths in persisted form
    ///
    /// ### Arguments
    /// - `layout`: The portable layout paths are persisted relative to
    ///
    /// ### Returns
    /// - `RecentFiles`: The copy, with paths on the portable volume made relative
    pub(in crate::fulgur::settings) fn to_persisted(&self, layout: &PortableLayout) -> Self {
        Self {
            files: self
                .files
                .iter()
                .map(|file| to_persisted_path(file, Some(layout)))
                .collect(),
            max_files: self.max_files,
        }
    }

    /// Resolve paths read from the settings file into in-memory paths
    ///
    /// ### Arguments
    /// - `layout`: The portable layout relative paths are resolved against
    pub(in crate::fulgur::settings) fn resolve_persisted(&mut self, layout: &PortableLayout) {
        for file in &mut self.files {
            *file = from_persisted_path(file, Some(layout));
        }
    }
}
