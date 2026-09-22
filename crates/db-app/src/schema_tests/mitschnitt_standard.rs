//! F6: die Zusammenfassungs-Vorlage im eigenen Schnitt.
//!
//! Die Vorlage ist Daten, kein Code -- sie kommt per Migrations-Step in jede
//! Datenbank und wird in einem zweiten Step Standard, ohne eine bewusste Wahl
//! zu ueberstimmen. Diese Tests halten die Zusagen fest: sie existiert mit
//! genau diesen fuenf Abschnitten, die beiden Steps stehen getrennt am Ende,
//! der Standard wird nur dort gesetzt, wo niemand gewaehlt hat (auch die
//! Abwahl '""' ist eine Wahl), die Sprache nur dort, wo der Spalten-Default
//! 'null' steht -- eine FEHLENDE Sprachzeile bleibt fehlend, damit das
//! Frontend die Systemsprache eintraegt --, und ein zweiter Lauf desselben SQL
//! aendert nichts.

use super::*;
use anlg_db_core::Db;

const VORLAGE_STEP_ID: &str = "20260902001000_mitschnitt_standard_vorlage";
const STANDARD_STEP_ID: &str = "20260902001100_mitschnitt_standard_als_standard";
/// Der letzte Step, den ein Bestand vor den beiden Vorlagen-Steps kennt. Die
/// Simulation "Bestand, dann voller Lauf" haengt daran; wandert er, faehrt
/// sie still eine fiktive Vorgeschichte.
const LETZTER_BESTANDS_STEP_ID: &str = "20260901120000_drop_cloud_layer";
const VORLAGE_ID: &str = "mitschnitt-standard";
const ABSCHNITTE: [(&str, &str); 4] = [
    (
        "Kurzfassung",
        "Drei Sätze: worum ging es, was ist das Ergebnis, was passiert als Nächstes. Fließtext, keine Aufzählung.",
    ),
    (
        "Entscheidungen",
        "Was wurde entschieden, je ein Stichpunkt, mit dem Grund, wenn er genannt wurde. Nur echte Entscheidungen, keine Vorschläge oder Ideen.",
    ),
    (
        "Bälle",
        "Wer macht was bis wann. Ein Stichpunkt je Ball: Name · Aufgabe · Termin (oder ‚kein Termin genannt‘). Ohne klaren Verantwortlichen ist es kein Ball, sondern eine offene Frage.",
    ),
    (
        "Offene Fragen",
        "Was unentschieden oder unklar blieb, und wer die Antwort schuldet.",
    ),
];

fn step(id: &str) -> &'static anlg_db_migrate::MigrationStep {
    APP_MIGRATION_STEPS
        .iter()
        .find(|step| step.id == id)
        .unwrap_or_else(|| panic!("Migrations-Step {id} ist nicht registriert"))
}

/// Alle Steps vor der Vorlage -- verankert am letzten Bestands-Step.
fn bestands_steps() -> &'static [anlg_db_migrate::MigrationStep] {
    let steps = migration_steps_before(VORLAGE_STEP_ID);
    assert_eq!(
        steps.last().map(|step| step.id),
        Some(LETZTER_BESTANDS_STEP_ID),
        "die Bestands-Simulation muss direkt vor dem Vorlagen-Step enden"
    );
    steps
}

async fn setting(db: &Db, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value_json FROM app_settings WHERE id = ?")
        .bind(id)
        .fetch_optional(db.pool())
        .await
        .unwrap()
}

async fn setting_updated_at(db: &Db, id: &str) -> String {
    sqlx::query_scalar("SELECT updated_at FROM app_settings WHERE id = ?")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap()
}

/// Datenbank auf dem Stand VOR den beiden Steps, ohne Settings-Zeilen.
async fn bestand() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(
        &db,
        anlg_db_migrate::DbSchema {
            steps: bestands_steps(),
        },
    )
    .await
    .unwrap();
    db
}

/// Bestand mit einer vorhandenen Settings-Zeile, danach der volle Lauf --
/// genau das, was ein Bestand beim ersten Start mit dieser Version erlebt.
async fn bestand_mit_setting_nach_migration(id: &str, value_json: &str) -> Db {
    let db = bestand().await;
    sqlx::query(
        "INSERT INTO app_settings (id, value_json, updated_at)
         VALUES (?, ?, '2026-08-31T07:11:39.375Z')",
    )
    .bind(id)
    .bind(value_json)
    .execute(db.pool())
    .await
    .unwrap();
    // Bis EINSCHLIESSLICH des Standard-Steps, nicht bis zum Ende: Gegenstand
    // dieser Datei ist das Verhalten von 20260902001100. Ein spaeterer
    // Standard-Step (20260904140100) stellt bewusst um -- das ist SEINE
    // Zusage und wird in seiner eigenen Datei geprueft, nicht hier
    // umgeschrieben.
    anlg_db_migrate::migrate(&db, schema_through(STANDARD_STEP_ID))
        .await
        .unwrap();
    db
}

/// Frische Datenbank auf dem Stand direkt nach dem Standard-Step.
async fn frisch_bis_standard_step() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(&db, schema_through(STANDARD_STEP_ID))
        .await
        .unwrap();
    db
}

#[tokio::test]
async fn frische_datenbank_traegt_die_vorlage_mit_genau_diesen_abschnitten() {
    let db = test_db().await;

    let row = get_template(db.pool(), VORLAGE_ID)
        .await
        .unwrap()
        .expect("Vorlage 'mitschnitt-standard' fehlt nach prepare_schema");

    assert_eq!(row.title, "Mitschnitt Standard");
    assert_eq!(
        row.description,
        // Bis zum 07.09.2026 endete die Zeile auf ", Zahlen und Namen zur
        // Gegenpruefung." Der Abschnitt ist mit dem Step 20260907120000 in den
        // Rahmen gezogen, also zaehlt die Beschreibung ihn auch nicht mehr auf.
        "Eigener Schnitt: Kurzfassung, Entscheidungen, Bälle, offene Fragen."
    );
    assert_eq!(row.category.as_deref(), Some("Mitschnitt"));
    assert!(!row.pinned);
    assert_eq!(row.pin_order, None);
    assert_eq!(
        row.icon_json, r##"{"type":"icon","value":"notebook-tabs","color":"#9ca3af"}"##,
        "kein eigenes Icon: der Spalten-Default gilt"
    );

    let targets: Vec<String> = serde_json::from_str(row.targets_json.as_deref().unwrap()).unwrap();
    assert!(
        !targets.is_empty(),
        "targets_json muss ein String-Array sein"
    );

    let sections: Vec<serde_json::Value> = serde_json::from_str(&row.sections_json).unwrap();
    let sections = sections
        .iter()
        .map(|section| {
            (
                section["title"].as_str().unwrap().to_string(),
                section["description"].as_str().unwrap().to_string(),
            )
        })
        .collect::<Vec<_>>();
    let erwartet = ABSCHNITTE
        .iter()
        .map(|(title, description)| (title.to_string(), description.to_string()))
        .collect::<Vec<_>>();
    assert_eq!(sections, erwartet);
}

/// Zwei Steps, getrennt: der Vorlagen-Seed fasst keine Einstellung an, der
/// Standard-Step keine Vorlage. So kann der Reparaturpfad den Seed nachspielen,
/// ohne je eine Wahl zu ueberschreiben.
///
/// Die Zusage ist ihre Reihenfolge zueinander, nicht das Ende der Liste: seit
/// 20260902001200 (Willkommens-Titel) stehen Steps dahinter, und jeder weitere
/// kommt ebenfalls dahinter.
#[test]
fn die_beiden_steps_stehen_getrennt_und_in_dieser_reihenfolge() {
    let position = |id: &str| {
        APP_MIGRATION_STEPS
            .iter()
            .position(|step| step.id == id)
            .unwrap_or_else(|| panic!("Migrations-Step {id} ist nicht registriert"))
    };
    assert_eq!(
        position(STANDARD_STEP_ID),
        position(VORLAGE_STEP_ID) + 1,
        "der Standard-Step folgt unmittelbar auf den Vorlagen-Step"
    );

    let vorlage = step(VORLAGE_STEP_ID).sql;
    assert!(vorlage.contains("INSERT OR IGNORE INTO templates"));
    assert!(
        !vorlage.contains("app_settings"),
        "der Vorlagen-Seed darf keine Einstellung anfassen"
    );

    let standard = step(STANDARD_STEP_ID).sql;
    assert!(standard.contains("app_settings"));
    assert!(
        !standard.contains("INTO templates"),
        "der Standard-Step darf keine Vorlage schreiben"
    );
}

// C8 (Opus-Review der Vorlagen-Runde, 02.09.2026): ai_language wird NICHT
// mehr per INSERT gesetzt. Eine fehlende Zeile ist die Vorbedingung dafuer,
// dass initializeApplicationSettings (settings/queries.ts) die Systemsprache
// eintraegt -- und ai_language treibt auch die Oberflaechensprache
// (i18n/provider.tsx). Ein Seed "de" haette jeder Neuinstallation deutsche
// UI gegeben, unabhaengig vom System. Vorher stand hier Some("\"de\"").
#[tokio::test]
async fn frische_datenbank_setzt_die_vorlage_als_standard_und_laesst_die_sprache_dem_system() {
    let db = frisch_bis_standard_step().await;

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"mitschnitt-standard\""),
        "value_json ist JSON: der String traegt seine Anfuehrungszeichen"
    );
    assert_eq!(
        setting(&db, "ai_language").await,
        None,
        "eine fehlende Zeile bleibt fehlend -- das Frontend setzt die Systemsprache"
    );
}

/// dem gemessenen Bestand: keine Zeile selected_template_id, ai_language "de-DE".
#[tokio::test]
async fn bestand_ohne_wahl_bekommt_den_standard_und_behaelt_seine_sprache() {
    let db = bestand_mit_setting_nach_migration("ai_language", "\"de-DE\"").await;

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"mitschnitt-standard\""),
        "fehlende Zeile ist keine Wahl"
    );
    assert_eq!(
        setting(&db, "ai_language").await.as_deref(),
        Some("\"de-DE\"")
    );
    assert_eq!(
        setting_updated_at(&db, "ai_language").await,
        "2026-08-31T07:11:39.375Z",
        "eine unangetastete Zeile behaelt ihren Zeitstempel"
    );
}

#[tokio::test]
async fn bestand_ganz_ohne_settings_bekommt_den_standard_und_keine_sprache() {
    let db = bestand().await;
    anlg_db_migrate::migrate(&db, schema_through(STANDARD_STEP_ID))
        .await
        .unwrap();

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"mitschnitt-standard\"")
    );
    assert_eq!(setting(&db, "ai_language").await, None);
}

#[tokio::test]
async fn bewusste_vorlagenwahl_bleibt_stehen() {
    let db =
        bestand_mit_setting_nach_migration("selected_template_id", "\"default-daily-standup\"")
            .await;

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"default-daily-standup\"")
    );
    // Die Vorlage selbst kommt trotzdem an -- nur der Standard bleibt.
    assert!(get_template(db.pool(), VORLAGE_ID).await.unwrap().is_some());
}

/// '""' schreibt das Frontend, wenn der Nutzer den Standard abwaehlt
/// (templates/template-form.tsx): eine Wahl fuer "Auto", keine Nicht-Wahl.
#[tokio::test]
async fn abwahl_des_standards_bleibt_stehen() {
    let db = bestand_mit_setting_nach_migration("selected_template_id", "\"\"").await;

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"\""),
        "die bewusste Abwahl darf der Step nicht ueberschreiben"
    );
    assert_eq!(
        setting_updated_at(&db, "selected_template_id").await,
        "2026-08-31T07:11:39.375Z"
    );
}

/// 'null' ist der Spalten-Default von value_json -- nie bewusst geschrieben.
#[tokio::test]
async fn spaltendefault_null_gilt_als_keine_wahl() {
    let db = bestand_mit_setting_nach_migration("selected_template_id", "null").await;

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"mitschnitt-standard\"")
    );
    assert_ne!(
        setting_updated_at(&db, "selected_template_id").await,
        "2026-08-31T07:11:39.375Z",
        "eine gesetzte Zeile traegt einen neuen Zeitstempel"
    );
}

#[tokio::test]
async fn gespeicherte_sprache_bleibt_stehen() {
    for sprache in ["\"de-DE\"", "\"en\"", "\"\""] {
        let db = bestand_mit_setting_nach_migration("ai_language", sprache).await;
        assert_eq!(
            setting(&db, "ai_language").await.as_deref(),
            Some(sprache),
            "ai_language {sprache} darf der Step nicht anfassen"
        );
    }
}

#[tokio::test]
async fn sprache_spaltendefault_null_wird_deutsch() {
    let db = bestand_mit_setting_nach_migration("ai_language", "null").await;

    assert_eq!(setting(&db, "ai_language").await.as_deref(), Some("\"de\""));
}

/// Der Reparaturpfad in prepare_schema baut eine verschwundene templates-Tabelle
/// wieder auf und spielt die 17 Upstream-Seeds nach. Blieb unsere Vorlage dabei
/// aussen vor, zeigte selected_template_id ins Leere -- und die App fiele
/// still auf "Auto" zurueck (loadTemplate liefert null, keine Meldung).
#[tokio::test]
async fn nach_tabellen_reparatur_zeigt_der_standard_nicht_ins_leere() {
    let db = test_db().await;
    sqlx::query("DROP TABLE templates")
        .execute(db.pool())
        .await
        .unwrap();

    prepare_schema(&db).await.unwrap();

    assert!(
        get_template(db.pool(), VORLAGE_ID).await.unwrap().is_some(),
        "Vorlage 'mitschnitt-standard' fehlt nach der Reparatur"
    );
    assert_standard_vorlage_ist_aufloesbar(&db).await;
    assert_vorlagenbestand_steht(&db).await;
}

/// Opus-Fund: fehlt die Tabelle, waehrend der Vorlagen-Step noch aussteht,
/// starb migrate an "no such table: templates" -- die Reparatur dahinter kam
/// nie dran. Jetzt legt prepare_schema die Tabelle VOR migrate an; der Icon-
/// Step (schon verbucht) kommt ueber den has_icon_json-Check der Reparatur.
#[tokio::test]
async fn ohne_tabelle_und_mit_ausstehendem_step_laeuft_prepare_schema_durch() {
    let db = bestand().await;
    sqlx::query("DROP TABLE templates")
        .execute(db.pool())
        .await
        .unwrap();

    prepare_schema(&db)
        .await
        .expect("prepare_schema muss auch ohne templates-Tabelle durchlaufen");

    assert_vorlagenbestand_steht(&db).await;
    assert_standard_vorlage_ist_aufloesbar(&db).await;
    let has_icon_json: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('templates') WHERE name = 'icon_json')",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(has_icon_json, "icon_json fehlt nach der Reparatur");
}

#[tokio::test]
async fn zweiter_lauf_desselben_sql_aendert_nichts() {
    let db = test_db().await;

    let abbild = |db: &Db| {
        let pool = db.pool().clone();
        async move {
            let templates: Vec<(String, String, String)> =
                sqlx::query_as("SELECT id, sections_json, updated_at FROM templates ORDER BY id")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            let settings: Vec<(String, String, String)> =
                sqlx::query_as("SELECT id, value_json, updated_at FROM app_settings ORDER BY id")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            (templates, settings)
        }
    };

    let vorher = abbild(&db).await;
    for id in [VORLAGE_STEP_ID, STANDARD_STEP_ID] {
        sqlx::raw_sql(step(id).sql)
            .execute(db.pool())
            .await
            .unwrap_or_else(|error| panic!("{id} muss ein zweites Mal durchlaufen: {error}"));
    }
    let nachher = abbild(&db).await;

    assert_eq!(vorher.0.len(), 7, "keine Duplikate in templates");
    assert_eq!(vorher, nachher);
}
