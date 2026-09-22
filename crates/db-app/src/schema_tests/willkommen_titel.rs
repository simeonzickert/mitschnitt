//! Der Titel einer BESTEHENDEN Willkommens-Sitzung.
//!
//! Der Quellcode legt seit da7f3d41c8 "Welcome to Mitschnitt" an; eine vorher
//! angelegte Sitzung traegt den alten Titel in ZWEI Feldern weiter --
//! `sessions.title` (Seitenleiste, Suchindex, Markdown-Spiegel) und
//! `event_json.title` (Oberflaeche, Stichwoerter, Vorbereitungs-Notiz). Diese
//! Tests halten fest, was der Step 20260902001200 tut und vor allem, was er
//! NICHT anfasst: einen selbst vergebenen Titel, eine fremde Sitzung, eine
//! Zeile ohne gueltiges JSON -- und beim zweiten Lauf gar nichts mehr.

use super::*;
use anlg_db_core::Db;

const TITEL_STEP_ID: &str = "20260902001200_willkommen_titel_mitschnitt";
const TRACKING_ID: &str = "anarlog-onboarding-demo-v1";
const ALT: &str = "Welcome to Anarlog";
const NEU: &str = "Welcome to Mitschnitt";
/// Wie die Zeile in der einzigen Installation aussieht (gemessen 02.09.2026 an
/// einer Kopie samt -wal/-shm): der alte Titel in beiden Feldern, dazu Link
/// und Beschreibung des Originals -- die faengt getSessionEvent beim Lesen ab
/// und dieser Step laesst sie deshalb bewusst stehen.
const LIVE_EVENT_JSON: &str = r#"{"tracking_id":"anarlog-onboarding-demo-v1","calendar_id":"","title":"Welcome to Anarlog","started_at":"2026-08-28T19:32:41.638Z","ended_at":"","is_all_day":false,"has_recurrence_rules":false,"meeting_link":"https://anarlog.so/onboarding-demo/","description":"A private, prerecorded introduction to Anarlog."}"#;
const ZEITSTEMPEL: &str = "2026-08-28T19:32:41.638Z";

fn titel_step() -> &'static anlg_db_migrate::MigrationStep {
    APP_MIGRATION_STEPS
        .iter()
        .find(|step| step.id == TITEL_STEP_ID)
        .unwrap_or_else(|| panic!("Migrations-Step {TITEL_STEP_ID} ist nicht registriert"))
}

/// Datenbank auf dem Stand VOR dem Titel-Step.
async fn bestand_vor_titel_step() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(
        &db,
        anlg_db_migrate::DbSchema {
            steps: migration_steps_before(TITEL_STEP_ID),
        },
    )
    .await
    .unwrap();
    db
}

async fn sitzung_anlegen(db: &Db, id: &str, title: &str, event_json: &str) {
    sqlx::query(
        "INSERT INTO sessions (id, title, event_json, updated_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(id)
    .bind(title)
    .bind(event_json)
    .bind(ZEITSTEMPEL)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn zeile(db: &Db, id: &str) -> (String, String, String) {
    sqlx::query_as("SELECT title, event_json, updated_at FROM sessions WHERE id = ?")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap()
}

fn event_titel(event_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(event_json)
        .ok()?
        .get("title")?
        .as_str()
        .map(str::to_string)
}

/// Bestand anlegen, dann den vollen Lauf -- genau das, was eine bestehende
/// Installation beim ersten Start mit dieser Version erlebt.
async fn nach_vollem_lauf(sitzungen: &[(&str, &str, &str)]) -> Db {
    let db = bestand_vor_titel_step().await;
    for (id, title, event_json) in sitzungen {
        sitzung_anlegen(&db, id, title, event_json).await;
    }
    anlg_db_migrate::migrate(&db, schema()).await.unwrap();
    db
}

#[tokio::test]
async fn die_bestehende_willkommens_sitzung_heisst_danach_mitschnitt() {
    let db = nach_vollem_lauf(&[("willkommen", ALT, LIVE_EVENT_JSON)]).await;

    let (title, event_json, _) = zeile(&db, "willkommen").await;
    assert_eq!(title, NEU, "sessions.title -- Seitenleiste, Suche, Spiegel");
    assert_eq!(
        event_titel(&event_json).as_deref(),
        Some(NEU),
        "event_json.title -- Oberflaeche, Stichwoerter, Vorbereitung"
    );
    assert_eq!(
        event_json,
        LIVE_EVENT_JSON.replace(ALT, NEU),
        "nur der Titel faellt: Reihenfolge, Link und Beschreibung bleiben Zeichen fuer Zeichen"
    );
}

/// Ein selbst vergebener Titel ist eine Entscheidung des Nutzers. Er bleibt --
/// und zwar auch dann, wenn das andere Feld noch den alten Wert traegt: die
/// beiden Felder werden einzeln geprueft, nicht als Paket.
#[tokio::test]
async fn ein_selbst_umbenannter_titel_bleibt_unangetastet() {
    let db = nach_vollem_lauf(&[("umbenannt", "Meine Willkommensnotiz", LIVE_EVENT_JSON)]).await;

    let (title, event_json, _) = zeile(&db, "umbenannt").await;
    assert_eq!(title, "Meine Willkommensnotiz");
    assert_eq!(
        event_titel(&event_json).as_deref(),
        Some(NEU),
        "das andere Feld traegt noch den Alt-Titel und wird trotzdem korrigiert"
    );
}

#[tokio::test]
async fn ein_umbenannter_event_titel_bleibt_unangetastet() {
    let eigener = LIVE_EVENT_JSON.replace(ALT, "Mein Termin");
    let db = nach_vollem_lauf(&[("event-umbenannt", ALT, &eigener)]).await;

    let (title, event_json, _) = zeile(&db, "event-umbenannt").await;
    assert_eq!(title, NEU);
    assert_eq!(event_titel(&event_json).as_deref(), Some("Mein Termin"));
}

/// Ohne die tracking_id ist es nicht die Willkommens-Sitzung -- auch wenn sie
/// zufaellig genauso heisst.
#[tokio::test]
async fn eine_fremde_sitzung_mit_demselben_titel_bleibt_unangetastet() {
    let fremd = LIVE_EVENT_JSON.replace(TRACKING_ID, "irgendein-anderer-termin");
    let db = nach_vollem_lauf(&[("fremd", ALT, &fremd)]).await;

    let (title, event_json, updated_at) = zeile(&db, "fremd").await;
    assert_eq!(title, ALT);
    assert_eq!(event_titel(&event_json).as_deref(), Some(ALT));
    assert_eq!(updated_at, ZEITSTEMPEL);
}

/// Gemessen an der Live-Datenbank: 4 von 9 Sitzungen haben kein gueltiges JSON
/// in event_json (Spalten-Default ''). json_extract bricht darauf mit
/// "malformed JSON" ab -- der Step muss trotzdem durchlaufen.
#[tokio::test]
async fn sitzungen_ohne_gueltiges_json_lassen_den_step_durchlaufen() {
    let db = nach_vollem_lauf(&[
        ("leer", ALT, ""),
        ("kaputt", ALT, "kein json"),
        ("willkommen", ALT, LIVE_EVENT_JSON),
    ])
    .await;

    assert_eq!(
        zeile(&db, "leer").await.0,
        ALT,
        "keine tracking_id, kein Fall"
    );
    assert_eq!(zeile(&db, "kaputt").await.0, ALT);
    assert_eq!(zeile(&db, "willkommen").await.0, NEU);
}

/// Ein zweiter Lauf trifft null Zeilen: updated_at bleibt stehen, und der
/// Suchindex wird nicht erneut angestossen (search_index_sessions_update
/// feuert bei jedem UPDATE, auch bei einem, der nichts aendert).
#[tokio::test]
async fn zweiter_lauf_desselben_sql_aendert_nichts() {
    let db = nach_vollem_lauf(&[
        ("willkommen", ALT, LIVE_EVENT_JSON),
        (
            "fremd",
            ALT,
            &LIVE_EVENT_JSON.replace(TRACKING_ID, "anderer"),
        ),
    ])
    .await;

    let abbild = |db: &Db| {
        let pool = db.pool().clone();
        async move {
            let sitzungen: Vec<(String, String, String, String)> = sqlx::query_as(
                "SELECT id, title, event_json, updated_at FROM sessions ORDER BY id",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            let schmutzig: Vec<(String, i64)> = sqlx::query_as(
                "SELECT entity_id, generation FROM search_index_dirty
                 WHERE entity_type = 'session' ORDER BY entity_id",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            (sitzungen, schmutzig)
        }
    };

    let vorher = abbild(&db).await;
    sqlx::raw_sql(titel_step().sql)
        .execute(db.pool())
        .await
        .expect("der Step muss ein zweites Mal durchlaufen");
    let nachher = abbild(&db).await;

    assert_eq!(vorher, nachher);
}
