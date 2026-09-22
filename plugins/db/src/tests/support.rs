use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::ipc::{Channel, InvokeResponseBody};

use crate::{QueryEvent, runtime};

pub(super) fn capture_channel() -> (Channel<QueryEvent>, Arc<Mutex<Vec<QueryEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let channel = Channel::new(move |body| {
        let InvokeResponseBody::Json(payload) = body else {
            return Ok(());
        };
        let event: QueryEvent =
            serde_json::from_str(&payload).expect("channel payload should parse");
        captured.lock().unwrap().push(event);
        Ok(())
    });
    (channel, events)
}

pub(super) async fn next_event(
    events: &Arc<Mutex<Vec<QueryEvent>>>,
    index: usize,
) -> anyhow::Result<QueryEvent> {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(event) = events.lock().unwrap().get(index).cloned() {
                return event;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(anyhow::Error::from)
}

pub(super) async fn setup_runtime() -> (tempfile::TempDir, Arc<runtime::PluginDbRuntime>) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("app.db");
    let db = anlg_db_core::Db::open(anlg_db_core::DbOpenOptions {
        storage: anlg_db_core::DbStorage::Local(&db_path),
        journal_mode_wal: true,
        foreign_keys: true,
        max_connections: Some(4),
    })
    .await
    .unwrap();
    anlg_db_app::prepare_schema(&db).await.unwrap();
    sqlx::query(
        "UPDATE storage_migration_state
         SET importer_version = ?, parity_verified = 1
         WHERE id = 'legacy_v1'",
    )
    .bind(anlg_db_app::LEGACY_IMPORTER_VERSION)
    .execute(db.pool())
    .await
    .unwrap();

    (dir, Arc::new(runtime::PluginDbRuntime::new(Arc::new(db))))
}

pub(super) async fn setup_unmigrated_runtime_with_max_connections(
    max_connections: u32,
) -> (tempfile::TempDir, Arc<runtime::PluginDbRuntime>) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("app.db");
    let db = anlg_db_core::Db::open(anlg_db_core::DbOpenOptions {
        storage: anlg_db_core::DbStorage::Local(&db_path),
        journal_mode_wal: true,
        foreign_keys: true,
        max_connections: Some(max_connections),
    })
    .await
    .unwrap();

    (dir, Arc::new(runtime::PluginDbRuntime::new(Arc::new(db))))
}

pub(super) async fn setup_unmigrated_runtime() -> (tempfile::TempDir, Arc<runtime::PluginDbRuntime>)
{
    setup_unmigrated_runtime_with_max_connections(4).await
}
