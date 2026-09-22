//! Die universelle Disziplin zieht in den Rahmen, die fuenf behaltenen
//! Upstream-Vorlagen werden deutsch (Entscheid 04.09.2026).
//!
//! Der Step 20260904160000 traegt drei Zusagen, und jede hat hier ihren Test:
//!
//! 1. KEINE Vorlage wiederholt eine Rahmen-Regel. Der Regeltext steht ab jetzt
//!    einmal in `crates/template-app/assets/enhance.system.md.jinja`; sieben
//!    Kopien in Vorlagen-Beschreibungen wuerden driften. Gemessen wird der
//!    ENDZUSTAND der Datenbank nach allen Migrationen -- nicht der Quelltext
//!    einer einzelnen Migration. Der Seed von 'mitschnitt-kompakt' ist
//!    eingefroren und traegt den alten Text weiter; was zaehlt, ist was am
//!    Ende in der Tabelle steht.
//! 2. Die fuenf tragen deutsche Titel, deutsche Abschnitte und das gemeinsame
//!    Rueckgrat (TL;DR, Aufgaben-Tabelle, offene Fragen).
//! 3. Geaendert wird nur der AUSLIEFERUNGSSTAND. Wer eine dieser Vorlagen
//!    bearbeitet hat, behaelt seine Fassung -- Feld fuer Feld gemessen, nie
//!    ueber Zeitstempel.
//!
//! Dazu der Reparaturpfad: er spielt die Seeds nach, und die sind INSERT OR
//! IGNORE. Ohne den Disziplin-Step am Ende von `replay_template_seeds` braechte
//! jede Tabellen-Reparatur die englischen Fassungen zurueck.

use super::*;
use anlg_db_core::Db;

const STEP_ID: &str = "20260904160000_disziplin_in_den_rahmen";

/// Die fuenf behaltenen Upstream-Vorlagen mit ihrem neuen deutschen Titel und
/// dem Abschnitt, der sie von den anderen vier unterscheidet.
const DIE_FUENF: [(&str, &str, &str); 5] = [
    ("default-one-on-one-meeting", "1:1-Gespräch", "Ziele und Fortschritt"),
    ("default-client-kickoff", "Kunden-Kickoff", "Erfolgskriterien"),
    ("default-lecture-notes", "Vorlesungsnotizen", "Begriffe und Definitionen"),
    ("default-sprint-planning", "Sprint-Planung", "Definition von fertig"),
    ("default-sprint-retrospective", "Sprint-Retrospektive", "Was wir gelernt haben"),
];

/// Das gemeinsame Rueckgrat, das jede der sieben Vorlagen traegt.
/// Der Abschnitt "Zahlen und Namen zur Gegenpruefung" stand hier bis zum
/// 07.09.2026 mit drin. Er ist seit dem Step
/// 20260907120000_bitte_gegenpruefen_in_den_rahmen aus allen sieben Vorlagen
/// gestrichen und lebt als "Bitte gegenpruefen" im Rahmen weiter -- deshalb
/// darf ihn dieser Test nicht mehr fordern.
const RUECKGRAT: [&str; 3] = ["TL;DR", "Aufgaben", "Offene Fragen"];

/// Merkmale der sieben Rahmen-Regeln, wie sie frueher in Vorlagen-Texten
/// standen. Keins davon darf nach dem Step noch in einer Vorlage stehen.
const RAHMEN_REGELN_IN_VORLAGEN: [&str; 6] = [
    "erfinde nichts",
    "ist weder Entscheidung noch Aufgabe",
    "Tiefe kommt aus der ANZAHL",
    "fällt ganz weg, statt kurz zu bleiben",
    "rechne nie aus deinem eigenen Gefühl für heute",
    "Hat niemand sie übernommen",
];

async fn frisch() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(&db, schema_through(STEP_ID))
        .await
        .unwrap();
    db
}

async fn vor_dem_step() -> Db {
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

async fn bis_zum_step(db: &Db) {
    anlg_db_migrate::migrate(db, schema_through(STEP_ID))
        .await
        .unwrap();
}

async fn vorlage(db: &Db, id: &str) -> (String, String, String) {
    sqlx::query_as("SELECT title, description, sections_json FROM templates WHERE id = ?")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap()
}

fn abschnittstitel(sections_json: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(sections_json)
        .expect("sections_json ist JSON")
        .as_array()
        .expect("sections_json ist ein Array")
        .iter()
        .map(|s| s["title"].as_str().expect("Abschnitt hat einen Titel").to_string())
        .collect()
}

/// DER tragende Test des Umzugs: nach allen Migrationen traegt KEINE Vorlage
/// mehr eine Rahmen-Regel in ihrem Text. Er liest den Endzustand der Tabelle,
/// nicht eine Migrationsdatei -- der eingefrorene kompakt-Seed traegt den
/// alten Wortlaut weiter, und genau deshalb waere ein Quelltext-Scan hier ein
/// Test, der die falsche Sache prueft.
///
/// Wann wird er rot: sobald jemand eine dieser Regeln wieder in eine
/// Beschreibung kopiert, oder wenn der Disziplin-Step nicht greift (etwa weil
/// eine Vorbedingung nicht mehr bytegenau zum Seed passt). Welchen Fehler
/// laesst er durch: eine sinngleiche Umformulierung -- er prueft woertliche
/// Wiederkehr, nicht Bedeutung.
#[tokio::test]
async fn keine_vorlage_wiederholt_eine_rahmen_regel() {
    let db = frisch().await;
    let zeilen: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, description, sections_json FROM templates ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap();

    assert_eq!(zeilen.len(), 7, "{zeilen:?}");
    for (id, description, sections_json) in zeilen {
        for regel in RAHMEN_REGELN_IN_VORLAGEN {
            assert!(
                !description.contains(regel),
                "Vorlage '{id}' wiederholt die Rahmen-Regel {regel:?} in ihrer \
                 Beschreibung -- sie gehoert nach enhance.system.md.jinja"
            );
            assert!(
                !sections_json.contains(regel),
                "Vorlage '{id}' wiederholt die Rahmen-Regel {regel:?} in einem \
                 Abschnitt -- sie gehoert nach enhance.system.md.jinja"
            );
        }
    }
}

/// Die fuenf sind deutsch und tragen das Rueckgrat -- Titel, Abschnittstitel
/// und ihren jeweils unterscheidenden Abschnitt.
#[tokio::test]
async fn die_fuenf_sind_deutsch_und_tragen_das_rueckgrat() {
    let db = frisch().await;

    for (id, titel, unterscheidend) in DIE_FUENF {
        let (ist_titel, _, sections_json) = vorlage(&db, id).await;
        assert_eq!(ist_titel, titel, "Titel von '{id}'");

        let titel_der_abschnitte = abschnittstitel(&sections_json);
        for pflicht in RUECKGRAT {
            assert!(
                titel_der_abschnitte.iter().any(|t| t == pflicht),
                "'{id}' fehlt der Rueckgrat-Abschnitt {pflicht:?}: {titel_der_abschnitte:?}"
            );
        }
        assert!(
            titel_der_abschnitte.iter().any(|t| t == unterscheidend),
            "'{id}' fehlt sein unterscheidender Abschnitt {unterscheidend:?}: \
             {titel_der_abschnitte:?}"
        );
        assert_eq!(
            titel_der_abschnitte.first().map(String::as_str),
            Some("TL;DR"),
            "das TL;DR steht bei '{id}' nicht oben: {titel_der_abschnitte:?}"
        );
        assert!(
            !sections_json.contains("Action Items")
                && !sections_json.contains("Wins & Challenges"),
            "'{id}' traegt noch englische Abschnitte"
        );
    }
}

/// Jede der sieben Vorlagen gibt ihre Aufgaben als Tabelle mit denselben drei
/// Spalten aus. Das war der Grund fuer den Umbau: ohne die Tabelle steht
/// hinterher nicht da, wer was bis wann macht.
#[tokio::test]
async fn jede_vorlage_hat_die_aufgaben_tabelle_mit_denselben_spalten() {
    let db = frisch().await;
    let zeilen: Vec<(String, String)> =
        sqlx::query_as("SELECT id, sections_json FROM templates ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap();

    let mut mit_tabelle = 0usize;
    for (id, sections_json) in zeilen {
        // 'mitschnitt-standard' ist der Rueckweg und bleibt bewusst
        // unangetastet; alle anderen sechs tragen die Tabelle.
        if id == "mitschnitt-standard" {
            continue;
        }
        assert!(
            sections_json.contains("| Aufgabe | Wer | Bis |"),
            "'{id}' hat keine Aufgaben-Tabelle mit den drei Spalten"
        );
        mit_tabelle += 1;
    }
    assert_eq!(mit_tabelle, 6, "sechs der sieben Vorlagen tragen die Tabelle");
}

/// Eine vom Nutzer bearbeitete Vorlage wird nicht ueberschrieben -- gemessen
/// am Inhalt, nie am Zeitstempel. Der Test aendert genau EIN Feld und erwartet,
/// dass die ganze Zeile stehen bleibt.
#[tokio::test]
async fn eine_bearbeitete_vorlage_bleibt_unangetastet() {
    for (id, _, _) in DIE_FUENF {
        let db = vor_dem_step().await;
        sqlx::query("UPDATE templates SET title = ? WHERE id = ?")
            .bind("Mein eigener Schnitt")
            .bind(id)
            .execute(db.pool())
            .await
            .unwrap();
        let vorher = vorlage(&db, id).await;

        bis_zum_step(&db).await;

        assert_eq!(
            vorlage(&db, id).await,
            vorher,
            "die bearbeitete Vorlage '{id}' wurde ueberschrieben"
        );
    }
}

/// Dasselbe fuer 'mitschnitt-kompakt': wer die Beschreibung umgeschrieben hat,
/// behaelt sie samt Abschnitten.
#[tokio::test]
async fn eine_bearbeitete_kompakt_vorlage_bleibt_unangetastet() {
    let db = vor_dem_step().await;
    sqlx::query("UPDATE templates SET description = ? WHERE id = 'mitschnitt-kompakt'")
        .bind("Meine Fassung.")
        .execute(db.pool())
        .await
        .unwrap();
    let vorher = vorlage(&db, "mitschnitt-kompakt").await;

    bis_zum_step(&db).await;

    assert_eq!(vorlage(&db, "mitschnitt-kompakt").await, vorher);
}

/// Anheften ist keine Textaenderung: wer eine Vorlage angeheftet, aber nichts
/// umgeschrieben hat, bekommt die bessere Fassung. Das ist der bewusste
/// Unterschied zum Aufraeum-Step, wo Anheften "die will ich behalten" heisst.
#[tokio::test]
async fn anheften_haelt_die_alte_fassung_nicht_fest() {
    let db = vor_dem_step().await;
    sqlx::query(
        "UPDATE templates SET pinned = 1, pin_order = 1 WHERE id = 'default-one-on-one-meeting'",
    )
    .execute(db.pool())
    .await
    .unwrap();

    bis_zum_step(&db).await;

    let (titel, _, _) = vorlage(&db, "default-one-on-one-meeting").await;
    assert_eq!(titel, "1:1-Gespräch");
    let (pinned, pin_order): (i64, Option<i64>) =
        sqlx::query_as("SELECT pinned, pin_order FROM templates WHERE id = 'default-one-on-one-meeting'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!((pinned, pin_order), (1, Some(1)), "der Pin bleibt");
}

/// Der Reparaturpfad spielt die Seeds nach, und die sind INSERT OR IGNORE --
/// sie greifen also GENAU dann, wenn die Zeile fehlt, also nach einer
/// Reparatur. Ohne den Disziplin-Step am Ende von `replay_template_seeds`
/// stuenden danach wieder englische Ueberschriften in der Liste.
#[tokio::test]
async fn nach_einer_reparatur_stehen_die_deutschen_fassungen() {
    let db = test_db().await;
    sqlx::query("DROP TABLE templates")
        .execute(db.pool())
        .await
        .unwrap();

    prepare_schema(&db).await.unwrap();

    for (id, titel, _) in DIE_FUENF {
        let (ist_titel, _, _) = vorlage(&db, id).await;
        assert_eq!(
            ist_titel, titel,
            "nach der Reparatur traegt '{id}' wieder den alten Titel"
        );
    }
    let (_, description, _) = vorlage(&db, "mitschnitt-kompakt").await;
    assert!(
        !description.contains("erfinde nichts"),
        "nach der Reparatur ist die ungetrimmte kompakt-Beschreibung zurueck: {description}"
    );
}
