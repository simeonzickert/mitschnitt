//! Startbeweis gegen eine KOPIE der Bestandsdatenbank.
//!
//! Der Ausbau der Cloud-Schicht bringt eine Migration mit, die Tabellen und
//! Trigger fallen laesst. Ein gruener `cargo check` sagt darueber nichts: die
//! Frage ist, ob der echte Startpfad eine gewachsene Datenbank noch oeffnet,
//! ohne dass eine Migration scheitert oder Gespraeche verschwinden.
//!
//! Der Test laeuft nur, wenn `MITSCHNITT_BESTANDS_DB` auf eine Kopie zeigt --
//! ohne die Variable meldet er sich als uebersprungen. Er fasst NIE die
//! produktive Datei an; der Aufrufer legt die Kopie an.

use std::path::PathBuf;

#[tokio::test]
async fn bestandsdatenbank_oeffnet_nach_dem_cloud_ausbau() {
    let Ok(path) = std::env::var("MITSCHNITT_BESTANDS_DB") else {
        eprintln!("uebersprungen: MITSCHNITT_BESTANDS_DB nicht gesetzt");
        return;
    };
    let path = PathBuf::from(path);
    assert!(path.is_file(), "Kopie nicht gefunden: {}", path.display());

    let db = anlg_db_core::Db::open(anlg_db_core::DbOpenOptions {
        storage: anlg_db_core::DbStorage::Local(&path),
        journal_mode_wal: true,
        foreign_keys: true,
        max_connections: Some(4),
    })
    .await
    .expect("Bestandsdatenbank muss sich oeffnen lassen");

    // Genau der Pfad, den der Desktop beim Start faehrt.
    db_app::prepare_schema(&db)
        .await
        .expect("Migrationen muessen auf dem Bestand durchlaufen");

    let count = |sql: &'static str| {
        let pool = db.pool().clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };

    let sessions = count("SELECT COUNT(*) FROM sessions").await;
    let transcripts = count("SELECT COUNT(*) FROM transcripts").await;
    let documents = count("SELECT COUNT(*) FROM session_documents").await;
    let humans = count("SELECT COUNT(*) FROM humans").await;
    let templates = count("SELECT COUNT(*) FROM templates").await;

    println!(
        "NACH  sessions={sessions} transcripts={transcripts} documents={documents} \
         humans={humans} templates={templates}"
    );

    assert!(sessions > 0, "Gespraeche duerfen nicht verschwinden");
    assert!(transcripts > 0, "Transkripte duerfen nicht verschwinden");

    // Die Cloud-Tabellen sind weg ...
    for tabelle in [
        "e2ee_records",
        "e2ee_dirty_rows",
        "e2ee_apply_guard",
        "cloudsync_writable_workspaces",
        "shared_session_cache",
        "session_share_activation",
        "enterprise_session_delivery_state",
        "synced_preferences",
        "attachment_transfer_jobs",
        "workspaces",
    ] {
        let vorhanden: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(tabelle)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(vorhanden, 0, "{tabelle} haette fallen muessen");
    }

    // ... und das Lokale steht.
    for tabelle in [
        "sessions",
        "transcripts",
        "session_documents",
        "humans",
        "templates",
        "attachment_local_state",
        "webhook_endpoints",
        "session_proposals",
        "search_index_dirty",
    ] {
        let vorhanden: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(tabelle)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(vorhanden, 1, "{tabelle} muss bleiben");
    }

    // Kein e2ee_dirty_*-Trigger darf ueberleben -- die haengen an sessions und
    // transcripts und wuerden jeden Schreibvorgang in eine tote Warteschlange
    // spiegeln.
    let trigger: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'e2ee_%'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(trigger, 0, "e2ee-Trigger haetten fallen muessen");

    // Und ein echter Schreibvorgang muss durchgehen.
    sqlx::query("INSERT INTO sessions (id, title) VALUES ('startbeweis', 'Startbeweis')")
        .execute(db.pool())
        .await
        .expect("INSERT muss durchgehen");
    sqlx::query("DELETE FROM sessions WHERE id = 'startbeweis'")
        .execute(db.pool())
        .await
        .expect("DELETE muss durchgehen");

    let integritaet: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(integritaet, "ok");
}
