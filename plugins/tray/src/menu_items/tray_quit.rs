use tauri::{
    AppHandle, Result,
    menu::{MenuItem, MenuItemKind},
};

use crate::{Text, current_menu_lang, tr};

use super::MenuItemHandler;

pub struct TrayQuit;

impl MenuItemHandler for TrayQuit {
    const ID: &'static str = "anlg_tray_quit";

    fn build(app: &AppHandle<tauri::Wry>) -> Result<MenuItemKind<tauri::Wry>> {
        let item = MenuItem::with_id(
            app,
            Self::ID,
            tr(Text::Quit, current_menu_lang()),
            true,
            Some("cmd+q"),
        )?;
        Ok(MenuItemKind::MenuItem(item))
    }

    fn handle(app: &AppHandle<tauri::Wry>) {
        app.exit(0);
    }
}
