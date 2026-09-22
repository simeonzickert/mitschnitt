mod commands;
mod error;
mod import;
mod runtime;

pub use error::{Error, Result};
pub use import::{ImportRunReport, ImportRunStatus, ImportSourceKind, ImportSourceScan};
pub use runtime::{open_app_db, open_app_db_unmigrated};
use tauri::Manager;

const PLUGIN_NAME: &str = "db";

pub type ManagedState = std::sync::Arc<runtime::PluginDbRuntime>;

#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TransactionStatement {
    pub sql: String,
    pub params: Vec<serde_json::Value>,
    #[serde(default)]
    pub expected_rows_affected: Option<u64>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StartupPhase {
    PreparingDatabase,
    MigratingDatabase,
    ImportingLegacyData,
    Ready,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatus {
    pub phase: StartupPhase,
    pub migration_current: Option<u32>,
    pub migration_total: Option<u32>,
}

impl StartupStatus {
    fn for_phase(phase: StartupPhase) -> Self {
        Self {
            phase,
            migration_current: None,
            migration_total: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, specta::Type, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct StorageMigrationState {
    pub phase: String,
    pub latest_run_id: String,
    pub parity_verified: bool,
    pub cutover_at: Option<String>,
    pub rollback_until: Option<String>,
    pub last_error: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportRun {
    pub id: String,
    pub importer_version: i64,
    pub source_root: String,
    pub dry_run: bool,
    pub status: String,
    pub discovered_count: i64,
    pub imported_count: i64,
    pub matched_count: i64,
    pub skipped_count: i64,
    pub conflict_count: i64,
    pub error_count: i64,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error: String,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportItemReport {
    pub source_path: String,
    pub source_kind: String,
    pub source_sha256: String,
    pub status: String,
    pub discovered_count: i64,
    pub imported_count: i64,
    pub matched_count: i64,
    pub skipped_count: i64,
    pub conflict_count: i64,
    pub error: String,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportTargetReport {
    pub source_path: String,
    pub table_name: String,
    pub target_id: String,
    pub status: String,
    pub error: String,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportReport {
    pub state: StorageMigrationState,
    pub latest_run: Option<LegacyImportRun>,
    pub items: Vec<LegacyImportItemReport>,
    pub targets: Vec<LegacyImportTargetReport>,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyCleanupStatus {
    pub migration_ready: bool,
    pub migration_verified: bool,
    pub available: bool,
    pub already_cleaned: bool,
    pub file_count: u64,
    pub total_bytes: u64,
    pub source_root: String,
    pub blocking_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyCleanupResult {
    pub deleted_file_count: u64,
    pub deleted_bytes: u64,
}

#[derive(Debug, Clone, Copy, serde::Serialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionIngestApplyResult {
    Applied,
    AlreadyApplied,
    Rejected,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, PartialEq)]
pub struct ExecuteProxyResult {
    rows: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, PartialEq)]
#[serde(tag = "event", content = "data")]
pub enum QueryEvent {
    #[serde(rename = "result")]
    Result(Vec<serde_json::Value>),
    #[serde(rename = "error")]
    Error(String),
}

fn make_specta_builder<R: tauri::Runtime>() -> tauri_specta::Builder<R> {
    tauri_specta::Builder::<R>::new()
        .plugin_name(PLUGIN_NAME)
        .commands(tauri_specta::collect_commands![
            commands::list_meetings,
            commands::get_meeting,
            commands::get_meeting_transcript,
            commands::get_recurring_meeting_history,
            commands::execute,
            commands::execute_transaction,
            commands::execute_proxy,
            commands::get_legacy_import_report,
            commands::get_legacy_cleanup_status,
            commands::cleanup_legacy_files,
            commands::run_legacy_import,
            commands::find_import_sources,
            commands::scan_import_source,
            commands::run_source_import,
            commands::get_import_run,
            commands::apply_session_ingest,
            commands::subscribe,
            commands::unsubscribe,
            commands::get_startup_status,
            commands::wait_until_ready,
        ])
        .error_handling(tauri_specta::ErrorHandlingMode::Result)
}

async fn bootstrap_app_database<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    db: std::sync::Arc<anlg_db_core::Db>,
    runtime: &runtime::PluginDbRuntime,
) -> std::result::Result<(), String> {
    runtime
        .ensure_app_schema()
        .await
        .map_err(|error| error.to_string())?;
    if import::legacy_import_attempt_required(db.pool())
        .await
        .map_err(|error| error.to_string())?
    {
        runtime.set_startup_status_if_running(StartupStatus::for_phase(
            StartupPhase::ImportingLegacyData,
        ));
    }
    import::import_legacy_data(&app, db.pool())
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn init<R: tauri::Runtime>(
    db: std::sync::Arc<anlg_db_core::Db>,
) -> tauri::plugin::TauriPlugin<R> {
    let specta_builder = make_specta_builder();

    tauri::plugin::Builder::new(PLUGIN_NAME)
        .invoke_handler(specta_builder.invoke_handler())
        .setup(move |app, _| {
            let runtime =
                std::sync::Arc::new(runtime::PluginDbRuntime::new(std::sync::Arc::clone(&db)));
            if let Ok(vault_base) = import::resolve_target_vault(app.app_handle()) {
                runtime.set_vault_base(vault_base);
            }
            let startup_db = std::sync::Arc::clone(&db);
            let startup_runtime = std::sync::Arc::clone(&runtime);
            let startup_app = app.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let result =
                    bootstrap_app_database(startup_app, startup_db, &startup_runtime).await;
                startup_runtime.finish_startup(result);
            });
            app.manage(runtime);
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests;
