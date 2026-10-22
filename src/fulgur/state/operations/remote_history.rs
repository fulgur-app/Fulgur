use crate::fulgur::{Fulgur, shared_state::SharedAppState, state::RemoteHistory};
use gpui_kit::{App, BorrowAppContext};

impl Fulgur {
    /// Move a URL to the top of the remote history and persist the change.
    ///
    /// ### Arguments
    /// - `url`: Canonical URL just opened from the Open Remote File dialog
    /// - `cx`: The application context
    pub fn record_remote_history_url(url: String, cx: &mut App) {
        let mut history = Self::shared_state(cx).remote_history.clone();
        if history.record(url) {
            Self::store_remote_history(history, cx);
        }
    }

    /// Forget one remote history URL and persist the change.
    ///
    /// ### Arguments
    /// - `url`: The URL to forget
    /// - `cx`: The application context
    pub fn forget_remote_history_url(url: &str, cx: &mut App) {
        let mut history = Self::shared_state(cx).remote_history.clone();
        if history.remove(url) {
            Self::store_remote_history(history, cx);
        }
    }

    /// Forget every remote history URL and persist the change.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn clear_remote_history(cx: &mut App) {
        let mut history = Self::shared_state(cx).remote_history.clone();
        if history.clear() {
            Self::store_remote_history(history, cx);
        }
    }

    /// Publish a changed remote history to every window and hand it to the state writer.
    ///
    /// ### Arguments
    /// - `history`: The new history
    /// - `cx`: The application context
    fn store_remote_history(history: RemoteHistory, cx: &mut App) {
        cx.update_global::<SharedAppState, _>(|shared, _| {
            shared.state_writer.save_remote_history(history.clone());
            shared.remote_history = history;
        });
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{Fulgur, state::MAX_REMOTE_HISTORY_ENTRIES};
    use crate::test_support::setup_test_globals;
    use gpui_kit::TestAppContext;

    /// Read the in-memory remote history URLs.
    fn history_urls(cx: &mut TestAppContext) -> Vec<String> {
        cx.update(|cx| Fulgur::shared_state(cx).remote_history.urls().to_vec())
    }

    #[gpui_kit::test]
    fn recording_urls_keeps_the_most_recent_first(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        cx.update(|cx| {
            Fulgur::record_remote_history_url("ssh://a/".to_string(), cx);
            Fulgur::record_remote_history_url("ssh://b/".to_string(), cx);
            Fulgur::record_remote_history_url("ssh://a/".to_string(), cx);
        });
        assert_eq!(history_urls(cx), ["ssh://a/", "ssh://b/"]);
    }

    #[gpui_kit::test]
    fn the_history_keeps_at_most_the_configured_number_of_urls(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        cx.update(|cx| {
            for index in 0..=MAX_REMOTE_HISTORY_ENTRIES {
                Fulgur::record_remote_history_url(format!("ssh://host/{index}"), cx);
            }
        });
        let urls = history_urls(cx);
        assert_eq!(urls.len(), MAX_REMOTE_HISTORY_ENTRIES);
        assert_eq!(urls[0], format!("ssh://host/{MAX_REMOTE_HISTORY_ENTRIES}"));
    }

    #[gpui_kit::test]
    fn forgetting_a_url_keeps_the_others(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        cx.update(|cx| {
            Fulgur::record_remote_history_url("ssh://a/".to_string(), cx);
            Fulgur::record_remote_history_url("ssh://b/".to_string(), cx);
            Fulgur::forget_remote_history_url("ssh://a/", cx);
        });
        assert_eq!(history_urls(cx), ["ssh://b/"]);
    }

    #[gpui_kit::test]
    fn clearing_the_history_forgets_every_url(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        cx.update(|cx| {
            Fulgur::record_remote_history_url("ssh://a/".to_string(), cx);
            Fulgur::clear_remote_history(cx);
        });
        assert_eq!(history_urls(cx), Vec::<String>::new());
    }
}
