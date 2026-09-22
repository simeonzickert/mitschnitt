//! Das Aufraeumen der Vorlagen-Liste (Entscheid 04.09.2026).
//!
//! Dreizehn Vorlagen gehen: zwoelf aus dem Upstream-Seed, die mit der eigenen
//! Arbeit nichts zu tun haben, und unsere eigene 'mitschnitt-adaptive-minutes'
//! („alter schnitt raus"). Uebrig bleiben SIEBEN.
//!
//! Drei Zusagen tragen diesen Step, und jede hat hier ihren Test:
//!
//! 1. Geloescht wird nur der AUSLIEFERUNGSSTAND. Wer eine dieser Vorlagen
//!    bearbeitet oder angeheftet hat, behaelt sie -- gemessen an Titel,
//!    Beschreibung, Kategorie, Zielgruppen, Abschnitten und Pin. Das Icon
//!    steht bewusst NICHT im Vergleich; der Grund steht im Kopf der Migration
//!    (die Spalte fehlt im Reparaturpfad, ein DELETE, das sie nennt, reisst
//!    den Start mit).
//! 2. Die gewaehlte Standard-Vorlage zeigt nie ins Leere. Traf es ihre
//!    Vorlage, faellt sie auf "Auto" ('""'); ueberlebte die Vorlage, bleibt
//!    die Wahl.
//! 3. Der Rueckweg steht im Dateikopf der Migration; die Seeds bleiben
//!    unangetastet und tragen jede entfernte Vorlage im Wortlaut.
//!
//! Eine vierte Sache, die nur 'mitschnitt-adaptive-minutes' betrifft: sie
//! wird von einem Step DESSELBEN Tages angelegt und von einem spaeteren
//! entfernt. Auf einer frischen Datenbank laufen beide zum ersten Mal
//! hintereinander -- `anlegen_dann_entfernen_ist_stabil` misst genau das.

use super::*;
use anlg_db_core::Db;

const AUFRAEUM_STEP_ID: &str = "20260904150000_vorlagen_aufraeumen";
const WAHL_STEP_ID: &str = "20260904150100_wahl_nach_aufraeumen";
const ADAPTIVE_STEP_ID: &str = "20260904120000_adaptive_minutes_vorlage";

fn step(id: &str) -> &'static anlg_db_migrate::MigrationStep {
    APP_MIGRATION_STEPS
        .iter()
        .find(|step| step.id == id)
        .unwrap_or_else(|| panic!("Migrations-Step {id} ist nicht registriert"))
}

/// Datenbank auf dem Stand direkt VOR dem Aufraeum-Step: alle Seeds liegen,
/// nichts ist entfernt. Von hier aus simulieren die Tests, was ein Nutzer
/// vorher getan hat.
async fn vor_dem_aufraeumen() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(
        &db,
        anlg_db_migrate::DbSchema {
            steps: migration_steps_before(AUFRAEUM_STEP_ID),
        },
    )
    .await
    .unwrap();
    db
}

async fn bis_zum_ende(db: &Db) {
    anlg_db_migrate::migrate(db, schema_through(WAHL_STEP_ID))
        .await
        .unwrap();
}

async fn frisch_bis_zum_ende() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    bis_zum_ende(&db).await;
    db
}

async fn setting(db: &Db, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value_json FROM app_settings WHERE id = ?")
        .bind(id)
        .fetch_optional(db.pool())
        .await
        .unwrap()
}

async fn setting_updated_at(db: &Db, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT updated_at FROM app_settings WHERE id = ?")
        .bind(id)
        .fetch_optional(db.pool())
        .await
        .unwrap()
}

async fn wahl_setzen(db: &Db, value_json: &str) {
    sqlx::query(
        "INSERT INTO app_settings (id, value_json, updated_at)
         VALUES ('selected_template_id', ?, '2026-08-31T07:11:39.375Z')
         ON CONFLICT(id) DO UPDATE SET
           value_json = excluded.value_json,
           updated_at = excluded.updated_at",
    )
    .bind(value_json)
    .execute(db.pool())
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// Zusage 1: die Prüfzahl sieben
// ---------------------------------------------------------------------------

/// Die Prüfzahl des Auftrags: seine fuenf plus unsere zwei.
///
/// Rot, sobald eine Vorlage zu viel oder zu wenig dasteht -- also auch, wenn
/// das DELETE gar nicht greift (dann 20) oder eine Behaltene mit in die Liste
/// rutscht.
#[tokio::test]
async fn frische_datenbank_traegt_genau_sieben_vorlagen() {
    let db = frisch_bis_zum_ende().await;

    assert_eq!(
        template_ids(&db).await,
        VORLAGEN_NACH_AUFRAEUMEN
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
    );
}

/// Die fuenf behaltenen Upstream-Vorlagen kommen unveraendert durch -- Titel
/// und Abschnittszahl aus dem Seed. Rot, wenn eine von ihnen in die
/// Loeschliste geriete.
#[tokio::test]
async fn die_fuenf_behaltenen_stehen_vollstaendig_da() {
    let db = frisch_bis_zum_ende().await;

    for (id, titel, abschnitte) in [
        ("default-client-kickoff", "Client Kickoff Meeting", 6usize),
        ("default-lecture-notes", "Lecture Notes", 6),
        ("default-one-on-one-meeting", "1:1 Meeting", 5),
        ("default-sprint-planning", "Sprint Planning", 5),
        ("default-sprint-retrospective", "Sprint Retrospective", 4),
    ] {
        let row = get_template(db.pool(), id)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("behaltene Vorlage '{id}' fehlt"));
        assert_eq!(row.title, titel);
        let sections: Vec<serde_json::Value> = serde_json::from_str(&row.sections_json).unwrap();
        assert_eq!(sections.len(), abschnitte, "Vorlage '{id}'");
    }
}

/// Unsere beiden bleiben: der Standard und der Rueckweg.
#[tokio::test]
async fn unsere_zwei_vorlagen_bleiben() {
    let db = frisch_bis_zum_ende().await;

    for (id, abschnitte) in [("mitschnitt-kompakt", 7usize), ("mitschnitt-standard", 5)] {
        let row = get_template(db.pool(), id)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Vorlage '{id}' fehlt -- der Rueckweg ist zu"));
        assert!(!row.title.is_empty(), "Vorlage '{id}' ohne Titel");
        let sections: Vec<serde_json::Value> = serde_json::from_str(&row.sections_json).unwrap();
        assert_eq!(sections.len(), abschnitte, "Vorlage '{id}'");
    }
}

// ---------------------------------------------------------------------------
// Zusage 2: fremde Arbeit ueberlebt
// ---------------------------------------------------------------------------

/// Ein bearbeiteter Titel schuetzt die Zeile. Das ist der haeufigste Fall:
/// jemand hat die Vorlage umbenannt und weiterbenutzt.
#[tokio::test]
async fn bearbeiteter_titel_ueberlebt() {
    for id in ENTFERNTE_VORLAGEN {
        let db = vor_dem_aufraeumen().await;
        sqlx::query("UPDATE templates SET title = 'Meine Fassung' WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await
            .unwrap();

        bis_zum_ende(&db).await;

        let row = get_template(db.pool(), id).await.unwrap();
        assert!(
            row.is_some(),
            "die bearbeitete Vorlage '{id}' wurde geloescht"
        );
        assert_eq!(row.unwrap().title, "Meine Fassung");
    }
}

/// Auch wenn NUR die Abschnitte bearbeitet wurden und der Titel steht. Genau
/// dieser Fall faellt durch jeden Vergleich, der sich auf den Titel verlaesst.
#[tokio::test]
async fn bearbeitete_abschnitte_ueberleben() {
    for id in ENTFERNTE_VORLAGEN {
        let db = vor_dem_aufraeumen().await;
        sqlx::query(
            "UPDATE templates SET sections_json = '[{\"title\":\"Mein Abschnitt\",\"description\":\"\"}]' WHERE id = ?",
        )
        .bind(id)
        .execute(db.pool())
        .await
        .unwrap();

        bis_zum_ende(&db).await;

        assert!(
            get_template(db.pool(), id).await.unwrap().is_some(),
            "'{id}' wurde geloescht, obwohl die Abschnitte bearbeitet waren"
        );
    }
}

/// Beschreibung, Kategorie und Zielgruppen zaehlen genauso -- jedes Feld, das
/// die Oberflaeche bearbeitbar macht, schuetzt die Zeile.
#[tokio::test]
async fn jedes_bearbeitbare_feld_schuetzt_die_zeile() {
    for sql in [
        "UPDATE templates SET description = 'Meine Beschreibung' WHERE id = 'default-daily-standup'",
        "UPDATE templates SET category = 'Meine Kategorie' WHERE id = 'default-daily-standup'",
        "UPDATE templates SET category = NULL WHERE id = 'default-daily-standup'",
        "UPDATE templates SET targets_json = '[\"Ich\"]' WHERE id = 'default-daily-standup'",
        "UPDATE templates SET targets_json = NULL WHERE id = 'default-daily-standup'",
    ] {
        let db = vor_dem_aufraeumen().await;
        sqlx::query(sql).execute(db.pool()).await.unwrap();

        bis_zum_ende(&db).await;

        assert!(
            get_template(db.pool(), "default-daily-standup")
                .await
                .unwrap()
                .is_some(),
            "diese Aenderung hat die Zeile nicht geschuetzt: {sql}"
        );
    }
}

/// Angeheftet ist wie bearbeitet: die Zeile bleibt. Der Pin wird hier gesetzt,
/// OHNE den Inhalt anzufassen -- sonst pruefte dieser Test den Inhaltsvergleich
/// mit und sagte nichts ueber die Pin-Bedingung.
#[tokio::test]
async fn angeheftete_vorlage_ueberlebt() {
    for sql in [
        "UPDATE templates SET pinned = 1 WHERE id = 'default-investor-pitch'",
        "UPDATE templates SET pin_order = 3 WHERE id = 'default-investor-pitch'",
    ] {
        let db = vor_dem_aufraeumen().await;
        sqlx::query(sql).execute(db.pool()).await.unwrap();

        bis_zum_ende(&db).await;

        assert!(
            get_template(db.pool(), "default-investor-pitch")
                .await
                .unwrap()
                .is_some(),
            "die angeheftete Vorlage wurde geloescht ({sql})"
        );
    }
}

/// Der Gegenbeweis zu allen Schutz-Tests darueber: eine UNBERUEHRTE Zeile
/// verschwindet wirklich. Ohne den waere jeder Schutz-Test auch von einem
/// DELETE zu bestehen, das ueberhaupt nichts trifft.
#[tokio::test]
async fn die_unberuehrte_zeile_verschwindet_wirklich() {
    let db = vor_dem_aufraeumen().await;
    for id in ENTFERNTE_VORLAGEN {
        assert!(
            get_template(db.pool(), id).await.unwrap().is_some(),
            "'{id}' fehlt schon vor dem Aufraeum-Step -- der Test misst nichts"
        );
    }

    bis_zum_ende(&db).await;

    for id in ENTFERNTE_VORLAGEN {
        assert!(
            get_template(db.pool(), id).await.unwrap().is_none(),
            "'{id}' steht nach dem Aufraeum-Step noch da"
        );
    }
}

// ---------------------------------------------------------------------------
// Zusage 3: die Wahl zeigt nie ins Leere
// ---------------------------------------------------------------------------

/// Zeigte die Wahl auf eine entfernte Vorlage, faellt sie auf "Auto".
///
/// Ohne das laedt useEnhancedNotes eine ID ins Leere und die Zusammenfassung
/// faellt STILL auf Auto zurueck -- der Nutzer sieht ein anderes Ergebnis und
/// erfaehrt den Grund nie.
#[tokio::test]
async fn die_wahl_faellt_auf_auto_wenn_ihre_vorlage_geht() {
    for id in ENTFERNTE_VORLAGEN {
        let db = vor_dem_aufraeumen().await;
        wahl_setzen(&db, &format!("\"{id}\"")).await;

        bis_zum_ende(&db).await;

        assert_eq!(
            setting(&db, "selected_template_id").await.as_deref(),
            Some("\"\""),
            "die Wahl auf '{id}' zeigt nach dem Aufraeumen ins Leere"
        );
    }
}

/// Ueberlebt die Vorlage, bleibt auch die Wahl. Der Unterschied haengt nicht
/// an der ID, sondern daran, ob die Zeile noch da ist -- deshalb steht im SQL
/// 'NOT IN (SELECT id FROM templates)' und nicht nur die ID-Liste.
#[tokio::test]
async fn die_wahl_bleibt_wenn_ihre_vorlage_ueberlebt() {
    let db = vor_dem_aufraeumen().await;
    sqlx::query("UPDATE templates SET title = 'Meine Fassung' WHERE id = 'default-daily-standup'")
        .execute(db.pool())
        .await
        .unwrap();
    wahl_setzen(&db, "\"default-daily-standup\"").await;

    bis_zum_ende(&db).await;

    assert_eq!(
        setting(&db, "selected_template_id").await.as_deref(),
        Some("\"default-daily-standup\""),
        "die Vorlage steht noch, also darf die Wahl nicht zurueckgesetzt werden"
    );
    assert_eq!(
        setting_updated_at(&db, "selected_template_id").await.as_deref(),
        Some("2026-08-31T07:11:39.375Z"),
        "eine unangetastete Zeile behaelt ihren Zeitstempel"
    );
}

/// Jede andere Wahl bleibt unberuehrt, samt Zeitstempel: eine behaltene
/// Vorlage, unsere beiden, die Abwahl '""' und der Spaltendefault 'null'.
#[tokio::test]
async fn jede_andere_wahl_bleibt_unberuehrt() {
    for wahl in [
        "\"default-lecture-notes\"",
        "\"mitschnitt-kompakt\"",
        "\"mitschnitt-standard\"",
        "\"\"",
        "null",
    ] {
        let db = vor_dem_aufraeumen().await;
        wahl_setzen(&db, wahl).await;

        bis_zum_ende(&db).await;

        assert_eq!(
            setting(&db, "selected_template_id").await.as_deref(),
            Some(wahl),
            "die Wahl {wahl} darf der Step nicht anfassen"
        );
        assert_eq!(
            setting_updated_at(&db, "selected_template_id").await.as_deref(),
            Some("2026-08-31T07:11:39.375Z"),
            "die Wahl {wahl} darf auch ihren Zeitstempel behalten"
        );
    }
}

/// Und am Ende gilt die Hausregel weiter: die gewaehlte Vorlage muss es geben.
#[tokio::test]
async fn der_standard_ist_nach_dem_aufraeumen_aufloesbar() {
    let db = frisch_bis_zum_ende().await;

    assert_eq!(
        assert_standard_vorlage_ist_aufloesbar(&db).await,
        "mitschnitt-kompakt"
    );
}

// ---------------------------------------------------------------------------
// Reihenfolge und Idempotenz
// ---------------------------------------------------------------------------

/// 'mitschnitt-adaptive-minutes' wird von einem Step DESSELBEN Tages angelegt
/// und von einem spaeteren entfernt. Auf einer frischen Datenbank laufen beide
/// zum ersten Mal hintereinander: nach dem Anlege-Step ist sie da, nach dem
/// Aufraeum-Step weg. Rot, wenn die Steps in falscher Reihenfolge stuenden --
/// dann waere sie am Ende wieder da.
#[tokio::test]
async fn anlegen_dann_entfernen_ist_stabil() {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(&db, schema_through(ADAPTIVE_STEP_ID))
        .await
        .unwrap();
    assert!(
        get_template(db.pool(), "mitschnitt-adaptive-minutes")
            .await
            .unwrap()
            .is_some(),
        "der Anlege-Step hat die Vorlage nicht gesetzt"
    );

    bis_zum_ende(&db).await;

    assert!(
        get_template(db.pool(), "mitschnitt-adaptive-minutes")
            .await
            .unwrap()
            .is_none(),
        "der Aufraeum-Step laeuft nicht nach dem Anlege-Step"
    );
}

/// Die Reihenfolge mechanisch, nicht per Augenmass: beide Steps stehen hinter
/// allen Vorlagen-Seeds, und die Wahl kommt nach dem Loeschen -- sonst
/// pruefte sie einen Bestand, den es noch gibt.
#[tokio::test]
async fn die_steps_stehen_in_der_richtigen_reihenfolge() {
    let position = |id: &str| {
        APP_MIGRATION_STEPS
            .iter()
            .position(|step| step.id == id)
            .unwrap_or_else(|| panic!("Step {id} ist nicht registriert"))
    };

    for seed in [
        "20260524000000_default_templates",
        "20260902001000_mitschnitt_standard_vorlage",
        ADAPTIVE_STEP_ID,
        "20260904140000_mitschnitt_kompakt_vorlage",
    ] {
        assert!(
            position(seed) < position(AUFRAEUM_STEP_ID),
            "der Seed {seed} muss vor dem Aufraeum-Step stehen"
        );
    }
    assert!(position(AUFRAEUM_STEP_ID) < position(WAHL_STEP_ID));
}

/// Der Aufraeum-Step fasst keine Einstellung an -- das ist die Voraussetzung
/// dafuer, dass der Reparaturpfad ihn nachspielen darf. Gemessen am SQL.
#[tokio::test]
async fn der_aufraeum_step_fasst_keine_einstellung_an() {
    let sql = step(AUFRAEUM_STEP_ID).sql;
    let anweisungen: String = sql
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        !anweisungen.contains("app_settings"),
        "der Aufraeum-Step darf keine Einstellung anfassen"
    );
}

/// Ein zweiter Lauf desselben SQL aendert nichts. Der Reparaturpfad
/// (replay_template_seeds) spielt genau diesen Step nach -- er muss auf einer
/// bereits aufgeraeumten Datenbank folgenlos bleiben.
#[tokio::test]
async fn zweiter_lauf_desselben_sql_aendert_nichts() {
    let db = frisch_bis_zum_ende().await;
    let vorher = template_ids(&db).await;
    let wahl_vorher = setting_updated_at(&db, "selected_template_id").await;

    for id in [AUFRAEUM_STEP_ID, WAHL_STEP_ID] {
        sqlx::raw_sql(step(id).sql).execute(db.pool()).await.unwrap();
    }

    assert_eq!(template_ids(&db).await, vorher);
    assert_eq!(
        setting_updated_at(&db, "selected_template_id").await,
        wahl_vorher
    );
}

/// Der Reparaturpfad endet beim selben Bestand. Ohne den Aufraeum-Step in
/// `replay_template_seeds` braechte jede Tabellen-Reparatur die dreizehn
/// entfernten Vorlagen zurueck.
#[tokio::test]
async fn nach_tabellen_reparatur_stehen_wieder_sieben() {
    let db = Db::connect_memory_plain().await.unwrap();
    prepare_schema(&db).await.unwrap();

    sqlx::query("DROP TABLE templates")
        .execute(db.pool())
        .await
        .unwrap();
    prepare_schema(&db).await.unwrap();

    assert_vorlagenbestand_steht(&db).await;
    assert_standard_vorlage_ist_aufloesbar(&db).await;
}
