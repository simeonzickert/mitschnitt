use super::DockMenuItem;

pub struct DockNewNote;

impl DockMenuItem for DockNewNote {
    fn title(_app: &tauri::AppHandle<tauri::Wry>) -> String {
        tauri_plugin_tray::tr(
            tauri_plugin_tray::Text::NewNote,
            tauri_plugin_tray::current_menu_lang(),
        )
        .to_string()
    }

    fn handle(app: &tauri::AppHandle<tauri::Wry>) {
        tauri_plugin_tray::AnlgMenuItem::AppNew.handle(app);
    }
}
