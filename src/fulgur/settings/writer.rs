use super::Settings;
use gpui_kit::BackgroundExecutor;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

/// Serializes every write of the settings file and coalesces background writes.
#[derive(Default)]
pub struct SettingsWriter {
    inner: Arc<SettingsWriterInner>,
}

/// State shared between the UI thread and the background writes.
#[derive(Default)]
struct SettingsWriterInner {
    /// Explicit target file, `None` for the default settings location
    path: Option<PathBuf>,
    /// Newest snapshot waiting for a background write
    pending: Mutex<Option<Settings>>,
    /// Held for the whole duration of a write, so two writes never interleave
    write_lock: Mutex<()>,
}

impl SettingsWriterInner {
    /// Write a snapshot to the target file.
    ///
    /// ### Arguments
    /// - `settings`: The snapshot to persist
    ///
    /// ### Errors
    /// - Returns an error if the target path cannot be resolved or the write fails.
    ///
    /// ### Returns
    /// - `Ok(())`: The file holds the snapshot
    /// - `Err(anyhow::Error)`: The snapshot could not be written
    fn write(&self, settings: &Settings) -> anyhow::Result<()> {
        match &self.path {
            Some(path) => settings.save_to_path(path),
            None => settings.save(),
        }
    }

    /// Write the pending snapshot, if any, and log a failure.
    fn flush_pending(&self) {
        let _write_guard = self.write_lock.lock();
        let Some(settings) = self.pending.lock().take() else {
            return;
        };
        if let Err(e) = self.write(&settings) {
            log::error!("Failed to save settings in the background: {e}");
        }
    }
}

impl SettingsWriter {
    /// Create a writer targeting an explicit settings file.
    ///
    /// ### Arguments
    /// - `path`: The settings file to write
    ///
    /// ### Returns
    /// - `Self`: A writer with no pending snapshot
    #[cfg(all(test, feature = "gpui-test-support"))]
    fn with_path(path: PathBuf) -> Self {
        Self {
            inner: Arc::new(SettingsWriterInner {
                path: Some(path),
                ..SettingsWriterInner::default()
            }),
        }
    }

    /// Write a snapshot and block the caller until the file is written.
    ///
    /// ### Arguments
    /// - `settings`: The snapshot to persist
    ///
    /// ### Errors
    /// - Returns an error if the target path cannot be resolved or the write fails.
    ///
    /// ### Returns
    /// - `Ok(())`: The file holds the snapshot
    /// - `Err(anyhow::Error)`: The snapshot could not be written
    pub fn save_blocking(&self, settings: &Settings) -> anyhow::Result<()> {
        let _write_guard = self.inner.write_lock.lock();
        self.inner.pending.lock().take();
        self.inner.write(settings)
    }

    /// Hand a snapshot to a background write without waiting for it.
    ///
    /// ### Arguments
    /// - `settings`: The snapshot to persist, replacing any snapshot still pending
    /// - `executor`: The executor running the write
    pub fn save_in_background(&self, settings: Settings, executor: &BackgroundExecutor) {
        *self.inner.pending.lock() = Some(settings);
        let inner = Arc::clone(&self.inner);
        executor
            .spawn(async move { inner.flush_pending() })
            .detach();
    }

    /// Write the pending snapshot now, blocking the caller, so it survives the app quitting.
    pub fn flush(&self) {
        self.inner.flush_pending();
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::SettingsWriter;
    use crate::fulgur::settings::Settings;
    use gpui_kit::TestAppContext;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    /// Build settings whose recent files list holds a single file.
    ///
    /// ### Arguments
    /// - `file`: The recent file
    ///
    /// ### Returns
    /// - `Settings`: Default settings carrying `file` as the only recent file
    fn settings_with_recent_file(file: &str) -> Settings {
        let mut settings = Settings::new();
        settings.add_recent_file(PathBuf::from(file));
        settings
    }

    /// Read the recent files persisted at a path.
    ///
    /// ### Arguments
    /// - `path`: The settings file
    ///
    /// ### Returns
    /// - `Vec<PathBuf>`: The persisted recent files, most recent first
    fn persisted_recent_files(path: &Path) -> Vec<PathBuf> {
        Settings::load_from_path(path)
            .expect("settings file should load")
            .get_recent_files()
    }

    #[gpui_kit::test]
    fn background_save_coalesces_to_the_newest_snapshot(cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("settings.json");
        let writer = SettingsWriter::with_path(path.clone());
        let executor = cx.executor();
        writer.save_in_background(settings_with_recent_file("/a"), &executor);
        writer.save_in_background(settings_with_recent_file("/b"), &executor);
        cx.run_until_parked();
        assert_eq!(persisted_recent_files(&path), [PathBuf::from("/b")]);
    }

    #[gpui_kit::test]
    fn blocking_save_supersedes_a_pending_background_snapshot(cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("settings.json");
        let writer = SettingsWriter::with_path(path.clone());
        writer.save_in_background(settings_with_recent_file("/older"), &cx.executor());
        writer
            .save_blocking(&settings_with_recent_file("/newer"))
            .expect("blocking save should succeed");
        cx.run_until_parked();
        assert_eq!(
            persisted_recent_files(&path),
            [PathBuf::from("/newer")],
            "the older background snapshot must not overwrite the blocking save"
        );
    }

    #[gpui_kit::test]
    fn flush_writes_the_pending_snapshot_immediately(cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("settings.json");
        let writer = SettingsWriter::with_path(path.clone());
        writer.save_in_background(settings_with_recent_file("/a"), &cx.executor());
        writer.flush();
        assert_eq!(persisted_recent_files(&path), [PathBuf::from("/a")]);
    }
}
