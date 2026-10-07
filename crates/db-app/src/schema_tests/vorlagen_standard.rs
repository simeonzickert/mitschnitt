//! Der Schnitt auf vier Vorlagen (Entscheid 06.10.2026).
//!
//! 'mitschnitt-kompakt' heisst „Standard“, steht gepinnt oben und bekommt
//! einen neuen Zitate-Abschnitt; 'default-lecture-notes' heisst „Vortrag &
//! Schulung“; 'mitschnitt-standard' und die zwei Sprint-Vorlagen gehen.
//! Der Wahl-Step setzt eine auf Entferntes zeigende Auswahl auf den Standard.

use super::*;
use anlg_db_core::Db;

const VORLAGEN_STEP_ID: &str = "20260911120000_vorlagen_standard";
const WAHL_STEP_ID: &str = "20260911120100_wahl_nach_vorlagen_standard";
const DAVOR_STEP_ID: &str = "20260910130000_import_run_session_counters";

fn step(id: &str) -> &'static anlg_db_migrate::MigrationStep {
    APP_MIGRATION_STEPS
        .iter()
        .find(|step| step.id == id)
        .unwrap_or_else(|| panic!("Migrations-Step {id} ist nicht registriert"))
}

/// Bestand einer Alt-Installation: alles bis direkt vor dem neuen Step.
async fn bestand() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(&db, schema_through(DAVOR_STEP_ID))
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

async fn titel(db: &Db, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT title FROM templates WHERE id = ?")
        .bind(id)
        .fetch_optional(db.pool())
        .await
        .unwrap()
}

const VIER: [&str; 4] = [
    "default-client-kickoff",
    "default-lecture-notes",
    "default-one-on-one-meeting",
    "mitschnitt-kompakt",
];

const ZITATE_NEU: &str = "Höchstens drei wörtliche Aussagen, und nur solche, die beim Lesen etwas hinzufügen, was die Zusammenfassung nicht schon sagt: eine Haltung, eine Zusage oder eine Zahl in den eigenen Worten des Sprechers, jeweils mit dem Sprecher, aber nur bei eindeutiger Zuordnung. Gibt es keine solche Aussage, entfällt der Abschnitt ganz, ohne Überschrift und ohne Füll-Zitate. Ist die Formulierung im Transkript verstümmelt, lass das Zitat weg, statt es zu reparieren.";

async fn assert_vier_stehen(db: &Db) {
    assert_eq!(template_ids(db).await, VIER);

    let titel_der = |id: &'static str| async move { titel(db, id).await.unwrap() };
    assert_eq!(titel_der("mitschnitt-kompakt").await, "Standard");
    assert_eq!(titel_der("default-one-on-one-meeting").await, "1:1-Gespräch");
    assert_eq!(titel_der("default-client-kickoff").await, "Kunden-Kickoff");
    assert_eq!(titel_der("default-lecture-notes").await, "Vortrag & Schulung");

    let (pinned, pin_order, description, sections): (bool, Option<i64>, String, String) =
        sqlx::query_as(
            "SELECT pinned, pin_order, description, sections_json FROM templates
             WHERE id = 'mitschnitt-kompakt'",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(pinned, "Standard muss gepinnt sein");
    assert_eq!(pin_order, Some(0));
    assert!(description.contains("Voreinstellung"));
    assert!(description.contains("fast jedes Gespräch"));
    let sections: Vec<serde_json::Value> = serde_json::from_str(&sections).unwrap();
    let zitate = sections.last().unwrap();
    assert_eq!(zitate["title"], "Zitate");
    assert_eq!(zitate["description"], ZITATE_NEU);
    assert_eq!(sections.len(), 6);

    // Die anderen drei sind nicht gepinnt: Standard steht allein oben.
    let gepinnt: Vec<String> =
        sqlx::query_scalar("SELECT id FROM templates WHERE pinned = 1 ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert_eq!(gepinnt, ["mitschnitt-kompakt"]);
}

#[tokio::test]
async fn frische_datenbank_traegt_genau_die_vier() {
    let db = Db::connect_memory_plain().await.unwrap();
    ende(&db).await;
    assert_vier_stehen(&db).await;
    assert_eq!(wahl(&db).await.as_deref(), Some("\"mitschnitt-kompakt\""));
}

#[tokio::test]
async fn bestehende_datenbank_raeumt_auf_und_laesst_importierte_stehen() {
    let db = bestand().await;
    assert_eq!(
        template_ids(&db).await.len(),
        7,
        "Ausgangslage: die sieben vom 04.09."
    );
    sqlx::query(
        "INSERT INTO templates (id, title, description, category, targets_json, sections_json)
         VALUES ('3f2b6c1e-0000-4000-8000-000000000001', 'Meine Vorlage', 'x', 'Eigen', '[]', '[]')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    wahl_setzen(&db, "\"mitschnitt-standard\"").await;

    ende(&db).await;

    let mut erwartet: Vec<String> = VIER.iter().map(|s| s.to_string()).collect();
    erwartet.push("3f2b6c1e-0000-4000-8000-000000000001".into());
    erwartet.sort();
    assert_eq!(template_ids(&db).await, erwartet);
    assert_eq!(
        titel(&db, "3f2b6c1e-0000-4000-8000-000000000001").await.as_deref(),
        Some("Meine Vorlage")
    );
    assert_eq!(titel(&db, "mitschnitt-kompakt").await.as_deref(), Some("Standard"));
    assert_eq!(wahl(&db).await.as_deref(), Some("\"mitschnitt-kompakt\""));
}

#[tokio::test]
async fn jede_entfernte_wahl_faellt_auf_den_standard() {
    for id in [
        "mitschnitt-standard",
        "default-sprint-planning",
        "default-sprint-retrospective",
    ] {
        let db = bestand().await;
        wahl_setzen(&db, &format!("\"{id}\"")).await;
        ende(&db).await;
        assert_eq!(wahl(&db).await.as_deref(), Some("\"mitschnitt-kompakt\""), "{id}");
    }
}

#[tokio::test]
async fn andere_wahl_bleibt() {
    for value in ["\"default-lecture-notes\"", "\"\"", "\"3f2b6c1e-0000-4000-8000-000000000001\"", "\"mitschnitt-kompakt\""] {
        let db = bestand().await;
        wahl_setzen(&db, value).await;
        ende(&db).await;
        assert_eq!(wahl(&db).await.as_deref(), Some(value));
    }
}

#[tokio::test]
async fn bearbeitete_vorlagen_ueberleben() {
    let db = bestand().await;
    // Sprint-Planung umgeschrieben, Standard-Rueckweg angeheftet,
    // Kompakt-Beschreibung vom Nutzer geaendert.
    sqlx::query("UPDATE templates SET sections_json = '[]' WHERE id = 'default-sprint-planning'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE templates SET pinned = 1, pin_order = 3 WHERE id = 'mitschnitt-standard'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE templates SET description = 'Meine' WHERE id = 'mitschnitt-kompakt'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE templates SET description = 'Meine Talks' WHERE id = 'default-lecture-notes'")
        .execute(db.pool())
        .await
        .unwrap();
    wahl_setzen(&db, "\"mitschnitt-standard\"").await;

    ende(&db).await;

    let ids = template_ids(&db).await;
    assert!(ids.contains(&"default-sprint-planning".to_string()));
    assert!(ids.contains(&"mitschnitt-standard".to_string()));
    assert!(!ids.contains(&"default-sprint-retrospective".to_string()));
    // Bearbeitete Texte bleiben; Titel/Pin gelten trotzdem.
    let d: String = sqlx::query_scalar("SELECT description FROM templates WHERE id = 'mitschnitt-kompakt'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(d, "Meine");
    assert_eq!(titel(&db, "default-lecture-notes").await.as_deref(), Some("Vorlesungsnotizen"));
    // Die Vorlage der Wahl steht noch: die Wahl bleibt.
    assert_eq!(wahl(&db).await.as_deref(), Some("\"mitschnitt-standard\""));
}

#[tokio::test]
async fn zweiter_lauf_aendert_nichts() {
    let db = Db::connect_memory_plain().await.unwrap();
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
async fn nach_tabellen_reparatur_steht_der_standard_wie_in_runde_eins() {
    let db = Db::connect_memory_plain().await.unwrap();
    prepare_schema(&db).await.unwrap();
    sqlx::query("DROP TABLE templates").execute(db.pool()).await.unwrap();
    prepare_schema(&db).await.unwrap();
    // Seit Runde 2 (20260912120000) sind es nur noch drei; das 1:1 fehlt.
    assert_eq!(
        template_ids(&db).await,
        ["default-client-kickoff", "default-lecture-notes", "mitschnitt-kompakt"]
    );
    assert_eq!(titel(&db, "mitschnitt-kompakt").await.as_deref(), Some("Standard"));
    assert_eq!(titel(&db, "default-lecture-notes").await.as_deref(), Some("Vortrag & Schulung"));
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

/// Bestands-DBs aus der Zeit vor dem Namens-Scrub (21.09.) tragen eine andere
/// Beschreibung vor " Schnitt: ..." -- die Zeile muss trotzdem weg.
#[tokio::test]
async fn alt_fassung_der_standard_beschreibung_wird_trotzdem_entfernt() {
    let db = bestand().await;
    sqlx::query(
        "UPDATE templates SET description = 'Testers Schnitt: Kurzfassung, Entscheidungen, Bälle, offene Fragen.'
         WHERE id = 'mitschnitt-standard'",
    )
    .execute(db.pool())
    .await
    .unwrap();
    wahl_setzen(&db, "\"mitschnitt-standard\"").await;

    ende(&db).await;

    assert_eq!(template_ids(&db).await, VIER);
    assert_eq!(wahl(&db).await.as_deref(), Some("\"mitschnitt-kompakt\""));
}

#[tokio::test]
async fn fremde_beschreibung_der_standard_zeile_bleibt() {
    let db = bestand().await;
    sqlx::query("UPDATE templates SET description = 'Ganz anders.' WHERE id = 'mitschnitt-standard'")
        .execute(db.pool())
        .await
        .unwrap();
    ende(&db).await;
    assert!(template_ids(&db).await.contains(&"mitschnitt-standard".to_string()));
}

#[tokio::test]
async fn geaenderte_kategorie_oder_zielgruppen_schuetzen_die_zeile() {
    for (id, feld) in [
        ("default-sprint-planning", "category = 'Eigen'"),
        ("default-sprint-retrospective", "targets_json = '[\"Ich\"]'"),
        ("mitschnitt-standard", "category = 'Eigen'"),
    ] {
        let db = bestand().await;
        sqlx::query(&*format!("UPDATE templates SET {feld} WHERE id = ?").leak())
            .bind(id)
            .execute(db.pool())
            .await
            .unwrap();
        ende(&db).await;
        assert!(
            template_ids(&db).await.contains(&id.to_string()),
            "{id} mit geaendertem Feld muss bleiben"
        );
    }
}
