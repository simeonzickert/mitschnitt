use tauri::ipc::Channel;

use crate::{ExecuteProxyResult, ManagedState, QueryEvent, TransactionStatement};

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_meetings(
    state: tauri::State<'_, ManagedState>,
    input: anlg_agent_access::ListMeetingsInput,
) -> Result<anlg_agent_access::MeetingPage, String> {
    anlg_agent_access::list_meetings(state.pool(), input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_meeting(
    state: tauri::State<'_, ManagedState>,
    input: anlg_agent_access::GetMeetingInput,
) -> Result<anlg_agent_access::Meeting, String> {
    anlg_agent_access::get_meeting(state.pool(), input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_meeting_transcript(
    state: tauri::State<'_, ManagedState>,
    input: anlg_agent_access::GetMeetingTranscriptInput,
) -> Result<anlg_agent_access::TranscriptPage, String> {
    anlg_agent_access::get_meeting_transcript(state.pool(), input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_recurring_meeting_history(
    state: tauri::State<'_, ManagedState>,
    input: anlg_agent_access::GetRecurringMeetingHistoryInput,
) -> Result<anlg_agent_access::MeetingPage, String> {
    anlg_agent_access::get_recurring_meeting_history(state.pool(), input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn execute(
    state: tauri::State<'_, ManagedState>,
    sql: String,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    state
        .execute(sql, params)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn execute_transaction(
    state: tauri::State<'_, ManagedState>,
    statements: Vec<TransactionStatement>,
) -> Result<Vec<u64>, String> {
    state
        .execute_transaction(statements)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn execute_proxy(
    state: tauri::State<'_, ManagedState>,
    sql: String,
    params: Vec<serde_json::Value>,
    method: String,
) -> Result<ExecuteProxyResult, String> {
    let method = method
        .parse::<anlg_db_execute::ProxyQueryMethod>()
        .map_err(|error| error.to_string())?;
    state
        .execute_proxy(sql, params, method)
        .await
        .map(|result| ExecuteProxyResult { rows: result.rows })
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_legacy_import_report(
    state: tauri::State<'_, ManagedState>,
) -> Result<crate::LegacyImportReport, String> {
    crate::import::get_legacy_import_report(state.pool())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_legacy_cleanup_status(
    state: tauri::State<'_, ManagedState>,
) -> Result<crate::LegacyCleanupStatus, String> {
    crate::import::get_legacy_cleanup_status(state.pool(), state.vault_base().ok())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn cleanup_legacy_files(
    state: tauri::State<'_, ManagedState>,
) -> Result<crate::LegacyCleanupResult, String> {
    state
        .cleanup_legacy_files()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn run_legacy_import(
    state: tauri::State<'_, ManagedState>,
    dry_run: bool,
) -> Result<String, String> {
    state
        .rerun_legacy_import(dry_run)
        .await
        .map_err(|error| error.to_string())
}

/// Die bekannten Orte absuchen, statt den Menschen nach einem Pfad zu fragen.
#[tauri::command]
#[specta::specta]
pub(crate) async fn find_import_sources(
    state: tauri::State<'_, ManagedState>,
) -> Result<Vec<crate::ImportSourceScan>, String> {
    let target_vault = state.vault_base().map_err(|error| error.to_string())?;
    crate::import::find_import_sources(state.pool(), target_vault)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn scan_import_source(
    state: tauri::State<'_, ManagedState>,
    source_path: String,
) -> Result<crate::ImportSourceScan, String> {
    let target_vault = state.vault_base().map_err(|error| error.to_string())?;
    crate::import::scan_import_source(
        state.pool(),
        std::path::Path::new(&source_path),
        target_vault,
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn run_source_import(
    state: tauri::State<'_, ManagedState>,
    source_path: String,
    dry_run: bool,
    copy_audio: bool,
) -> Result<String, String> {
    let target_vault = state.vault_base().map_err(|error| error.to_string())?;
    crate::import::run_source_import(
        state.pool(),
        std::path::Path::new(&source_path),
        target_vault,
        dry_run,
        copy_audio,
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_import_run(
    state: tauri::State<'_, ManagedState>,
    run_id: String,
) -> Result<crate::ImportRunReport, String> {
    crate::import::get_import_run(state.pool(), &run_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn apply_session_ingest(
    state: tauri::State<'_, ManagedState>,
    workspace_id: String,
    envelope: serde_json::Value,
) -> Result<crate::SessionIngestApplyResult, String> {
    let envelope = match serde_json::from_value(envelope) {
        Ok(envelope) => envelope,
        Err(error) => {
            tracing::warn!(%workspace_id, %error, "rejected malformed session ingest envelope");
            return Ok(crate::SessionIngestApplyResult::Rejected);
        }
    };
    match anlg_session_ingest::apply_session_envelope(state.pool(), &workspace_id, &envelope).await
    {
        Ok(outcome) => Ok(match outcome {
            anlg_session_ingest::ApplyOutcome::Applied => crate::SessionIngestApplyResult::Applied,
            anlg_session_ingest::ApplyOutcome::AlreadyApplied => {
                crate::SessionIngestApplyResult::AlreadyApplied
            }
        }),
        Err(error) if error.is_retryable() => Err(error.to_string()),
        Err(error) => {
            tracing::warn!(%workspace_id, %error, "rejected permanent session ingest envelope");
            Ok(crate::SessionIngestApplyResult::Rejected)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn subscribe(
    state: tauri::State<'_, ManagedState>,
    sql: String,
    params: Vec<serde_json::Value>,
    on_event: Channel<QueryEvent>,
) -> Result<anlg_db_reactive::SubscriptionRegistration, String> {
    state
        .subscribe(
            sql,
            params,
            crate::runtime::QueryEventChannel::new(on_event),
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn unsubscribe(
    state: tauri::State<'_, ManagedState>,
    subscription_id: String,
) -> Result<(), String> {
    state
        .unsubscribe(&subscription_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn get_startup_status(state: tauri::State<'_, ManagedState>) -> crate::StartupStatus {
    state.startup_status()
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn wait_until_ready(state: tauri::State<'_, ManagedState>) -> Result<(), String> {
    state
        .wait_until_ready()
        .await
        .map_err(|error| error.to_string())
}
