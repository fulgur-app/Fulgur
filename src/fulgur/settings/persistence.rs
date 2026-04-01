use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::fulgur::utils::atomic_write::atomic_write_file;

use super::Settings;

impl Settings {
    /// Get the path to the settings file
    ///
    /// ### Returns
    /// - `Ok(PathBuf)`: The path to the settings file
    /// - `Err(anyhow::Error)`: If there was an error getting the path
    fn settings_file_path() -> anyhow::Result<PathBuf> {
        let mut path = crate::fulgur::utils::paths::config_dir()?;
        path.push("settings.json");
        Ok(path)
    }

    /// Save the settings to a specific path
    ///
    /// ### Description
    /// Core implementation for saving settings. Can be used with custom paths
    /// for testing or alternative storage locations. Uses atomic file writes to
    /// prevent corruption when multiple windows write simultaneously.
    ///
    /// ### Arguments
    /// - `path`: The path to save the settings to
    ///
    /// ### Returns
    /// - `Ok(())`: The result of the operation
    /// - `Err(anyhow::Error)`: If there was an error saving the settings
    pub fn save_to_path(&mut self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        atomic_write_file(path, json.as_bytes())
    }

    /// Load the settings from a specific path
    ///
    /// ### Description
    /// Core implementation for loading settings. Can be used with custom paths
    /// for testing or alternative storage locations.
    ///
    /// ### Arguments
    /// - `path`: The path to load the settings from
    ///
    /// ### Returns
    /// - `Ok(Settings)`: The loaded settings
    /// - `Err(anyhow::Error)`: If there was an error loading the settings
    pub fn load_from_path(path: &PathBuf) -> anyhow::Result<Self> {
        let json = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read settings file: {}", e))?;
        let settings: Settings = serde_json::from_str(&json)
            .map_err(|e| anyhow::anyhow!("Failed to parse settings: {}", e))?;
        Ok(settings)
    }

    /// Save the settings to the default state file location
    ///
    /// ### Returns
    /// - `Ok(())`: The result of the operation
    /// - `Err(anyhow::Error)`: If there was an error saving the settings
    pub fn save(&mut self) -> anyhow::Result<()> {
        let path = Self::settings_file_path()?;
        self.save_to_path(&path)
    }

    /// Load the settings from the default state file location
    ///
    /// ### Returns
    /// - `Ok(Settings)`: The loaded settings
    /// - `Err(anyhow::Error)`: If there was an error loading the settings
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::settings_file_path()?;
        Self::load_from_path(&path)
    }

    /// Get the recent files
    ///
    /// ### Returns
    /// - `Vec<PathBuf>`: The recent files
    pub fn get_recent_files(&mut self) -> Vec<PathBuf> {
        let mut files = self.recent_files.get_files().clone();
        files.reverse();
        files
    }

    /// Add a file to the recent files
    ///
    /// ### Arguments
    /// - `file`: The file to add
    ///
    /// ### Returns
    /// - `Ok(())`: The result of the operation
    /// - `Err(anyhow::Error)`: If there was an error adding the file
    pub fn add_file(&mut self, file: PathBuf) -> anyhow::Result<()> {
        if self.recent_files.get_files().contains(&file) {
            self.recent_files.remove_file(file.clone());
        }
        self.recent_files.add_file(file);
        self.save()
    }
}
