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
/// The logger is always initialized with `Debug` level capacity so that the effective
/// level can be changed at runtime via `set_debug_mode`. In release builds the
/// effective level starts at `Info`; debug builds always use `Debug`.
///
/// ### Returns
/// - `Ok(())`: If the logger was initialized successfully
/// - `Err(anyhow::Error)`: If the logger could not be initialized
pub fn init() -> anyhow::Result<()> {
    let log_path = log_file_path()?;
    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .set_time_offset_to_local()
        .unwrap_or_else(|builder| builder)
        .build();
    let log_file = File::create(&log_path)?;
    WriteLogger::init(LevelFilter::Debug, config, log_file)?;
    #[cfg(not(debug_assertions))]
    log::set_max_level(LevelFilter::Info);
    log::info!("Logger initialized at: {:?}", log_path);
    Ok(())
}

/// Update the effective log level based on the `debug_mode` setting.
///
/// In debug builds this is a no-op — the level is always `Debug`.
/// In release builds, `debug_mode = true` enables `Debug`-level output;
/// `false` restores `Info`.
///
/// ### Arguments
/// - `debug_mode`: Whether debug-level logging should be active
pub fn set_debug_mode(debug_mode: bool) {
    let level = if debug_mode || cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    log::set_max_level(level);
}

#[cfg(test)]
mod tests {
    use super::*;

    // All assertions live in a single test function to avoid races on the
    // process-wide `log::max_level()` global when tests run in parallel.
    #[test]
    fn set_debug_mode_updates_max_level() {
        // true always yields Debug, in both debug and release builds.
        set_debug_mode(true);
        assert_eq!(log::max_level(), LevelFilter::Debug);

        // false: release → Info, debug build → still Debug (cfg! guards inside
        // set_debug_mode keep it at Debug so dev builds are never silenced).
        set_debug_mode(false);
        #[cfg(debug_assertions)]
        assert_eq!(log::max_level(), LevelFilter::Debug);
        #[cfg(not(debug_assertions))]
        assert_eq!(log::max_level(), LevelFilter::Info);

        // Toggling back to true always restores Debug.
        set_debug_mode(true);
        assert_eq!(log::max_level(), LevelFilter::Debug);
    }
}
