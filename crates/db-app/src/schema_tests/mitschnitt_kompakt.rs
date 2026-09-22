//! Die neue Standard-Vorlage „Mitschnitt Kompakt".
//!
//! Sie ist das Ergebnis des Vergleichs dreier Fassungen an derselben echten
//! Sitzung (eine Zwei-Stunden-Aufnahme, 02.09.2026): die Substanz des Swift-Prompts plus der
//! Gegenpruef-Abschnitt aus 'mitschnitt-standard'. Zwei Steps, wie bei den
//! Nachbarn getrennt: der Seed fasst keine Einstellung an, der Standard-Step
//! keine Vorlage.
//!
//! Was diese Tests festhalten, und warum jeweils:
//!
//! - Die sieben Abschnitte in genau dieser Reihenfolge. `template_numbered`
//!   nummeriert sie und sagt dem Modell „use every section in order".
//! - Der Gegenpruef-Abschnitt traegt die Regel aus 'mitschnitt-standard'
//!   WORTGLEICH -- gepruefte, nicht behauptete Uebernahme. Der Abschnitt ist
//!   der einzige Grund, warum die alte Vorlage im Vergleich ueberhaupt gewann.
//! - Die neue Fristen-Regel ist da. Sie schliesst einen gemessenen Abstrich:
//!   die Tabellen-Fassung schrieb sechsmal „offen", wo im Gespraech „jetzt"
//!   oder „heute" gesagt wurde.
//! - Die alten Vorlagen bleiben vollstaendig erhalten -- der Rueckweg ist die
//!   halbe Zusage.
//! - Der Standard-Step ueberschreibt genau zwei Werte und keinen dritten.

use super::*;
use anlg_db_core::Db;

const VORLAGE_STEP_ID: &str = "20260904140000_mitschnitt_kompakt_vorlage";
const STANDARD_STEP_ID: &str = "20260904140100_mitschnitt_kompakt_als_standard";
/// Der letzte Step vor diesen beiden. Die Bestands-Simulation haengt daran;
/// wandert er, faehrt sie still eine fiktive Vorgeschichte.
const LETZTER_BESTANDS_STEP_ID: &str = "20260904120000_adaptive_minutes_vorlage";
const VORLAGE_ID: &str = "mitschnitt-kompakt";

/// Die Abschnitte der Vorlage nach dem Step
/// 20260907120000_bitte_gegenpruefen_in_den_rahmen. Bis dahin stand zwischen
/// "Offene Fragen" und "Zitate" noch "Zahlen und Namen zur Gegenpruefung";
/// der Abschnitt lebt seit dem 07.09.2026 als "Bitte gegenpruefen" im Rahmen
/// (enhance.system.md.jinja) und wird in schema_tests/bitte_gegenpruefen.rs
/// geprueft.
const ABSCHNITTE: [&str; 6] = [
    "TL;DR",
    "Entscheidungen",
    "Themen",
    "Aufgaben",
    "Offene Fragen",
    "Zitate",
];

fn step(id: &str) -> &'static anlg_db_migrate::MigrationStep {
    APP_MIGRATION_STEPS
        .iter()
        .find(|step| step.id == id)
        .unwrap_or_else(|| panic!("Migrations-Step {id} ist nicht registriert"))
}

/// Der Anweisungsteil eines Steps: ohne Kommentarzeilen und ohne alles, was
/// INNERHALB von Zeichenketten steht. Beides ist noetig -- die Kopfzeilen
/// erklaeren ausdruecklich, was ein Step NICHT tut (und nennen dabei Namen wie
/// `ai_language`), und die Regeltexte der Vorlage enthalten deutsche Woerter,
/// die gross geschrieben wie SQL aussehen.
fn anweisungen(sql: &str) -> String {
    sql.lines()
        .filter(|zeile| !zeile.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
        .split('\'')
        .step_by(2)
        .collect::<Vec<_>>()
        .join(" ")
}

async fn test_db() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    prepare_schema(&db).await.unwrap();
    db
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

/// Datenbank auf dem Stand VOR den beiden Steps.
async fn bestand() -> Db {
    let steps = migration_steps_before(VORLAGE_STEP_ID);
    assert_eq!(
        steps.last().map(|step| step.id),
        Some(LETZTER_BESTANDS_STEP_ID),
        "die Bestands-Simulation muss direkt vor dem Vorlagen-Step enden"
    );
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(&db, anlg_db_migrate::DbSchema { steps })
        .await
        .unwrap();
    db
}

/// Bestand mit einer gesetzten Wahl, danach die beiden neuen Steps -- genau
/// das, was ein Bestand beim ersten Start mit dieser Version erlebt.
///
/// UPSERT statt INSERT: der Bestand hat die Zeile schon, 20260902001100 hat
/// sie gesetzt. Ein blankes INSERT starb hier an der UNIQUE-Bedingung -- und
/// damit steht auch fest, dass die Ausgangslage echt ist.
async fn bestand_mit_wahl(value_json: &str) -> Db {
    let db = bestand().await;
    sqlx::query(
        "INSERT INTO app_settings (id, value_json, updated_at)
         VALUES ('selected_template_id', ?, '2026-08-31T07:11:39.375Z')
         ON CONFLICT(id) DO UPDATE
           SET value_json = excluded.value_json, updated_at = excluded.updated_at",
    )
    .bind(value_json)
    .execute(db.pool())
    .await
    .unwrap();
    anlg_db_migrate::migrate(&db, schema_through(STANDARD_STEP_ID))
        .await
        .unwrap();
    db
}

async fn abschnitte(db: &Db) -> Vec<(String, String)> {
    let row = get_template(db.pool(), VORLAGE_ID)
        .await
        .unwrap()
        .expect("Vorlage 'mitschnitt-kompakt' fehlt nach prepare_schema");
    let sections: Vec<serde_json::Value> = serde_json::from_str(&row.sections_json).unwrap();
    sections
        .iter()
        .map(|section| {
            (
                section["title"].as_str().unwrap().to_string(),
                section["description"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn frische_datenbank_traegt_die_vorlage_mit_genau_diesen_sieben_abschnitten() {
    let db = test_db().await;
    let row = get_template(db.pool(), VORLAGE_ID).await.unwrap().unwrap();

    assert_eq!(row.title, "Mitschnitt Kompakt");
    assert_eq!(row.category.as_deref(), Some("Mitschnitt"));
    assert!(!row.pinned);
    assert_eq!(row.pin_order, None);
    assert_eq!(
        row.icon_json, r##"{"type":"icon","value":"notebook-tabs","color":"#9ca3af"}"##,
        "kein eigenes Icon: der Spalten-Default gilt"
    );

    let titel = abschnitte(&db)
        .await
        .into_iter()
        .map(|(titel, _)| titel)
        .collect::<Vec<_>>();
    assert_eq!(titel, ABSCHNITTE, "sechs Abschnitte in dieser Reihenfolge");
}

/// Der Gegenpruef-Abschnitt stand bis zum 07.09.2026 in dieser Vorlage, und
/// dieser Test hat bewiesen, dass sein Wortlaut unveraendert aus
/// 'mitschnitt-standard' uebernommen war. Sein Gegenstand ist weg: der Step
/// 20260907120000 hat den Abschnitt aus allen sieben Vorlagen gestrichen,
/// weil er in einer Zwei-Stunden-Aufnahme 154 Woerter blosse Aufzaehlung
/// produzierte. An seine Stelle tritt "Bitte gegenpruefen" im Rahmen.
///
/// Was hier bleibt, ist die Umkehrung -- und die ist der eigentliche Wert:
/// KEINE Vorlage darf den Abschnitt zurueckholen, sonst stuende er doppelt,
/// weil der Rahmen seinen Nachfolger ohnehin anhaengt. Die volle Pruefung
/// ueber alle sieben steht in schema_tests/bitte_gegenpruefen.rs.
///
/// Wann wird er rot: sobald jemand den Abschnitt hier wieder eintraegt.
/// Welchen Fehler laesst er durch: einen unter anderem Namen eingefuegten
/// Abschnitt gleichen Inhalts.
#[tokio::test]
async fn die_vorlage_holt_den_gegenpruef_abschnitt_nicht_zurueck() {
    let db = test_db().await;

    assert!(
        !abschnitte(&db)
            .await
            .into_iter()
            .any(|(titel, _)| titel == "Zahlen und Namen zur Gegenprüfung"),
        "der Abschnitt ist seit dem 07.09.2026 im Rahmen und darf hier nicht          wieder stehen -- sonst steht er zweimal in derselben Zusammenfassung"
    );
}

/// Die Aufgaben-Tabelle ist das, was DIESE Vorlage ausmacht: drei Spalten,
/// immer dieselben. Die beiden Fristen-Regeln standen bis zum 04.09.2026 hier
/// daneben und sind mit dem Step 20260904160000 in den Rahmen gezogen -- der
/// gemessene Abstrich (sechsmal „offen", wo „jetzt" gesagt wurde) wird jetzt
/// von `template-app`, `die_fristen_regel_haelt_beide_haelften` gehalten, und
/// gilt damit fuer JEDE Vorlage statt nur fuer diese.
///
/// Die Umkehrung gehoert dazu: die Fristen-Regel darf hier NICHT mehr stehen,
/// sonst sind es wieder zwei Fassungen, die driften koennen.
#[tokio::test]
async fn die_aufgaben_regel_traegt_die_tabellen_spalten_und_keine_rahmen_regel() {
    let db = test_db().await;
    let regel = abschnitte(&db)
        .await
        .into_iter()
        .find(|(titel, _)| titel == "Aufgaben")
        .expect("Abschnitt 'Aufgaben'")
        .1;

    assert!(
        regel.contains("| Aufgabe | Wer | Bis |"),
        "die Spalten der Tabelle stehen in der Regel: {regel:?}"
    );
    assert!(
        regel.contains("Pipe-Zeichen"),
        "die Tabellen-Mechanik bleibt in der Vorlage, sie ist abschnittseigen: {regel:?}"
    );
    assert!(
        !regel.contains("rechne nie aus deinem eigenen Gefühl für heute"),
        "die Fristen-Regel steht jetzt im Rahmen und darf hier nicht doppelt \
         stehen: {regel:?}"
    );
}

/// Die Verdichtungs-Regel, und sie ist gemessen entstanden: der erste Lauf am
/// 04.09.2026 fiel in ELF Themen-Ueberschriften fuer ein 23-Minuten-
/// Zweiergespraech auseinander und kam auf 1582 Woerter. Mit dieser Regel
/// waren es 6 Ueberschriften und 1406. Faellt sie weg, kehrt die Zerfaserung
/// zurueck -- und Scanbarkeit ist die oberste Anforderung dieser Vorlage.
#[tokio::test]
async fn die_themen_regel_verdichtet_statt_zu_zerfasern() {
    let db = test_db().await;
    let regel = abschnitte(&db)
        .await
        .into_iter()
        .find(|(titel, _)| titel == "Themen")
        .expect("Abschnitt 'Themen'")
        .1;

    assert!(
        regel.contains("vier bis sechs tragende Überschriften"),
        "ohne Obergrenze zerfasert das Modell in ein Dutzend Ueberschriften: {regel:?}"
    );
    assert!(
        regel.contains("Randnotiz bekommt keine eigene Überschrift"),
        "die Randnotiz ist der teuerste Einzelfall -- zwei Zeilen unter eigener \
         Ueberschrift: {regel:?}"
    );
    assert!(
        regel.contains("nicht noch einmal ausgeschrieben"),
        "die Entscheidungen stehen oben und duerfen sich hier nicht doppeln: {regel:?}"
    );
}

/// Jeder Abschnitt traegt seine Regel, nicht nur einen Titel -- die Vorlagen-
/// Form gibt einem Abschnitt keinen anderen Platz dafuer.
///
/// Die Beschreibung dagegen ist seit dem 04.09.2026 KURZ: die drei harten
/// Regeln, die ueber allen Abschnitten stehen, haben mit dem Step
/// 20260904160000 einen eigenen Traeger bekommen (enhance.system.md.jinja,
/// "# Hard Rules"). Was hier bleibt, ist was diese Vorlage ausmacht. Der Test
/// prueft deshalb jetzt eine OBERGRENZE statt einer Untergrenze: kehrt der
/// Regeltext in die Beschreibung zurueck, sind es wieder zwei Fassungen.
#[tokio::test]
async fn jeder_abschnitt_traegt_seine_regel_und_die_beschreibung_bleibt_kurz() {
    let db = test_db().await;
    for (titel, regel) in abschnitte(&db).await {
        assert!(
            regel.chars().count() >= 60,
            "Abschnitt '{titel}' traegt keine Regel, sondern nur {} Zeichen: {regel:?}",
            regel.chars().count()
        );
    }

    let row = get_template(db.pool(), VORLAGE_ID).await.unwrap().unwrap();
    assert!(
        row.description.chars().count() <= 400,
        "die Beschreibung traegt die harten Regeln nicht mehr, hier sind es \
         {} Zeichen: {:?}",
        row.description.chars().count(),
        row.description
    );
    assert!(
        row.description.contains("Aufgaben als Tabelle mit Wer und Bis"),
        "die Beschreibung muss weiter sagen, was diese Vorlage ausmacht: {:?}",
        row.description
    );
}

/// „Sie ersetzt nichts" ist eine Zusage ueber das SQL, nicht ueber die Absicht.
#[test]
fn der_vorlagen_step_legt_nur_an_und_fasst_keine_einstellung_an() {
    let anweisungen = anweisungen(step(VORLAGE_STEP_ID).sql).to_uppercase();

    assert!(anweisungen.contains("INSERT OR IGNORE INTO TEMPLATES"));
    assert!(
        !anweisungen.contains("APP_SETTINGS"),
        "der Vorlagen-Seed darf keine Einstellung anfassen -- sonst duerfte der \
         Reparaturpfad ihn nicht nachspielen"
    );
    for verboten in ["UPDATE ", "DELETE ", "DROP ", "ALTER "] {
        assert!(
            !anweisungen.contains(verboten),
            "der Step darf nichts Bestehendes aendern, fand aber {verboten:?}"
        );
    }
}

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

    let standard = anweisungen(step(STANDARD_STEP_ID).sql);
    assert!(standard.contains("app_settings"));
    assert!(
        !standard.contains("INTO templates"),
        "der Standard-Step darf keine Vorlage schreiben"
    );
    assert!(
        !standard.contains("ai_language"),
        "die Sprache gehoert 20260902001100 und wird hier nicht angefasst"
    );
    assert!(
        !standard.to_uppercase().contains("DELETE "),
        "der Rueckweg haengt daran, dass keine Vorlage verschwindet"
    );
}

#[tokio::test]
async fn frische_datenbank_bekommt_kompakt_als_standard() {
    let db = test_db().await;

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"mitschnitt-kompakt\""),
        "value_json ist JSON: der String traegt seine Anfuehrungszeichen"
    );
    assert_eq!(
        assert_standard_vorlage_ist_aufloesbar(&db).await,
        VORLAGE_ID
    );
}

/// Der einzige Fall, in dem dieser Step eine bestehende Zeile ueberschreibt.
/// '"mitschnitt-standard"' kann nur aus unserem eigenen Seed 20260902001100
/// stammen oder aus der Wahl genau der Vorlage, die diese hier ersetzt.
#[tokio::test]
async fn der_alte_standard_wird_umgestellt() {
    let db = bestand_mit_wahl("\"mitschnitt-standard\"").await;

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"mitschnitt-kompakt\"")
    );
    assert_ne!(
        setting_updated_at(&db, "selected_template_id").await,
        "2026-08-31T07:11:39.375Z",
        "eine gesetzte Zeile traegt einen neuen Zeitstempel"
    );
}

#[tokio::test]
async fn spaltendefault_null_gilt_als_keine_wahl() {
    let db = bestand_mit_wahl("null").await;

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"mitschnitt-kompakt\"")
    );
}

#[tokio::test]
async fn fehlende_zeile_bekommt_den_standard() {
    let db = bestand().await;
    // Der Bestand HAT die Zeile (20260902001100 hat sie gesetzt) -- fuer den
    // INSERT-Pfad muss sie weg, sonst prueft dieser Test den UPDATE-Pfad und
    // waere eine Dublette von der_alte_standard_wird_umgestellt.
    sqlx::query("DELETE FROM app_settings WHERE id = 'selected_template_id'")
        .execute(db.pool())
        .await
        .unwrap();

    anlg_db_migrate::migrate(&db, schema_through(STANDARD_STEP_ID))
        .await
        .unwrap();

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"mitschnitt-kompakt\"")
    );
}

/// Jede ANDERE gewaehlte Vorlage bleibt unberuehrt -- sie kann nur aus einer
/// echten Wahl kommen. '""' ist die Abwahl („Auto"), auch eine Wahl.
#[tokio::test]
async fn jede_andere_wahl_bleibt_stehen() {
    for wahl in [
        "\"default-daily-standup\"",
        "\"mitschnitt-adaptive-minutes\"",
        "\"\"",
    ] {
        let db = bestand_mit_wahl(wahl).await;
        assert_eq!(
            setting(&db, "selected_template_id").await.as_deref(),
            Some(wahl),
            "die Wahl {wahl} darf der Step nicht ueberschreiben"
        );
        assert_eq!(
            setting_updated_at(&db, "selected_template_id").await,
            "2026-08-31T07:11:39.375Z",
            "eine unangetastete Zeile behaelt ihren Zeitstempel"
        );
    }
}

/// Der Rueckweg ist die halbe Zusage: „die neue kommt dazu". Ein Klick in der
/// Vorlagen-Liste muss reichen -- also muessen beide alten Vorlagen samt ihren
/// Abschnitten unveraendert dastehen.
///
/// Gemessen bis EINSCHLIESSLICH des Standard-Steps: das ist die Zusage DIESES
/// Steps. Dass der Nutzer 'mitschnitt-adaptive-minutes' am Abend desselben Tages
/// wieder abbestellt hat („alter schnitt raus", Step 20260904150000), ist eine
/// spaetere Entscheidung und wird in deren eigener Datei geprueft -- hier
/// stuende sonst eine Aussage ueber fremde Arbeit.
#[tokio::test]
async fn die_beiden_alten_vorlagen_bleiben_vollstaendig_erhalten() {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(&db, schema_through(STANDARD_STEP_ID))
        .await
        .unwrap();

    for (id, titel, anzahl) in [
        ("mitschnitt-standard", "Mitschnitt Standard", 5usize),
        (
            "mitschnitt-adaptive-minutes",
            "Adaptive Minutes (alter Schnitt)",
            6,
        ),
    ] {
        let row = get_template(db.pool(), id)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Vorlage '{id}' fehlt -- der Rueckweg ist zu"));
        assert_eq!(row.title, titel);
        let sections: Vec<serde_json::Value> = serde_json::from_str(&row.sections_json).unwrap();
        assert_eq!(sections.len(), anzahl, "Vorlage '{id}'");
    }
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
