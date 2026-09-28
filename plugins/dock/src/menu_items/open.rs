use super::DockMenuItem;

pub struct DockOpen;

impl DockMenuItem for DockOpen {
    fn title(app: &tauri::AppHandle<tauri::Wry>) -> String {
        tauri_plugin_tray::tr_fmt(
            tauri_plugin_tray::Text::OpenApp,
            tauri_plugin_tray::current_menu_lang(),
            &[app.package_info().name.as_str()],
        )
    }

    fn handle(app: &tauri::AppHandle<tauri::Wry>) {
        tauri_plugin_tray::AnlgMenuItem::TrayOpen.handle(app);
    }
}
