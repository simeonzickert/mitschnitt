use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::audio;
use super::db_source::SourceDatabase;
use super::identity::resolve_local_identity;

/// Feste Schluessel statt freier Saetze -- die Oberflaeche uebersetzt sie, und
/// ein umformulierter Satz wuerde ihre Behandlung still aushebeln.
pub const PROBLEM_SOURCE_OPEN: &str = "source_open";
pub const PROBLEM_NOT_ENOUGH_SPACE: &str = "not_enough_space";
pub const PROBLEM_NO_DATA_FOUND: &str = "no_data_found";
pub const PROBLEM_SOURCE_UNREADABLE: &str = "source_unreadable";
pub const PROBLEM_SAME_INSTALLATION: &str = "same_installation";

/// Puffer ueber dem gemessenen Platzbedarf. Die Kopie der Quell-Datenbank und
/// die Tondateien sind bekannt; SQLite-Journale und Dateisystem-Verschnitt sind
/// es nicht.
const SPACE_HEADROOM: f64 = 1.05;

/// Was in diesem Quellordner ueberhaupt steckt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ImportSourceKind {
    Database,
    Folder,
    Both,
    None,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceScan {
    pub kind: ImportSourceKind,
    pub source_root: String,
    pub source_app: Option<String>,
    pub session_count: i64,
    pub transcript_count: i64,
    pub document_count: i64,
    pub audio_file_count: i64,
    pub audio_bytes: i64,
    pub free_bytes_on_target: i64,
    pub oldest_session_at: Option<String>,
    pub newest_session_at: Option<String>,
    pub problems: Vec<String>,
}

impl ImportSourceKind {
    pub fn hat_datenbank(self) -> bool {
        matches!(self, Self::Database | Self::Both)
    }

    pub fn hat_ordner(self) -> bool {
        matches!(self, Self::Folder | Self::Both)
    }
}

impl ImportSourceScan {
    pub fn blocked(&self) -> Option<&str> {
        self.problems
            .iter()
            .find(|problem| {
                matches!(
                    problem.as_str(),
                    PROBLEM_SOURCE_OPEN
                        | PROBLEM_NOT_ENOUGH_SPACE
                        | PROBLEM_NO_DATA_FOUND
                        | PROBLEM_SOURCE_UNREADABLE
                        | PROBLEM_SAME_INSTALLATION
                )
            })
            .map(String::as_str)
    }

    pub fn required_bytes(&self, copy_audio: bool) -> i64 {
        if copy_audio { self.audio_bytes } else { 0 }
    }
}

pub fn source_db_path(source_root: &Path) -> PathBuf {
    source_root.join("app.db")
}

/// Ein Ordnerbestand erkennt sich an `sessions/<uuid>/_meta.json` -- genau das,
/// was der geerbte Ordner-Importer liest.
pub fn folder_session_count(source_root: &Path) -> i64 {
    let Ok(entries) = std::fs::read_dir(source_root.join("sessions")) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.path().join("_meta.json").is_file())
        .count() as i64
}

/// Die Ordnernamen, unter denen anarlog und seine Vorgaenger ihre Daten
/// abgelegt haben. Die App hiess nacheinander Hyprnote, Char und anarlog, und
/// jeder Name konnte einen eigenen Ordner hinterlassen.
///
/// Warum eine feste Liste und kein Suchlauf ueber die Platte: ein Suchlauf
/// findet irgendwann einen gleichnamigen Ordner in einem fremden Projekt, und
/// ein falscher Quellordner ist schlimmer als keiner. Wer einen Bestand an
/// einem ungewoehnlichen Ort hat, waehlt ihn weiterhin von Hand.
const BEKANNTE_ORDNERNAMEN: &[&str] = &[
    "hyprnote",
    "anarlog",
    "char",
    "com.hyprnote.stable",
    "com.hyprnote.dev",
];

/// Traegt dieser Ordner ueberhaupt Gespraeche?
///
/// Die Existenz des Ordners genuegt NICHT. Gemessen am 11.09.2026 auf des Betreibers
/// Rechner: `com.hyprnote.stable` existiert, enthaelt aber nur `auth.json` und
/// den Fensterzustand. Ohne diese Pruefung erschiene er als Fund mit null
/// Gespraechen -- ein Angebot, das ins Leere fuehrt.
fn traegt_daten(kandidat: &Path) -> bool {
    source_db_path(kandidat).is_file() || folder_session_count(kandidat) > 0
}

/// Die bekannten Orte, an denen ein Bestand liegen kann -- nur solche, die
/// wirklich Gespraeche tragen.
///
/// Doppelte werden ueber den aufgeloesten Pfad entfernt, nicht ueber den Namen:
/// auf APFS sind `hyprnote` und `Hyprnote` derselbe Ordner, und ohne diese
/// Faltung erschiene ein Bestand zweimal in der Liste (gemessen 11.09.2026).
pub fn bekannte_quellordner(eigener_datenordner: &Path) -> Vec<PathBuf> {
    let Some(data_dir) = dirs::data_dir() else {
        return Vec::new();
    };
    quellordner_unter(&data_dir, eigener_datenordner)
}

/// Der pruefbare Kern von [`bekannte_quellordner`]: derselbe Ablauf, aber gegen
/// ein uebergebenes Wurzelverzeichnis statt gegen das echte des Systems.
pub fn quellordner_unter(data_dir: &Path, eigener_datenordner: &Path) -> Vec<PathBuf> {
    let eigen = eigener_datenordner
        .canonicalize()
        .unwrap_or_else(|_| eigener_datenordner.to_path_buf());

    let mut gesehen: Vec<PathBuf> = Vec::new();
    let mut treffer: Vec<PathBuf> = Vec::new();

    for name in BEKANNTE_ORDNERNAMEN {
        let kandidat = data_dir.join(name);
        if !kandidat.is_dir() || !traegt_daten(&kandidat) {
            continue;
        }
        let aufgeloest = kandidat.canonicalize().unwrap_or_else(|_| kandidat.clone());
        if aufgeloest == eigen || gesehen.contains(&aufgeloest) {
            continue;
        }
        gesehen.push(aufgeloest);
        treffer.push(kandidat);
    }

    treffer
}

fn guess_source_app(source_root: &Path) -> Option<String> {
    let name = source_root.file_name()?.to_string_lossy().to_lowercase();
    if name.contains("hyprnote") || name.contains("anarlog") {
        Some("anarlog".to_owned())
    } else {
        None
    }
}

pub async fn scan_import_source(
    pool: &SqlitePool,
    source_root: &Path,
    target_vault: &Path,
) -> crate::Result<ImportSourceScan> {
    let audio_files = audio::discover(source_root);
    let free_bytes = fs4::available_space(target_vault)
        .or_else(|_| fs4::available_space(Path::new("/")))
        .unwrap_or(0) as i64;

    let mut scan = ImportSourceScan {
        kind: ImportSourceKind::None,
        source_root: source_root.to_string_lossy().into_owned(),
        source_app: guess_source_app(source_root),
        session_count: 0,
        transcript_count: 0,
        document_count: 0,
        audio_file_count: audio_files.len() as i64,
        audio_bytes: audio::total_bytes(&audio_files) as i64,
        free_bytes_on_target: free_bytes,
        oldest_session_at: None,
        newest_session_at: None,
        problems: Vec::new(),
    };

    if !source_root.is_dir() {
        scan.problems.push(PROBLEM_SOURCE_UNREADABLE.to_owned());
        return Ok(scan);
    }

    let folder_sessions = folder_session_count(source_root);
    let has_folder = folder_sessions > 0;
    let db_path = source_db_path(source_root);
    let mut has_database = false;

    if db_path.is_file() {
        match SourceDatabase::open(&db_path).await {
            Ok(source) => {
                has_database = true;
                scan.session_count = source.count("sessions").await.unwrap_or(0);
                scan.transcript_count = source.count("transcripts").await.unwrap_or(0);
                scan.document_count = source.count("session_documents").await.unwrap_or(0);
                let (oldest, newest) = source.session_time_span().await.unwrap_or((None, None));
                scan.oldest_session_at = oldest;
                scan.newest_session_at = newest;

                if let (Ok(Some(source_workspace)), Ok(local)) = (
                    source.workspace_id().await,
                    resolve_local_identity(pool).await,
                ) && source_workspace == local.workspace_id
                {
                    scan.problems.push(PROBLEM_SAME_INSTALLATION.to_owned());
                }
            }
            Err(_) => scan.problems.push(PROBLEM_SOURCE_OPEN.to_owned()),
        }
    }

    scan.kind = match (has_database, has_folder) {
        (true, true) => ImportSourceKind::Both,
        (true, false) => ImportSourceKind::Database,
        (false, true) => ImportSourceKind::Folder,
        (false, false) => ImportSourceKind::None,
    };

    if !has_database {
        scan.session_count = folder_sessions;
    }

    if scan.kind == ImportSourceKind::None
        && !scan.problems.iter().any(|p| p == PROBLEM_SOURCE_OPEN)
    {
        scan.problems.push(PROBLEM_NO_DATA_FOUND.to_owned());
    }

    let needed = (scan.audio_bytes as f64 * SPACE_HEADROOM) as i64;
    if needed > 0 && free_bytes < needed {
        scan.problems.push(PROBLEM_NOT_ENOUGH_SPACE.to_owned());
    }

    Ok(scan)
}
