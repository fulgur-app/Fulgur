use log::LevelFilter;
use simplelog::*;
use std::fs::{self, File};
use std::path::PathBuf;

/// Get the path to the log file
///
/// @return: The path to the log file
fn log_file_path() -> anyhow::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var("APPDATA")?;
        let mut path = PathBuf::from(app_data);
        path.push("Fulgur");
        fs::create_dir_all(&path)?;
        path.push("fulgur.log");
        Ok(path)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME")?;
        let mut path = PathBuf::from(home);
        path.push(".fulgur");
        fs::create_dir_all(&path)?;
        path.push("fulgur.log");
        Ok(path)
    }
}

/// Initialize the file logger
///
/// @return: Result indicating success or failure
pub fn init() -> anyhow::Result<()> {
    let log_path = log_file_path()?;
    #[cfg(debug_assertions)]
    let log_level = LevelFilter::Debug;
    #[cfg(not(debug_assertions))]
    let log_level = LevelFilter::Info;
    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .set_time_offset_to_local()
        .unwrap_or_else(|builder| builder)
        .build();
    let log_file = File::create(&log_path)?;
    WriteLogger::init(log_level, config, log_file)?;
    log::info!("Logger initialized at: {:?}", log_path);
    Ok(())
}

/// Get the log file path for display purposes
///
/// @return: The path to the log file as a string
#[allow(dead_code)]
pub fn get_log_path() -> Option<String> {
    log_file_path()
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}
