mod agent_skills;
mod agents;
mod appearance;
mod commands;
mod db;
mod embedded_cli;
mod ext;
mod mirror;
mod search_index;
mod settings_transfer;
mod startup;
mod store;
mod supervisor;
mod vault_move;

use db::open_desktop_db;
use ext::*;
use store::*;

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Emitter, Manager};
use tauri_plugin_permissions::{Permission, PermissionsPluginExt};
use tauri_plugin_windows::AppWindow;

#[cfg(any(feature = "dev", feature = "devtools"))]
const STAGING_BUNDLE_ID: &str = "media.zickert.mitschnitt.staging";

const APP_EXIT_REQUESTED_EVENT: &str = "app-exit-requested";
static EXIT_FLUSH_COMPLETE: AtomicBool = AtomicBool::new(false);
static EXIT_FLUSH_REQUESTED: AtomicBool = AtomicBool::new(false);
const EXIT_FLUSH_FALLBACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const EXIT_HARD_FALLBACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

fn mark_exit_flush_complete() {
    EXIT_FLUSH_COMPLETE.store(true, Ordering::SeqCst);
}

fn start_exit_hard_fallback() {
    std::thread::spawn(|| {
        std::thread::sleep(EXIT_HARD_FALLBACK_TIMEOUT);
        std::process::exit(0);
    });
}

fn should_allow_immediate_exit() -> bool {
    EXIT_FLUSH_COMPLETE.load(Ordering::SeqCst) || anlg_intercept::should_force_quit()
}

// Upstream konnte hier per MOCK_AUDIO gebuendelte Testaufnahmen statt des
// Mikrofons einspielen. Das Crate dahinter (audio-mock) lebte ausschliesslich
// von den 77 MB Audio-Fixtures in crates/data, die dieser Fork nicht mitfuehrt.
fn create_audio_provider(_bundle_id: &str) -> std::sync::Arc<dyn anlg_audio_actual::AudioProvider> {
    std::sync::Arc::new(anlg_audio_actual::ActualAudio)
}

pub fn main() {
    startup::apply_linux_webkit_workarounds();

    // Keep a process-wide Tokio runtime for Tauri plugins, but leave it before
    // Builder::build(). tauri-plugin-single-instance's Linux setup uses zbus's
    // blocking Connection::build, which starts a nested runtime and panics if
    // this thread is already inside #[tokio::main].
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    tauri::async_runtime::set(runtime.handle().clone());

    let context = tauri::generate_context!();
    let identifier = context.config().identifier.clone();

    // Ein Panic auf dem Main-Thread muss den Prozess BEENDEN. Ohne diesen Hook
    // laeuft die Abwicklung in drop(runtime), und die wartet endlos auf den
    // blockierenden accept() des Single-Instance-Listeners: ein Prozess ohne
    // Fenster, ohne Absturzbericht, ohne Logzeile. Gemessen 01.09.2026 -- der
    // DB-Plugin-Setup panickte, der Prozess sass minutenlang untaetig da, und
    // der Nutzer sah nichts als ein Dock-Icon, das "nicht startet".
    install_main_thread_panic_hook(identifier.clone());

    // The single-instance plugin only starts with the builder, which is too
    // late to keep a second launch from racing an in-flight startup migration.
    let launch_lock = match startup::acquire_launch_lock(&identifier) {
        startup::LaunchLockState::Acquired(lock) => Some(lock),
        startup::LaunchLockState::HeldByAnotherProcess => {
            startup::exit_for_already_running_instance()
        }
        startup::LaunchLockState::Unavailable(reason) => {
            eprintln!("starting without the launch lock: {reason}");
            None
        }
    };

    let (root_supervisor_ctx, root_supervisor_handle, db) = runtime.block_on(async {
        let (root_supervisor_ctx, root_supervisor_handle) =
            match supervisor::spawn_root_supervisor().await {
                Some((ctx, handle)) => (Some(ctx), Some(handle)),
                None => (None, None),
            };

        let startup_indicator = startup::SlowStartupIndicator::show_after_delay();
        let db = match open_desktop_db(&identifier).await {
            Ok(db) => db,
            Err(error) => {
                startup_indicator.dismiss();
                exit_after_startup_failure(&identifier, &error)
            }
        };
        startup_indicator.dismiss();
        (root_supervisor_ctx, root_supervisor_handle, db)
    });

    let audio: std::sync::Arc<dyn anlg_audio_actual::AudioProvider> =
        create_audio_provider(&context.config().identifier);

    let mut builder = tauri_plugin_windows::extend_builder(tauri::Builder::default())
        .manage(audio)
        .manage(db.clone());

    // https://docs.crabnebula.dev/plugins/tauri-e2e-tests/#macos-support
    #[cfg(all(target_os = "macos", feature = "automation"))]
    {
        builder = builder.plugin(tauri_plugin_automation::init());
    }

    // https://v2.tauri.app/plugin/deep-linking/#desktop
    // should always be the first plugin
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                AppWindow::Main.request_webview_health_check(app, &window);
            }
        }));
    }

    builder = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_opener2::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_tracing::init());

    #[cfg(not(feature = "app-store"))]
    {
        builder = builder.plugin(tauri_plugin_agent::init());
    }

    builder = builder
        .plugin(tauri_plugin_db::init(db.clone()))
        .plugin(tauri_plugin_bedrock::init());

    builder = builder
        .plugin(tauri_plugin_importer::init())
        .plugin(tauri_plugin_calendar::init())
        .plugin(tauri_plugin_todo::init());

    #[cfg(not(feature = "app-store"))]
    {
        builder = builder.plugin(tauri_plugin_hooks::init());
    }

    builder = builder
        .plugin(tauri_plugin_icon::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_sidecar2::init())
        .plugin(tauri_plugin_permissions::init());

    #[cfg(not(feature = "app-store"))]
    {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder = builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_deeplink2::init())
        .plugin(tauri_plugin_fs_sync::init())
        .plugin(tauri_plugin_fs2::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_path2::init())
        .plugin(tauri_plugin_export::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_local_api::init())
        .plugin(tauri_plugin_local_auth::init())
        .plugin(tauri_plugin_mcp::init())
        .plugin(tauri_plugin_messenger::init())
        .plugin(tauri_plugin_misc::init())
        .plugin(tauri_plugin_template::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_detect::init())
        .plugin(tauri_plugin_dock::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_notify::init())
        .plugin(tauri_plugin_overlay::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_store2::init());

    #[cfg(not(feature = "app-store"))]
    {
        builder = builder.plugin(tauri_plugin_updater2::init());
    }

    builder = builder
        .plugin(tauri_plugin_tray::init(!cfg!(feature = "app-store")))
        .plugin(tauri_plugin_settings::init())
        .plugin(tauri_plugin_sfx::init())
        .plugin(tauri_plugin_shortcut::init())
        .plugin(tauri_plugin_dictation::init())
        .plugin(tauri_plugin_windows::init())
        .plugin(tauri_plugin_js::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_windows::persisted_window_state_flags())
                .with_denylist(&["composer"])
                .build(),
        )
        .plugin(tauri_plugin_transcription::init())
        .plugin(tauri_plugin_tantivy::init())
        .plugin(tauri_plugin_audio_priority::init())
        .plugin(tauri_plugin_local_llm::init())
        .plugin(tauri_plugin_local_stt::init(
            tauri_plugin_local_stt::InitOptions {
                parent_supervisor: root_supervisor_ctx
                    .as_ref()
                    .map(|ctx| ctx.supervisor.get_cell()),
            },
        ));

    #[cfg(not(feature = "app-store"))]
    {
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ));
    }

    #[cfg(any(debug_assertions, feature = "devtools"))]
    {
        builder = builder.plugin(tauri_plugin_relay::init());
    }

    #[cfg(all(not(debug_assertions), not(feature = "devtools")))]
    {
        let plugin = tauri_plugin_prevent_default::init();
        builder = builder.plugin(plugin);
    }

    #[cfg(target_os = "macos")]
    {
        builder = builder.menu(tauri_plugin_tray::build_app_menu);
    }

    let specta_builder = make_specta_builder::<tauri::Wry>();

    let root_supervisor_ctx_for_run = root_supervisor_ctx.clone();
    let identifier_for_setup = identifier.clone();

    let app_result = builder
        .invoke_handler(specta_builder.invoke_handler())
        .on_window_event(tauri_plugin_windows::on_window_event)
        .setup(move |app| {
            let app_handle = app.handle().clone();

            specta_builder.mount_events(&app_handle);

            #[cfg(any(windows, target_os = "linux"))]
            {
                // https://v2.tauri.app/ko/plugin/deep-linking/#desktop-1
                // Registration shells out to update-desktop-database/xdg-mime on Linux,
                // which are missing on NixOS; failing setup here panics the app.
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(error) = app.deep_link().register_all() {
                    tracing::warn!(%error, "failed to register deep link handlers");
                }
            }

            {
                use tauri_plugin_tray::TrayPluginExt;
                use tauri_plugin_windows::WindowsPluginExt;

                let appearance_settings =
                    appearance::load_app_appearance_settings::<tauri::Wry, _>(&app_handle);

                if let Err(error) = app_handle
                    .windows()
                    .set_show_app_in_dock(appearance_settings.show_app_in_dock)
                {
                    tracing::warn!(%error, "failed to apply dock visibility during startup");
                }

                if appearance_settings.show_tray_icon {
                    if let Err(error) = app_handle.tray().create_tray_menu() {
                        tracing::warn!(%error, "failed to create tray menu during startup");
                    }
                }
            }

            {
                use tauri_plugin_tray::AnlgMenuItem;
                app_handle.on_menu_event(|app, event| {
                    if let Ok(item) = AnlgMenuItem::try_from(event.id().clone()) {
                        item.handle(app);
                    } else {
                        tauri_plugin_tray::handle_agenda_menu_event(app, event.id());
                    }
                });
            }

            #[cfg(not(feature = "app-store"))]
            {
                use tauri_plugin_settings::SettingsPluginExt;
                if let Ok(base) = app_handle.settings().vault_base()
                    && let Err(e) = agents::write_agents_file(base.as_std_path())
                {
                    tracing::error!("failed to write AGENTS.md: {}", e);
                }
            }

            if let (Some(ctx), Some(handle)) = (&root_supervisor_ctx, root_supervisor_handle) {
                supervisor::monitor_supervisor(handle, ctx.is_exiting.clone(), app_handle.clone());
            }

            {
                use tauri_plugin_local_llm::LocalLlmPluginExt;
                if false {
                    app_handle.local_llm().start_server();
                }
            }

            search_index::spawn(app_handle.clone(), db.clone());

            // Mitschnitt-Fork: keep the readable Markdown mirror next to the
            // database in step. Best effort -- if the data folder cannot be
            // resolved the app runs on and `mitschnitt mirror status` reports the
            // gap, rather than the mirror taking startup down.
            match db::desktop_db_dir(&identifier_for_setup) {
                Some(dir) => {
                    // Derselbe Ordner, den die App gleich oeffnet, geht als
                    // Zettel an die CLI. Er wird bei JEDEM Start geschrieben,
                    // nicht bei der Installation: sonst entstuende auf einer
                    // Maschine mit bereits installierter CLI nie einer, und ein
                    // Bauartwechsel ohne Versionssprung liesse einen veralteten
                    // stehen.
                    embedded_cli::refresh_data_dir_hint(&dir);
                    // The commands resolve the mirror through the same folder
                    // the watcher uses, so a chosen root can never apply to one
                    // and not to the other.
                    app.manage(mirror::MirrorHome(dir.clone()));
                    // One gate for the watcher and the folder-change command,
                    // so a move and a mirror pass can never run over each other.
                    let gate = mirror::MirrorGate::default();
                    app.manage(gate.clone());
                    mirror::spawn(db.clone(), dir, gate)
                }
                None => tracing::error!(
                    "application data directory is unavailable; the markdown mirror is not running"
                ),
            }

            #[cfg(not(feature = "app-store"))]
            embedded_cli::spawn_auto_install(app_handle);

            Ok(())
        })
        .build(context);

    let app = match app_result {
        Ok(app) => app,
        Err(error) => exit_after_startup_failure(&identifier, &error),
    };

    // The single-instance plugin took over when the builder finished.
    drop(launch_lock);

    match get_onboarding_flag() {
        None => {}
        Some(false) => {
            if let Err(error) = app.set_onboarding_needed(false) {
                tracing::warn!(%error, "failed to persist onboarding state during startup");
            }
        }
        Some(true) => {
            use tauri_plugin_settings::SettingsPluginExt;
            use tauri_plugin_store2::Store2PluginExt;

            let _ = app.settings().reset();
            let _ = app.store2().reset();
            let _ = app.set_onboarding_needed(true);

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let permissions = app_handle.permissions();
                let _ = permissions.reset(Permission::Microphone).await;
                let _ = permissions.reset(Permission::SystemAudio).await;
                let _ = permissions.reset(Permission::ScreenRecording).await;
                let _ = permissions.reset(Permission::Accessibility).await;
                let _ = permissions.reset(Permission::Calendar).await;
                let _ = permissions.reset(Permission::Reminders).await;
            });
        }
    }

    if let Err(error) = AppWindow::Main.show(app.handle()) {
        exit_after_startup_failure(&identifier, &error);
    }

    #[cfg(target_os = "macos")]
    anlg_intercept::setup_force_quit_handler();

    // Ab hier ist ein Main-Thread-Panic kein Startfehler mehr (siehe Hook).
    STARTUP_COMPLETE.store(true, Ordering::SeqCst);

    #[allow(unused_variables)]
    app.run(move |app, event| match event {
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            if let Err(error) = AppWindow::Main.show(app) {
                tracing::error!(%error, "failed to reopen main window");
            }
        }
        tauri::RunEvent::ExitRequested { api, .. } => {
            if let Some(ref ctx) = root_supervisor_ctx_for_run {
                ctx.mark_exiting();
            }

            if should_allow_immediate_exit() {
                return;
            }

            api.prevent_exit();
            let first_request = !EXIT_FLUSH_REQUESTED.swap(true, Ordering::SeqCst);
            if first_request {
                start_exit_hard_fallback();
            }
            if app.emit_to("main", APP_EXIT_REQUESTED_EVENT, ()).is_err() {
                mark_exit_flush_complete();
                app.exit(0);
            } else if first_request {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(EXIT_FLUSH_FALLBACK_TIMEOUT).await;
                    if !EXIT_FLUSH_COMPLETE.swap(true, Ordering::SeqCst) {
                        tracing::warn!(
                            "forcing app exit after frontend flush acknowledgement timed out"
                        );
                        app_handle.exit(0);
                    }
                });
            }
        }
        tauri::RunEvent::Exit => {
            if let Some(ref ctx) = root_supervisor_ctx_for_run {
                ctx.mark_exiting();
                ctx.stop();
            }

            anlg_host::kill_processes_by_matcher(anlg_host::ProcessMatcher::Sidecar);
        }
        _ => {}
    });
    drop(runtime);
}

fn startup_failure_message(error: &impl std::fmt::Display) -> String {
    format!("Mitschnitt failed to start: {error}")
}

// Der Standard-Hook druckt weiter nach stderr (und mit RUST_BACKTRACE den
// Stack); danach endet ein Main-Thread-Panic mit Logzeile, Hinweisfenster und
// Prozessende. Panics auf anderen Threads bleiben unveraendert -- dort faengt
// sie die Runtime.
//
// Zwei Phasen, weil der Hook das ganze Prozessleben lang scharf ist: waehrend
// des Starts ist es ein Startfehler. Danach (STARTUP_COMPLETE) laufen auch
// synchrone Tauri-Commands, Tray- und Fenster-Callbacks auf diesem Thread;
// deren Panic wuerde sonst mit "could not start" beantwortet, und die Sidecars
// (Transkription, Whisper) blieben als Waisen zurueck. Der Frontend-Flush aus
// ExitRequested ist auf einem panickenden Main-Thread nicht mehr erreichbar.
//
// Die Diagnose laeuft bewusst NICHT durch exit_after_startup_failure: dessen
// Alert-Auswahl matcht Fehlertexte ("database is locked" ...), und ein
// zufaellig passender Panic-Text wuerde einen Code-Fehler als Datenproblem
// verkleiden.
//
// Absturzberichte gehen nirgendwohin: der Sentry-Sender (Client, Minidump-
// Helfer, Einwilligungs-Schalter) ist am 01.09.2026 ausgebaut (ZICK-257,
// ZICK-260). Was ein Absturz hinterlaesst, ist genau das hier -- Logzeile,
// Hinweisfenster, Prozessende -- und die Logdatei, die der Nutzer selbst
// weitergibt.
static STARTUP_COMPLETE: AtomicBool = AtomicBool::new(false);
static PANIC_EXIT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

fn install_main_thread_panic_hook(identifier: String) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        if std::thread::current().name() != Some("main") {
            return;
        }
        // Panickt etwas innerhalb dieses Ausgangs, kehrt der Hook hier sofort
        // zurueck und Rust bricht als Double-Panic ab -- statt rekursiv in den
        // Stack-Overflow zu laufen.
        if PANIC_EXIT_IN_PROGRESS.swap(true, Ordering::SeqCst) {
            return;
        }

        let message = format!("panic on main thread: {info}");
        eprintln!("{message}");
        append_startup_failure_to_log(&identifier, &message);

        if STARTUP_COMPLETE.load(Ordering::SeqCst) {
            anlg_host::kill_processes_by_matcher(anlg_host::ProcessMatcher::Sidecar);
            show_native_alert(
                "display alert \"Mitschnitt had to close\" message \"Mitschnitt hit an internal error and had to close. Your existing data was left unchanged. Please open Mitschnitt again. If this keeps happening, send the log file to support.\" as critical buttons {\"OK\"} default button \"OK\"",
            );
        } else {
            show_native_alert(
                "display alert \"Mitschnitt could not start\" message \"Your existing data was left unchanged. Please restart the app. If the problem continues, contact support.\" as critical buttons {\"OK\"} default button \"OK\"",
            );
        }

        std::process::exit(1);
    }));
}

fn show_native_alert(script: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", script])
            .spawn();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = script;
    }
}

fn exit_after_startup_failure(identifier: &str, error: &impl std::fmt::Display) -> ! {
    let message = startup_failure_message(error);
    eprintln!("{message}");
    tracing::error!(error = %error, "desktop startup failed");
    append_startup_failure_to_log(identifier, &message);

    #[cfg(target_os = "macos")]
    {
        // Startup can fail before the database is reachable, so the alert text
        // is fixed per failure class instead of embedding the error.
        let alert = if db::is_transient_lock_error(error) {
            "display alert \"Mitschnitt is not ready yet\" message \"Another Mitschnitt process is still using your data, possibly finishing an update. Your existing data was left unchanged. Please wait a moment and open Mitschnitt again.\" as critical buttons {\"OK\"} default button \"OK\""
        } else if db::is_newer_schema_error(error) {
            "display alert \"Mitschnitt needs an update\" message \"Your data was created by a newer version of Mitschnitt, and this older version cannot open it. Your existing data was left unchanged. Please install the latest version of Mitschnitt.\" as critical buttons {\"OK\"} default button \"OK\""
        } else {
            "display alert \"Mitschnitt could not start\" message \"Your existing data was left unchanged. Please restart the app. If the problem continues, contact support.\" as critical buttons {\"OK\"} default button \"OK\""
        };
        let _ = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", alert])
            .spawn();
    }

    std::process::exit(1);
}

// Startup failures happen before the tracing plugin exists, so append directly
// to the same log file support already asks users for.
fn append_startup_failure_to_log(identifier: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;

        let Some(home) = dirs::home_dir() else {
            return;
        };
        let dir = home.join("Library/Logs").join(identifier);
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("app.log"))
        else {
            return;
        };
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");
        let _ = writeln!(file, "{timestamp} ERROR mitschnitt::startup: {message}");
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (identifier, message);
    }
}

fn get_onboarding_flag() -> Option<bool> {
    let parse_value = |v: &str| -> Option<bool> {
        match v {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => {
                if let Ok(timestamp) = v.parse::<u64>() {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()?
                        .as_millis() as u64;
                    let elapsed = now.saturating_sub(timestamp * 1000);
                    if elapsed < 2500 { Some(true) } else { None }
                } else {
                    None
                }
            }
        }
    };

    pico_args::Arguments::from_env()
        .opt_value_from_str::<_, String>("--onboarding")
        .ok()
        .flatten()
        .and_then(|v| parse_value(&v))
        .or_else(|| {
            std::env::var("ONBOARDING")
                .ok()
                .and_then(|v| parse_value(&v))
        })
}

fn make_specta_builder<R: tauri::Runtime>() -> tauri_specta::Builder<R> {
    tauri_specta::Builder::<R>::new()
        .commands(tauri_specta::collect_commands![
            commands::get_onboarding_needed::<tauri::Wry>,
            commands::set_onboarding_needed::<tauri::Wry>,
            commands::get_dismissed_toasts::<tauri::Wry>,
            commands::set_dismissed_toasts::<tauri::Wry>,
            commands::get_env::<tauri::Wry>,
            commands::show_devtool::<tauri::Wry>,
            commands::is_app_store_build,
            commands::complete_app_exit::<tauri::Wry>,
            commands::get_tinybase_values::<tauri::Wry>,
            commands::get_pinned_tabs::<tauri::Wry>,
            commands::set_pinned_tabs::<tauri::Wry>,
            commands::get_recently_opened_sessions::<tauri::Wry>,
            commands::set_recently_opened_sessions::<tauri::Wry>,
            commands::check_embedded_cli::<tauri::Wry>,
            commands::install_embedded_cli::<tauri::Wry>,
            commands::list_skill_agents,
            commands::install_agent_skill,
            // Mitschnitt-Fork (F14): settings export/import.
            settings_transfer::export_settings_bundle::<tauri::Wry>,
            settings_transfer::inspect_settings_bundle,
            settings_transfer::read_settings_bundle,
            // Mitschnitt-Fork (F16): the one folder per meeting.
            mirror::mirror_session_folder::<tauri::Wry>,
            mirror::mirror_root::<tauri::Wry>,
            mirror::mirror_move_root::<tauri::Wry>,
            mirror::mirror_forget_audio::<tauri::Wry>,
            mirror::mirror_forget_session::<tauri::Wry>,
            // Mitschnitt-Fork: the storage-location move, database included
            // (see vault_move.rs's module doc). Supersedes
            // tauri-plugin-settings's own move_vault, which never touched
            // app.db.
            vault_move::move_vault::<tauri::Wry>,
        ])
        .error_handling(tauri_specta::ErrorHandlingMode::Result)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn tokio_runtime_is_not_entered_after_block_on_returns() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            assert!(tokio::runtime::Handle::try_current().is_ok());
        });
        assert!(tokio::runtime::Handle::try_current().is_err());
    }

    #[test]
    fn tauri_async_runtime_can_spawn_after_block_on_returns() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        tauri::async_runtime::set(runtime.handle().clone());
        runtime.block_on(async {});
        assert!(tokio::runtime::Handle::try_current().is_err());

        let (tx, rx) = std::sync::mpsc::channel();
        tauri::async_runtime::spawn(async move {
            let _ = tx.send(());
        });
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .expect("spawned task should run on the process-wide runtime");
    }

    #[test]
    fn startup_failure_message_includes_the_original_error() {
        let message = startup_failure_message(&"legacy import did not pass parity verification");

        assert_eq!(
            message,
            "Mitschnitt failed to start: legacy import did not pass parity verification"
        );
    }

    #[test]
    fn complete_quit_allows_immediate_exit_without_frontend_flush() {
        assert!(!should_allow_immediate_exit());
        anlg_intercept::set_force_quit();
        assert!(should_allow_immediate_exit());
    }

    #[test]
    fn export_types() {
        const OUTPUT_FILE: &str = "../src/types/tauri.gen.ts";

        make_specta_builder::<tauri::Wry>()
            .export(
                specta_typescript::Typescript::default()
                    .formatter(specta_typescript::formatter::prettier)
                    .bigint(specta_typescript::BigIntExportBehavior::Number),
                OUTPUT_FILE,
            )
            .unwrap();

        let content = std::fs::read_to_string(OUTPUT_FILE).unwrap();
        std::fs::write(OUTPUT_FILE, format!("// @ts-nocheck\n{content}")).unwrap();
    }
}
