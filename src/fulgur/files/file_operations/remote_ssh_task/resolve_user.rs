use crate::fulgur::{
    Fulgur,
    sync::ssh::{auth::SshAuth, credentials::SshCredKey, session::SshHostConfig, url::RemoteSpec},
};

impl Fulgur {
    /// Prepare the credentials of a remote request, then continue once the user is known.
    ///
    /// A password embedded in the URL moves into the session credential cache. When the
    /// URL has no user, the `User` from `~/.ssh/config` applies, and the username dialog is
    /// shown only when none is configured.
    ///
    /// ### Arguments
    /// - `window`: The window used to show the username dialog
    /// - `cx`: The application context
    /// - `spec`: Remote location, possibly without user and with a URL password
    /// - `on_resolved`: Continuation receiving the spec with its user set and no password
    pub(in crate::fulgur::files::file_operations) fn with_resolved_remote_user(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
        mut spec: RemoteSpec,
        on_resolved: impl Fn(
            &mut Fulgur,
            RemoteSpec,
            &mut gpui_kit::Window,
            &mut gpui_kit::Context<Fulgur>,
        ) + 'static,
    ) {
        if spec.user.is_none() {
            spec.user = SshHostConfig::load(&spec.host).user;
        }
        let url_password = spec.password_in_url.take();
        if let (Some(user), Some(password)) = (spec.user.clone(), url_password) {
            let key = SshCredKey::new(spec.host.clone(), spec.port, user);
            Fulgur::shared_state(cx)
                .ssh_session_cache
                .lock()
                .insert(key, SshAuth::Password(password));
        }
        if spec.user.is_some() {
            on_resolved(self, spec, window, cx);
            return;
        }

        let entity = cx.entity().downgrade();
        let host = spec.host.clone();
        let port = spec.port;
        self.show_ssh_username_dialog(window, cx, &host, port, move |user, window, cx| {
            let mut spec_with_user = spec.clone();
            spec_with_user.user = Some(user);
            if let Some(entity) = entity.upgrade() {
                entity.update(cx, |fulgur, cx| {
                    on_resolved(fulgur, spec_with_user, window, cx);
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_with_resolved_remote_user_moves_url_password_to_cache(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use crate::fulgur::{
            Fulgur,
            files::file_operations::test_helpers::setup_fulgur,
            sync::ssh::{
                auth::SshAuth,
                credentials::SshCredKey,
                url::{RemoteSpec, parse_remote_url},
            },
        };
        use std::{cell::RefCell, rc::Rc};

        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let spec = parse_remote_url("ssh://alice:secret@example.com:2222/tmp/notes.txt")
            .expect("valid remote URL");
        assert!(spec.password_in_url.is_some());
        let resolved: Rc<RefCell<Option<RemoteSpec>>> = Rc::new(RefCell::new(None));
        let resolved_in_callback = Rc::clone(&resolved);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.with_resolved_remote_user(window, cx, spec, move |_, spec, _, _| {
                    *resolved_in_callback.borrow_mut() = Some(spec);
                });
            });
        });

        let resolved = resolved
            .take()
            .expect("a URL with a user resolves without the username dialog");
        assert_eq!(resolved.user.as_deref(), Some("alice"));
        assert!(
            resolved.password_in_url.is_none(),
            "the password must not travel further in the spec"
        );
        visual_cx.update(|_, cx| {
            let cache = Fulgur::shared_state(cx).ssh_session_cache.lock();
            let cached = cache.get(&SshCredKey::new("example.com", 2222, "alice"));
            assert!(
                matches!(cached, Some(SshAuth::Password(password)) if password.as_str() == "secret")
            );
        });
    }
}
