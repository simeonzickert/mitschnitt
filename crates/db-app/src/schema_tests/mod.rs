use super::*;
use anlg_db_core::Db;

async fn test_db() -> Db {
    let db = Db::open(anlg_db_core::DbOpenOptions {
        storage: anlg_db_core::DbStorage::Memory,
        journal_mode_wal: true,
        foreign_keys: true,
        max_connections: Some(1),
    })
    .await
    .unwrap();
    prepare_schema(&db).await.unwrap();
    db
}

fn migration_steps_before(id: &str) -> &'static [anlg_db_migrate::MigrationStep] {
    let index = APP_MIGRATION_STEPS
        .iter()
        .position(|step| step.id == id)
        .unwrap();
    &APP_MIGRATION_STEPS[..index]
}

/// Alle Steps bis EINSCHLIESSLICH `id`. Fuer Tests, deren Gegenstand das
/// Verhalten genau eines Steps ist: ein spaeterer Step darf ihr Urteil nicht
/// kippen. Ohne das haette jeder neue Standard-Step die Aussagen der
/// vorherigen Runde umgeschrieben statt sie stehen zu lassen.
fn schema_through(id: &str) -> anlg_db_migrate::DbSchema {
    let index = APP_MIGRATION_STEPS
        .iter()
        .position(|step| step.id == id)
        .unwrap_or_else(|| panic!("Migrations-Step {id} ist nicht registriert"));
    anlg_db_migrate::DbSchema {
        steps: &APP_MIGRATION_STEPS[..=index],
    }
}

/// Die IDs eines Vorlagen-Seeds, aus dem SQL gelesen: jede Zeile, die mit
/// `('` beginnt, traegt eine ID im ersten Literal.
fn seed_template_ids(sql: &str) -> Vec<String> {
    sql.lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("('"))
        .filter_map(|rest| rest.split('\'').next())
        .map(str::to_string)
        .collect()
}

async fn template_ids(db: &Db) -> Vec<String> {
    sqlx::query_scalar("SELECT id FROM templates ORDER BY id")
        .fetch_all(db.pool())
        .await
        .unwrap()
}

/// Der Bestand nach dem Aufraeumen vom 04.09.2026 (Step
/// 20260904150000_vorlagen_aufraeumen), alphabetisch: die fuenf behaltenen
/// Upstream-Vorlagen plus die beiden forkeigenen, die bleiben.
const VORLAGEN_NACH_AUFRAEUMEN: [&str; 7] = [
    "default-client-kickoff",
    "default-lecture-notes",
    "default-one-on-one-meeting",
    "default-sprint-planning",
    "default-sprint-retrospective",
    "mitschnitt-kompakt",
    "mitschnitt-standard",
];

/// Die dreizehn, die der Aufraeum-Step entfernt.
const ENTFERNTE_VORLAGEN: [&str; 13] = [
    "default-board-meeting",
    "default-brainstorming-session",
    "default-customer-discovery",
    "default-daily-standup",
    "default-executive-briefing",
    "default-incident-postmortem",
    "default-investor-pitch",
    "default-performance-review",
    "default-product-roadmap-review",
    "default-project-kickoff",
    "default-sales-discovery-call",
    "default-technical-design-review",
    "mitschnitt-adaptive-minutes",
];

/// Nach jeder Reparatur muss derselbe Bestand stehen wie nach einer normalen
/// Migration: genau die sieben. Der Reparaturpfad spielt die Seeds nach UND
/// den Aufraeum-Step dahinter (replay_template_seeds) -- ohne den zweiten
/// Schritt braechte jede Reparatur die dreizehn entfernten Vorlagen zurueck.
async fn assert_vorlagenbestand_steht(db: &Db) {
    let ids = template_ids(db).await;
    // Die Seed-Dateien bleiben die Quelle: der Aufraeum-Step darf nur die
    // dreizehn treffen, alles andere aus den Seeds muss dastehen.
    let upstream = seed_template_ids(include_str!(
        "../../migrations/20260524000000_default_templates.sql"
    ));
    assert_eq!(upstream.len(), 17, "der Upstream-Seed traegt 17 Vorlagen");
    for id in &upstream {
        let erwartet = VORLAGEN_NACH_AUFRAEUMEN.contains(&id.as_str());
        assert_eq!(
            ids.contains(id),
            erwartet,
            "Upstream-Vorlage {id}: erwartet vorhanden={erwartet}"
        );
    }
    for id in ENTFERNTE_VORLAGEN {
        assert!(
            !ids.contains(&id.to_string()),
            "Vorlage '{id}' ist nach dem Aufraeumen wieder da"
        );
    }
    for id in VORLAGEN_NACH_AUFRAEUMEN {
        assert!(ids.contains(&id.to_string()), "Vorlage '{id}' fehlt");
    }
    assert_eq!(ids.len(), 7, "{ids:?}");
}

/// Die gewaehlte Standard-Vorlage muss es geben. Zeigt `selected_template_id`
/// auf eine Vorlage, die nicht existiert, faellt die App STILL auf "Auto"
/// zurueck (loadTemplate liefert null, keine Meldung) -- der Nutzer sieht eine
/// andere Zusammenfassung und erfaehrt den Grund nie. Die Zusage ist deshalb
/// „aufloesbar“, nicht „ein bestimmter Name“: der Name wandert mit jedem neuen
/// Standard, die Aufloesbarkeit nie.
async fn assert_standard_vorlage_ist_aufloesbar(db: &Db) -> String {
    let gewaehlt: String =
        sqlx::query_scalar("SELECT value_json FROM app_settings WHERE id = 'selected_template_id'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let id: String = serde_json::from_str(&gewaehlt)
        .unwrap_or_else(|_| panic!("selected_template_id ist kein JSON-String: {gewaehlt}"));
    assert!(!id.is_empty(), "ohne Wahl gaebe es keinen Standard");
    assert!(
        template_ids(db).await.contains(&id),
        "die gewaehlte Standard-Vorlage '{id}' existiert nicht"
    );
    id
}

fn legacy_attachment_transfer_jobs_sql() -> String {
    include_str!("../../migrations/20260717150000_attachment_transfer_jobs.sql")
        .replace(
            "WHERE direction = 'delete' AND phase <> 'completed';",
            "WHERE direction = 'delete';",
        )
        .replace(
            "WHERE direction = 'upload'\n  AND remote_object_id <> ''\n  AND phase <> 'completed';",
            "WHERE direction = 'upload' AND remote_object_id <> '';",
        )
        .replace(
            "WHERE direction = 'delete'\n  AND remote_object_id <> ''\n  AND phase <> 'completed';",
            "WHERE direction = 'delete' AND remote_object_id <> '';",
        )
}

fn schema_with_legacy_attachment_transfer_jobs() -> anlg_db_migrate::DbSchema {
    let mut steps = APP_MIGRATION_STEPS.to_vec();
    let migration = steps
        .iter_mut()
        .find(|step| step.id == "20260717150000_attachment_transfer_jobs")
        .unwrap();
    migration.sql = Box::leak(legacy_attachment_transfer_jobs_sql().into_boxed_str());

    anlg_db_migrate::DbSchema {
        steps: Box::leak(steps.into_boxed_slice()),
    }
}

async fn attachment_transfer_jobs_schema_checksum_for_test(db: &Db) -> String {
    let mut transaction = db.pool().begin().await.unwrap();
    let checksum = attachment_transfer_jobs_schema_checksum(&mut transaction)
        .await
        .unwrap();
    transaction.rollback().await.unwrap();
    checksum
}

async fn test_db_without_default_templates() -> Db {
    let db = Db::open(anlg_db_core::DbOpenOptions {
        storage: anlg_db_core::DbStorage::Memory,
        journal_mode_wal: true,
        foreign_keys: true,
        max_connections: Some(1),
    })
    .await
    .unwrap();
    anlg_db_migrate::migrate(
        &db,
        anlg_db_migrate::DbSchema {
            steps: &APP_MIGRATION_STEPS[..2],
        },
    )
    .await
    .unwrap();
    sqlx::query(include_str!(
        "../../migrations/20260712170000_template_icons.sql"
    ))
    .execute(db.pool())
    .await
    .unwrap();
    db
}

mod adaptive_minutes;
mod attachments;
mod bitte_gegenpruefen;
mod consent;
mod disziplin_in_den_rahmen;
mod entities;
mod migrations;
mod mitschnitt_kompakt;
mod mitschnitt_standard;
mod search_index;
mod transcript_live_deltas;
mod voiceprints;
mod vorlagen_aufraeumen;
mod willkommen_titel;
