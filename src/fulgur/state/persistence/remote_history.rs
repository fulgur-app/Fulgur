/// Most URLs the remote history keeps.
pub const MAX_REMOTE_HISTORY_ENTRIES: usize = 10;

/// URLs recently opened through the Open Remote File dialog, most recent first.
///
/// Entries are distinct and capped at `MAX_REMOTE_HISTORY_ENTRIES`. They are
/// expected in canonical form without a password, as produced by
/// `format_remote_history_url`, so equivalent spellings collapse into one entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteHistory {
    urls: Vec<String>,
}

impl RemoteHistory {
    /// Build a history from URLs ordered most recent first.
    ///
    /// ### Arguments
    /// - `urls`: The URLs, most recent first
    ///
    /// ### Returns
    /// - `RemoteHistory`: The first occurrence of each URL, capped at
    ///   `MAX_REMOTE_HISTORY_ENTRIES` entries
    #[must_use]
    pub fn from_urls(urls: impl IntoIterator<Item = String>) -> Self {
        let mut history = Self::default();
        for url in urls {
            if history.urls.len() == MAX_REMOTE_HISTORY_ENTRIES {
                break;
            }
            if !history.urls.contains(&url) {
                history.urls.push(url);
            }
        }
        history
    }

    /// Move a URL to the top of the history, dropping the oldest entry when full.
    ///
    /// ### Arguments
    /// - `url`: The URL that was just used
    ///
    /// ### Returns
    /// - `bool`: `true` when the history changed, `false` when the URL was
    ///   already the most recent entry
    pub fn record(&mut self, url: String) -> bool {
        if self.urls.first() == Some(&url) {
            return false;
        }
        self.urls.retain(|existing| *existing != url);
        self.urls.insert(0, url);
        self.urls.truncate(MAX_REMOTE_HISTORY_ENTRIES);
        true
    }

    /// Forget one URL.
    ///
    /// ### Arguments
    /// - `url`: The URL to forget
    ///
    /// ### Returns
    /// - `bool`: `true` when the URL was remembered, `false` when it was not
    pub fn remove(&mut self, url: &str) -> bool {
        let remembered = self.urls.len();
        self.urls.retain(|existing| existing != url);
        self.urls.len() != remembered
    }

    /// Forget every URL.
    ///
    /// ### Returns
    /// - `bool`: `true` when the history changed, `false` when it was already empty
    pub fn clear(&mut self) -> bool {
        if self.urls.is_empty() {
            return false;
        }
        self.urls.clear();
        true
    }

    /// The remembered URLs.
    ///
    /// ### Returns
    /// - `&[String]`: The URLs, most recent first
    #[must_use]
    pub fn urls(&self) -> &[String] {
        &self.urls
    }

    /// Whether no URL is remembered.
    ///
    /// ### Returns
    /// - `bool`: `true` when the history is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.urls.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_REMOTE_HISTORY_ENTRIES, RemoteHistory};

    fn url(index: usize) -> String {
        format!("ssh://host/file-{index}")
    }

    #[test]
    fn record_puts_the_newest_url_first() {
        let mut history = RemoteHistory::default();
        assert!(history.record(url(1)));
        assert!(history.record(url(2)));
        assert_eq!(history.urls(), [url(2), url(1)]);
    }

    #[test]
    fn recording_a_known_url_moves_it_to_the_top_without_duplicating_it() {
        let mut history = RemoteHistory::from_urls([url(1), url(2), url(3)]);
        assert!(history.record(url(3)));
        assert_eq!(history.urls(), [url(3), url(1), url(2)]);
    }

    #[test]
    fn recording_the_most_recent_url_again_changes_nothing() {
        let mut history = RemoteHistory::from_urls([url(1), url(2)]);
        assert!(!history.record(url(1)));
        assert_eq!(history.urls(), [url(1), url(2)]);
    }

    #[test]
    fn host_and_file_urls_of_the_same_server_are_distinct_entries() {
        let mut history = RemoteHistory::default();
        history.record("ssh://10.0.0.1/".to_string());
        history.record("ssh://10.0.0.1/path/to/file".to_string());
        assert_eq!(history.urls().len(), 2);
    }

    #[test]
    fn record_drops_the_oldest_url_when_full() {
        let mut history = RemoteHistory::from_urls((0..MAX_REMOTE_HISTORY_ENTRIES).map(url));
        assert!(history.record(url(99)));
        assert_eq!(history.urls().len(), MAX_REMOTE_HISTORY_ENTRIES);
        assert_eq!(history.urls()[0], url(99));
        assert!(
            !history
                .urls()
                .contains(&url(MAX_REMOTE_HISTORY_ENTRIES - 1)),
            "the least recently used URL must be dropped"
        );
    }

    #[test]
    fn from_urls_keeps_first_occurrences_and_caps_the_length() {
        let urls = [url(1), url(1)]
            .into_iter()
            .chain((2..MAX_REMOTE_HISTORY_ENTRIES + 5).map(url));
        let history = RemoteHistory::from_urls(urls);
        assert_eq!(history.urls().len(), MAX_REMOTE_HISTORY_ENTRIES);
        assert_eq!(history.urls()[0], url(1));
        assert_eq!(history.urls()[1], url(2));
    }

    #[test]
    fn remove_forgets_only_the_given_url_and_keeps_the_order() {
        let mut history = RemoteHistory::from_urls([url(1), url(2), url(3)]);
        assert!(history.remove(&url(2)));
        assert_eq!(history.urls(), [url(1), url(3)]);
        assert!(
            !history.remove(&url(2)),
            "a forgotten URL is no longer there"
        );
    }

    #[test]
    fn clear_reports_whether_anything_was_forgotten() {
        let mut history = RemoteHistory::from_urls([url(1)]);
        assert!(history.clear());
        assert!(history.is_empty());
        assert!(!history.clear());
    }
}
