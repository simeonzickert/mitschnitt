use super::DockMenuItem;

pub struct DockRestart;

impl DockMenuItem for DockRestart {
    fn title(app: &tauri::AppHandle<tauri::Wry>) -> String {
        tauri_plugin_tray::tr_fmt(
            tauri_plugin_tray::Text::RestartApp,
            tauri_plugin_tray::current_menu_lang(),
            &[app.package_info().name.as_str()],
        )
    }

    fn handle(app: &tauri::AppHandle<tauri::Wry>) {
        app.restart();
    }
}
