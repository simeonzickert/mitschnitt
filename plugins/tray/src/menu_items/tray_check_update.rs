use std::sync::{
    Mutex,
    atomic::{AtomicU8, Ordering},
};

use tauri::{
    AppHandle, Result,
    menu::{MenuItem, MenuItemKind},
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_updater2::Updater2PluginExt;
use tauri_specta::Event;

use super::MenuItemHandler;
use crate::{Text, TrayPluginExt, current_menu_lang, tr, tr_fmt};

const STATE_CHECK_FOR_UPDATE: u8 = 0;
const STATE_DOWNLOADING: u8 = 1;
const STATE_RESTART_TO_APPLY: u8 = 2;

static UPDATE_STATE: AtomicU8 = AtomicU8::new(STATE_CHECK_FOR_UPDATE);
static PENDING_VERSION: Mutex<Option<String>> = Mutex::new(None);

pub struct TrayCheckUpdate;

impl TrayCheckUpdate {
    pub fn set_state(app: &AppHandle<tauri::Wry>, state: UpdateMenuState) -> Result<()> {
        let lang = current_menu_lang();
        let (text, enabled, state_value) = match &state {
            UpdateMenuState::CheckForUpdate => {
                (tr(Text::CheckForUpdates, lang), true, STATE_CHECK_FOR_UPDATE)
            }
            UpdateMenuState::Downloading => {
                (tr(Text::Downloading, lang), false, STATE_DOWNLOADING)
            }
            UpdateMenuState::RestartToApply(_) => (
                tr(Text::RestartToApplyUpdate, lang),
                true,
                STATE_RESTART_TO_APPLY,
            ),
        };

        if let UpdateMenuState::RestartToApply(version) = state {
            *PENDING_VERSION.lock().unwrap() = Some(version);
        }

        UPDATE_STATE.store(state_value, Ordering::SeqCst);

        if let Some(menu) = app.menu()
            && let Some(item) = menu.get(Self::ID)
            && let MenuItemKind::MenuItem(menu_item) = item
        {
            menu_item.set_text(text)?;
            menu_item.set_enabled(enabled)?;
        }

        app.tray().refresh_menu()?;

        Ok(())
    }

    fn get_state() -> u8 {
        UPDATE_STATE.load(Ordering::SeqCst)
    }

    fn pending_version() -> Option<String> {
        PENDING_VERSION.lock().unwrap().clone()
    }

    async fn apply_update(app: AppHandle<tauri::Wry>, version: String) {
        if let Err(e) = app.updater2().install_and_relaunch(&version).await {
            let lang = current_menu_lang();
            app.dialog()
                .message(tr_fmt(Text::FailedToInstallUpdate, lang, &[&e.to_string()]))
                .title(tr(Text::UpdateFailed, lang))
                .show(|_| {});
        }
    }
}

#[derive(Debug, Clone)]
pub enum UpdateMenuState {
    CheckForUpdate,
    Downloading,
    RestartToApply(String),
}

impl MenuItemHandler for TrayCheckUpdate {
    const ID: &'static str = "anlg_tray_check_update";

    fn build(app: &AppHandle<tauri::Wry>) -> Result<MenuItemKind<tauri::Wry>> {
        let state = Self::get_state();
        let lang = current_menu_lang();

        let (text, enabled) = match state {
            STATE_DOWNLOADING => (tr(Text::Downloading, lang), false),
            STATE_RESTART_TO_APPLY => (tr(Text::RestartToApplyUpdate, lang), true),
            _ => (tr(Text::CheckForUpdates, lang), true),
        };
        let item = MenuItem::with_id(app, Self::ID, text, enabled, None::<&str>)?;
        Ok(MenuItemKind::MenuItem(item))
    }

    fn handle(app: &AppHandle<tauri::Wry>) {
        let current_state = Self::get_state();

        if current_state == STATE_RESTART_TO_APPLY {
            if let Some(version) = Self::pending_version() {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    Self::apply_update(app, version).await;
                });
            }
            return;
        }

        if current_state == STATE_DOWNLOADING {
            return;
        }

        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            match app.updater2().check().await {
                Ok(Some(version)) => {
                    if app.updater2().has_cached_update(&version) {
                        let event = tauri_plugin_updater2::UpdateReadyEvent { version };
                        if let Err(e) = event.emit(&app) {
                            tracing::warn!("failed_emit_update_ready_event: {e}");
                        }
                        return;
                    }

                    let lang = current_menu_lang();
                    let app_for_dialog = app.clone();
                    let version_for_download = version.clone();
                    app.dialog()
                        .message(tr_fmt(Text::UpdateAvailableBody, lang, &[&version]))
                        .title(tr(Text::UpdateAvailableTitle, lang))
                        .buttons(MessageDialogButtons::OkCancelCustom(
                            tr(Text::Download, lang).to_string(),
                            tr(Text::Later, lang).to_string(),
                        ))
                        .show(move |accepted| {
                            if accepted {
                                let app = app_for_dialog;
                                let version = version_for_download;
                                tauri::async_runtime::spawn(async move {
                                    if let Err(e) = app.updater2().download(&version).await {
                                        let _ = TrayCheckUpdate::set_state(
                                            &app,
                                            UpdateMenuState::CheckForUpdate,
                                        );
                                        let lang = current_menu_lang();
                                        app.dialog()
                                            .message(tr_fmt(
                                                Text::FailedToDownloadUpdate,
                                                lang,
                                                &[&e.to_string()],
                                            ))
                                            .title(tr(Text::UpdateFailed, lang))
                                            .show(|_| {});
                                    }
                                });
                            }
                        });
                }
                Ok(None) => {
                    let lang = current_menu_lang();
                    app.dialog()
                        .message(tr(Text::NoUpdatesAvailable, lang))
                        .title(tr(Text::CheckForUpdates, lang))
                        .show(|_| {});
                }
                Err(e) => {
                    let lang = current_menu_lang();
                    app.dialog()
                        .message(tr_fmt(Text::FailedToCheckForUpdates, lang, &[&e.to_string()]))
                        .title(tr(Text::UpdateCheckFailed, lang))
                        .show(|_| {});
                }
            }
        });
    }
}
