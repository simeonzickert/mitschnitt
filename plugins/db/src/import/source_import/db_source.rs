use std::path::{Path, PathBuf};

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};

/// Eine geoeffnete Kopie der Quell-Datenbank. Die Quelle selbst wird NIE
/// geoeffnet: die alte App haelt sie im WAL-Modus, und ein direkter Lesezugriff
/// liefert dann still Unvollstaendiges. Kopiert wird immer mit `-wal` und
/// `-shm`, damit SQLite den Journalrest beim Oeffnen der Kopie nachspielt.
pub struct SourceDatabase {
    pub pool: SqlitePool,
    copy_path: PathBuf,
    _scratch: tempfile::TempDir,
}

impl SourceDatabase {
    pub async fn open(source_db: &Path) -> crate::Result<Self> {
        let scratch = tempfile::tempdir()?;
        let target = scratch.path().join("source.db");

        std::fs::copy(source_db, &target)?;
        // Die Beinamen tragen den Rest der Schreibvorgaenge. Fehlen sie, ist die
        // Quelle sauber geschlossen -- das ist kein Fehler.
        for suffix in ["-wal", "-shm"] {
            let sidecar = sidecar_path(source_db, suffix);
            if sidecar.exists() {
                std::fs::copy(&sidecar, format!("{}{suffix}", target.display()))?;
            }
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}", target.display()))
            .await?;

        Ok(Self {
            pool,
            copy_path: target,
            _scratch: scratch,
        })
    }

    /// Pfad der Kopie -- was `ATTACH DATABASE` bekommt. Nie der Quellpfad.
    pub fn attached_path(&self) -> String {
        self.copy_path.to_string_lossy().into_owned()
    }

    pub async fn count(&self, table: &str) -> crate::Result<i64> {
        count_rows(&self.pool, table).await
    }

    pub async fn workspace_id(&self) -> crate::Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT workspace_id FROM sessions WHERE workspace_id <> '' \
             GROUP BY workspace_id ORDER BY COUNT(*) DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn session_time_span(&self) -> crate::Result<(Option<String>, Option<String>)> {
        let row = sqlx::query(
            "SELECT MIN(NULLIF(created_at,'')), MAX(NULLIF(created_at,'')) FROM sessions \
             WHERE deleted_at IS NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok((row.try_get(0).ok().flatten(), row.try_get(1).ok().flatten()))
    }
}

fn sidecar_path(db: &Path, suffix: &str) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Tabellennamen stammen ausschliesslich aus fest verdrahteten Listen im
/// Importer, nie aus Eingaben.
pub async fn count_rows(pool: &SqlitePool, table: &str) -> crate::Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    Ok(sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await?)
}

/// Die Tabellen, die kopiert werden -- in Reihenfolge ihrer Abhaengigkeiten:
/// Menschen und Termine stehen vor den Sitzungen, alles Sitzungsgebundene
/// danach.
pub const IMPORT_TABLES: &[&str] = &[
    "humans",
    "events",
    "sessions",
    "session_documents",
    "transcripts",
    "session_participants",
];

/// Spalten, die auf die eigene Installation umgeschluesselt werden.
pub const REMAPPED_COLUMNS: &[&str] = &["workspace_id", "owner_user_id"];

/// Liest die Spaltennamen einer Tabelle. Beide Seiten werden geschnitten, damit
/// ein Schema-Unterschied nicht in einen Laufzeitfehler kippt, sondern in
/// weniger uebertragene Spalten.
pub async fn shared_columns(
    source: &SqlitePool,
    target: &SqlitePool,
    table: &str,
) -> crate::Result<Vec<String>> {
    let source_columns = table_columns(source, table).await?;
    let target_columns = table_columns(target, table).await?;
    Ok(source_columns
        .into_iter()
        .filter(|column| target_columns.iter().any(|other| other == column))
        .collect())
}

async fn table_columns(pool: &SqlitePool, table: &str) -> crate::Result<Vec<String>> {
    let sql = format!("SELECT name FROM pragma_table_info('{table}')");
    Ok(sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .fetch_all(pool)
        .await?)
}
