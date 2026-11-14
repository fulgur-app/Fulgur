use crate::fulgur::themes::{BundledThemes, themes_directory_path};
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Clone, Deserialize)]
pub struct ThemeInfo {
    pub name: String,
    pub mode: String,
}

#[derive(Clone, Deserialize)]
pub struct ThemeFile {
    pub name: String,
    pub author: String,
    pub themes: Vec<ThemeInfo>,
    #[serde(skip)]
    pub path: PathBuf,
}

impl ThemeFile {
    /// Load a theme file from a path
    ///
    /// ### Arguments
    /// - `path`: The path to the theme file
    ///
    /// ### Errors
    /// Returns an error if the theme file cannot be read or if it fails to
    /// deserialize from JSON.
    ///
    /// ### Returns
    /// - `anyhow::Result<Self>`: The theme file
    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        let json = fs::read_to_string(&path)?;
        let mut theme_file: ThemeFile = serde_json::from_str(&json)?;
        theme_file.path = path;
        Ok(theme_file)
    }
}

#[derive(Clone)]
pub struct Themes {
    pub default_themes: Vec<ThemeFile>,
    pub user_themes: Vec<ThemeFile>,
}

impl Themes {
    /// Load the theme settings from the themes folder
    ///
    /// ### Errors
    /// Returns an error if the themes directory cannot be resolved or read,
    /// or if any bundled or user theme file fails to load.
    ///
    /// ### Returns
    /// - `anyhow::Result<Self>`: The theme settings
    pub fn load() -> anyhow::Result<Self> {
        let themes_dir = themes_directory_path()?;
        let themes_files = fs::read_dir(&themes_dir)?;
        let default_themes: Vec<ThemeFile> = BundledThemes::iter()
            .map(|file| ThemeFile::load(themes_dir.join(file.as_ref())))
            .collect::<Result<Vec<ThemeFile>, anyhow::Error>>()?;
        let default_themes_names = BundledThemes::iter()
            .map(|file| file.as_ref().to_string())
            .collect::<Vec<String>>();
        let user_themes: Vec<ThemeFile> = themes_files
            .filter_map(|entry| {
                entry.ok().and_then(|entry| {
                    let filename = entry.file_name().to_string_lossy().to_string();
                    if default_themes_names.contains(&filename) {
                        None
                    } else {
                        Some(entry)
                    }
                })
            })
            .filter_map(|entry| ThemeFile::load(entry.path()).ok())
            .collect();
        Ok(Themes {
            default_themes,
            user_themes,
        })
    }
}
