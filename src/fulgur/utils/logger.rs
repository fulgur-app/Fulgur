use crate::fulgur::utils::paths;
use anyhow::anyhow;
use flexi_logger::{Age, Cleanup, Criterion, FileSpec, Logger, LoggerHandle, Naming};
use log::LevelFilter;
use std::path::PathBuf;
use std::sync::OnceLock;

static LOGGER_HANDLE: OnceLock<LoggerHandle> = OnceLock::new();

/// Convert `ThreadId(7)` debug output to plain `7`.
///
/// ### Returns
/// - `String`: The current thread id without the `ThreadId(...)` wrapper
fn current_thread_id() -> String {
    let thread_id = format!("{:?}", std::thread::current().id());
    thread_id
        .strip_prefix("ThreadId(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(&thread_id)
        .to_string()
}

/// File log formatter keeping the legacy style:
/// `2026-04-06T15:53:54.698025+02:00 [DEBUG] (2) target: message`
///
/// ### Arguments
/// - `w`: The output writer used by `flexi_logger`
/// - `now`: Deferred timestamp provider for the current record
/// - `record`: The `log` record to format
///
/// ### Returns
/// - `Ok(())`: If the log line was written successfully
/// - `Err(std::io::Error)`: If writing to the output fails
fn file_log_format(
    w: &mut dyn std::io::Write,
    now: &mut flexi_logger::DeferredNow,
    record: &log::Record,
) -> Result<(), std::io::Error> {
    let timestamp = now.format_rfc3339();
    let thread_id = current_thread_id();
    write!(
        w,
        "{} [{}] ({}) {}: {}",
        timestamp,
        record.level(),
        thread_id,
        record.target(),
        record.args()
    )
}

/// Get the directory where log files are stored.
///
/// ### Returns
/// - `Ok(PathBuf)`: The path to the log directory
/// - `Err(anyhow::Error)`: If the log file path could not be determined or created
fn log_directory() -> anyhow::Result<PathBuf> {
    paths::config_dir()
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
    let log_dir = log_directory()?;
    let logger_handle = Logger::with(LevelFilter::Debug)
        .log_to_file(
            FileSpec::default()
                .directory(log_dir.clone())
                .basename("Fulgur")
                .suffix("log")
                .suppress_timestamp(),
        )
        .format_for_files(file_log_format)
        .append()
        .rotate(
            Criterion::Age(Age::Day),
            Naming::Timestamps,
            Cleanup::KeepLogFiles(10),
        )
        .start()?;
    LOGGER_HANDLE
        .set(logger_handle)
        .map_err(|_| anyhow!("logger already initialized"))?;
    #[cfg(not(debug_assertions))]
    log::set_max_level(LevelFilter::Info);
    log::info!("Logger initialized in: {:?}", log_dir);
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
    use crate::fulgur::utils::logger::{current_thread_id, file_log_format, set_debug_mode};
    use flexi_logger::DeferredNow;
    use log::{Level, LevelFilter, Record};
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

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

    #[test]
    fn current_thread_id_is_compact() {
        let id = current_thread_id();
        assert!(!id.is_empty(), "Thread id should not be empty");
        assert!(
            !id.starts_with("ThreadId("),
            "Thread id helper should strip debug wrapper syntax"
        );
        assert!(
            !id.ends_with(')'),
            "Thread id helper should not include trailing parenthesis"
        );
    }

    #[test]
    fn file_log_format_matches_expected_shape_without_extra_newline() {
        let mut now = DeferredNow::new();
        let mut output = Vec::new();
        let args = format_args!("hello logger");
        let record = Record::builder()
            .args(args)
            .level(Level::Debug)
            .target("test::target")
            .build();

        file_log_format(&mut output, &mut now, &record).expect("Formatting should succeed");

        let line = String::from_utf8(output).expect("Formatted log line should be valid UTF-8");
        assert!(
            !line.ends_with('\n'),
            "Formatter should not append a newline"
        );
        assert!(
            !line.contains("\n\n"),
            "Formatter must not emit empty lines"
        );
        assert!(
            line.contains(" [DEBUG] ("),
            "Line should include level and thread id section"
        );
        assert!(
            line.contains(") test::target: hello logger"),
            "Line should include target and message"
        );

        let timestamp = line
            .split(" [DEBUG] ")
            .next()
            .expect("Line should start with a timestamp");
        assert!(
            OffsetDateTime::parse(timestamp, &Rfc3339).is_ok(),
            "Timestamp should be RFC3339"
        );
    }
}
