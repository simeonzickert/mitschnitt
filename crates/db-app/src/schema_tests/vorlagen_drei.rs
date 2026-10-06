//! Der Schnitt auf drei Vorlagen (Entscheid 06.10.2026, Runde 2).
//!
//! 'default-one-on-one-meeting' geht; 'default-client-kickoff' bekommt die
//! neue Abschnittsfolge, 'default-lecture-notes' Zitate und Aufgaben zuletzt.
//! Der Wahl-Step setzt „Auto“ und die entfernte Vorlage auf den Standard.

use super::*;
use anlg_db_core::Db;

const VORLAGEN_STEP_ID: &str = "20260912120000_vorlagen_drei";
const WAHL_STEP_ID: &str = "20260912120100_wahl_nach_vorlagen_drei";
const RUNDE1_ENDE_ID: &str = "20260911120100_wahl_nach_vorlagen_standard";

fn step(id: &str) -> &'static anlg_db_migrate::MigrationStep {
    APP_MIGRATION_STEPS
        .iter()
        .find(|step| step.id == id)
        .unwrap_or_else(|| panic!("Migrations-Step {id} ist nicht registriert"))
}

/// Bestand einer Installation auf dem Stand von Runde 1 (vier Vorlagen).
async fn bestand() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(&db, schema_through(RUNDE1_ENDE_ID))
        .await
        .unwrap();
    db
}

async fn ende(db: &Db) {
    anlg_db_migrate::migrate(db, schema_through(WAHL_STEP_ID))
        .await
        .unwrap();
}

async fn wahl_setzen(db: &Db, value_json: &str) {
    sqlx::query(
        "INSERT INTO app_settings (id, value_json, updated_at)
         VALUES ('selected_template_id', ?, '2026-08-31T07:11:39.375Z')
         ON CONFLICT(id) DO UPDATE SET value_json = excluded.value_json",
    )
    .bind(value_json)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn wahl(db: &Db) -> Option<String> {
    sqlx::query_scalar("SELECT value_json FROM app_settings WHERE id = 'selected_template_id'")
        .fetch_optional(db.pool())
        .await
        .unwrap()
}

const DREI: [&str; 3] = [
    "default-client-kickoff",
    "default-lecture-notes",
    "mitschnitt-kompakt",
];

const KICKOFF_FOLGE: [&str; 11] = [
    "TL;DR",
    "Projektüberblick",
    "Beteiligte und Rollen",
    "Zeitplan und Meilensteine",
    "Entscheidungen",
    "Erfolgskriterien",
    "Kommunikation",
    "Weitere Themen",
    "Aufgaben",
    "Offene Fragen",
    "Zitate",
];

const VORTRAG_FOLGE: [&str; 7] = [
    "TL;DR",
    "Kernaussagen",
    "Begriffe und Definitionen",
    "Beispiele und Anwendungen",
    "Offene Fragen",
    "Zitate",
    "Aufgaben",
];

const WEITERE_THEMEN: &str = "Nur Themen, die im Gespräch wirklich vorkamen und in keinen der Abschnitte oben passen: eine eigene Überschrift je Thema, benannt nach dem, worum es ging. Gibt es keine, entfällt dieser Teil ganz.";

async fn abschnitte(db: &Db, id: &str) -> Vec<(String, String)> {
    let json: String = sqlx::query_scalar("SELECT sections_json FROM templates WHERE id = ?")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let werte: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    werte
        .iter()
        .map(|w| {
            (
                w["title"].as_str().unwrap().to_string(),
                w["description"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn beschreibung<'a>(abschnitte: &'a [(String, String)], titel: &str) -> &'a str {
    abschnitte
        .iter()
        .find(|(t, _)| t == titel)
        .unwrap_or_else(|| panic!("Abschnitt {titel} fehlt"))
        .1
        .as_str()
}

async fn assert_drei_stehen(db: &Db) {
    assert_eq!(template_ids(db).await, DREI);

    let kompakt = abschnitte(db, "mitschnitt-kompakt").await;
    let kickoff = abschnitte(db, "default-client-kickoff").await;
    let vortrag = abschnitte(db, "default-lecture-notes").await;

    let titel = |a: &[(String, String)]| a.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();
    assert_eq!(titel(&kickoff), KICKOFF_FOLGE);
    assert_eq!(titel(&vortrag), VORTRAG_FOLGE);
    assert!(!titel(&vortrag).contains(&"Entscheidungen".to_string()));

    // Wortgleich mit dem Standard.
    for titel in ["Entscheidungen", "Zitate"] {
        assert_eq!(beschreibung(&kickoff, titel), beschreibung(&kompakt, titel), "{titel}");
    }
    assert_eq!(beschreibung(&vortrag, "Zitate"), beschreibung(&kompakt, "Zitate"));
    assert_eq!(beschreibung(&kickoff, "Weitere Themen"), WEITERE_THEMEN);
    assert!(beschreibung(&kickoff, "Aufgaben").contains("| Aufgabe | Wer | Bis |"));
    assert!(beschreibung(&vortrag, "Aufgaben").contains("| Aufgabe | Wer | Bis |"));

    let beschr: String =
        sqlx::query_scalar("SELECT description FROM templates WHERE id = 'default-client-kickoff'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(beschr.contains("Entscheidungen und Zusagen werden mitgeschrieben."));

    let gepinnt: Vec<String> =
        sqlx::query_scalar("SELECT id FROM templates WHERE pinned = 1 ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert_eq!(gepinnt, ["mitschnitt-kompakt"]);
}

#[tokio::test]
async fn frische_datenbank_traegt_genau_die_drei() {
    let db = Db::connect_memory_plain().await.unwrap();
    ende(&db).await;
    assert_drei_stehen(&db).await;
    assert_eq!(wahl(&db).await.as_deref(), Some("\"mitschnitt-kompakt\""));
}

#[tokio::test]
async fn bestand_nach_runde_eins_wird_umgebaut_und_importiertes_bleibt() {
    let db = bestand().await;
    assert_eq!(template_ids(&db).await.len(), 4, "Ausgangslage: Runde 1");
    sqlx::query(
        "INSERT INTO templates (id, title, description, category, targets_json, sections_json)
         VALUES ('3f2b6c1e-0000-4000-8000-000000000001', 'Meine Vorlage', 'x', 'Eigen', '[]', '[]')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    ende(&db).await;

    let mut erwartet: Vec<String> = DREI.iter().map(|s| s.to_string()).collect();
    erwartet.push("3f2b6c1e-0000-4000-8000-000000000001".into());
    erwartet.sort();
    assert_eq!(template_ids(&db).await, erwartet);
    let eigene: String = sqlx::query_scalar(
        "SELECT title FROM templates WHERE id = '3f2b6c1e-0000-4000-8000-000000000001'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(eigene, "Meine Vorlage");
}

#[tokio::test]
async fn bestand_ohne_fremde_zeilen_traegt_danach_die_drei() {
    let db = bestand().await;
    ende(&db).await;
    assert_drei_stehen(&db).await;
}

#[tokio::test]
async fn bearbeitete_vorlagen_ueberleben_unveraendert() {
    let db = bestand().await;
    sqlx::query("UPDATE templates SET description = 'Mein 1:1' WHERE id = 'default-one-on-one-meeting'")
        .execute(db.pool())
        .await
        .unwrap();
    let kickoff_vorher: String = {
        sqlx::query("UPDATE templates SET category = 'Eigen' WHERE id = 'default-client-kickoff'")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query_scalar("SELECT sections_json FROM templates WHERE id = 'default-client-kickoff'")
            .fetch_one(db.pool())
            .await
            .unwrap()
    };
    sqlx::query("UPDATE templates SET pinned = 1, pin_order = 2 WHERE id = 'default-lecture-notes'")
        .execute(db.pool())
        .await
        .unwrap();
    let vortrag_vorher = abschnitte(&db, "default-lecture-notes").await;

    ende(&db).await;

    assert!(template_ids(&db).await.contains(&"default-one-on-one-meeting".to_string()));
    let kickoff_nachher: String =
        sqlx::query_scalar("SELECT sections_json FROM templates WHERE id = 'default-client-kickoff'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(kickoff_nachher, kickoff_vorher, "geaenderte Kategorie schuetzt den Kickoff");
    assert_eq!(abschnitte(&db, "default-lecture-notes").await, vortrag_vorher);
}

#[tokio::test]
async fn jedes_einzelne_feld_schuetzt_die_zeile() {
    for (id, feld) in [
        ("default-one-on-one-meeting", "title = 'Mein 1:1'"),
        ("default-one-on-one-meeting", "sections_json = '[]'"),
        ("default-one-on-one-meeting", "targets_json = '[\"Ich\"]'"),
        ("default-one-on-one-meeting", "pinned = 1, pin_order = 0"),
        ("default-client-kickoff", "description = 'Mein Kickoff'"),
        ("default-client-kickoff", "targets_json = '[\"Ich\"]'"),
        ("default-lecture-notes", "title = 'Meine Talks'"),
        ("default-lecture-notes", "sections_json = '[]'"),
    ] {
        let db = bestand().await;
        let vorher: (String, String) = sqlx::query_as(&*format!(
            "SELECT description, sections_json FROM templates WHERE id = '{id}'"
        ).leak())
        .fetch_one(db.pool())
        .await
        .unwrap();
        sqlx::query(&*format!("UPDATE templates SET {feld} WHERE id = ?").leak())
            .bind(id)
            .execute(db.pool())
            .await
            .unwrap();
        let geaendert: (String, String) = sqlx::query_as(&*format!(
            "SELECT description, sections_json FROM templates WHERE id = '{id}'"
        ).leak())
        .fetch_one(db.pool())
        .await
        .unwrap();
        ende(&db).await;
        assert!(template_ids(&db).await.contains(&id.to_string()), "{id} {feld}");
        let nachher: (String, String) = sqlx::query_as(&*format!(
            "SELECT description, sections_json FROM templates WHERE id = '{id}'"
        ).leak())
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(nachher, geaendert, "{id} {feld}: bearbeitete Zeile bleibt 1:1");
        let _ = vorher;
    }
}

#[tokio::test]
async fn auto_und_entfernte_wahl_fallen_auf_den_standard() {
    for value in ["\"\"", "\"__auto__\"", "\"default-one-on-one-meeting\""] {
        let db = bestand().await;
        wahl_setzen(&db, value).await;
        ende(&db).await;
        assert_eq!(wahl(&db).await.as_deref(), Some("\"mitschnitt-kompakt\""), "{value}");
    }
}

#[tokio::test]
async fn andere_wahl_bleibt() {
    for value in [
        "\"default-lecture-notes\"",
        "\"default-client-kickoff\"",
        "\"3f2b6c1e-0000-4000-8000-000000000001\"",
        "\"mitschnitt-kompakt\"",
        "null",
    ] {
        let db = bestand().await;
        wahl_setzen(&db, value).await;
        ende(&db).await;
        assert_eq!(wahl(&db).await.as_deref(), Some(value), "{value}");
    }
}

#[tokio::test]
async fn wahl_der_bearbeiteten_einsvoneins_vorlage_bleibt() {
    let db = bestand().await;
    sqlx::query("UPDATE templates SET description = 'Mein 1:1' WHERE id = 'default-one-on-one-meeting'")
        .execute(db.pool())
        .await
        .unwrap();
    wahl_setzen(&db, "\"default-one-on-one-meeting\"").await;
    ende(&db).await;
    assert_eq!(wahl(&db).await.as_deref(), Some("\"default-one-on-one-meeting\""));
}

#[tokio::test]
async fn ohne_wahlzeile_entsteht_keine() {
    let db = bestand().await;
    sqlx::query("DELETE FROM app_settings WHERE id = 'selected_template_id'")
        .execute(db.pool())
        .await
        .unwrap();
    ende(&db).await;
    assert_eq!(wahl(&db).await, None);
}

#[tokio::test]
async fn zweiter_lauf_aendert_nichts() {
    let db = bestand().await;
    wahl_setzen(&db, "\"\"").await;
    ende(&db).await;
    let abbild = |db: &Db| {
        let pool = db.pool().clone();
        async move {
            let t: Vec<(String, String, String, String, bool, String)> = sqlx::query_as(
                "SELECT id, title, description, sections_json, pinned, updated_at FROM templates ORDER BY id",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            let s: Vec<(String, String, String)> =
                sqlx::query_as("SELECT id, value_json, updated_at FROM app_settings ORDER BY id")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            (t, s)
        }
    };
    let vorher = abbild(&db).await;
    for id in [VORLAGEN_STEP_ID, WAHL_STEP_ID] {
        sqlx::raw_sql(step(id).sql).execute(db.pool()).await.unwrap();
    }
    assert_eq!(abbild(&db).await, vorher);
}

#[tokio::test]
async fn nach_tabellen_reparatur_stehen_wieder_die_drei() {
    let db = Db::connect_memory_plain().await.unwrap();
    prepare_schema(&db).await.unwrap();
    sqlx::query("DROP TABLE templates").execute(db.pool()).await.unwrap();
    prepare_schema(&db).await.unwrap();
    assert_drei_stehen(&db).await;
}

#[tokio::test]
async fn der_vorlagen_step_fasst_keine_einstellung_an() {
    let sql = step(VORLAGEN_STEP_ID).sql;
    let ohne_kommentare: String = sql
        .lines()
        .filter(|l| !l.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!ohne_kommentare.contains("app_settings"));
    assert!(!ohne_kommentare.contains("icon_json"));
}
