use crate::{TrayPluginExt, schedule::TrayScheduleEvent};

#[tauri::command]
#[specta::specta]
pub async fn set_tray_icon_visible(
    app: tauri::AppHandle<tauri::Wry>,
    visible: bool,
) -> Result<(), String> {
    app.tray().set_visible(visible).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_tray_schedule(
    app: tauri::AppHandle<tauri::Wry>,
    events: Vec<TrayScheduleEvent>,
) -> Result<(), String> {
    app.tray()
        .set_schedule(events)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn set_tray_recording_title(
    app: tauri::AppHandle<tauri::Wry>,
    title: Option<String>,
) -> Result<(), String> {
    app.tray()
        .set_recording_title(title)
        .map_err(|error| error.to_string())
}

// Von der Oberflaeche bei jedem Start (voller Einstellungs-Schnappschuss)
// UND bei jeder Aenderung der Einstellung "Main language" aufgerufen
// (settings/queries.ts::applySettingSideEffects) -- derselbe Weg wie
// set_tray_icon_visible/set_tray_schedule oben. `ai_language` ist der rohe,
// nicht vorverarbeitete Wert; die Aufloesung uebernimmt Rust selbst
// (anlg_menu_lang::resolve_lang), damit Tray/Dock/Dialoge dieselbe binaere
// Vereinfachung tragen, egal welcher Kanal sie ausloest.
#[tauri::command]
#[specta::specta]
pub async fn set_tray_menu_language(
    app: tauri::AppHandle<tauri::Wry>,
    ai_language: Option<String>,
) -> Result<(), String> {
    app.tray()
        .set_menu_language(ai_language.as_deref())
        .map_err(|error| error.to_string())
}
