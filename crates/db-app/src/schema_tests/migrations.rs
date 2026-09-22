use super::*;

#[tokio::test]
async fn legacy_mobile_schema_adopts_preexisting_alter_migrations() {
    let db = Db::connect_memory_plain().await.unwrap();
    sqlx::raw_sql(
        "CREATE TABLE sessions (id TEXT PRIMARY KEY NOT NULL);
         CREATE TABLE calendars (id TEXT PRIMARY KEY NOT NULL, deleted_at TEXT);
         CREATE TABLE events (id TEXT PRIMARY KEY NOT NULL, deleted_at TEXT);
         CREATE TABLE session_attachments (
            id TEXT PRIMARY KEY NOT NULL,
            cloud_sync_enabled INTEGER NOT NULL DEFAULT 0
         );
         PRAGMA user_version = 1;",
    )
    .execute(db.pool())
    .await
    .unwrap();

    adopt_legacy_mobile_schema_migration(db.pool())
        .await
        .unwrap();

    let (success, checksum): (bool, Vec<u8>) = sqlx::query_as(
        "SELECT success, checksum FROM _sqlx_migrations WHERE version = 20260711000000",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(success);
    assert_eq!(
        checksum,
        migration_checksum(include_str!(
            "../../migrations/20260711000000_calendar_event_tombstones.sql"
        ))
    );

    let attachment_checksum: Vec<u8> =
        sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version = 20260717170000")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        attachment_checksum,
        migration_checksum(include_str!(
            "../../migrations/20260717170000_attachment_cloud_sync_intent.sql"
        ))
    );
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_env = "gnu", target_arch = "aarch64"),
    all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"),
    all(target_os = "linux", target_env = "musl", target_arch = "aarch64"),
    all(target_os = "linux", target_env = "musl", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64"),
))]
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_env = "gnu", target_arch = "aarch64"),
    all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"),
    all(target_os = "linux", target_env = "musl", target_arch = "aarch64"),
    all(target_os = "linux", target_env = "musl", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64"),
))]
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_env = "gnu", target_arch = "aarch64"),
    all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"),
    all(target_os = "linux", target_env = "musl", target_arch = "aarch64"),
    all(target_os = "linux", target_env = "musl", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64"),
))]
#[tokio::test]
async fn migration_repairs_empty_titles_from_summary_headings() {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(
        &db,
        anlg_db_migrate::DbSchema {
            steps: migration_steps_before("20260713164500_repair_empty_session_titles"),
        },
    )
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO sessions (id, title)
             VALUES ('json', ''), ('markdown', '   '), ('generic', ''), ('existing', 'Keep Me')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
            "INSERT INTO session_documents
             (id, session_id, kind, body_format, body, sort_order)
             VALUES
             ('json-summary', 'json', 'summary', 'prosemirror_json',
              '{\"type\":\"doc\",\"content\":[{\"type\":\"heading\",\"attrs\":{\"level\":1},\"content\":[{\"type\":\"text\",\"text\":\"Transcript Test \"},{\"type\":\"text\",\"text\":\"Utterances\"}]}]}', 0),
             ('markdown-summary', 'markdown', 'summary', 'markdown',
              char(10) || '# Markdown Title' || char(10) || char(10) || 'Details', 0),
             ('generic-summary', 'generic', 'summary', 'markdown', '# Summary' || char(10) || 'Details', 0),
             ('existing-summary', 'existing', 'summary', 'markdown', '# Replacement' || char(10) || 'Details', 0)",
        )
        .execute(db.pool())
        .await
        .unwrap();

    anlg_db_migrate::migrate(&db, schema()).await.unwrap();

    let titles =
        sqlx::query_as::<_, (String, String)>("SELECT id, title FROM sessions ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap()
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(titles["json"], "Transcript Test Utterances");
    assert_eq!(titles["markdown"], "Markdown Title");
    assert_eq!(titles["generic"], "");
    assert_eq!(titles["existing"], "Keep Me");
}

#[tokio::test]
async fn prepare_schema_recreates_templates_after_repair_migration_was_already_applied() {
    let db = test_db().await;

    sqlx::query("DROP TABLE templates")
        .execute(db.pool())
        .await
        .unwrap();

    prepare_schema(&db).await.unwrap();

    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM templates")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(row_count > 0);

    let icon_json: String =
        sqlx::query_scalar("SELECT icon_json FROM templates ORDER BY id LIMIT 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        icon_json,
        r##"{"type":"icon","value":"notebook-tabs","color":"#9ca3af"}"##
    );
}

// C8 (4), Review 02.09.2026: die verlorene Tabelle legt der Vor-Migrate-
// Schritt in prepare_schema wieder an (Step 20260413020000 ist verbucht,
// Tabelle weg), nicht der Repair-Step dahinter -- der frueheren Name sagte
// das Falsche.
#[tokio::test]
async fn prepare_schema_seeds_templates_when_the_pre_migrate_step_recreates_the_lost_table() {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(
        &db,
        anlg_db_migrate::DbSchema {
            steps: &APP_MIGRATION_STEPS[..3],
        },
    )
    .await
    .unwrap();

    sqlx::query("DROP TABLE templates")
        .execute(db.pool())
        .await
        .unwrap();

    prepare_schema(&db).await.unwrap();

    // Ein Bestand ohne den Icon-Step: der Vor-Migrate-Schritt legt die Tabelle
    // an, migrate haengt icon_json an, und beide Seeds kommen vollstaendig
    // zurueck.
    assert_vorlagenbestand_steht(&db).await;
}

// C8 (6), Review 02.09.2026: migration_sql laeuft im Reparaturpfad und darf
// dort nicht panicken -- ein fehlender Step ist ein Fehler, den prepare_schema
// zurueckgibt, kein Absturz beim Start.
#[test]
fn migration_sql_reports_an_unknown_step_instead_of_panicking() {
    // Am HEAD davor war das ein Panic ("Migrations-Step … ist nicht
    // registriert"), gemessen per catch_unwind, bevor die Signatur wechselte.
    let error = migration_sql("20990101000000_gibt_es_nicht").unwrap_err();
    assert!(matches!(
        error,
        AppSchemaError::MissingMigrationStep("20990101000000_gibt_es_nicht")
    ));
    assert_eq!(
        error.to_string(),
        "Migrations-Step 20990101000000_gibt_es_nicht ist nicht registriert"
    );
    assert!(migration_sql("20260413020000_templates").is_ok());
}

// C8 (5), Forge 7 (Review 02.09.2026): eine leere Datenbank geht ganz durch
// migrate -- der Vor-Migrate-Schritt legt die templates-Tabelle nur an, wenn
// sie VERLOREN ging (Step 20260413020000 verbucht, Tabelle weg). Auf einer
// frischen Datenbank ist der Step nicht verbucht, also bleibt die Anlage
// migrate ueberlassen, und die Historie traegt den Step als regulaer
// ausgefuehrt.
#[tokio::test]
async fn a_fresh_database_gets_its_templates_table_from_migrate_not_from_the_repair() {
    let db = Db::connect_memory_plain().await.unwrap();

    prepare_schema(&db).await.unwrap();

    let recorded: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM _sqlx_migrations WHERE version = 20260413020000 AND success = 1
        )",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(recorded, "der templates-Step fehlt in _sqlx_migrations");
    let has_icon_json: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('templates') WHERE name = 'icon_json')",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(has_icon_json, "icon_json fehlt an der frischen Tabelle");
}

// C8 (7), Review 02.09.2026: der Vertrag hinter prepare_schema, als Test.
// anlg_db_migrate::migrate faehrt nur die Steps. Nach einem Verlust der
// Tabelle baut ein Repair-Step sie leer wieder auf, aber der Upstream-Seed
// (20260524000000) laeuft nicht erneut -- seine Version ist verbucht. Uebrig
// bleibt, was juengere Steps einfuegen: seit F6 nur 'mitschnitt-standard'.
// Wer migrate direkt aufruft, bekommt also eine Tabelle ohne die 17
// Upstream-Vorlagen. prepare_schema ist der einzige Einstieg, der beide Seeds
// nachspielt (8d80aa1b4a) -- dieser Test haelt fest, dass migrate allein das
// NICHT tut, damit niemand den Reparaturpfad fuer ueberfluessig haelt.
#[tokio::test]
async fn migrate_alone_leaves_a_lost_templates_table_without_the_upstream_seeds() {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(
        &db,
        anlg_db_migrate::DbSchema {
            steps: &APP_MIGRATION_STEPS[..3],
        },
    )
    .await
    .unwrap();

    sqlx::query("DROP TABLE templates")
        .execute(db.pool())
        .await
        .unwrap();

    anlg_db_migrate::migrate(&db, schema()).await.unwrap();

    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM templates ORDER BY id")
        .fetch_all(db.pool())
        .await
        .unwrap();
    assert_eq!(
        ids,
        vec![
            "mitschnitt-kompakt".to_string(),
            "mitschnitt-standard".to_string()
        ],
        "migrate allein spielt die Upstream-Seeds nicht nach -- das ist Sache von prepare_schema; \
         stehen bleiben nur die forkeigenen Vorlagen, deren Steps hier noch ausstehen"
    );
}
