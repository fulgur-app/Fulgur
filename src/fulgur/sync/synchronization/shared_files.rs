use crate::fulgur::Fulgur;
use crate::fulgur::settings::ServerProfile;
use crate::fulgur::sync::share;
use crate::fulgur::ui::tabs::editor_tab;
use crate::fulgur::ui::tabs::tab::Tab;
use gpui::{Context, Window};
use std::sync::Arc;

impl Fulgur {
    /// Process shared files received from every active sync profile.
    ///
    /// ### Arguments
    /// - `window`: The window to create new tabs in
    /// - `cx`: The application context
    pub fn process_shared_files_from_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_profiles: Vec<ServerProfile> = self
            .settings
            .app_settings
            .synchronization_settings
            .profiles
            .iter()
            .filter(|p| p.is_active)
            .cloned()
            .collect();
        let http_agent = Arc::clone(&Fulgur::shared_state(cx).http_agent);
        for profile in &active_profiles {
            let sync_state = Fulgur::shared_state(cx).sync_state_for(&profile.id);

            // Decryption and decompression are CPU-bound; kick them off on a
            // background worker so a batch of incoming shares never freezes the
            // UI thread. The worker pushes plaintext into `pending_decrypted_files`
            // and acknowledges v2 shares once they decrypt successfully.
            share::start_decryption_if_idle(profile, &sync_state, &http_agent);

            // Open everything that has finished decrypting. This is the only part
            // that needs `window`/`cx`, and it is cheap (no crypto).
            let decrypted_files = {
                let mut pending = sync_state.pending_decrypted_files.lock();
                if pending.is_empty() {
                    continue;
                }
                std::mem::take(&mut *pending)
            };
            for decrypted in decrypted_files {
                let tab_id = self.next_tab_id;
                self.next_tab_id += 1;
                let new_tab = Tab::Editor(editor_tab::EditorTab::from_content(
                    tab_id,
                    &decrypted.content,
                    decrypted.file_name.clone(),
                    window,
                    cx,
                    &self.settings.editor_settings,
                ));
                self.tabs.push(new_tab);
                self.active_tab_index = Some(self.tabs.len() - 1);
                self.pending_tab_scroll = Some(self.tabs.len() - 1);
                log::info!("Opened shared file: {}", decrypted.file_name);
            }
        }
    }
}
