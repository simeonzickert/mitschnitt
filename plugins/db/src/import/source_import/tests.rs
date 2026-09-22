use std::path::{Path, PathBuf};

use anlg_db_core::Db;
use sqlx::SqlitePool;

use super::*;

/// Alle Namen in diesen Testdaten sind erfunden. Echte Kunden-, Kollegen- oder
/// Mailadressen gehoeren nicht in ein Repo, das irgendwann oeffentlich wird.
const ERFUNDENE_SITZUNG: &str = "11111111-1111-4111-8111-111111111111";
const ZWEITE_SITZUNG: &str = "22222222-2222-4222-8222-222222222222";
const FREMDER_ARBEITSBEREICH: &str = "99999999-9999-4999-8999-999999999999";

/// Bewusst eine DATEI, keine Datenbank im Arbeitsspeicher. SQLite macht jedes
/// `ATTACH` speicherresident, sobald die Hauptdatenbank speicherresident ist --
/// der angehaengte Bestand kommt dann leer an, und der ganze Datenbankpfad
/// waere im Test unsichtbar geblieben (am 10.09.2026 genau so erlebt).
async fn ziel_datenbank(ordner: &Path) -> Db {
    let db = Db::connect_local_plain(ordner.join("app.db"))
        .await
        .unwrap();
    anlg_db_app::prepare_schema(&db).await.unwrap();
    db
}

/// Baut eine Quelle mit demselben Schema wie das Ziel. Das ist kein
/// Testkniff, sondern der gemessene Zustand: alle sieben uebertragenen
/// Tabellen sind zwischen anarlog und Mitschnitt Spalte fuer Spalte gleich
/// (nachgemessen 10.09.2026 an 395 Sitzungen).
async fn quelle_bauen(root: &Path) -> PathBuf {
    quelle_bauen_mit(root, false).await
}

/// Mit `spiegel = true` liegt neben der Datenbank ein Ordnerbestand, wie ihn
/// die alte App mitschreibt. Ohne ihn ist `scan.kind` NIE `Folder` oder `Both`
/// und der ganze Ordnerpfad bleibt ungetestet -- genau die Luecke, durch die
/// der Aufraeum-Fund gekommen ist.
async fn quelle_bauen_mit(root: &Path, spiegel: bool) -> PathBuf {
    let db_path = quelle_datenbank_bauen(root).await;
    if spiegel {
        ordner_spiegel_bauen(
            root,
            ERFUNDENE_SITZUNG,
            "Werkstattrunde",
            "Zusammenfassung aus dem Ordner",
        );
    }
    db_path
}

async fn quelle_datenbank_bauen(root: &Path) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    let db_path = root.join("app.db");
    let db = Db::connect_local_plain(&db_path).await.unwrap();
    anlg_db_app::prepare_schema(&db).await.unwrap();

    for (id, titel, updated_at) in [
        (
            ERFUNDENE_SITZUNG,
            "Werkstattrunde",
            "2026-05-01T10:00:00.000Z",
        ),
        (
            ZWEITE_SITZUNG,
            "Zaunbau am Deich",
            "2026-05-02T10:00:00.000Z",
        ),
    ] {
        sqlx::query(
            "INSERT INTO sessions (id, workspace_id, owner_user_id, title, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(FREMDER_ARBEITSBEREICH)
        .bind(FREMDER_ARBEITSBEREICH)
        .bind(titel)
        .bind("2026-05-01T09:00:00.000Z")
        .bind(updated_at)
        .execute(db.pool())
        .await
        .unwrap();
    }

    sqlx::query(
        "INSERT INTO transcripts \
           (id, workspace_id, owner_user_id, session_id, words_json, metadata_json, updated_at) \
         VALUES ('t-1', ?, ?, ?, ?, '{\"vorhanden\":true}', '2026-05-01T10:00:00.000Z')",
    )
    .bind(FREMDER_ARBEITSBEREICH)
    .bind(FREMDER_ARBEITSBEREICH)
    .bind(ERFUNDENE_SITZUNG)
    .bind(r#"[{"text":"hallo","timing":{"source":"synthetic_text"}}]"#)
    .execute(db.pool())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO session_documents (id, workspace_id, session_id, title, updated_at) \
         VALUES ('d-1', ?, ?, 'Notiz', '2026-05-01T10:00:00.000Z')",
    )
    .bind(FREMDER_ARBEITSBEREICH)
    .bind(ERFUNDENE_SITZUNG)
    .execute(db.pool())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO humans (id, workspace_id, owner_user_id, name, updated_at) \
         VALUES ('h-1', ?, ?, 'Renke Ostermann', '2026-05-01T10:00:00.000Z')",
    )
    .bind(FREMDER_ARBEITSBEREICH)
    .bind(FREMDER_ARBEITSBEREICH)
    .execute(db.pool())
    .await
    .unwrap();

    db.pool().close().await;
    db_path
}

fn ton_hinterlegen(root: &Path, session_id: &str, bytes: &[u8]) {
    let dir = root.join("sessions").join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("audio.mp3"), bytes).unwrap();
}

async fn zeilen(pool: &SqlitePool, tabelle: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM {tabelle}"
    )))
    .fetch_one(pool)
    .await
    .unwrap()
}

fn sha256_datei(pfad: &Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(pfad).unwrap();
    Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// --- F60: eine fremde app.db wird gelesen -----------------------------------

#[tokio::test]
async fn f60_scan_findet_die_daten_der_fremden_datenbank() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let scan = scan_import_source(ziel.pool(), quelle.path(), ziel_vault.path())
        .await
        .unwrap();

    assert_eq!(scan.kind, ImportSourceKind::Database);
    assert_eq!(scan.session_count, 2);
    assert_eq!(scan.transcript_count, 1);
    assert_eq!(scan.document_count, 1);
    assert!(scan.problems.is_empty(), "unerwartet: {:?}", scan.problems);
}

#[tokio::test]
async fn f60_import_bringt_alle_sechs_tabellen_an() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let run_id = run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();
    let bericht = get_import_run(ziel.pool(), &run_id).await.unwrap();

    assert_eq!(
        bericht.status,
        ImportRunStatus::Completed,
        "Fehler: {:?}",
        bericht.error
    );
    assert_eq!(zeilen(ziel.pool(), "sessions").await, 2);
    assert_eq!(zeilen(ziel.pool(), "transcripts").await, 1);
    assert_eq!(zeilen(ziel.pool(), "session_documents").await, 1);
    assert_eq!(zeilen(ziel.pool(), "humans").await, 1);
}

// --- F61: laufende Quell-App bricht nichts ----------------------------------

#[tokio::test]
async fn f61_nicht_eingecheckte_wal_zeilen_kommen_trotzdem_an() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    let db_path = quelle_bauen(quelle.path()).await;

    // Die Quell-App simulieren: WAL-Modus, Zeile schreiben, Verbindung OFFEN
    // lassen. Ohne Kopie der Beinamen fehlt genau diese Zeile.
    let laufend = Db::connect_local_plain(&db_path).await.unwrap();
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(laufend.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, workspace_id, owner_user_id, title, updated_at) \
         VALUES ('33333333-3333-4333-8333-333333333333', ?, ?, 'Spaeter Nachtrag', \
                 '2026-05-03T10:00:00.000Z')",
    )
    .bind(FREMDER_ARBEITSBEREICH)
    .bind(FREMDER_ARBEITSBEREICH)
    .execute(laufend.pool())
    .await
    .unwrap();

    let ziel = ziel_datenbank(ziel_vault.path()).await;
    let scan = scan_import_source(ziel.pool(), quelle.path(), ziel_vault.path())
        .await
        .unwrap();

    assert_eq!(
        scan.session_count, 3,
        "die noch nicht eingecheckte Sitzung fehlt -- die Kopie nimmt -wal nicht mit"
    );
    drop(laufend);
}

// --- F62: die Quelle bleibt unberuehrt --------------------------------------

#[tokio::test]
async fn f62_quelle_ist_nach_dem_import_byte_gleich() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    let db_path = quelle_bauen(quelle.path()).await;
    ton_hinterlegen(quelle.path(), ERFUNDENE_SITZUNG, b"tondaten-erfunden");

    let db_vorher = sha256_datei(&db_path);
    let ton_pfad = quelle
        .path()
        .join("sessions")
        .join(ERFUNDENE_SITZUNG)
        .join("audio.mp3");
    let ton_vorher = sha256_datei(&ton_pfad);

    let ziel = ziel_datenbank(ziel_vault.path()).await;
    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, true)
        .await
        .unwrap();

    assert_eq!(
        db_vorher,
        sha256_datei(&db_path),
        "Quell-Datenbank veraendert"
    );
    assert_eq!(
        ton_vorher,
        sha256_datei(&ton_pfad),
        "Quell-Tondatei veraendert"
    );
}

// --- F63: je Gespraech gewinnt die juengere Seite ---------------------------

#[tokio::test]
async fn f63_aeltere_quelle_ueberschreibt_juengeren_bestand_nicht() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    // Dieselbe Sitzungskennung, aber juenger als die Quelle (Quelle: 01.05.).
    sqlx::query(
        "INSERT INTO sessions (id, workspace_id, owner_user_id, title, updated_at) \
         VALUES (?, 'eigener-bereich', 'eigener-bereich', 'Von Hand korrigiert', \
                 '2026-06-01T10:00:00.000Z')",
    )
    .bind(ERFUNDENE_SITZUNG)
    .execute(ziel.pool())
    .await
    .unwrap();

    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let titel = sqlx::query_scalar::<_, String>("SELECT title FROM sessions WHERE id = ?")
        .bind(ERFUNDENE_SITZUNG)
        .fetch_one(ziel.pool())
        .await
        .unwrap();
    assert_eq!(
        titel, "Von Hand korrigiert",
        "die aeltere Quellfassung hat die juengere eigene ueberschrieben"
    );
}

#[tokio::test]
async fn f63_juengere_quelle_gewinnt_gegen_aelteren_bestand() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    sqlx::query(
        "INSERT INTO sessions (id, workspace_id, owner_user_id, title, updated_at) \
         VALUES (?, 'eigener-bereich', 'eigener-bereich', 'Alter Stand', \
                 '2026-01-01T10:00:00.000Z')",
    )
    .bind(ERFUNDENE_SITZUNG)
    .execute(ziel.pool())
    .await
    .unwrap();

    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let titel = sqlx::query_scalar::<_, String>("SELECT title FROM sessions WHERE id = ?")
        .bind(ERFUNDENE_SITZUNG)
        .fetch_one(ziel.pool())
        .await
        .unwrap();
    assert_eq!(titel, "Werkstattrunde");
}

// --- F64: ein zweiter Lauf verdoppelt nichts --------------------------------

#[tokio::test]
async fn f64_zweiter_lauf_aendert_keine_zeilenzahl() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    ton_hinterlegen(quelle.path(), ERFUNDENE_SITZUNG, b"tondaten-erfunden");
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, true)
        .await
        .unwrap();
    let nach_eins = (
        zeilen(ziel.pool(), "sessions").await,
        zeilen(ziel.pool(), "transcripts").await,
        zeilen(ziel.pool(), "session_documents").await,
        zeilen(ziel.pool(), "session_attachments").await,
    );

    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, true)
        .await
        .unwrap();
    let nach_zwei = (
        zeilen(ziel.pool(), "sessions").await,
        zeilen(ziel.pool(), "transcripts").await,
        zeilen(ziel.pool(), "session_documents").await,
        zeilen(ziel.pool(), "session_attachments").await,
    );

    assert_eq!(nach_eins, nach_zwei);
}

// --- F65: Umschluesselung auf die eigene Installation -----------------------

#[tokio::test]
async fn f65_importierte_sitzung_traegt_die_eigene_bereichskennung() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    sqlx::query(
        "INSERT OR REPLACE INTO app_settings (id, value_json) \
         VALUES ('cloudsync_workspace_binding', '{\"workspace_id\":\"eigener-bereich\"}')",
    )
    .execute(ziel.pool())
    .await
    .unwrap();

    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let fremde =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE workspace_id = ?")
            .bind(FREMDER_ARBEITSBEREICH)
            .fetch_one(ziel.pool())
            .await
            .unwrap();
    assert_eq!(fremde, 0, "fremde Bereichskennung ist mitgekommen");

    let eigene =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE workspace_id = ?")
            .bind("eigener-bereich")
            .fetch_one(ziel.pool())
            .await
            .unwrap();
    assert_eq!(eigene, 2);

    // Auch die Kindtabellen, sonst haengen sie am fremden Bereich.
    let fremde_transkripte =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transcripts WHERE workspace_id = ?")
            .bind(FREMDER_ARBEITSBEREICH)
            .fetch_one(ziel.pool())
            .await
            .unwrap();
    assert_eq!(fremde_transkripte, 0);
}

#[tokio::test]
async fn f65_herkunft_steht_in_der_sitzung() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let run_id = run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let metadata =
        sqlx::query_scalar::<_, String>("SELECT metadata_json FROM sessions WHERE id = ?")
            .bind(ERFUNDENE_SITZUNG)
            .fetch_one(ziel.pool())
            .await
            .unwrap();
    let parsed = serde_json::from_str::<serde_json::Value>(&metadata).unwrap();
    assert_eq!(parsed["import"]["source_kind"], "database");
    assert_eq!(parsed["import"]["run_id"], run_id);
    assert_eq!(
        parsed["import"]["original_workspace_id"],
        FREMDER_ARBEITSBEREICH
    );
    assert!(
        parsed["import"]["imported_at"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
}

#[tokio::test]
async fn f65_zeitqualitaet_steht_am_transkript() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let metadata =
        sqlx::query_scalar::<_, String>("SELECT metadata_json FROM transcripts WHERE id = 't-1'")
            .fetch_one(ziel.pool())
            .await
            .unwrap();
    let parsed = serde_json::from_str::<serde_json::Value>(&metadata).unwrap();
    assert_eq!(parsed["timing_quality"], "synthetic");
    assert_eq!(parsed["vorhanden"], true, "bestehende Felder verloren");
}

// --- F66: der Ton zieht mit -------------------------------------------------

#[tokio::test]
async fn f66_ton_wird_kopiert_registriert_und_als_vorhanden_gemeldet() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    ton_hinterlegen(quelle.path(), ERFUNDENE_SITZUNG, b"tondaten-erfunden");
    ton_hinterlegen(quelle.path(), ZWEITE_SITZUNG, b"zweite-aufnahme");
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let run_id = run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, true)
        .await
        .unwrap();
    let bericht = get_import_run(ziel.pool(), &run_id).await.unwrap();

    assert_eq!(bericht.audio_copied, 2);
    assert_eq!(bericht.audio_bytes_copied, 17 + 15);

    for session_id in [ERFUNDENE_SITZUNG, ZWEITE_SITZUNG] {
        let ziel_datei = ziel_vault
            .path()
            .join("sessions")
            .join(session_id)
            .join("audio.mp3");
        assert!(ziel_datei.is_file(), "Tondatei fehlt: {session_id}");

        let abspielbar = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM session_attachments AS a \
             JOIN attachment_local_state AS s ON s.attachment_id = a.id \
             WHERE a.session_id = ? AND s.availability = 'present' \
               AND a.storage_kind = 'local_file'",
        )
        .bind(session_id)
        .fetch_one(ziel.pool())
        .await
        .unwrap();
        assert_eq!(abspielbar, 1, "kein abspielbarer Anhang fuer {session_id}");
    }
}

#[tokio::test]
async fn f66_ton_ohne_passende_sitzung_wird_uebersprungen_statt_zu_scheitern() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    ton_hinterlegen(quelle.path(), "kennt-keiner", b"waise");
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let run_id = run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, true)
        .await
        .unwrap();
    let bericht = get_import_run(ziel.pool(), &run_id).await.unwrap();

    assert_eq!(bericht.status, ImportRunStatus::Completed);
    assert_eq!(bericht.audio_copied, 0);
    assert_eq!(zeilen(ziel.pool(), "session_attachments").await, 0);
}

// --- F67: Platzbedarf ist bekannt und bricht ab -----------------------------

#[tokio::test]
async fn f67_platzbedarf_wird_vor_dem_start_beziffert() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    ton_hinterlegen(quelle.path(), ERFUNDENE_SITZUNG, b"0123456789");
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let scan = scan_import_source(ziel.pool(), quelle.path(), ziel_vault.path())
        .await
        .unwrap();

    assert_eq!(scan.audio_file_count, 1);
    assert_eq!(scan.audio_bytes, 10);
    assert_eq!(scan.required_bytes(true), 10);
    assert_eq!(
        scan.required_bytes(false),
        0,
        "ohne Ton kostet der Lauf nichts"
    );
    assert!(scan.free_bytes_on_target > 0, "freier Platz nicht gemessen");
}

#[tokio::test]
async fn f67_zu_wenig_platz_ist_ein_blocker_und_nur_bei_ton() {
    let mut scan = ImportSourceScan {
        kind: ImportSourceKind::Database,
        source_root: "/quelle".into(),
        source_app: None,
        session_count: 1,
        transcript_count: 0,
        document_count: 0,
        audio_file_count: 1,
        audio_bytes: 1_000_000,
        free_bytes_on_target: 10,
        oldest_session_at: None,
        newest_session_at: None,
        problems: vec![scan::PROBLEM_NOT_ENOUGH_SPACE.to_owned()],
    };
    assert_eq!(scan.blocked(), Some(scan::PROBLEM_NOT_ENOUGH_SPACE));
    assert!(scan.required_bytes(true) > scan.free_bytes_on_target);

    scan.problems.clear();
    assert_eq!(scan.blocked(), None);
}

// --- F69: ein Trockenlauf schreibt nichts -----------------------------------

#[tokio::test]
async fn f69_trockenlauf_legt_keine_zeile_an_und_kopiert_keinen_ton() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    ton_hinterlegen(quelle.path(), ERFUNDENE_SITZUNG, b"tondaten-erfunden");
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let run_id = run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), true, true)
        .await
        .unwrap();
    let bericht = get_import_run(ziel.pool(), &run_id).await.unwrap();

    assert!(bericht.dry_run);
    assert!(
        bericht.discovered > 0,
        "der Trockenlauf hat nichts gezaehlt"
    );
    // Frueher stand hier `imported == 0`. Das war eine einkodierte Null: der
    // Abschluss erzwang sie, der Test bestaetigte sie, und der Satz ueber dem
    // Knopf lautete deshalb immer "von N kaemen 0 herueber" -- ausgerechnet
    // ueber dem Knopf, der alles importiert. Der Trockenlauf beziffert jetzt,
    // was KAEME; dass er nichts schreibt, steht in den Zeilenzahlen darunter.
    assert!(
        bericht.imported > 0,
        "der Trockenlauf beziffert nicht, was herueberkaeme"
    );
    assert!(
        bericht.conversations_imported > 0,
        "der Trockenlauf beziffert keine Gespraeche"
    );
    assert_eq!(zeilen(ziel.pool(), "sessions").await, 0);
    assert_eq!(zeilen(ziel.pool(), "transcripts").await, 0);
    assert_eq!(zeilen(ziel.pool(), "session_documents").await, 0);
    assert_eq!(zeilen(ziel.pool(), "session_attachments").await, 0);
    assert!(!ziel_vault.path().join("sessions").exists());
}

// --- Anti-E: nichts wird geloescht ------------------------------------------

#[tokio::test]
async fn anti_e_fremde_eigene_sitzung_bleibt_unangetastet() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    sqlx::query(
        "INSERT INTO sessions (id, workspace_id, owner_user_id, title, updated_at) \
         VALUES ('eigene-sitzung', 'eigener-bereich', 'eigener-bereich', 'Nur bei mir', \
                 '2026-04-01T10:00:00.000Z')",
    )
    .execute(ziel.pool())
    .await
    .unwrap();

    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let titel = sqlx::query_scalar::<_, String>("SELECT title FROM sessions WHERE id = ?")
        .bind("eigene-sitzung")
        .fetch_one(ziel.pool())
        .await
        .unwrap();
    assert_eq!(titel, "Nur bei mir");
    assert_eq!(
        zeilen(ziel.pool(), "sessions").await,
        3,
        "Zeilen verschwunden"
    );
}

// --- Erkennung und Verweigerung ---------------------------------------------

#[tokio::test]
async fn leerer_ordner_meldet_no_data_found() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let scan = scan_import_source(ziel.pool(), quelle.path(), ziel_vault.path())
        .await
        .unwrap();
    assert_eq!(scan.kind, ImportSourceKind::None);
    assert_eq!(scan.problems, vec![scan::PROBLEM_NO_DATA_FOUND]);
    assert_eq!(scan.blocked(), Some(scan::PROBLEM_NO_DATA_FOUND));
}

#[tokio::test]
async fn nicht_vorhandener_pfad_meldet_source_unreadable() {
    let ziel_vault = tempfile::tempdir().unwrap();
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let scan = scan_import_source(
        ziel.pool(),
        Path::new("/gibt/es/nicht/wirklich"),
        ziel_vault.path(),
    )
    .await
    .unwrap();
    assert_eq!(scan.problems, vec![scan::PROBLEM_SOURCE_UNREADABLE]);
}

#[tokio::test]
async fn eigene_installation_als_quelle_wird_verweigert() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    // Ziel traegt dieselbe Bereichskennung wie die Quelle.
    sqlx::query(
        "INSERT OR REPLACE INTO app_settings (id, value_json) VALUES ('cloudsync_workspace_binding', ?)",
    )
    .bind(format!(
        "{{\"workspace_id\":\"{FREMDER_ARBEITSBEREICH}\"}}"
    ))
    .execute(ziel.pool())
    .await
    .unwrap();

    let scan = scan_import_source(ziel.pool(), quelle.path(), ziel_vault.path())
        .await
        .unwrap();
    assert!(
        scan.problems
            .contains(&scan::PROBLEM_SAME_INSTALLATION.to_owned())
    );

    let run_id = run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();
    let bericht = get_import_run(ziel.pool(), &run_id).await.unwrap();
    assert_eq!(bericht.status, ImportRunStatus::Failed);
    assert_eq!(
        bericht.error.as_deref(),
        Some(scan::PROBLEM_SAME_INSTALLATION)
    );
    assert_eq!(zeilen(ziel.pool(), "sessions").await, 0);
}

// --- Lauf gegen einen echten Bestand ----------------------------------------

/// Kein Teil des normalen Testlaufs: braucht eine echte Alt-Installation und
/// laeuft je nach Groesse Minuten. Aufruf:
///
/// ```text
/// MITSCHNITT_IMPORT_QUELLE="$HOME/Library/Application Support/hyprnote" \
///   cargo test -p tauri-plugin-db echter_bestand -- --ignored --nocapture
/// ```
///
/// Die Quelle wird ausschliesslich gelesen; der Test prueft das per Pruefsumme
/// mit, statt es zu behaupten.
#[tokio::test]
#[ignore = "braucht eine echte Alt-Installation (MITSCHNITT_IMPORT_QUELLE)"]
async fn echter_bestand_kommt_vollstaendig_an_und_laesst_die_quelle_in_ruhe() {
    let Ok(quelle) = std::env::var("MITSCHNITT_IMPORT_QUELLE") else {
        panic!("MITSCHNITT_IMPORT_QUELLE nicht gesetzt");
    };
    let quelle = PathBuf::from(quelle);
    let ton = std::env::var("MITSCHNITT_IMPORT_TON").is_ok();

    // Ein weggeworfener Zielordner macht jede Zahl aus diesem Test
    // unnachpruefbar. Mit MITSCHNITT_IMPORT_ZIEL bleibt er stehen, und wer die
    // Meldung anzweifelt, zaehlt selbst nach.
    let flucht_ordner = tempfile::tempdir().unwrap();
    let ziel_vault = match std::env::var("MITSCHNITT_IMPORT_ZIEL") {
        Ok(pfad) => {
            let pfad = PathBuf::from(pfad);
            std::fs::create_dir_all(&pfad).unwrap();
            pfad
        }
        Err(_) => flucht_ordner.path().to_path_buf(),
    };
    let ziel_vault = ziel_vault.as_path();
    eprintln!("ZIEL {}", ziel_vault.display());
    let ziel = ziel_datenbank(ziel_vault).await;

    let db_vorher = sha256_datei(&quelle.join("app.db"));

    let scan = scan_import_source(ziel.pool(), &quelle, ziel_vault)
        .await
        .unwrap();
    eprintln!(
        "SCAN art={:?} sitzungen={} transkripte={} dokumente={} tondateien={} tonbytes={} probleme={:?}",
        scan.kind,
        scan.session_count,
        scan.transcript_count,
        scan.document_count,
        scan.audio_file_count,
        scan.audio_bytes,
        scan.problems
    );
    assert!(scan.session_count > 0, "die Quelle wurde nicht gelesen");

    let run_id = run_source_import(ziel.pool(), &quelle, ziel_vault, false, ton)
        .await
        .unwrap();
    let bericht = get_import_run(ziel.pool(), &run_id).await.unwrap();
    eprintln!(
        "LAUF 1 status={:?} gespraeche={}/{} zeilen_gefunden={} zeilen_uebernommen={} uebersprungen={} konflikte={} fehler={} ton={} tonbytes={} fehlertext={:?}",
        bericht.status,
        bericht.conversations_imported,
        bericht.conversations_discovered,
        bericht.discovered,
        bericht.imported,
        bericht.skipped,
        bericht.conflicts,
        bericht.errors,
        bericht.audio_copied,
        bericht.audio_bytes_copied,
        bericht.error
    );

    let nach_eins = (
        zeilen(ziel.pool(), "sessions").await,
        zeilen(ziel.pool(), "transcripts").await,
        zeilen(ziel.pool(), "session_documents").await,
        zeilen(ziel.pool(), "session_participants").await,
        zeilen(ziel.pool(), "humans").await,
        zeilen(ziel.pool(), "events").await,
        zeilen(ziel.pool(), "session_attachments").await,
    );
    eprintln!("NACH LAUF 1 {nach_eins:?}");
    assert_eq!(
        nach_eins.0, scan.session_count,
        "nicht alle Sitzungen sind angekommen"
    );

    // F64 am echten Bestand.
    let run_zwei = run_source_import(ziel.pool(), &quelle, ziel_vault, false, ton)
        .await
        .unwrap();
    let bericht_zwei = get_import_run(ziel.pool(), &run_zwei).await.unwrap();
    let nach_zwei = (
        zeilen(ziel.pool(), "sessions").await,
        zeilen(ziel.pool(), "transcripts").await,
        zeilen(ziel.pool(), "session_documents").await,
        zeilen(ziel.pool(), "session_participants").await,
        zeilen(ziel.pool(), "humans").await,
        zeilen(ziel.pool(), "events").await,
        zeilen(ziel.pool(), "session_attachments").await,
    );
    eprintln!(
        "NACH LAUF 2 {nach_zwei:?} (ton erneut kopiert: {})",
        bericht_zwei.audio_copied
    );
    for zeile in sqlx::query_scalar::<_, String>(
        "SELECT id || ' | ' || session_id || ' | ' || relative_path FROM session_attachments",
    )
    .fetch_all(ziel.pool())
    .await
    .unwrap_or_default()
    {
        eprintln!("ANHANG {zeile}");
    }
    for zeile in sqlx::query_scalar::<_, String>(
        "SELECT status || ' x' || COUNT(*) || ' :: ' || MIN(source_path) || ' :: ' || MIN(error) \
         FROM migration_import_items GROUP BY status",
    )
    .fetch_all(ziel.pool())
    .await
    .unwrap_or_default()
    {
        eprintln!("POSTEN {zeile}");
    }
    let run_drei = run_source_import(ziel.pool(), &quelle, ziel_vault, false, ton)
        .await
        .unwrap();
    let _ = get_import_run(ziel.pool(), &run_drei).await.unwrap();
    let nach_drei = (
        zeilen(ziel.pool(), "sessions").await,
        zeilen(ziel.pool(), "transcripts").await,
        zeilen(ziel.pool(), "session_documents").await,
        zeilen(ziel.pool(), "session_participants").await,
        zeilen(ziel.pool(), "humans").await,
        zeilen(ziel.pool(), "events").await,
        zeilen(ziel.pool(), "session_attachments").await,
    );
    eprintln!("NACH LAUF 3 {nach_drei:?}");
    assert_eq!(nach_zwei, nach_drei, "der Bestand ist nicht eingeschwungen");
    assert_eq!(nach_eins, nach_zwei, "der zweite Lauf hat verdoppelt");

    // F62 am echten Bestand.
    assert_eq!(
        db_vorher,
        sha256_datei(&quelle.join("app.db")),
        "die Quell-Datenbank wurde veraendert"
    );

    // F65 am echten Bestand.
    let fremde = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sessions WHERE workspace_id <> \
         (SELECT workspace_id FROM sessions GROUP BY workspace_id ORDER BY COUNT(*) DESC LIMIT 1)",
    )
    .fetch_one(ziel.pool())
    .await
    .unwrap();
    assert_eq!(fremde, 0, "mehr als eine Bereichskennung im Ziel");

    let einstufungen = sqlx::query_scalar::<_, String>(
        "SELECT json_extract(metadata_json,'$.timing_quality') FROM transcripts \
         WHERE json_extract(metadata_json,'$.timing_quality') IS NOT NULL",
    )
    .fetch_all(ziel.pool())
    .await
    .unwrap_or_default();
    eprintln!(
        "ZEITQUALITAET vergeben fuer {} Transkripte",
        einstufungen.len()
    );
}

// --- Dieselbe Zusammenfassung aus zwei Quellen ------------------------------

/// Legt neben die Datenbank einen Ordner-Spiegel, wie ihn die alte App
/// mitschreibt: dieselbe Zusammenfassung ein zweites Mal als Datei.
fn ordner_spiegel_bauen(root: &Path, session_id: &str, titel: &str, koerper: &str) {
    let dir = root.join("sessions").join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("_meta.json"),
        serde_json::json!({
            "id": session_id,
            "title": titel,
            "created_at": "2026-05-01T09:00:00.000Z",
            "updated_at": "2026-05-01T10:00:00.000Z",
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("_summary.md"), koerper).unwrap();
}

#[tokio::test]
async fn dieselbe_zusammenfassung_aus_datenbank_und_ordner_ergibt_eine_zeile() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    let db_path = quelle_bauen(quelle.path()).await;

    // Zusammenfassung in der Datenbank ...
    let db = Db::connect_local_plain(&db_path).await.unwrap();
    sqlx::query(
        "INSERT INTO session_documents (id, workspace_id, session_id, kind, title, body, body_format, updated_at) \
         VALUES ('d-summary', 'w', ?, 'summary', 'Summary', 'Zusammenfassung des Gespraechs', 'markdown', '2026-05-01T10:00:00.000Z')",
    )
    .bind(ERFUNDENE_SITZUNG)
    .execute(db.pool())
    .await
    .unwrap();
    db.pool().close().await;

    // ... und derselbe Text noch einmal als Datei daneben, bis auf eine
    // Leerzeile -- genau die Form, die im echten Bestand 146-mal auftrat.
    ordner_spiegel_bauen(
        quelle.path(),
        ERFUNDENE_SITZUNG,
        "Werkstattrunde",
        "Zusammenfassung des Gespraechs\n",
    );

    let ziel = ziel_datenbank(ziel_vault.path()).await;
    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let zusammenfassungen = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM session_documents \
         WHERE session_id = ? AND kind = 'summary' AND deleted_at IS NULL",
    )
    .bind(ERFUNDENE_SITZUNG)
    .fetch_one(ziel.pool())
    .await
    .unwrap();

    assert_eq!(
        zusammenfassungen, 1,
        "dasselbe Dokument ist aus beiden Quellen als zwei Zeilen angekommen"
    );
}

#[tokio::test]
async fn zwei_echte_dokumente_derselben_sitzung_bleiben_zwei() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    let db_path = quelle_bauen(quelle.path()).await;

    // Zweites, inhaltlich anderes Dokument derselben Art in der QUELLE. Zwei
    // solcher Paare gibt es im echten Bestand -- sie duerfen nicht
    // zusammengezogen werden, nur weil sie sich denselben Platz teilen.
    let db = Db::connect_local_plain(&db_path).await.unwrap();
    sqlx::query(
        "INSERT INTO session_documents (id, workspace_id, session_id, kind, title, body, updated_at) \
         VALUES ('d-2', 'w', ?, 'note', 'Zweite Notiz', 'anderer Text', '2026-05-01T10:00:00.000Z')",
    )
    .bind(ERFUNDENE_SITZUNG)
    .execute(db.pool())
    .await
    .unwrap();
    db.pool().close().await;

    let ziel = ziel_datenbank(ziel_vault.path()).await;
    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let notizen = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM session_documents WHERE session_id = ? AND kind = 'note'",
    )
    .bind(ERFUNDENE_SITZUNG)
    .fetch_one(ziel.pool())
    .await
    .unwrap();
    assert_eq!(
        notizen, 2,
        "zwei echte Dokumente wurden zu einem verschmolzen"
    );
}

// --- Der Ordnerpfad, bisher voellig ungetestet ------------------------------

/// Prüfsumme ueber JEDE Datei unterhalb eines Pfades, nach Pfad sortiert.
fn baum_pruefsumme(wurzel: &Path) -> Vec<(String, String)> {
    fn sammeln(dir: &Path, wurzel: &Path, aus: &mut Vec<(String, String)>) {
        let Ok(eintraege) = std::fs::read_dir(dir) else {
            return;
        };
        for eintrag in eintraege.flatten() {
            let pfad = eintrag.path();
            if pfad.is_dir() {
                sammeln(&pfad, wurzel, aus);
            } else if pfad.is_file() {
                let relativ = pfad
                    .strip_prefix(wurzel)
                    .unwrap_or(&pfad)
                    .to_string_lossy()
                    .into_owned();
                aus.push((relativ, sha256_datei(&pfad)));
            }
        }
    }
    let mut aus = Vec::new();
    sammeln(wurzel, wurzel, &mut aus);
    aus.sort();
    aus
}

#[tokio::test]
async fn ordner_neben_datenbank_wird_als_both_erkannt() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen_mit(quelle.path(), true).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let scan = scan_import_source(ziel.pool(), quelle.path(), ziel_vault.path())
        .await
        .unwrap();
    assert_eq!(scan.kind, ImportSourceKind::Both);
}

#[tokio::test]
async fn kein_einziges_byte_der_quelle_veraendert_sich() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen_mit(quelle.path(), true).await;
    ton_hinterlegen(quelle.path(), ERFUNDENE_SITZUNG, b"tondaten-erfunden");
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    let vorher = baum_pruefsumme(quelle.path());
    assert!(vorher.len() >= 3, "Quellbaum zu duenn fuer diesen Test");

    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, true)
        .await
        .unwrap();

    let nachher = baum_pruefsumme(quelle.path());
    assert_eq!(
        vorher, nachher,
        "der Import hat den Quellbaum veraendert (Datei fehlt, kam dazu oder wurde beschrieben)"
    );
}

/// Der Fund, der diese Runde ausgeloest hat: ein Lauf gegen eine FREMDE Quelle
/// darf den Aufraeum-Knopf nicht auf den Ordner des Nutzers richten.
#[tokio::test]
async fn fremdimport_bewaffnet_den_aufraeum_knopf_nicht() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen_mit(quelle.path(), true).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    // Grund 1: der Fremdlauf hat den Zeiger nicht an sich gerissen.
    let zeiger = sqlx::query_scalar::<_, String>(
        "SELECT latest_run_id FROM storage_migration_state WHERE id = 'legacy_v1'",
    )
    .fetch_one(ziel.pool())
    .await
    .unwrap();
    let fremd_wurzel = quelle.path().to_string_lossy().into_owned();
    let zeigt_auf_fremd = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM migration_import_runs WHERE id = ? AND source_root = ?",
    )
    .bind(&zeiger)
    .bind(&fremd_wurzel)
    .fetch_one(ziel.pool())
    .await
    .unwrap();
    assert_eq!(
        zeigt_auf_fremd, 0,
        "der Migrationszeiger zeigt auf die fremde Quelle"
    );

    // Grund 2: selbst dann wuerde der Aufraeum-Knopf nichts anbieten.
    let status = crate::import::get_legacy_cleanup_status(ziel.pool(), Some(ziel_vault.path()))
        .await
        .unwrap();
    assert!(
        !status.available,
        "der Aufraeum-Knopf bietet an, in einem fremden Ordner zu loeschen"
    );
    assert_eq!(status.file_count, 0);
}

#[tokio::test]
async fn ein_fehlschlag_mittendrin_laesst_keinen_halben_bestand_zurueck() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    // Die Uebernahme laeuft in der Reihenfolge humans, events, sessions,
    // session_documents, transcripts, session_participants. Ein Ausloeser auf
    // `transcripts` laesst also die ersten vier Tabellen durch und bricht
    // dann -- genau der Fall, der frueher 395 Sitzungen ohne ein einziges
    // Transkript hinterlassen haette.
    sqlx::query(
        "CREATE TRIGGER stolperstein BEFORE INSERT ON transcripts \
         BEGIN SELECT RAISE(ABORT, 'stolperstein'); END",
    )
    .execute(ziel.pool())
    .await
    .unwrap();

    let run_id = run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();
    let bericht = get_import_run(ziel.pool(), &run_id).await.unwrap();

    assert_eq!(bericht.status, ImportRunStatus::Failed);
    for tabelle in ["sessions", "humans", "events", "session_documents"] {
        assert_eq!(
            zeilen(ziel.pool(), tabelle).await,
            0,
            "{tabelle} steht nach einem Abbruch halb da"
        );
    }
}

// --- Nichts steht doppelt ---------------------------------------------------

/// Der Altstand `.md` neben der gepflegten `_summary.md`: am gemessenen Bestand
/// 146-mal vorhanden, `_summary.md` in 146 von 146 juenger, in keinem einzigen
/// Fall inhaltsgleich. Beides als eigenes Dokument zu fuehren erzeugte 146
/// Dubletten.
#[tokio::test]
async fn versteckter_altstand_wird_kein_zweites_dokument() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_datenbank_bauen(quelle.path()).await;

    let ordner = quelle.path().join("sessions").join(ERFUNDENE_SITZUNG);
    std::fs::create_dir_all(&ordner).unwrap();
    std::fs::write(
        ordner.join("_meta.json"),
        format!(
            r#"{{"id":"{ERFUNDENE_SITZUNG}","created_at":"2026-05-01T09:00:00Z","title":"Werkstattrunde"}}"#
        ),
    )
    .unwrap();
    // Zwei verschiedene Kennungen -- genau die Form, an der die Erkennung
    // frueher ausstieg.
    std::fs::write(
        ordner.join(".md"),
        format!(
            "---\nid: alt-1\nsession_id: {ERFUNDENE_SITZUNG}\ntitle: Summary\n---\n\nAlter Stand vom Januar"
        ),
    )
    .unwrap();
    std::fs::write(
        ordner.join("_summary.md"),
        format!(
            "---\nid: neu-1\nsession_id: {ERFUNDENE_SITZUNG}\ntitle: Summary\n---\n\nGepflegte Fassung vom Juli"
        ),
    )
    .unwrap();

    let ziel = ziel_datenbank(ziel_vault.path()).await;
    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let koerper = sqlx::query_scalar::<_, String>(
        "SELECT body FROM session_documents \
         WHERE session_id = ? AND kind = 'summary' AND deleted_at IS NULL",
    )
    .bind(ERFUNDENE_SITZUNG)
    .fetch_all(ziel.pool())
    .await
    .unwrap();

    assert_eq!(
        koerper.len(),
        1,
        "der versteckte Altstand steht als zweites Dokument daneben: {koerper:?}"
    );
    assert!(
        koerper[0].contains("Juli"),
        "nicht die juengere Fassung hat den Platz bekommen: {koerper:?}"
    );
}

/// Gegenprobe: im EIGENEN Datenordner bleibt es bei der Entscheidung des
/// Originals, den abweichenden Altstand als Rettungskopie zu behalten. Dort
/// werden die Quelldateien spaeter aufgeraeumt.
#[tokio::test]
async fn im_eigenen_datenordner_bleibt_die_rettungskopie_erhalten() {
    let vault = tempfile::tempdir().unwrap();
    let ordner = vault.path().join("sessions").join(ERFUNDENE_SITZUNG);
    std::fs::create_dir_all(&ordner).unwrap();
    std::fs::write(
        ordner.join("_meta.json"),
        format!(
            r#"{{"id":"{ERFUNDENE_SITZUNG}","created_at":"2026-05-01T09:00:00Z","title":"Werkstattrunde"}}"#
        ),
    )
    .unwrap();
    std::fs::write(
        ordner.join(".md"),
        format!(
            "---\nid: gleich-1\nsession_id: {ERFUNDENE_SITZUNG}\ntitle: Summary\n---\n\nAlter Stand"
        ),
    )
    .unwrap();
    std::fs::write(
        ordner.join("_summary.md"),
        format!(
            "---\nid: gleich-1\nsession_id: {ERFUNDENE_SITZUNG}\ntitle: Summary\n---\n\nNeuer Stand"
        ),
    )
    .unwrap();

    let ziel = ziel_datenbank(vault.path()).await;
    crate::import::legacy_vault_fuer_tests(ziel.pool(), vault.path())
        .await
        .unwrap();

    let anzahl =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM session_documents WHERE session_id = ?")
            .bind(ERFUNDENE_SITZUNG)
            .fetch_one(ziel.pool())
            .await
            .unwrap();
    assert_eq!(
        anzahl, 2,
        "die Rettungskopie im eigenen Ordner ist verschwunden"
    );
}

// --- Ehrliche Zahlen im Bericht ---------------------------------------------

#[tokio::test]
async fn gespraeche_werden_getrennt_von_datenbankzeilen_gezaehlt() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    let db_path = quelle_datenbank_bauen(quelle.path()).await;

    // Eine geloeschte Sitzung: sie kommt mit, erscheint aber in keiner Liste
    // und darf deshalb nicht als Gespraech zaehlen. Im echten Bestand sind das
    // 6 von 395.
    let db = Db::connect_local_plain(&db_path).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, workspace_id, owner_user_id, title, updated_at, deleted_at) \
         VALUES ('geloescht', 'w', 'w', 'Papierkorb', '2026-05-01T10:00:00.000Z', \
                 '2026-05-02T10:00:00.000Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    db.pool().close().await;

    let ziel = ziel_datenbank(ziel_vault.path()).await;
    let run_id = run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();
    let bericht = get_import_run(ziel.pool(), &run_id).await.unwrap();

    // Zwei sichtbare Gespraeche, die geloeschte zaehlt nicht mit.
    assert_eq!(bericht.conversations_discovered, 2);
    assert_eq!(bericht.conversations_imported, 2);

    // Die Zeilensumme liegt darueber -- sie darf bleiben, heisst aber nicht
    // Gespraech.
    assert!(
        bericht.discovered > bericht.conversations_discovered,
        "Zeilensumme und Gespraechszahl sind dieselbe Zahl geworden"
    );

    // Und die geloeschte Sitzung ist trotzdem angekommen.
    assert_eq!(zeilen(ziel.pool(), "sessions").await, 3);
}

#[tokio::test]
async fn ein_gescheiterter_lauf_nennt_seinen_grund_und_behaelt_seine_zahlen() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_datenbank_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    sqlx::query(
        "INSERT OR REPLACE INTO app_settings (id, value_json) \
         VALUES ('cloudsync_workspace_binding', ?)",
    )
    .bind(format!("{{\"workspace_id\":\"{FREMDER_ARBEITSBEREICH}\"}}"))
    .execute(ziel.pool())
    .await
    .unwrap();

    let run_id = run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();
    let bericht = get_import_run(ziel.pool(), &run_id).await.unwrap();

    assert_eq!(bericht.status, ImportRunStatus::Failed);
    // Der Grund muss unterscheidbar sein: same_installation, source_open und
    // ein echter Datenbankfehler sahen vorher gleich aus.
    assert_eq!(
        bericht.error.as_deref(),
        Some(scan::PROBLEM_SAME_INSTALLATION)
    );
    assert!(bericht.errors > 0, "ein Fehlschlag ohne Fehlerzaehler");
}

#[tokio::test]
async fn ein_reiner_ordnerbestand_bekommt_herkunft_und_zeitqualitaet() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();

    // KEINE app.db -- nur der Ordner. Dieser Zweig lief bisher voellig ohne
    // Etikett durch.
    let ordner = quelle.path().join("sessions").join(ERFUNDENE_SITZUNG);
    std::fs::create_dir_all(&ordner).unwrap();
    std::fs::write(
        ordner.join("_meta.json"),
        format!(
            r#"{{"id":"{ERFUNDENE_SITZUNG}","created_at":"2026-05-01T09:00:00Z","title":"Werkstattrunde"}}"#
        ),
    )
    .unwrap();
    std::fs::write(
        ordner.join("transcript.json"),
        r#"[{"text":"hallo","timing":{"source":"synthetic_text"},"start_ms":0,"end_ms":400,"channel":0}]"#,
    )
    .unwrap();

    let ziel = ziel_datenbank(ziel_vault.path()).await;
    let scan = scan_import_source(ziel.pool(), quelle.path(), ziel_vault.path())
        .await
        .unwrap();
    assert_eq!(scan.kind, ImportSourceKind::Folder);

    let run_id = run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let metadata =
        sqlx::query_scalar::<_, String>("SELECT metadata_json FROM sessions WHERE id = ?")
            .bind(ERFUNDENE_SITZUNG)
            .fetch_one(ziel.pool())
            .await
            .unwrap();
    let parsed = serde_json::from_str::<serde_json::Value>(&metadata).unwrap();
    assert_eq!(
        parsed["import"]["source_kind"], "folder",
        "ein reiner Ordnerbestand kommt ohne Herkunft an: {metadata}"
    );
    assert_eq!(parsed["import"]["run_id"], run_id);
}

#[tokio::test]
async fn eine_eigene_juengere_sitzung_behauptet_nicht_importiert_zu_sein() {
    let quelle = tempfile::tempdir().unwrap();
    let ziel_vault = tempfile::tempdir().unwrap();
    quelle_datenbank_bauen(quelle.path()).await;
    let ziel = ziel_datenbank(ziel_vault.path()).await;

    // Dieselbe Kennung, aber juenger -- die Regel "juengere Seite gewinnt"
    // verteidigt sie. Genau diese Sitzung bekam frueher trotzdem den
    // Herkunfts-Stempel, weil der Verbund ueber JEDE gemeinsame Kennung lief.
    sqlx::query(
        "INSERT INTO sessions (id, workspace_id, owner_user_id, title, updated_at, metadata_json) \
         VALUES (?, 'eigener-bereich', 'eigener-bereich', 'Von Hand korrigiert', \
                 '2026-06-01T10:00:00.000Z', '{}')",
    )
    .bind(ERFUNDENE_SITZUNG)
    .execute(ziel.pool())
    .await
    .unwrap();

    run_source_import(ziel.pool(), quelle.path(), ziel_vault.path(), false, false)
        .await
        .unwrap();

    let metadata =
        sqlx::query_scalar::<_, String>("SELECT metadata_json FROM sessions WHERE id = ?")
            .bind(ERFUNDENE_SITZUNG)
            .fetch_one(ziel.pool())
            .await
            .unwrap();
    let parsed = serde_json::from_str::<serde_json::Value>(&metadata).unwrap();
    assert!(
        parsed.get("import").is_none(),
        "die eigene juengere Sitzung behauptet, importiert zu sein: {metadata}"
    );
}

// ---------------------------------------------------------------------------
// Die bekannten Quellordner finden, statt nach einem Pfad zu fragen
// (Betreiber, 11.09.2026: „Niemand weiss, wo der Folder liegt.")
// ---------------------------------------------------------------------------

fn ordner_mit_datenbank(wurzel: &std::path::Path, name: &str) -> std::path::PathBuf {
    let ordner = wurzel.join(name);
    std::fs::create_dir_all(&ordner).unwrap();
    std::fs::write(ordner.join("app.db"), b"nicht leer").unwrap();
    ordner
}

#[test]
fn ein_bekannter_ordner_mit_daten_wird_gefunden() {
    let wurzel = tempfile::tempdir().unwrap();
    ordner_mit_datenbank(wurzel.path(), "hyprnote");
    let eigen = wurzel.path().join("media.zickert.mitschnitt");

    let treffer = super::scan::quellordner_unter(wurzel.path(), &eigen);

    assert_eq!(treffer.len(), 1, "der Bestand muss gefunden werden");
    assert!(treffer[0].ends_with("hyprnote"));
}

#[test]
fn ein_bekannter_ordner_ohne_daten_wird_nicht_angeboten() {
    // Gemessen auf dem Rechner des Betreibers am 11.09.2026: `com.hyprnote.stable`
    // existiert, traegt aber nur auth.json und den Fensterzustand. Ohne die
    // Datenpruefung erschiene er als Fund mit null Gespraechen.
    let wurzel = tempfile::tempdir().unwrap();
    let leer = wurzel.path().join("com.hyprnote.stable");
    std::fs::create_dir_all(&leer).unwrap();
    std::fs::write(leer.join("auth.json"), b"{}").unwrap();

    let treffer = super::scan::quellordner_unter(wurzel.path(), &wurzel.path().join("egal"));

    assert!(
        treffer.is_empty(),
        "ein Ordner ohne Gespraeche ist kein Angebot, gefunden: {treffer:?}"
    );
}

#[test]
fn der_eigene_datenordner_wird_nie_angeboten() {
    let wurzel = tempfile::tempdir().unwrap();
    let eigen = ordner_mit_datenbank(wurzel.path(), "hyprnote");

    let treffer = super::scan::quellordner_unter(wurzel.path(), &eigen);

    assert!(
        treffer.is_empty(),
        "ein Import in sich selbst darf nicht vorgeschlagen werden"
    );
}

#[test]
fn derselbe_ordner_unter_zwei_namen_erscheint_nur_einmal() {
    // Auf APFS sind `hyprnote` und `Hyprnote` derselbe Ordner. Hier als
    // Symlink nachgestellt, damit der Test auf jedem Dateisystem dasselbe
    // prueft: die Faltung laeuft ueber den aufgeloesten Pfad, nicht den Namen.
    let wurzel = tempfile::tempdir().unwrap();
    ordner_mit_datenbank(wurzel.path(), "hyprnote");
    std::os::unix::fs::symlink(
        wurzel.path().join("hyprnote"),
        wurzel.path().join("anarlog"),
    )
    .unwrap();

    let treffer = super::scan::quellordner_unter(wurzel.path(), &wurzel.path().join("egal"));

    assert_eq!(
        treffer.len(),
        1,
        "derselbe Bestand darf nicht zweimal angeboten werden, gefunden: {treffer:?}"
    );
}

#[test]
fn ein_ordnerbestand_ohne_datenbank_wird_gefunden() {
    // Eine alte Hyprnote-Installation, die nie migriert hat: keine app.db,
    // aber Sitzungsordner. Genau der Fall, den die Kollegen mitbringen.
    let wurzel = tempfile::tempdir().unwrap();
    let alt = wurzel.path().join("hyprnote");
    let sitzung = alt
        .join("sessions")
        .join("11111111-1111-4111-8111-111111111111");
    std::fs::create_dir_all(&sitzung).unwrap();
    std::fs::write(sitzung.join("_meta.json"), b"{}").unwrap();

    let treffer = super::scan::quellordner_unter(wurzel.path(), &wurzel.path().join("egal"));

    assert_eq!(treffer.len(), 1, "ein reiner Ordnerbestand ist ein Fund");
}
