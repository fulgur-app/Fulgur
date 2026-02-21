use log::LevelFilter;
use simplelog::*;
use std::fs::File;
use std::path::PathBuf;

use crate::fulgur::utils::paths;

/// Get the path to the log file, create the log file directory if it doesn't exist
///
/// ### Returns
/// - `Ok(PathBuf)`: The path to the log file
/// - `Err(anyhow::Error)`: If the log file path could not be determined or created
fn log_file_path() -> anyhow::Result<PathBuf> {
    paths::config_file("fulgur.log")
}

/// Initialize the file logger
///
/// ### Returns
/// - `Ok(())`: If the logger was initialized successfully
/// - `Err(anyhow::Error)`: If the logger could not be initialized
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
