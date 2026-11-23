use crate::fulgur::utils::paths;
use anyhow::anyhow;
use flexi_logger::{
    Age, Cleanup, Criterion, FileSpec, LogfileSelector, Logger, LoggerHandle, Naming,
};
use log::LevelFilter;
use std::backtrace::Backtrace;
use std::fmt::Display;
use std::panic::{Location, PanicHookInfo};
use std::path::PathBuf;
use std::sync::OnceLock;

static LOGGER_HANDLE: OnceLock<LoggerHandle> = OnceLock::new();

thread_local! {
    /// Compact thread id, computed once per thread instead of once per log record.
    static THREAD_ID: String = compute_thread_id();
}

/// Convert `ThreadId(7)` debug output to plain `7`.
///
/// ### Returns
/// - `String`: The current thread id without the `ThreadId(...)` wrapper
fn compute_thread_id() -> String {
    let thread_id = format!("{:?}", std::thread::current().id());
    thread_id
        .strip_prefix("ThreadId(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(&thread_id)
        .to_string()
}

/// Run a closure with the cached compact thread id of the current thread.
///
/// ### Description
/// The id is stored in thread-local storage so the logging hot path performs no
/// allocation after the first record emitted by a given thread. When the
/// thread-local has already been destroyed (logging during thread teardown), the
/// placeholder `?` is used instead.
///
/// ### Arguments
/// - `f`: Receives the compact thread id as a string slice
///
/// ### Returns
/// - `R`: Whatever the closure returns
fn with_current_thread_id<R>(f: impl FnOnce(&str) -> R) -> R {
    let mut f = Some(f);
    match THREAD_ID.try_with(|id| f.take().expect("closure is called at most once")(id)) {
        Ok(result) => result,
        Err(_) => f
            .take()
            .expect("closure is unused when the thread local is gone")("?"),
    }
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
    with_current_thread_id(|thread_id| {
        write!(
            w,
            "{} [{}] ({}) {}: {}",
            timestamp,
            record.level(),
            thread_id,
            record.target(),
            record.args()
        )
    })
}

/// Get the directory where log files are stored.
///
/// ### Returns
/// - `Ok(PathBuf)`: The path to the log directory
/// - `Err(anyhow::Error)`: If the log file path could not be determined or created
fn log_directory() -> anyhow::Result<PathBuf> {
    paths::config_dir()
}

/// Configure the daily-rotated file logger, without starting it.
///
/// ### Arguments
/// - `log_dir`: The directory the log files are written to
///
/// ### Returns
/// - `Logger`: The logger configuration shared by `init` and the tests
fn file_logger(log_dir: PathBuf) -> Logger {
    Logger::with(LevelFilter::Debug)
        .log_to_file(
            FileSpec::default()
                .directory(log_dir)
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
}

/// Initialize the file logger
///
/// The logger is always initialized with `Debug` level capacity so that the effective
/// level can be changed at runtime via `set_debug_mode`. In release builds the
/// effective level starts at `Info`; debug builds always use `Debug`.
///
/// ### Errors
/// Returns an error if the log directory cannot be resolved or created, or if
/// the underlying `flexi_logger` initialization fails.
///
/// ### Returns
/// - `Ok(())`: If the logger was initialized successfully
/// - `Err(anyhow::Error)`: If the logger could not be initialized
pub fn init() -> anyhow::Result<()> {
    let log_dir = log_directory()?;
    let logger_handle = file_logger(log_dir.clone()).start()?;
    LOGGER_HANDLE
        .set(logger_handle)
        .map_err(|_| anyhow!("logger already initialized"))?;
    #[cfg(not(debug_assertions))]
    log::set_max_level(LevelFilter::Info);
    log::info!("Logger initialized in: {}", log_dir.display());
    Ok(())
}

/// Install a process-wide panic hook that writes every panic to the log file.
///
pub fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log_panic(info);
        previous_hook(info);
    }));
}

/// Log a panic at error level and flush the logger.
///
/// ### Arguments
/// - `info`: The panic information handed to the panic hook
fn log_panic(info: &PanicHookInfo<'_>) {
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("<unnamed>");
    let report = panic_report(
        thread_name,
        info.location(),
        info.payload_as_str(),
        &Backtrace::force_capture(),
    );
    log::error!("{report}");
    if let Some(handle) = LOGGER_HANDLE.get() {
        handle.flush();
    }
}

/// Build the log message describing a panic.
///
/// ### Arguments
/// - `thread_name`: The name of the panicking thread
/// - `location`: Where the panic originated, when known
/// - `message`: The panic payload, when it is a string
/// - `backtrace`: The backtrace captured in the panic hook
///
/// ### Returns
/// - `String`: A multi-line report: thread, location and message, then the backtrace
fn panic_report(
    thread_name: &str,
    location: Option<&Location<'_>>,
    message: Option<&str>,
    backtrace: &impl Display,
) -> String {
    let location = location.map_or_else(|| "<unknown location>".to_string(), ToString::to_string);
    let message = message.unwrap_or("<non-string panic payload>");
    format!("Thread '{thread_name}' panicked at {location}: {message}\nBacktrace:\n{backtrace}")
}

/// Get the path of the log file the logger is currently writing to.
///
/// ### Returns
/// - `Some(PathBuf)`: The current log file
/// - `None`: The logger is not initialized, or the current file could not be listed
pub fn current_log_file() -> Option<PathBuf> {
    current_log_file_of(LOGGER_HANDLE.get()?)
}

/// Get the path of the log file a given logger is currently writing to.
///
/// ### Arguments
/// - `handle`: The handle of a logger configured by `file_logger`
///
/// ### Returns
/// - `Some(PathBuf)`: The current log file
/// - `None`: The current file does not exist yet, or could not be listed
fn current_log_file_of(handle: &LoggerHandle) -> Option<PathBuf> {
    match handle.existing_log_files(&LogfileSelector::none().with_r_current()) {
        Ok(files) => files.into_iter().next(),
        Err(e) => {
            log::warn!("Failed to list the current log file: {e}");
            None
        }
    }
}

/// Update the effective log level based on the `debug_mode` setting.
///
/// In debug builds this is a no-op: the level is always `Debug`.
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
    use crate::fulgur::utils::logger::{
        current_log_file, current_log_file_of, file_log_format, file_logger, panic_report,
        set_debug_mode, with_current_thread_id,
    };
    use flexi_logger::DeferredNow;
    use log::{Level, LevelFilter, Record};
    use std::panic::Location;
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
        with_current_thread_id(|id| {
            assert!(!id.is_empty(), "Thread id should not be empty");
            assert!(
                !id.starts_with("ThreadId("),
                "Thread id helper should strip debug wrapper syntax"
            );
            assert!(
                !id.ends_with(')'),
                "Thread id helper should not include trailing parenthesis"
            );
        });
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

    // Built from the same configuration as `init`, so changing the rotation
    // naming in a way the `rCURRENT` lookup no longer matches fails here.
    #[test]
    fn current_log_file_is_the_file_the_logger_writes_to() {
        let log_dir = tempfile::tempdir().expect("Temp dir should be created");
        let (logger, handle) = file_logger(log_dir.path().to_path_buf())
            .build()
            .expect("Logger should build without being installed globally");
        let args = format_args!("view log marker");
        logger.log(
            &Record::builder()
                .args(args)
                .level(Level::Info)
                .target("test::target")
                .build(),
        );
        logger.flush();

        let current = current_log_file_of(&handle).expect("Current log file should be found");
        let content = std::fs::read_to_string(&current).expect("Current log file should be read");
        assert!(
            content.contains("view log marker"),
            "{} is not the file the logger writes to",
            current.display()
        );
    }

    // Only `main` initializes the global logger, so it stays unset in the test binary.
    #[test]
    fn current_log_file_is_none_while_the_logger_is_not_initialized() {
        assert!(current_log_file().is_none());
    }

    #[test]
    fn panic_report_includes_thread_location_message_and_backtrace() {
        let location = Location::caller();
        let report = panic_report(
            "state-writer",
            Some(location),
            Some("index out of bounds"),
            &"frame 0: fulgur::main",
        );
        assert_eq!(
            report,
            format!(
                "Thread 'state-writer' panicked at {location}: index out of bounds\nBacktrace:\nframe 0: fulgur::main"
            )
        );
    }

    #[test]
    fn panic_report_uses_placeholders_for_missing_location_and_payload() {
        let report = panic_report("main", None, None, &"");
        assert!(
            report.starts_with(
                "Thread 'main' panicked at <unknown location>: <non-string panic payload>\n"
            ),
            "Unexpected report: {report}"
        );
    }
}
