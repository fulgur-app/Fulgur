use crate::fulgur::{Fulgur, settings::EditorSettings};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem};
use gpui_kit::{App, Entity};

/// Create the File Monitoring group of the Editor settings page
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `default_editor_settings`: The default editor settings, used for reset values
///
/// ### Returns
/// - `SettingGroup`: The File Monitoring settings group
pub(super) fn file_monitoring_group(
    entity: &Entity<Fulgur>,
    default_editor_settings: &EditorSettings,
) -> SettingGroup {
    SettingGroup::new().title("File Monitoring").items(vec![
        SettingItem::new(
            "Watch Files",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| entity.read(cx).settings.editor_settings.watch_files
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.watch_files = val;
                            if val {
                                this.start_file_watcher(cx);
                            } else {
                                this.stop_file_watcher();
                            }
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.watch_files),
        )
        .description("Monitor files for external changes."),
    ])
}
