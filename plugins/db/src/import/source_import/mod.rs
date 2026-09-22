use std::path::Path;

use sqlx::{Row, SqlitePool};

pub mod audio;
mod db_source;
mod identity;
mod scan;
mod timing;

#[cfg(test)]
mod tests;

pub use scan::{ImportSourceKind, ImportSourceScan, scan_import_source};

/// Sucht die bekannten Orte ab und liefert einen fertigen Scan je Fund.
///
/// Betreiber, 11.09.2026: „Niemand von den Menschen, die anarlog oder Mitschnitt
/// benutzen, weiss, wo der Folder liegt." Ein Ordner-Auswahldialog setzt genau
/// dieses Wissen voraus. Diese Liste nimmt es ihnen ab; wer einen Bestand an
/// einem ungewoehnlichen Ort hat, waehlt weiterhin von Hand.
pub async fn find_import_sources(
    pool: &SqlitePool,
    target_vault: &Path,
) -> crate::Result<Vec<ImportSourceScan>> {
    let mut gefunden = Vec::new();
    for kandidat in scan::bekannte_quellordner(target_vault) {
        let ergebnis = scan_import_source(pool, &kandidat, target_vault).await?;
        // Ein Fund, der sich selbst als leer meldet, gehoert nicht in eine
        // Vorschlagsliste -- er wuerde als Angebot gelesen.
        if ergebnis.kind != ImportSourceKind::None {
            gefunden.push(ergebnis);
        }
    }
    Ok(gefunden)
}

use db_source::{IMPORT_TABLES, REMAPPED_COLUMNS, SourceDatabase, shared_columns};
use identity::resolve_local_identity;

static SOURCE_IMPORT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, serde::Serialize, specta::Type, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ImportRunReport {
    #[sqlx(rename = "id")]
    pub run_id: String,
    pub status: ImportRunStatus,
    pub dry_run: bool,
    #[sqlx(rename = "discovered_count")]
    pub discovered: i64,
    #[sqlx(rename = "imported_count")]
    pub imported: i64,
    #[sqlx(rename = "skipped_count")]
    pub skipped: i64,
    #[sqlx(rename = "conflict_count")]
    pub conflicts: i64,
    #[sqlx(rename = "error_count")]
    pub errors: i64,
    /// Gespraeche, nicht Datenbankzeilen. Am gemessenen Bestand stehen 395
    /// Gespraeche gegen 2.355 Zeilen ueber sechs Tabellen -- wer die
    /// Zeilensumme "Gespraeche" nennt, sagt das Sechsfache.
    #[sqlx(rename = "sessions_discovered")]
    pub conversations_discovered: i64,
    #[sqlx(rename = "sessions_imported")]
    pub conversations_imported: i64,
    pub audio_copied: i64,
    pub audio_bytes_copied: i64,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

/// Der Zustand eines Laufs, wie ihn die Oberflaeche unterscheiden muss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum ImportRunStatus {
    Running,
    Completed,
    CompletedWithIssues,
    Failed,
}

#[derive(Default)]
struct Tally {
    discovered: i64,
    imported: i64,
    /// Geloeschte Sitzungen kommen mit (sonst faellt beim Import eine
    /// Entscheidung ueber fremde Daten), zaehlen aber nicht als Gespraech:
    /// sie erscheinen in keiner Liste. Am gemessenen Bestand sind das 6 von 395.
    sessions_discovered: i64,
    sessions_imported: i64,
    skipped: i64,
    conflicts: i64,
    errors: i64,
    audio_copied: i64,
    audio_bytes_copied: i64,
}

pub async fn get_import_run(pool: &SqlitePool, run_id: &str) -> crate::Result<ImportRunReport> {
    Ok(sqlx::query_as::<_, ImportRunReport>(
        "SELECT id, status, dry_run, discovered_count, imported_count, skipped_count, \
                conflict_count, error_count, sessions_discovered, sessions_imported, \
                audio_copied, audio_bytes_copied, started_at, \
                completed_at, NULLIF(error,'') AS error \
         FROM migration_import_runs WHERE id = ?",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await?)
}

/// Importiert aus einem beliebigen Quellordner. Die Quelle wird ausschliesslich
/// gelesen; geschrieben wird nur in die eigene Datenbank und den eigenen
/// Datenordner.
pub async fn run_source_import(
    pool: &SqlitePool,
    source_root: &Path,
    target_vault: &Path,
    dry_run: bool,
    copy_audio: bool,
) -> crate::Result<String> {
    let _guard = SOURCE_IMPORT_LOCK.lock().await;

    let scan = scan_import_source(pool, source_root, target_vault).await?;
    let run_id = uuid::Uuid::new_v4().to_string();
    let source_root_text = source_root.to_string_lossy().into_owned();

    sqlx::query(
        "INSERT INTO migration_import_runs (id, importer_version, source_root, dry_run, status) \
         VALUES (?, ?, ?, ?, 'running')",
    )
    .bind(&run_id)
    .bind(anlg_db_app::LEGACY_IMPORTER_VERSION)
    .bind(&source_root_text)
    .bind(dry_run)
    .execute(pool)
    .await?;

    // Der Platzbedarf haengt am Ton-Schalter -- die Voranzeige rechnet
    // vorsichtshalber mit Ton, hier zaehlt der tatsaechliche Auftrag.
    let required = (scan.required_bytes(copy_audio) as f64 * 1.05) as i64;
    let space_blocked =
        required > 0 && scan.free_bytes_on_target < required && !dry_run && copy_audio;
    let blocking = scan
        .blocked()
        .filter(|problem| *problem != scan::PROBLEM_NOT_ENOUGH_SPACE)
        .map(str::to_owned)
        .or_else(|| space_blocked.then(|| scan::PROBLEM_NOT_ENOUGH_SPACE.to_owned()));

    if let Some(problem) = blocking {
        fail_run(pool, &run_id, &problem, &Tally::default()).await?;
        return Ok(run_id);
    }

    let mut tally = Tally::default();

    match import_everything(
        pool,
        source_root,
        target_vault,
        &scan,
        &run_id,
        dry_run,
        copy_audio,
        &mut tally,
    )
    .await
    {
        Ok(()) => finish_run(pool, &run_id, &tally).await?,
        Err(error) => fail_run(pool, &run_id, &error.to_string(), &tally).await?,
    }

    Ok(run_id)
}

/// Gibt zurueck, ob der Lauf etwas offen gelassen hat.
async fn ordner_lauf(
    pool: &SqlitePool,
    source_root: &Path,
    dry_run: bool,
    tally: &mut Tally,
) -> bool {
    match super::legacy_vault::import_foreign_vault(pool, source_root, dry_run).await {
        Ok(folder_run_id) => match get_import_run(pool, &folder_run_id).await {
            Ok(folder) => {
                tally.discovered += folder.discovered;
                tally.imported += folder.imported;
                tally.skipped += folder.skipped;
                tally.conflicts += folder.conflicts;
                tally.errors += folder.errors;
                folder.errors > 0
            }
            Err(error) => {
                tracing::warn!(%error, "Ordnerlauf ohne lesbares Protokoll");
                tally.errors += 1;
                false
            }
        },
        Err(error) => {
            tracing::warn!(%error, "Ordnerbestand konnte nicht importiert werden");
            tally.errors += 1;
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn import_everything(
    pool: &SqlitePool,
    source_root: &Path,
    target_vault: &Path,
    scan: &ImportSourceScan,
    run_id: &str,
    dry_run: bool,
    copy_audio: bool,
    tally: &mut Tally,
) -> crate::Result<()> {
    // Ordnerbestand zuerst, Datenbank danach: der Datenbankpfad vergleicht
    // Zeitstempel gegen das, was bereits dasteht, und gewinnt damit je Gespraech
    // nur dort, wo er wirklich juenger ist. Umgekehrt (Datenbank zuerst) waere
    // ein Ordnerstand, der juenger ist als die Datenbank, nur noch als Konflikt
    // vermerkt statt uebernommen -- gemessen 10.09.2026 an 854 Eintraegen.
    let hat_ordner = scan.kind.hat_ordner();
    let ordner_offen = if hat_ordner {
        ordner_lauf(pool, source_root, dry_run, tally).await
    } else {
        false
    };

    if scan.kind.hat_datenbank() {
        import_from_database(pool, source_root, run_id, dry_run, tally).await?;
    }

    // Der geerbte Ordner-Importer verarbeitet die Dateien in Fundreihenfolge und
    // legt einen Anhang beiseite ("missing dependency"), wenn dessen Sitzung noch
    // nicht dasteht -- am gemessenen Bestand traf das genau einen von 2.480
    // Eintraegen. Der Nachschlag laeuft nur dann und ist billig: was schon steht,
    // erkennt der Importer an seiner Pruefsumme und ueberspringt es. Ohne ihn
    // braeuchte der Bestand einen zweiten Import von Hand, um vollstaendig zu sein.
    if ordner_offen && !dry_run {
        ordner_lauf(pool, source_root, dry_run, tally).await;
    }

    // Was der ORDNER geliefert hat, bekommt sein Etikett hier. Der
    // Datenbankzweig stempelt nur, was er selbst gewonnen hat -- am gemessenen
    // Bestand 80 von 389 Sitzungen; die uebrigen 309 kamen aus dem Ordner und
    // standen ohne Herkunft und ohne Warnung vor erfundenen Wortzeiten da.
    // Ein reiner Ordnerbestand hatte gar kein Etikett.
    //
    // Gestempelt wird nur, was der Ordner-Importer wirklich EINGEFUEGT hat
    // (`status = 'inserted'`), nicht was er nur wiedererkannt hat: eine eigene
    // Sitzung, die zufaellig dieselbe Kennung traegt, darf nicht behaupten,
    // sie sei importiert.
    if hat_ordner && !dry_run {
        stempel_fuer_ordnerdateien(pool, run_id, source_root, scan.kind, tally).await?;
    }

    if copy_audio && !dry_run {
        import_audio(pool, source_root, target_vault, tally).await?;
    } else if copy_audio {
        tally.discovered += scan.audio_file_count;
        tally.skipped += scan.audio_file_count;
    }

    Ok(())
}

async fn import_from_database(
    pool: &SqlitePool,
    source_root: &Path,
    run_id: &str,
    dry_run: bool,
    tally: &mut Tally,
) -> crate::Result<()> {
    let source = SourceDatabase::open(&scan::source_db_path(source_root)).await?;
    let local = resolve_local_identity(pool).await?;

    for table in IMPORT_TABLES {
        tally.discovered += source.count(table).await.unwrap_or(0);
    }
    tally.sessions_discovered +=
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE deleted_at IS NULL")
            .fetch_one(&source.pool)
            .await
            .unwrap_or(0);

    // Die Spaltenlisten werden VOR dem Belegen der Verbindung geholt. Der
    // Zielpool hat im Zweifel genau eine Verbindung; wer sie haelt und
    // gleichzeitig eine zweite anfragt, wartet auf sich selbst, bis der Pool
    // in eine Zeitueberschreitung laeuft (an genau dieser Stelle erlebt).
    let mut layouts = Vec::new();
    for table in IMPORT_TABLES {
        layouts.push((*table, shared_columns(&source.pool, pool, table).await?));
    }

    let source_db_copy = source.attached_path();
    let mut connection = pool.acquire().await?;

    // Der Pfad steht im Anweisungstext, nicht als gebundener Wert: SQLite
    // nimmt einen Parameter an dieser Stelle zwar entgegen, haengt dann aber
    // eine namenlose temporaere Datenbank an, und jede Abfrage darauf meldet
    // "no such table" (gemessen 10.09.2026). Der Pfad ist unser eigener
    // Zwischenspeicher, kein Eingabewert; die Hochkommas werden verdoppelt.
    let attach = format!(
        "ATTACH DATABASE '{}' AS import_src",
        source_db_copy.replace('\'', "''")
    );
    sqlx::query(sqlx::AssertSqlSafe(attach))
        .execute(&mut *connection)
        .await?;

    // Alle sechs Tabellen und beide Stempel-Schleifen in EINER Transaktion.
    // Ohne sie ist der schlechteste Ausgang nicht "nichts uebernommen",
    // sondern "halb uebernommen": scheitert `transcripts`, staenden 395
    // Sitzungen ohne ein einziges Transkript da. Ein halber Bestand sieht aus
    // wie ein ganzer, und niemand sucht danach. Deshalb bricht lieber alles.
    //
    // ATTACH und DETACH bleiben ausserhalb -- SQLite erlaubt beides nicht
    // innerhalb einer offenen Transaktion.
    let mut zwischenstand = Tally::default();
    let outcome = uebernahme_in_einer_transaktion(
        &mut connection,
        &layouts,
        &local,
        run_id,
        dry_run,
        &mut zwischenstand,
    )
    .await;

    sqlx::query("DETACH DATABASE import_src")
        .execute(&mut *connection)
        .await?;
    drop(connection);

    // Gezaehlt wird, was festgeschrieben WURDE -- oder beim Trockenlauf, was
    // festgeschrieben WORDEN WAERE. Beides sind echte Zahlen aus demselben
    // Lauf; nur der Abschluss unterscheidet sich (COMMIT gegen ROLLBACK).
    if outcome.is_ok() {
        tally.imported += zwischenstand.imported;
        tally.skipped += zwischenstand.skipped;
        tally.conflicts += zwischenstand.conflicts;
        tally.errors += zwischenstand.errors;
        tally.sessions_imported += zwischenstand.sessions_imported;
    }

    outcome
}

async fn uebernahme_in_einer_transaktion(
    connection: &mut sqlx::SqliteConnection,
    layouts: &[(&str, Vec<String>)],
    local: &identity::LocalIdentity,
    run_id: &str,
    dry_run: bool,
    tally: &mut Tally,
) -> crate::Result<()> {
    sqlx::query("BEGIN").execute(&mut *connection).await?;

    match copy_tables(&mut *connection, layouts, local, run_id, tally).await {
        Ok(()) => {
            // Der Trockenlauf nimmt dieselbe Arbeit am Ende zurueck. Frueher
            // meldete er strukturell "0 kaemen herueber" -- ein Satz, der
            // direkt ueber dem Knopf stand, der alles importiert, und der
            // sagte, es gebe hier nichts zu tun. Jetzt beziffert er, was
            // KAEME, und schreibt trotzdem nichts.
            let abschluss = if dry_run { "ROLLBACK" } else { "COMMIT" };
            sqlx::query(abschluss).execute(&mut *connection).await?;
            Ok(())
        }
        Err(error) => {
            // Das Zuruecknehmen darf den urspruenglichen Fehler nicht
            // verdecken: er ist die Aussage, das Zuruecknehmen nur Aufraeumen.
            if let Err(rollback) = sqlx::query("ROLLBACK").execute(&mut *connection).await {
                tracing::error!(%rollback, "Zuruecknehmen der Uebernahme fehlgeschlagen");
            }
            Err(error)
        }
    }
}

async fn copy_tables(
    connection: &mut sqlx::SqliteConnection,
    layouts: &[(&str, Vec<String>)],
    local: &identity::LocalIdentity,
    run_id: &str,
    tally: &mut Tally,
) -> crate::Result<()> {
    for (table, columns) in layouts {
        if columns.is_empty() {
            continue;
        }

        let has_updated_at = columns.iter().any(|column| column == "updated_at");

        // Wer schon dasteht und juenger ist, bleibt unangetastet. Diese Zahl
        // wird VOR dem Schreiben gemessen, sonst ist sie hinterher nicht mehr
        // von einem Treffer zu unterscheiden.
        if has_updated_at {
            let sql = format!(
                "SELECT COUNT(*) FROM import_src.{table} AS incoming \
                 JOIN main.{table} AS existing ON existing.id = incoming.id \
                 WHERE existing.updated_at >= incoming.updated_at"
            );
            tally.conflicts += sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
                .fetch_one(&mut *connection)
                .await
                .unwrap_or(0);
        }

        // Der Ordner-Spiegel ist AUS der Datenbank erzeugt: dieselbe
        // Zusammenfassung liegt einmal als Zeile in `session_documents` und
        // einmal als `_summary.md` daneben. Beide Seiten vergeben eigene
        // Kennungen, der Hausabgleich vergleicht aber ueber `id` -- also
        // landete dasselbe Dokument zweimal (gemessen 10.09.2026: 146
        // Dubletten, 131 davon nach Normalisierung zeichengleich, die
        // uebrigen 19 unterschieden sich nur in der Markdown-Auszeichnung,
        // Aehnlichkeit ueber 0,99).
        //
        // Statt zu loeschen, uebernimmt das Datenbank-Dokument die Kennung der
        // schon vorhandenen Zeile. Dadurch greift der Zeitstempel-Waechter
        // unten ganz normal: die juengere Seite gewinnt den Platz, und es
        // entsteht keine zweite Zeile. Nur wo BEIDE Seiten genau eine Zeile
        // fuer denselben Platz haben -- sonst waeren die zwei Sitzungen, die
        // schon in der Quelle zwei Zusammenfassungen tragen, stillschweigend
        // auf eine zusammengezogen.
        // Welche Sitzungen dieser Lauf tatsaechlich anfasst -- festgehalten
        // VOR dem Schreiben, denn hinterher ist eine uebernommene nicht mehr
        // von einer unveraenderten zu unterscheiden. Der Stempel unten haengt
        // daran: frueher lief er ueber JEDE Kennung, die in beiden Datenbanken
        // vorkommt, und behauptete damit auch von eigenen juengeren Sitzungen,
        // sie seien importiert -- ausgerechnet von denen, die die Regel
        // "juengere Seite gewinnt" gerade verteidigt hatte.
        if *table == "sessions" {
            sqlx::query("DROP TABLE IF EXISTS temp.import_beruehrte_sitzungen")
                .execute(&mut *connection)
                .await?;
            sqlx::query(
                "CREATE TEMP TABLE import_beruehrte_sitzungen AS \
                 SELECT incoming.id AS id, incoming.workspace_id AS quell_arbeitsbereich \
                 FROM import_src.sessions AS incoming \
                 LEFT JOIN main.sessions AS vorhanden ON vorhanden.id = incoming.id \
                 WHERE vorhanden.id IS NULL \
                    OR incoming.updated_at > vorhanden.updated_at",
            )
            .execute(&mut *connection)
            .await?;
            // Die Gespraechszahl misst, was NACH dem Lauf hier steht -- nicht,
            // welcher der beiden Wege es gebracht hat. Zaehlte sie nur die vom
            // Datenbankweg gewonnenen Plaetze, meldete sie am gemessenen Bestand
            // 74 statt 389, weil der Ordnerlauf die uebrigen schon gebracht
            // hatte. Fuer den Menschen zaehlt, wie viele Gespraeche da sind.
            //
            // Die Zeile steht NACH dem Einfuegen der Sitzungen (weiter unten in
            // derselben Runde), deshalb wird sie dort gesetzt, nicht hier.
        }

        if *table == "session_documents" {
            sqlx::query("DROP TABLE IF EXISTS temp.import_doc_map")
                .execute(&mut *connection)
                .await?;
            sqlx::query(
                "CREATE TEMP TABLE import_doc_map AS \
                 SELECT incoming.id AS quelle_id, vorhanden.id AS ziel_id \
                 FROM import_src.session_documents AS incoming \
                 JOIN main.session_documents AS vorhanden \
                   ON vorhanden.session_id = incoming.session_id \
                  AND vorhanden.kind = incoming.kind \
                  AND IFNULL(vorhanden.template_id,'') = IFNULL(incoming.template_id,'') \
                 WHERE vorhanden.deleted_at IS NULL \
                   AND vorhanden.id NOT IN (SELECT id FROM import_src.session_documents) \
                   AND (SELECT COUNT(*) FROM import_src.session_documents AS i2 \
                        WHERE i2.session_id = incoming.session_id \
                          AND i2.kind = incoming.kind \
                          AND IFNULL(i2.template_id,'') = IFNULL(incoming.template_id,'')) = 1 \
                   AND (SELECT COUNT(*) FROM main.session_documents AS v2 \
                        WHERE v2.session_id = incoming.session_id \
                          AND v2.kind = incoming.kind \
                          AND IFNULL(v2.template_id,'') = IFNULL(incoming.template_id,'') \
                          AND v2.deleted_at IS NULL \
                          AND v2.id NOT IN (SELECT id FROM import_src.session_documents)) = 1",
            )
            .execute(&mut *connection)
            .await?;
        }

        let column_list = columns.join(", ");
        let select_list = columns
            .iter()
            .map(|column| {
                if *table == "session_documents" && column == "id" {
                    "COALESCE(karte.ziel_id, incoming.id) AS id".to_owned()
                } else if REMAPPED_COLUMNS.contains(&column.as_str()) {
                    let value = if column == "workspace_id" {
                        &local.workspace_id
                    } else {
                        &local.owner_user_id
                    };
                    format!("'{}' AS {column}", value.replace('\'', "''"))
                } else {
                    format!("incoming.{column}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        let updates = columns
            .iter()
            .filter(|column| *column != "id")
            .map(|column| format!("{column} = excluded.{column}"))
            .collect::<Vec<_>>()
            .join(", ");

        // Kein DELETE, kein REPLACE: bestehende eigene Zeilen koennen nur
        // ueberschrieben werden, wenn die Quelle juenger ist -- und Zeilen, die
        // die Quelle nicht kennt, werden nie beruehrt.
        let guard = if has_updated_at {
            format!("WHERE excluded.updated_at > main.{table}.updated_at")
        } else {
            String::new()
        };
        let quelle = if *table == "session_documents" {
            format!(
                "import_src.{table} AS incoming \
                 LEFT JOIN temp.import_doc_map AS karte ON karte.quelle_id = incoming.id"
            )
        } else {
            format!("import_src.{table} AS incoming")
        };
        let sql = format!(
            "INSERT INTO main.{table} ({column_list}) \
             SELECT {select_list} FROM {quelle} \
             WHERE TRUE \
             ON CONFLICT(id) DO UPDATE SET {updates} {guard}"
        );

        // Kein Weitermachen nach einem Fehlschlag: eine uebersprungene Tabelle
        // ist kein "ein Fehler", sondern ein Bestand ohne Transkripte, ohne
        // Teilnehmer oder ohne Dokumente. Der Fehler reisst die Transaktion mit.
        let result = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&mut *connection)
            .await
            .inspect_err(|error| tracing::error!(%table, %error, "Uebernahme abgebrochen"))?;
        tally.imported += result.rows_affected() as i64;

        if *table == "sessions" {
            tally.sessions_imported += sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM import_src.sessions AS quelle \
                 JOIN main.sessions AS vorhanden ON vorhanden.id = quelle.id \
                 WHERE quelle.deleted_at IS NULL AND vorhanden.deleted_at IS NULL",
            )
            .fetch_one(&mut *connection)
            .await
            .unwrap_or(0);
        }
    }

    tally.skipped += (tally.discovered - tally.imported).max(0);

    stamp_provenance(connection, run_id, local).await?;
    stamp_timing_quality(connection).await?;

    Ok(())
}

/// Etikett fuer einen Bestand, der nur aus Ordnerdateien kam. Ohne
/// Datenbankzweig gibt es keine `import_src`-Datenbank, an der sich haengen
/// liesse -- die Liste der angefassten Sitzungen kommt deshalb aus dem
/// Laufprotokoll des Ordner-Importers.
async fn stempel_fuer_ordnerdateien(
    pool: &SqlitePool,
    run_id: &str,
    source_root: &Path,
    kind: ImportSourceKind,
    tally: &mut Tally,
) -> crate::Result<()> {
    let sitzungen = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT target.target_id \
         FROM migration_import_targets AS target \
         JOIN migration_import_runs AS run ON run.id = target.run_id \
         WHERE target.table_name = 'sessions' \
           AND target.status = 'inserted' \
           AND run.source_root = ? \
           AND run.dry_run = 0",
    )
    .bind(source_root.to_string_lossy().as_ref())
    .fetch_all(pool)
    .await?;

    let imported_at = chrono_now();
    let source_root_text = source_root.to_string_lossy().into_owned();

    for id in &sitzungen {
        let Some(metadata_json) = sqlx::query_scalar::<_, String>(
            "SELECT metadata_json FROM sessions \
             WHERE id = ? AND deleted_at IS NULL \
               AND COALESCE(json_extract(metadata_json, '$.import.run_id'), '') <> ?",
        )
        .bind(id)
        .bind(run_id)
        .fetch_optional(pool)
        .await?
        else {
            continue;
        };

        let herkunft = serde_json::json!({
            "source_kind": "folder",
            "source_app": "anarlog",
            "source_root": source_root_text,
            "run_id": run_id,
            "imported_at": imported_at,
            "original_workspace_id": "",
        });
        sqlx::query("UPDATE sessions SET metadata_json = ? WHERE id = ?")
            .bind(mit_herkunft(&metadata_json, herkunft))
            .bind(id)
            .execute(pool)
            .await?;
    }

    // Zeitqualitaet fuer die Transkripte dieser Sitzungen -- dieselbe
    // Einstufung wie im Datenbankzweig, damit die Oberflaeche nicht zwei
    // verschiedene Antworten auf dieselbe Frage bekommt.
    for id in &sitzungen {
        let zeilen = sqlx::query(
            "SELECT id, metadata_json, \
                    instr(words_json, 'synthetic_text') > 0, \
                    (instr(words_json, 'provider_word') > 0 \
                     OR instr(words_json, 'decoder_frame') > 0) \
             FROM transcripts WHERE session_id = ?",
        )
        .bind(id)
        .fetch_all(pool)
        .await?;

        for zeile in zeilen {
            let transcript_id: String = zeile.try_get(0)?;
            let metadata_json: String = zeile.try_get(1).unwrap_or_else(|_| "{}".to_owned());
            let synthetic: i64 = zeile.try_get(2).unwrap_or(0);
            let genuine: i64 = zeile.try_get(3).unwrap_or(0);
            let Some(quality) = timing::classify(synthetic > 0, genuine > 0) else {
                continue;
            };
            sqlx::query("UPDATE transcripts SET metadata_json = ? WHERE id = ?")
                .bind(timing::with_timing_quality(&metadata_json, Some(quality)))
                .bind(&transcript_id)
                .execute(pool)
                .await?;
        }
    }

    // Ohne Datenbankzweig gibt es keine andere Quelle fuer die Gespraechszahl.
    // Mit ihm hat er sie bereits gesetzt, und zwar als "was steht jetzt hier" --
    // dazuzuzaehlen wuerde doppelt zaehlen.
    if !kind.hat_datenbank() {
        tally.sessions_imported += sitzungen.len() as i64;
    }
    Ok(())
}

/// Herkunft in ein `metadata_json`-Objekt schreiben, ohne bestehende Felder zu
/// verlieren.
fn mit_herkunft(metadata_json: &str, herkunft: serde_json::Value) -> String {
    let mut value = serde_json::from_str::<serde_json::Value>(metadata_json)
        .ok()
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    if let Some(object) = value.as_object_mut() {
        object.insert("import".to_owned(), herkunft);
    }
    value.to_string()
}

/// Herkunft an die Sitzung schreiben. Ohne sie ist eine importierte Sitzung
/// spaeter nicht mehr von einer eigenen zu unterscheiden.
async fn stamp_provenance(
    connection: &mut sqlx::SqliteConnection,
    run_id: &str,
    local: &identity::LocalIdentity,
) -> crate::Result<()> {
    let source_root = sqlx::query_scalar::<_, String>(
        "SELECT source_root FROM main.migration_import_runs WHERE id = ?",
    )
    .bind(run_id)
    .fetch_optional(&mut *connection)
    .await?
    .unwrap_or_default();

    let rows = sqlx::query(
        "SELECT b.id, main.sessions.metadata_json, b.quell_arbeitsbereich \
         FROM temp.import_beruehrte_sitzungen AS b \
         JOIN main.sessions ON main.sessions.id = b.id",
    )
    .fetch_all(&mut *connection)
    .await?;

    let imported_at = chrono_now();

    for row in rows {
        let id: String = row.try_get(0)?;
        let metadata_json: String = row.try_get(1).unwrap_or_else(|_| "{}".to_owned());
        let original_workspace_id: String = row.try_get(2).unwrap_or_default();

        let mut value = serde_json::from_str::<serde_json::Value>(&metadata_json)
            .ok()
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

        if let Some(object) = value.as_object_mut() {
            object.insert(
                "import".to_owned(),
                serde_json::json!({
                    "source_kind": "database",
                    "source_app": "anarlog",
                    "source_root": source_root,
                    "run_id": run_id,
                    "imported_at": imported_at,
                    "original_workspace_id": original_workspace_id,
                }),
            );
        }

        sqlx::query("UPDATE main.sessions SET metadata_json = ? WHERE id = ?")
            .bind(value.to_string())
            .bind(&id)
            .execute(&mut *connection)
            .await?;
    }

    let _ = local;
    Ok(())
}

/// Einmal beim Import berechnet, damit die Oberflaeche nicht 337 MB JSON
/// durchsuchen muss. Die Einstufung selbst kommt aus `instr` in SQLite -- die
/// Woerter verlassen die Datenbank nie.
async fn stamp_timing_quality(connection: &mut sqlx::SqliteConnection) -> crate::Result<()> {
    let rows = sqlx::query(
        "SELECT main.transcripts.id, main.transcripts.metadata_json, \
                instr(main.transcripts.words_json, 'synthetic_text') > 0 AS hat_erfundene, \
                (instr(main.transcripts.words_json, 'provider_word') > 0 \
                 OR instr(main.transcripts.words_json, 'decoder_frame') > 0) AS hat_echte \
         FROM import_src.transcripts AS incoming \
         JOIN main.transcripts ON main.transcripts.id = incoming.id",
    )
    .fetch_all(&mut *connection)
    .await?;

    for row in rows {
        let id: String = row.try_get(0)?;
        let metadata_json: String = row.try_get(1).unwrap_or_else(|_| "{}".to_owned());
        let synthetic: i64 = row.try_get(2).unwrap_or(0);
        let genuine: i64 = row.try_get(3).unwrap_or(0);

        let Some(quality) = timing::classify(synthetic > 0, genuine > 0) else {
            continue;
        };

        sqlx::query("UPDATE main.transcripts SET metadata_json = ? WHERE id = ?")
            .bind(timing::with_timing_quality(&metadata_json, Some(quality)))
            .bind(&id)
            .execute(&mut *connection)
            .await?;
    }

    Ok(())
}

async fn import_audio(
    pool: &SqlitePool,
    source_root: &Path,
    target_vault: &Path,
    tally: &mut Tally,
) -> crate::Result<()> {
    let files = audio::discover(source_root);
    tally.discovered += files.len() as i64;

    for entry in files {
        let known = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE id = ?")
            .bind(&entry.session_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        if known == 0 {
            tally.skipped += 1;
            continue;
        }

        let destination = target_vault
            .join("sessions")
            .join(&entry.session_id)
            .join(&entry.relative_path);

        // Eine bereits vorhandene Datei gleicher Groesse wird nicht noch einmal
        // kopiert -- sonst kostet der zweite Lauf 6,6 GB Schreiben fuer nichts.
        let already_there = std::fs::metadata(&destination)
            .map(|meta| meta.len() == entry.size_bytes)
            .unwrap_or(false);

        if !already_there {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            match std::fs::copy(&entry.path, &destination) {
                Ok(bytes) => {
                    tally.audio_copied += 1;
                    tally.audio_bytes_copied += bytes as i64;
                }
                Err(error) => {
                    tracing::warn!(pfad = %entry.path.display(), %error, "Tondatei nicht kopiert");
                    tally.errors += 1;
                    continue;
                }
            }
        }

        register_attachment(pool, &entry).await?;
        tally.imported += 1;
    }

    Ok(())
}

/// Form und Kennung folgen exakt dem, was die eigene App selbst schreibt
/// (gemessen 10.09.2026): `session-audio:<sitzung>`, `storage_kind` =
/// `local_file`, dazu eine Zeile in `attachment_local_state` mit
/// `availability = 'present'` -- diese Zeile ist es, die den Ton abspielbar
/// macht.
async fn register_attachment(pool: &SqlitePool, entry: &audio::SourceAudio) -> crate::Result<()> {
    let attachment_id = audio::attachment_id(entry);
    let workspace_id = resolve_local_identity(pool).await?.workspace_id;

    sqlx::query(
        "INSERT INTO session_attachments \
           (id, workspace_id, session_id, filename, relative_path, content_type, size_bytes, \
            storage_kind, source_type, source_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 'local_file', 'session_audio', 'primary') \
         ON CONFLICT(id) DO UPDATE SET \
           workspace_id = excluded.workspace_id, filename = excluded.filename, \
           relative_path = excluded.relative_path, content_type = excluded.content_type, \
           size_bytes = excluded.size_bytes, storage_kind = excluded.storage_kind, \
           deleted_at = NULL",
    )
    .bind(&attachment_id)
    .bind(&workspace_id)
    .bind(&entry.session_id)
    .bind(&entry.filename)
    .bind(&entry.relative_path)
    .bind(entry.content_type)
    .bind(entry.size_bytes as i64)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO attachment_local_state \
           (attachment_id, session_id, relative_path, availability) \
         VALUES (?, ?, ?, 'present') \
         ON CONFLICT(attachment_id) DO UPDATE SET \
           session_id = excluded.session_id, relative_path = excluded.relative_path, \
           availability = 'present'",
    )
    .bind(&attachment_id)
    .bind(&entry.session_id)
    .bind(&entry.relative_path)
    .execute(pool)
    .await?;

    Ok(())
}

async fn finish_run(pool: &SqlitePool, run_id: &str, tally: &Tally) -> crate::Result<()> {
    let status = if tally.errors > 0 {
        ImportRunStatus::CompletedWithIssues
    } else {
        ImportRunStatus::Completed
    };

    sqlx::query(
        "UPDATE migration_import_runs \
         SET status = ?, discovered_count = ?, imported_count = ?, skipped_count = ?, \
             conflict_count = ?, error_count = ?, sessions_discovered = ?, \
             sessions_imported = ?, audio_copied = ?, audio_bytes_copied = ?, \
             completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ?",
    )
    .bind(status)
    .bind(tally.discovered)
    // Der Trockenlauf beziffert jetzt, was KAEME -- die Null hier war frueher
    // einkodiert und machte aus jeder Vorschau den Satz "es gibt nichts zu tun".
    .bind(tally.imported)
    .bind(tally.skipped)
    .bind(tally.conflicts)
    .bind(tally.errors)
    .bind(tally.sessions_discovered)
    .bind(tally.sessions_imported)
    .bind(tally.audio_copied)
    .bind(tally.audio_bytes_copied)
    .bind(run_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Auch ein gescheiterter Lauf traegt seine Zahlen. Vorher blieben sie auf
/// null stehen, und die Oberflaeche meldete "0 Gespraeche waren schon
/// herueber" -- selbst wenn fuenf Tabellen und Gigabyte Ton bereits lagen. Wer
/// nach einem Abbruch aufraeumen oder fortsetzen will, braucht den Stand, nicht
/// eine Null.
async fn fail_run(
    pool: &SqlitePool,
    run_id: &str,
    error: &str,
    tally: &Tally,
) -> crate::Result<()> {
    sqlx::query(
        "UPDATE migration_import_runs \
         SET status = 'failed', error = ?, discovered_count = ?, imported_count = ?, \
             skipped_count = ?, conflict_count = ?, error_count = ?, \
             sessions_discovered = ?, sessions_imported = ?, audio_copied = ?, \
             audio_bytes_copied = ?, \
             completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ?",
    )
    .bind(error)
    .bind(tally.discovered)
    .bind(tally.imported)
    .bind(tally.skipped)
    .bind(tally.conflicts)
    .bind(tally.errors.max(1))
    .bind(tally.sessions_discovered)
    .bind(tally.sessions_imported)
    .bind(tally.audio_copied)
    .bind(tally.audio_bytes_copied)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    let days = secs / 86_400;
    let rest = secs % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Howard Hinnants `civil_from_days`. Ein Datumspaket allein fuer einen
/// Zeitstempel waere zu teuer, und SQLite kann ihn hier nicht liefern, weil er
/// in JSON landet.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
