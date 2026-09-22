const COMMANDS: &[&str] = &[
    "execute",
    "execute_proxy",
    "execute_transaction",
    "get_meeting",
    "get_meeting_transcript",
    "get_recurring_meeting_history",
    "get_legacy_cleanup_status",
    "get_legacy_import_report",
    "list_meetings",
    "cleanup_legacy_files",
    "run_legacy_import",
    "find_import_sources",
    "scan_import_source",
    "run_source_import",
    "get_import_run",
    "apply_session_ingest",
    "subscribe",
    "unsubscribe",
    "get_startup_status",
    "wait_until_ready",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
