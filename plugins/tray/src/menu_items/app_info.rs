use tauri::{
    AppHandle, Result,
    menu::{MenuItem, MenuItemKind},
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_misc::MiscPluginExt;

use crate::{Text, current_menu_lang, tr, tr_fmt};

use super::MenuItemHandler;

pub struct AppInfo;

impl MenuItemHandler for AppInfo {
    const ID: &'static str = "anlg_app_info";

    fn build(app: &AppHandle<tauri::Wry>) -> Result<MenuItemKind<tauri::Wry>> {
        let title = tr_fmt(
            Text::AboutApp,
            current_menu_lang(),
            &[app.package_info().name.as_str()],
        );
        let item = MenuItem::with_id(app, Self::ID, title, true, None::<&str>)?;
        Ok(MenuItemKind::MenuItem(item))
    }

    fn handle(app: &AppHandle<tauri::Wry>) {
        let lang = current_menu_lang();
        let app_name = app.package_info().name.clone();
        let app_version = app.package_info().version.to_string();
        let app_commit = app.misc().get_git_hash();

        let message = tr_fmt(Text::AboutBody, lang, &[&app_name, &app_version, &app_commit]);

        let app_clone = app.clone();

        app.dialog()
            .message(&message)
            .title(tr_fmt(Text::AboutApp, lang, &[&app_name]))
            .buttons(MessageDialogButtons::OkCancelCustom(
                tr(Text::Copy, lang).to_string(),
                tr(Text::Cancel, lang).to_string(),
            ))
            .show(move |result| {
                if result {
                    let _ = app_clone.clipboard().write_text(&message);
                }
            });
    }
}
