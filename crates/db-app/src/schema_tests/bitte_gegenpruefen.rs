//! Aus "Zahlen und Namen zur Gegenpruefung" wird "Bitte gegenpruefen", und der
//! Abschnitt zieht aus sieben Vorlagen in den Rahmen (Entscheid 07.09.2026).
//!
//! Der Step 20260907120000 traegt drei Zusagen, und jede hat hier ihren Test:
//!
//! 1. KEINE Vorlage listet den Abschnitt mehr -- weder als Abschnittstitel
//!    noch in ihrer Beschreibung. Gemessen wird der ENDZUSTAND der Tabelle
//!    nach allen Migrationen, nicht der Quelltext einer Migration: die Seeds
//!    sind eingefroren und tragen den alten Wortlaut weiter, ein
//!    Quelltext-Scan wuerde also die falsche Sache pruefen.
//! 2. Alles ANDERE an den sieben Vorlagen bleibt, wie es war. Ein Step, der
//!    einen Abschnitt streicht, darf nicht nebenbei einen zweiten mitnehmen.
//! 3. Geaendert wird nur der AUSLIEFERUNGSSTAND. Wer eine Vorlage bearbeitet
//!    hat, behaelt seine Fassung -- Feld fuer Feld gemessen, nie ueber
//!    Zeitstempel.
//!
//! Dazu der Reparaturpfad: er spielt die Seeds nach, und die sind INSERT OR
//! IGNORE. Ohne diesen Step am Ende von `replay_template_seeds` stuende nach
//! jeder Tabellen-Reparatur der gestrichene Abschnitt wieder in der Liste --
//! und damit doppelt, weil der Rahmen seinen Nachfolger ohnehin anhaengt.

use super::*;
use anlg_db_core::Db;

const STEP_ID: &str = "20260907120000_bitte_gegenpruefen_in_den_rahmen";

/// Der gestrichene Abschnittstitel, wortgleich wie er in allen sieben
/// Vorlagen stand.
const GESTRICHEN: &str = "Zahlen und Namen zur Gegenprüfung";

/// Die sieben Vorlagen und die Zahl ihrer Abschnitte NACH dem Step. Vorher
/// war es jeweils eine mehr; die Zahl steht hier ausgeschrieben, damit ein
/// Step, der versehentlich zwei Abschnitte nimmt, auffliegt statt
/// durchzurutschen.
const NACH_DEM_STEP: [(&str, usize); 7] = [
    ("default-client-kickoff", 8),
    ("default-lecture-notes", 6),
    ("default-one-on-one-meeting", 7),
    ("default-sprint-planning", 8),
    ("default-sprint-retrospective", 6),
    ("mitschnitt-kompakt", 6),
    ("mitschnitt-standard", 4),
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
        .map(|s| {
            s["title"]
                .as_str()
                .expect("Abschnitt hat einen Titel")
                .to_string()
        })
        .collect()
}

/// DER tragende Test: nach allen Migrationen listet keine der sieben Vorlagen
/// den Abschnitt mehr, weder als Titel noch in der Beschreibung.
///
/// Wann wird er rot: sobald der Step nicht greift (etwa weil eine
/// Vorbedingung nicht mehr bytegenau zum Seed passt), oder sobald jemand den
/// Abschnitt in eine Vorlage zurueckkopiert. Welchen Fehler laesst er durch:
/// eine sinngleiche Umbenennung -- er prueft den Wortlaut, nicht die Absicht.
#[tokio::test]
async fn keine_vorlage_listet_die_gegenpruef_liste_noch() {
    let db = frisch().await;
    let zeilen: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, description, sections_json FROM templates ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap();

    assert_eq!(zeilen.len(), 7, "{zeilen:?}");
    for (id, description, sections_json) in zeilen {
        assert!(
            !abschnittstitel(&sections_json)
                .iter()
                .any(|t| t == GESTRICHEN),
            "Vorlage '{id}' listet {GESTRICHEN:?} noch als Abschnitt -- der \
             Abschnitt lebt seit dem 07.09.2026 als 'Bitte gegenpruefen' im \
             Rahmen (enhance.system.md.jinja)"
        );
        assert!(
            !description.contains(GESTRICHEN),
            "Vorlage '{id}' nennt {GESTRICHEN:?} noch in ihrer Beschreibung: \
             {description}"
        );
    }
}

/// Vor dem Step trugen ihn ALLE sieben. Ohne diesen Test koennte der Step
/// spurlos nichts tun und der Test darueber trotzdem gruen sein.
#[tokio::test]
async fn vor_dem_step_trugen_ihn_alle_sieben() {
    let db = vor_dem_step().await;
    let zeilen: Vec<(String, String)> =
        sqlx::query_as("SELECT id, sections_json FROM templates ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap();

    assert_eq!(zeilen.len(), 7, "{zeilen:?}");
    for (id, sections_json) in zeilen {
        assert!(
            abschnittstitel(&sections_json)
                .iter()
                .any(|t| t == GESTRICHEN),
            "'{id}' trug den Abschnitt schon vorher nicht -- dann misst der \
             Step nebenan nichts"
        );
    }
}

/// Der Step nimmt GENAU einen Abschnitt je Vorlage, nicht zwei, und laesst die
/// uebrigen in ihrer Reihenfolge stehen.
#[tokio::test]
async fn genau_ein_abschnitt_faellt_weg_und_die_reihenfolge_bleibt() {
    let vorher = vor_dem_step().await;
    let nachher = frisch().await;

    for (id, erwartet) in NACH_DEM_STEP {
        let (_, _, sj_vorher) = vorlage(&vorher, id).await;
        let (_, _, sj_nachher) = vorlage(&nachher, id).await;
        let alt = abschnittstitel(&sj_vorher);
        let neu = abschnittstitel(&sj_nachher);

        assert_eq!(neu.len(), erwartet, "Abschnittszahl von '{id}': {neu:?}");
        assert_eq!(
            alt.len(),
            erwartet + 1,
            "vor dem Step hatte '{id}' nicht einen Abschnitt mehr: {alt:?}"
        );
        assert_eq!(
            neu,
            alt.iter()
                .filter(|t| *t != GESTRICHEN)
                .cloned()
                .collect::<Vec<_>>(),
            "'{id}' hat mehr verloren als den einen Abschnitt, oder die \
             Reihenfolge hat sich verschoben"
        );
    }
}

/// Die Beschreibungen der beiden forkeigenen Vorlagen zaehlen ihre Abschnitte
/// auf und muessen deshalb mitgezogen werden. Die fuenf Upstream-Vorlagen
/// beschreiben ihren Schwerpunkt statt ihrer Abschnitte und bleiben
/// unberuehrt -- auch das ist eine Zusage des Steps.
#[tokio::test]
async fn nur_die_beiden_zaehlenden_beschreibungen_aendern_sich() {
    let vorher = vor_dem_step().await;
    let nachher = frisch().await;

    for (id, _) in NACH_DEM_STEP {
        let (_, alt, _) = vorlage(&vorher, id).await;
        let (_, neu, _) = vorlage(&nachher, id).await;
        let zaehlt_abschnitte = id.starts_with("mitschnitt-");
        assert_eq!(
            alt != neu,
            zaehlt_abschnitte,
            "Beschreibung von '{id}': erwartet geaendert = {zaehlt_abschnitte}\n\
             alt: {alt}\nneu: {neu}"
        );
    }

    let (_, kompakt, _) = vorlage(&nachher, "mitschnitt-kompakt").await;
    assert!(
        kompakt.starts_with("Sechs Abschnitte:"),
        "die kompakt-Beschreibung zaehlt noch sieben: {kompakt}"
    );
}

/// Die Titel fasst der Step nicht an. Ein Step, der nebenbei umbenennt, waere
/// ein anderer Step.
#[tokio::test]
async fn kein_titel_aendert_sich() {
    let vorher = vor_dem_step().await;
    let nachher = frisch().await;

    for (id, _) in NACH_DEM_STEP {
        let (alt, _, _) = vorlage(&vorher, id).await;
        let (neu, _, _) = vorlage(&nachher, id).await;
        assert_eq!(alt, neu, "der Titel von '{id}' hat sich geaendert");
    }
}

/// Eine vom Nutzer bearbeitete Vorlage wird nicht ueberschrieben -- gemessen
/// am Inhalt, nie am Zeitstempel. Der Test aendert genau EIN Feld und erwartet,
/// dass die ganze Zeile stehen bleibt. Gefahren fuer jede der sieben und fuer
/// jedes der drei Felder, weil jede Vorbedingung alle drei nennt.
#[tokio::test]
async fn eine_bearbeitete_vorlage_bleibt_unangetastet() {
    for (id, _) in NACH_DEM_STEP {
        for feld in ["title", "description", "sections_json"] {
            let db = vor_dem_step().await;
            // Kein zusammengebautes SQL: die drei Anweisungen stehen
            // ausgeschrieben da, sonst verlangt sqlx 0.9 eine Zusicherung
            // ueber einen zur Laufzeit gebauten String.
            let stmt = match feld {
                "title" => "UPDATE templates SET title = ? WHERE id = ?",
                "description" => "UPDATE templates SET description = ? WHERE id = ?",
                _ => "UPDATE templates SET sections_json = ? WHERE id = ?",
            };
            sqlx::query(stmt)
                .bind("meine eigene Fassung")
                .bind(id)
                .execute(db.pool())
                .await
                .unwrap();
            let vorher = vorlage(&db, id).await;

            bis_zum_step(&db).await;

            assert_eq!(
                vorlage(&db, id).await,
                vorher,
                "'{id}' wurde ueberschrieben, obwohl '{feld}' bearbeitet war"
            );
        }
    }
}

/// Anheften ist keine Textaenderung: wer angeheftet, aber nichts umgeschrieben
/// hat, bekommt die neue Fassung, und der Pin bleibt. Derselbe bewusste
/// Unterschied zum Aufraeum-Step wie beim Disziplin-Step.
#[tokio::test]
async fn anheften_haelt_die_alte_fassung_nicht_fest() {
    let db = vor_dem_step().await;
    sqlx::query("UPDATE templates SET pinned = 1, pin_order = 3 WHERE id = 'mitschnitt-kompakt'")
        .execute(db.pool())
        .await
        .unwrap();

    bis_zum_step(&db).await;

    let (_, _, sections_json) = vorlage(&db, "mitschnitt-kompakt").await;
    assert!(!abschnittstitel(&sections_json).iter().any(|t| t == GESTRICHEN));
    let (pinned, pin_order): (i64, Option<i64>) =
        sqlx::query_as("SELECT pinned, pin_order FROM templates WHERE id = 'mitschnitt-kompakt'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!((pinned, pin_order), (1, Some(3)), "der Pin bleibt");
}

/// Der Reparaturpfad spielt die Seeds nach, und die sind INSERT OR IGNORE --
/// sie greifen also GENAU dann, wenn die Zeile fehlt. Ohne diesen Step am Ende
/// von `replay_template_seeds` stuende nach einer Reparatur der gestrichene
/// Abschnitt wieder in jeder Vorlage.
#[tokio::test]
async fn nach_einer_reparatur_ist_der_abschnitt_weiterhin_weg() {
    let db = test_db().await;
    sqlx::query("DROP TABLE templates")
        .execute(db.pool())
        .await
        .unwrap();

    prepare_schema(&db).await.unwrap();

    for (id, erwartet) in NACH_DEM_STEP {
        let (_, description, sections_json) = vorlage(&db, id).await;
        let titel = abschnittstitel(&sections_json);
        assert!(
            !titel.iter().any(|t| t == GESTRICHEN),
            "nach der Reparatur listet '{id}' den Abschnitt wieder: {titel:?}"
        );
        assert_eq!(titel.len(), erwartet, "Abschnittszahl von '{id}': {titel:?}");
        assert!(
            !description.contains(GESTRICHEN),
            "nach der Reparatur nennt '{id}' den Abschnitt wieder: {description}"
        );
    }
}
