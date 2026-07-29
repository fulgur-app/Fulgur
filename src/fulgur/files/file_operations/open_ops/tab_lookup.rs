use crate::fulgur::{Fulgur, editor_tab::TabLocation, sync::ssh::url::RemoteSpec, tab::Tab};
use std::path::PathBuf;

impl Fulgur {
    /// Find the index of a tab with the given file path
    ///
    /// ### Arguments
    /// - `path`: The path to search for
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(usize)`: The index of the tab if found
    /// - `None`: If the tab was not found
    #[must_use]
    pub fn find_tab_by_path(&self, path: &PathBuf, cx: &gpui::App) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            if let Tab::Editor(editor_tab) = tab.read(cx) {
                editor_tab.file_path().is_some_and(|p| p == path)
            } else {
                false
            }
        })
    }

    /// Find the index of an editor tab opened from the same remote location.
    ///
    /// ### Arguments
    /// - `spec`: Remote location to search for.
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(usize)`: The index of the matching remote tab.
    /// - `None`: If no tab matches this remote location.
    #[must_use]
    pub fn find_tab_by_remote_spec(&self, spec: &RemoteSpec, cx: &gpui::App) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            if let Tab::Editor(editor_tab) = tab.read(cx)
                && let TabLocation::Remote(existing_spec) = &editor_tab.location
            {
                existing_spec.host == spec.host
                    && existing_spec.port == spec.port
                    && existing_spec.user == spec.user
                    && existing_spec.path == spec.path
            } else {
                false
            }
        })
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{editor_tab::TabLocation, sync::ssh::url::RemoteSpec, tab::Tab};
    #[cfg(feature = "gpui-test-support")]
    use std::path::PathBuf;

    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::files::file_operations::test_helpers::{setup_fulgur, temp_test_path};
    #[cfg(feature = "gpui-test-support")]
    use gpui::TestAppContext;

    // ========== find_tab_by_path tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_path_returns_index_for_existing_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_find_tab_test.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                        }
                    });
                let expected_index = this.tabs.len() - 1;
                let result = this.find_tab_by_path(&path, cx);
                assert_eq!(result, Some(expected_index));
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_path_returns_none_for_unknown_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let result = this.find_tab_by_path(&PathBuf::from("/nonexistent/path.txt"), cx);
                assert_eq!(result, None);
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_path_ignores_settings_tabs(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.open_settings(window, cx);
                // Remove all editor tabs so only settings tabs remain
                this.tabs.retain(|t| matches!(t.read(cx), Tab::Settings(_)));
                let result = this.find_tab_by_path(&PathBuf::from("/any/path.txt"), cx);
                assert_eq!(result, None);
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_remote_spec_returns_index_for_existing_remote_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let spec = RemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: Some("alice".to_string()),
            path: "/var/log/syslog".to_string(),
            password_in_url: None,
        };

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Remote(spec.clone());
                        }
                    });
                let expected_index = this.tabs.len() - 1;
                let result = this.find_tab_by_remote_spec(&spec, cx);
                assert_eq!(result, Some(expected_index));
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_remote_spec_returns_none_for_unknown_remote_spec(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let spec = RemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: Some("alice".to_string()),
            path: "/var/log/syslog".to_string(),
            password_in_url: None,
        };

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let result = this.find_tab_by_remote_spec(&spec, cx);
                assert_eq!(result, None);
            });
        });
    }
}
