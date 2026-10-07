//! Der Step 20260913090000_kalender_dubletten_vereinigen.
//!
//! Gemessen an einer Kopie der einzigen Installation (06.10.2026): jede
//! Kalender-Kennung stand doppelt (Sync-Zeile vom 31.08., Import-Zeile vom
//! 11.09.), die Termine verteilten sich auf beide. Diese Tests bauen genau
//! diese Lage nach und halten fest, was der Step umhaengt, was er als
//! Grabstein markiert und was er NICHT anfasst.

use super::*;
use anlg_db_core::Db;

const STEP_ID: &str = "20260913090000_kalender_dubletten_vereinigen";

async fn bestand_vor_step() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(
        &db,
        anlg_db_migrate::DbSchema {
            steps: migration_steps_before(STEP_ID),
        },
    )
    .await
    .unwrap();
    db
}

async fn kalender(db: &Db, id: &str, tracking: &str, enabled: bool, updated_at: &str) {
    sqlx::query(
        "INSERT INTO calendars
         (id, tracking_id_calendar, name, enabled, provider, source, color, connection_id,
          created_at, updated_at)
         VALUES (?, ?, 'Privat', ?, 'apple', 'iCloud', '#111', 'conn-1', ?, ?)",
    )
    .bind(id)
    .bind(tracking)
    .bind(enabled)
    .bind(updated_at)
    .bind(updated_at)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn termin(
    db: &Db,
    id: &str,
    calendar_id: &str,
    tracking: &str,
    started_at: &str,
    deleted_at: Option<&str>,
    updated_at: &str,
) {
    sqlx::query(
        "INSERT INTO events
         (id, tracking_id_event, calendar_id, title, started_at, provider, deleted_at,
          created_at, updated_at)
         VALUES (?, ?, ?, 'Termin', ?, 'apple', ?, ?, ?)",
    )
    .bind(id)
    .bind(tracking)
    .bind(calendar_id)
    .bind(started_at)
    .bind(deleted_at)
    .bind(updated_at)
    .bind(updated_at)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn sitzung(db: &Db, id: &str, event_id: &str, event_json: &str) {
    sqlx::query(
        "INSERT INTO sessions (id, title, event_id, event_json, updated_at)
         VALUES (?, 'Sitzung', ?, ?, '2026-09-20T10:00:00Z')",
    )
    .bind(id)
    .bind(event_id)
    .bind(event_json)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn schritt_ausfuehren(db: &Db) {
    let step = APP_MIGRATION_STEPS
        .iter()
        .find(|step| step.id == STEP_ID)
        .unwrap();
    sqlx::raw_sql(step.sql).execute(db.pool()).await.unwrap();
}

async fn skalar<T>(db: &Db, sql: &str) -> T
where
    T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + Unpin,
{
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .fetch_one(db.pool())
        .await
        .unwrap()
}

type Stand = (
    Vec<(String, Option<String>, bool, String)>,
    Vec<(String, String, Option<String>, String)>,
    Vec<(String, String, String)>,
);

async fn stand(db: &Db) -> Stand {
    let kalender = sqlx::query_as("SELECT id, deleted_at, enabled, updated_at FROM calendars ORDER BY id")
        .fetch_all(db.pool())
        .await
        .unwrap();
    let termine = sqlx::query_as("SELECT id, calendar_id, deleted_at, updated_at FROM events ORDER BY id")
        .fetch_all(db.pool())
        .await
        .unwrap();
    let sitzungen = sqlx::query_as("SELECT id, event_id, event_json FROM sessions ORDER BY id")
        .fetch_all(db.pool())
        .await
        .unwrap();
    (kalender, termine, sitzungen)
}

/// Lage wie in der Live-Datenbank: K = Sync-Zeile (juenger), I = Import-Zeile.
/// Termin t1 existiert zweimal (lebend an K, geloescht an I; die Sitzung
/// haengt am geloeschten), t2 nur an I (lebend, mit Sitzung), t3 nur an K.
async fn live_lage() -> Db {
    let db = bestand_vor_step().await;
    kalender(&db, "K", "cal-A", false, "2026-10-06T21:00:00Z").await;
    kalender(&db, "I", "cal-A", true, "2026-09-25T10:00:00Z").await;
    termin(&db, "k-t1", "K", "t1", "2026-10-07T09:00:00Z", None, "2026-10-06T21:00:00Z").await;
    termin(
        &db,
        "i-t1",
        "I",
        "t1",
        "2026-10-07T09:00:00+00:00",
        Some("2026-09-25T12:00:00Z"),
        "2026-09-25T12:00:00Z",
    )
    .await;
    termin(&db, "i-t2", "I", "t2", "2026-10-08T09:00:00Z", None, "2026-09-25T10:00:00Z").await;
    termin(&db, "k-t3", "K", "t3", "2026-10-09T09:00:00Z", None, "2026-10-06T21:00:00Z").await;
    sitzung(
        &db,
        "s-t1",
        "i-t1",
        r#"{"tracking_id":"t1","calendar_id":"I","title":"Termin"}"#,
    )
    .await;
    sitzung(&db, "s-t2", "i-t2", r#"{"calendar_id":"I","title":"t2"}"#).await;
    sitzung(&db, "s-ohne-json", "", "").await;
    db
}

#[tokio::test]
async fn kalender_werden_vereinigt_und_gewinner_behaelt_seine_aktivierung() {
    let db = live_lage().await;
    schritt_ausfuehren(&db).await;

    // K bleibt (juengstes updated_at), I ist Grabstein.
    assert_eq!(
        skalar::<i64>(&db, "SELECT COUNT(*) FROM calendars WHERE deleted_at IS NULL").await,
        1
    );
    assert_eq!(
        skalar::<String>(&db, "SELECT id FROM calendars WHERE deleted_at IS NULL").await,
        "K"
    );
    assert_eq!(
        skalar::<i64>(&db, "SELECT COUNT(*) FROM calendars WHERE id = 'I' AND deleted_at IS NOT NULL")
            .await,
        1,
        "die andere Zeile bleibt als Grabstein, nichts wird hart geloescht"
    );
    assert!(
        !skalar::<bool>(&db, "SELECT enabled FROM calendars WHERE id = 'K'").await,
        "K ist in Mitschnitt gepflegt und aus: das 'aktiviert' der Import-Zeile wird nicht uebernommen"
    );
}

#[tokio::test]
async fn termine_und_sitzungen_zeigen_danach_auf_den_behaltenen_kalender() {
    let db = live_lage().await;
    schritt_ausfuehren(&db).await;

    assert_eq!(
        skalar::<i64>(&db, "SELECT COUNT(*) FROM events WHERE calendar_id = 'I'").await,
        0
    );
    assert_eq!(skalar::<i64>(&db, "SELECT COUNT(*) FROM events").await, 4, "kein Termin geloescht");
    assert_eq!(
        skalar::<i64>(&db, "SELECT COUNT(*) FROM events WHERE deleted_at IS NULL").await,
        3,
        "t1 (K), t2, t3 leben weiter; der geloeschte Zwilling bleibt geloescht"
    );
    // t2 war nur an I: jetzt an K, Sitzung zeigt unveraendert auf ihn.
    assert_eq!(
        skalar::<String>(&db, "SELECT calendar_id FROM events WHERE id = 'i-t2'").await,
        "K"
    );
    assert_eq!(
        skalar::<String>(&db, "SELECT event_id FROM sessions WHERE id = 's-t2'").await,
        "i-t2"
    );
    // event_json.calendar_id folgt; Sitzung ohne gueltiges JSON bleibt ''.
    assert_eq!(
        skalar::<String>(
            &db,
            "SELECT json_extract(event_json, '$.calendar_id') FROM sessions WHERE id = 's-t2'"
        )
        .await,
        "K"
    );
    assert_eq!(
        skalar::<String>(&db, "SELECT event_json FROM sessions WHERE id = 's-ohne-json'").await,
        ""
    );
}

#[tokio::test]
async fn sitzung_am_geloeschten_zwilling_zeigt_danach_auf_den_lebenden() {
    let db = live_lage().await;
    schritt_ausfuehren(&db).await;

    assert_eq!(
        skalar::<String>(&db, "SELECT event_id FROM sessions WHERE id = 's-t1'").await,
        "k-t1",
        "derselbe Termin (gleicher Beginn als Zeitpunkt, +00:00 = Z): die Sitzung zeigt auf den lebenden"
    );
    assert_eq!(
        skalar::<i64>(
            &db,
            "SELECT COUNT(*) FROM sessions WHERE event_id <> '' AND event_id NOT IN (SELECT id FROM events)"
        )
        .await,
        0,
        "kein verwaister Verweis"
    );
    assert_eq!(skalar::<i64>(&db, "SELECT COUNT(*) FROM sessions").await, 3, "keine Sitzung geloescht");
}

#[tokio::test]
async fn zwei_lebende_zwillinge_behalten_den_mit_sitzung() {
    let db = bestand_vor_step().await;
    kalender(&db, "K", "cal-A", true, "2026-10-06T21:00:00Z").await;
    kalender(&db, "I", "cal-A", true, "2026-09-25T10:00:00Z").await;
    // Beide leben; die Sitzung haengt am Import-Termin, der aeltere updated_at hat.
    termin(&db, "k-x", "K", "x", "2026-10-07T09:00:00Z", None, "2026-10-06T21:00:00Z").await;
    termin(&db, "i-x", "I", "x", "2026-10-07T09:00:00Z", None, "2026-09-25T10:00:00Z").await;
    sitzung(&db, "s-x", "i-x", "").await;
    schritt_ausfuehren(&db).await;

    assert_eq!(
        skalar::<String>(&db, "SELECT id FROM events WHERE deleted_at IS NULL").await,
        "i-x",
        "der Termin mit Sitzung gewinnt vor dem juengeren"
    );
    assert_eq!(
        skalar::<String>(&db, "SELECT event_id FROM sessions WHERE id = 's-x'").await,
        "i-x"
    );
    assert_eq!(
        skalar::<i64>(&db, "SELECT COUNT(*) FROM events WHERE id = 'k-x' AND deleted_at IS NOT NULL")
            .await,
        1,
        "der andere wird Grabstein, nicht geloescht"
    );
}

#[tokio::test]
async fn verschobener_termin_ist_keine_dublette_aber_sitzung_folgt_dem_einzigen_lebenden() {
    let db = bestand_vor_step().await;
    kalender(&db, "K", "cal-A", true, "2026-10-06T21:00:00Z").await;
    kalender(&db, "I", "cal-A", false, "2026-09-25T10:00:00Z").await;
    termin(&db, "k-v", "K", "v", "2026-10-29T12:00:00Z", None, "2026-10-06T21:00:00Z").await;
    termin(
        &db,
        "i-v",
        "I",
        "v",
        "2026-10-01T11:00:00Z",
        Some("2026-09-17T12:00:00Z"),
        "2026-09-17T12:00:00Z",
    )
    .await;
    sitzung(&db, "s-v", "i-v", "").await;
    schritt_ausfuehren(&db).await;

    assert_eq!(
        skalar::<String>(&db, "SELECT event_id FROM sessions WHERE id = 's-v'").await,
        "k-v",
        "verschobener Termin: kein Zwilling, aber der einzige lebende gleicher Kennung ist der Rueckfall der Sitzung"
    );
    assert_eq!(skalar::<i64>(&db, "SELECT COUNT(*) FROM events WHERE deleted_at IS NULL").await, 1);
    assert_eq!(
        skalar::<i64>(&db, "SELECT COUNT(*) FROM events WHERE id = 'i-v' AND deleted_at IS NOT NULL").await,
        1,
        "der verschobene (geloeschte) Termin selbst bleibt unveraendert"
    );
}

#[tokio::test]
async fn nicht_dubletten_bleiben_unberuehrt_und_zweiter_lauf_aendert_nichts() {
    let db = live_lage().await;
    // Fremde Kennung, gleiche Provider-Daten; leere Kennung; anderer Anschluss.
    kalender(&db, "andere", "cal-B", true, "2026-10-01T10:00:00Z").await;
    kalender(&db, "leer1", "", true, "2026-10-01T10:00:00Z").await;
    kalender(&db, "leer2", "", true, "2026-10-02T10:00:00Z").await;
    sqlx::query(
        "INSERT INTO calendars (id, tracking_id_calendar, name, provider, connection_id)
         VALUES ('anderer-anschluss', 'cal-A', 'Privat', 'apple', 'conn-2')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    termin(&db, "a-1", "andere", "t1", "2026-10-07T09:00:00Z", None, "2026-10-01T10:00:00Z").await;

    schritt_ausfuehren(&db).await;

    for id in ["andere", "leer1", "leer2", "anderer-anschluss"] {
        assert_eq!(
            skalar::<i64>(
                &db,
                &format!("SELECT COUNT(*) FROM calendars WHERE id = '{id}' AND deleted_at IS NULL")
            )
            .await,
            1,
            "{id} ist keine Dublette"
        );
    }
    assert_eq!(
        skalar::<String>(&db, "SELECT calendar_id FROM events WHERE id = 'a-1'").await,
        "andere"
    );

    let nach_lauf_1 = stand(&db).await;
    schritt_ausfuehren(&db).await;
    assert_eq!(nach_lauf_1, stand(&db).await, "zweiter Lauf aendert nichts");
}

#[tokio::test]
async fn voller_lauf_ueber_den_migrationslaeufer_ist_gruen_und_idempotent() {
    let db = bestand_vor_step().await;
    kalender(&db, "K", "cal-A", true, "2026-10-06T21:00:00Z").await;
    kalender(&db, "I", "cal-A", true, "2026-09-25T10:00:00Z").await;
    anlg_db_migrate::migrate(&db, schema()).await.unwrap();
    anlg_db_migrate::migrate(&db, schema()).await.unwrap();
    assert_eq!(
        skalar::<i64>(&db, "SELECT COUNT(*) FROM calendars WHERE deleted_at IS NULL").await,
        1
    );
}

#[tokio::test]
async fn gewinner_ohne_teilnehmer_uebernimmt_die_des_zwillings() {
    let db = bestand_vor_step().await;
    kalender(&db, "K", "cal-A", true, "2026-10-06T21:00:00Z").await;
    kalender(&db, "I", "cal-A", true, "2026-09-25T10:00:00Z").await;
    termin(&db, "k-p", "K", "p", "2026-10-07T09:00:00Z", None, "2026-10-06T21:00:00Z").await;
    termin(&db, "i-p", "I", "p", "2026-10-07T09:00:00Z", None, "2026-09-25T10:00:00Z").await;
    termin(&db, "k-q", "K", "q", "2026-10-08T09:00:00Z", None, "2026-10-06T21:00:00Z").await;
    termin(&db, "i-q", "I", "q", "2026-10-08T09:00:00Z", None, "2026-09-25T10:00:00Z").await;
    sqlx::query("UPDATE events SET participants_json = '[{\"email\":\"a@example.com\"}]' WHERE id = 'i-p'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE events SET participants_json = '[{\"email\":\"neu@example.com\"}]' WHERE id = 'k-q'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE events SET participants_json = '[{\"email\":\"alt@example.com\"}]' WHERE id = 'i-q'")
        .execute(db.pool())
        .await
        .unwrap();
    schritt_ausfuehren(&db).await;

    assert_eq!(
        skalar::<String>(&db, "SELECT participants_json FROM events WHERE id = 'k-p'").await,
        "[{\"email\":\"a@example.com\"}]",
        "die Luecke wird gefuellt"
    );
    assert_eq!(
        skalar::<String>(&db, "SELECT participants_json FROM events WHERE id = 'k-q'").await,
        "[{\"email\":\"neu@example.com\"}]",
        "vorhandene Teilnehmer des Gewinners bleiben, keine Listen werden gemischt"
    );
}

#[tokio::test]
async fn geloeschte_sitzung_zaehlt_beim_rang_nicht() {
    let db = bestand_vor_step().await;
    kalender(&db, "K", "cal-A", true, "2026-10-06T21:00:00Z").await;
    kalender(&db, "I", "cal-A", true, "2026-09-25T10:00:00Z").await;
    termin(&db, "k-x", "K", "x", "2026-10-07T09:00:00Z", None, "2026-10-06T21:00:00Z").await;
    termin(&db, "i-x", "I", "x", "2026-10-07T09:00:00Z", None, "2026-09-25T10:00:00Z").await;
    sitzung(&db, "s-geloescht", "i-x", "").await;
    sqlx::query("UPDATE sessions SET deleted_at = '2026-09-26T10:00:00Z'")
        .execute(db.pool())
        .await
        .unwrap();
    schritt_ausfuehren(&db).await;

    assert_eq!(
        skalar::<String>(&db, "SELECT id FROM events WHERE deleted_at IS NULL").await,
        "k-x",
        "eine geloeschte Sitzung macht den aelteren Termin nicht zum Gewinner"
    );
}
