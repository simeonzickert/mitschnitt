//! Die zusaetzliche Vorlage „Adaptive Minutes (alter Schnitt)".
//!
//! Sie holt den gehaerteten Zusammenfassungs-Prompt aus dem eingefrorenen
//! Swift-Vorgaenger zurueck (`ProtocolGenerator.protocolPrompt`) und ist
//! ausdruecklich ein Angebot, keine Umstellung: 'mitschnitt-standard' bleibt
//! Standard, keine Einstellung wird angefasst, keine bestehende Zeile
//! geaendert.
//!
//! Was diese Tests festhalten, und warum jeweils:
//!
//! - Die sechs Abschnitte in genau dieser Reihenfolge. Die Reihenfolge ist
//!   nicht Geschmack: `template_numbered` in `crates/template-app` nummeriert
//!   die Abschnitte und sagt dem Modell „use every section in order".
//! - Jeder Abschnitt traegt eine Regel, nicht nur einen Titel. Der Wert des
//!   alten Prompts steckt in den Regeln; ein Seed aus blossen Ueberschriften
//!   waere genau die stille Kuerzung, die dieser Lauf vermeiden sollte.
//! - Der Step fasst nichts an, was schon da ist. Kein `app_settings`, kein
//!   UPDATE, kein DELETE -- mechanisch geprueft am SQL, nicht versprochen.

use super::*;
use anlg_db_core::Db;

const STEP_ID: &str = "20260904120000_adaptive_minutes_vorlage";
const VORLAGE_ID: &str = "mitschnitt-adaptive-minutes";

/// Die Abschnitte des Swift-Prompts, in seiner Reihenfolge. „Diskussion" hiess
/// dort so und heisst hier „Themen"; der Prompt verbot ohnehin, den Titel
/// woertlich als Ueberschrift auszugeben, und die Vorlagen-Form gibt Titel aus.
const ABSCHNITTE: [&str; 6] = [
    "TL;DR",
    "Entscheidungen",
    "Themen",
    "Aufgaben",
    "Offene Fragen",
    "Zitate",
];

/// Bis EINSCHLIESSLICH dieses Steps, nicht bis zum Ende. Seit dem Aufraeumen
/// vom 04.09.2026 (20260904150000) entfernt ein SPAETERER Step diese Vorlage
/// wieder -- Entscheid: „alter schnitt raus". Der Seed bleibt trotzdem
/// registriert und wird weiter geprueft, aus zwei Gruenden: er ist der
/// Rueckweg (sein Wortlaut holt die Vorlage zurueck), und der Aufraeum-Step
/// vergleicht Feld fuer Feld GEGEN diesen Wortlaut -- driftet der Seed, trifft
/// das DELETE nicht mehr und tut still nichts.
async fn test_db() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(&db, schema_through(STEP_ID))
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
        .expect("Vorlage 'mitschnitt-adaptive-minutes' fehlt nach prepare_schema");

    assert_eq!(row.title, "Adaptive Minutes (alter Schnitt)");
    assert_eq!(row.category.as_deref(), Some("Mitschnitt"));
    assert!(!row.pinned);
    assert_eq!(row.pin_order, None);
    assert_eq!(
        row.icon_json, r##"{"type":"icon","value":"notebook-tabs","color":"#9ca3af"}"##,
        "kein eigenes Icon: der Spalten-Default gilt"
    );

    let sections: Vec<serde_json::Value> = serde_json::from_str(&row.sections_json).unwrap();
    let titel = sections
        .iter()
        .map(|section| section["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        titel, ABSCHNITTE,
        "die sechs Abschnitte des Swift-Prompts, in seiner Reihenfolge"
    );
}

/// Der Grund, warum diese Vorlage ueberhaupt zurueckgeholt wurde, sind die
/// Regeln -- nicht die Ueberschriften. Ein Abschnitt ohne Beschreibung waere
/// eine leere Huelse, und die Vorlagen-Form gibt einem Abschnitt keinen
/// anderen Platz fuer seine Regel.
#[tokio::test]
async fn jeder_abschnitt_traegt_seine_regel_und_nicht_nur_einen_titel() {
    let db = test_db().await;
    let row = get_template(db.pool(), VORLAGE_ID).await.unwrap().unwrap();

    let sections: Vec<serde_json::Value> = serde_json::from_str(&row.sections_json).unwrap();
    for section in &sections {
        let titel = section["title"].as_str().unwrap();
        let regel = section["description"]
            .as_str()
            .unwrap_or_else(|| panic!("Abschnitt '{titel}' hat gar keine Beschreibung"));
        assert!(
            regel.chars().count() >= 60,
            "Abschnitt '{titel}' traegt keine Regel, sondern nur {} Zeichen: {regel:?}",
            regel.chars().count()
        );
    }

    // Die drei Regeln, die im Swift-Prompt ueber ALLEN Abschnitten standen
    // („Hard rules"), haben in der Vorlagen-Form keinen eigenen Ort. Einziger
    // model-seitiger Freitext einer Vorlage ausserhalb der Abschnitte ist ihre
    // Beschreibung -- deshalb tragen sie sie, und deshalb ist sie lang.
    assert!(
        row.description.chars().count() >= 400,
        "die harten Regeln haben keinen anderen Traeger als die Beschreibung, \
         hier sind es nur {} Zeichen",
        row.description.chars().count()
    );
}

/// „Sie ersetzt nichts" ist eine Zusage ueber das SQL, nicht ueber die Absicht.
#[test]
fn der_step_legt_nur_an_und_aendert_nichts() {
    let sql = APP_MIGRATION_STEPS
        .iter()
        .find(|step| step.id == STEP_ID)
        .unwrap_or_else(|| panic!("Migrations-Step {STEP_ID} ist nicht registriert"))
        .sql;

    // Nur der Anweisungsteil zaehlt, und davon nur, was AUSSERHALB der
    // Zeichenketten steht. Beides ist noetig und beides hat sich sofort
    // bewiesen: die Kopfzeilen erklaeren ausdruecklich, was der Step nicht tut,
    // und der Titel der Vorlage heisst „(alter Schnitt)" -- gross geschrieben
    // ist das ein „ALTER ", und der erste Lauf dieses Tests fiel genau darueber.
    let ohne_kommentare = sql
        .lines()
        .filter(|zeile| !zeile.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    let anweisungen = ohne_kommentare
        .split('\'')
        .step_by(2)
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase();

    assert!(anweisungen.contains("INSERT OR IGNORE INTO TEMPLATES"));
    assert!(
        !anweisungen.contains("APP_SETTINGS"),
        "der Vorlagen-Seed darf keine Einstellung anfassen"
    );
    for verboten in ["UPDATE ", "DELETE ", "DROP ", "ALTER "] {
        assert!(
            !anweisungen.contains(verboten),
            "der Step darf nichts Bestehendes aendern, fand aber {verboten:?}"
        );
    }
}

/// Dieser Step laesst den Standard, wo er war. Gemessen bis EINSCHLIESSLICH
/// seiner selbst -- was ein SPAETERER Step tut, ist dessen Zusage und wird in
/// dessen Datei geprueft; hier stuende sonst eine Aussage ueber fremde Arbeit.
#[tokio::test]
async fn dieser_step_laesst_den_standard_unberuehrt() {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(&db, schema_through(STEP_ID))
        .await
        .unwrap();

    let gewaehlt: Option<String> =
        sqlx::query_scalar("SELECT value_json FROM app_settings WHERE id = 'selected_template_id'")
            .fetch_optional(db.pool())
            .await
            .unwrap();

    assert_eq!(gewaehlt.as_deref(), Some(r#""mitschnitt-standard""#));
}
