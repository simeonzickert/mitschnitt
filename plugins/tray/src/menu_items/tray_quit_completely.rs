use tauri::{
    AppHandle, Result,
    menu::{MenuItem, MenuItemKind},
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use crate::{Text, current_menu_lang, tr, tr_fmt};

use super::MenuItemHandler;

pub struct TrayQuitCompletely;

impl MenuItemHandler for TrayQuitCompletely {
    const ID: &'static str = "anlg_tray_quit_completely";

    fn build(app: &AppHandle<tauri::Wry>) -> Result<MenuItemKind<tauri::Wry>> {
        let item = MenuItem::with_id(
            app,
            Self::ID,
            tr(Text::QuitCompletelyMenuItem, current_menu_lang()),
            true,
            None::<&str>,
        )?;
        Ok(MenuItemKind::MenuItem(item))
    }

    fn handle(app: &AppHandle<tauri::Wry>) {
        let lang = current_menu_lang();
        let app_name = app.package_info().name.clone();
        let app = app.clone();

        app.dialog()
            .message(tr_fmt(Text::QuitWillStopInBackground, lang, &[&app_name]))
            .title(tr_fmt(Text::QuitAppCompletelyQuestion, lang, &[&app_name]))
            .buttons(MessageDialogButtons::OkCancelCustom(
                tr(Text::QuitCompletelyButton, lang).to_string(),
                tr(Text::Cancel, lang).to_string(),
            ))
            .show(move |confirmed| {
                if confirmed {
                    quit_completely(&app);
                }
            });
    }
}

pub fn quit_completely(app: &AppHandle<tauri::Wry>) {
    // Skip the frontend exit flush so the process terminates immediately.
    anlg_intercept::set_force_quit();
    app.exit(0);
}
