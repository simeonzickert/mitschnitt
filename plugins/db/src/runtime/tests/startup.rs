use super::*;

#[tokio::test]
async fn wait_until_ready_resolves_after_startup_finishes() {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_app::prepare_schema(&db).await.unwrap();
    let runtime = PluginDbRuntime::new(std::sync::Arc::new(db));

    assert_eq!(
        runtime.startup_status().phase,
        crate::StartupPhase::PreparingDatabase
    );
    let wait = runtime.wait_until_ready();
    runtime.finish_startup(Ok(()));
    wait.await.unwrap();
    assert_eq!(runtime.startup_status().phase, crate::StartupPhase::Ready);
}

// Bindeglied zwischen Erzeuger und Erkenner. `crates/db-migrate` schreibt die
// Meldung, und drei Stellen VERGLEICHEN sie als Zeichenkette, um daraus den
// Aktualisierungs-Hinweis statt des allgemeinen Startfehlers zu bauen
// (apps/desktop/src-tauri/src/db.rs, .../lib.rs, long-load-gate.tsx). Die
// koennen den Erzeuger nicht importieren, also pruefen sie gegen ein Literal
// -- und ein Literal merkt nicht, wenn der Erzeuger seine Worte aendert.
//
// Dieser Test rendert den ECHTEN Fehler und haelt ihn gegen genau die
// Teilzeichenkette, auf die jene drei vergleichen. Weicht der Wortlaut ab,
// wird er rot, statt dass der Nutzer stumm den falschen Dialog sieht.
#[test]
fn migration_error_keeps_the_wording_the_startup_dialog_matches() {
    const CONTRACT: &str = "created by a newer version of Mitschnitt";

    let ahead = anlg_db_migrate::MigrateError::DatabaseAhead {
        missing_version: 20,
        max_applied_version: 30,
    };
    let from_newer = anlg_db_migrate::MigrateError::SchemaFromNewerApp {
        min_supported_version: 20,
        max_known_version: 10,
    };

    for error in [ahead.to_string(), from_newer.to_string()] {
        assert!(
            error.contains(CONTRACT),
            "Der Startdialog erkennt diese Meldung nicht mehr: {error}"
        );
    }
}

#[tokio::test]
async fn wait_until_ready_surfaces_startup_failure() {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_app::prepare_schema(&db).await.unwrap();
    let runtime = PluginDbRuntime::new(std::sync::Arc::new(db));

    let wait = runtime.wait_until_ready();
    runtime.finish_startup(Err(
        "the database was created by a newer version of Mitschnitt".into(),
    ));
    let error = wait.await.unwrap_err();

    assert!(
        error
            .to_string()
            .contains("created by a newer version of Mitschnitt")
    );
    assert_eq!(runtime.startup_status().phase, crate::StartupPhase::Failed);
}

#[tokio::test]
async fn wait_until_ready_resolves_when_startup_already_finished() {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_app::prepare_schema(&db).await.unwrap();
    let runtime = PluginDbRuntime::new(std::sync::Arc::new(db));

    runtime.finish_startup(Ok(()));
    runtime.wait_until_ready().await.unwrap();
}

#[tokio::test]
async fn terminal_startup_status_ignores_late_schema_progress() {
    let db = Db::connect_memory_plain().await.unwrap();
    let runtime = PluginDbRuntime::new(std::sync::Arc::new(db));

    runtime.finish_startup(Err("startup failed".into()));
    runtime.set_startup_status_if_running(crate::StartupStatus {
        phase: crate::StartupPhase::MigratingDatabase,
        migration_current: Some(1),
        migration_total: Some(2),
    });

    assert_eq!(runtime.startup_status().phase, crate::StartupPhase::Failed);
}

#[tokio::test]
async fn open_app_db_unmigrated_skips_schema_preparation() {
    let db = crate::open_app_db_unmigrated(None).await.unwrap();
    let sessions_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'sessions'
        )",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();

    assert!(!sessions_exists);
}
