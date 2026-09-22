use std::sync::Arc;

use anlg_db_core::Db;

pub(crate) const DB_FILENAME: &str = "app.db";
const DB_OPEN_LOCK_RETRIES: u32 = 12;
const DB_OPEN_LOCK_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

pub async fn open_desktop_db(identifier: &str) -> Result<Arc<Db>, String> {
    let dir = desktop_db_dir(identifier)
        .ok_or_else(|| "application data directory is unavailable".to_string())?;
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create application data directory: {error}"))?;

    let db_path = dir.join(DB_FILENAME);

    // During an update relaunch the previous process can hold the database for
    // several seconds while it flushes and exits; retry instead of failing the
    // whole startup on a transient lock.
    let mut attempts = 0u32;
    let db = loop {
        match tauri_plugin_db::open_app_db_unmigrated(Some(&db_path)).await {
            Ok(db) => break db,
            Err(error) if attempts < DB_OPEN_LOCK_RETRIES && is_transient_lock_error(&error) => {
                attempts += 1;
                eprintln!(
                    "application database is locked by another process; \
                     retrying ({attempts}/{DB_OPEN_LOCK_RETRIES}): {error}"
                );
                tokio::time::sleep(DB_OPEN_LOCK_RETRY_DELAY).await;
            }
            Err(error) => {
                return Err(format!("failed to open application database: {error}"));
            }
        }
    };

    Ok(Arc::new(db))
}

pub fn is_transient_lock_error(error: &impl std::fmt::Display) -> bool {
    let message = error.to_string();
    message.contains("database is locked") || message.contains("database table is locked")
}

// Matches MigrateError::SchemaFromNewerApp, which reaches startup as a string
// after crossing the tauri plugin setup boundary.
pub fn is_newer_schema_error(error: &impl std::fmt::Display) -> bool {
    error
        .to_string()
        .contains("created by a newer version of Mitschnitt")
}
pub(crate) fn desktop_db_dir(identifier: &str) -> Option<std::path::PathBuf> {
    let data_dir = dirs::data_dir()?;
    // Where the settings plugin keeps `global.json` -- fixed for the life of
    // the install, since a lookup for the vault-path override has to start
    // somewhere that never itself moves. Never used as the database's home
    // directly; see `default_dir` below for that.
    let settings_base = anlg_storage::global::compute_default_base(identifier)?;

    // Mitschnitt-Fork (Klassen-Haertung, Zweitblick 27.08.): Der identifier-benannte
    // Ordner darf NUR als DB-Heimat in Betracht kommen, wenn er die Fork-Identitaet
    // traegt. Ohne diese Schranke oeffnete ein Channel-Overlay-Build (z. B.
    // com.hyprnote.stable) unter seinem Upstream-Identifier die PRODUKTIVE anarlog-DB
    // read-write, sobald der Upstream seine Daten dorthin migriert. Die Isolation
    // haengt damit nicht mehr allein an der gewaehlten Config.
    let default_dir = if identifier == anlg_storage::global::APP_BUNDLE_ID {
        let identifier_dir = data_dir.join(identifier);
        if identifier_dir.join(DB_FILENAME).is_file()
            && !settings_base.join(DB_FILENAME).is_file()
        {
            identifier_dir
        } else {
            settings_base.clone()
        }
    } else {
        settings_base.clone()
    };

    Some(resolve_db_dir(&settings_base, &default_dir))
}

/// Where `app.db` actually lives, once a vault-path override is folded in.
///
/// The database used to be pinned to `default_dir` forever: the storage
/// settings could move `sessions/`, `humans/`, ... to a new vault base, but
/// `app.db` -- the one thing that makes those folders mean anything -- never
/// followed, because this function never asked. It is a vault item like any
/// other, so it resolves through the exact same lookup the settings plugin
/// uses at startup (`SettingsPluginExt::resolve_startup_vault_base`):
/// `CHAR_VAULT_BASE` first, then `vault_path` in `global.json` under
/// `settings_base`, falling back to `default_dir` when neither is set. A
/// fresh install with no override ever configured resolves to exactly
/// `default_dir`, unchanged from before this existed.
///
/// Split out from `desktop_db_dir` so the override lookup -- the part that
/// changed -- can be exercised in a test without going through the real
/// `dirs::data_dir()`.
pub(crate) fn resolve_db_dir(
    settings_base: &std::path::Path,
    default_dir: &std::path::Path,
) -> std::path::PathBuf {
    anlg_storage::vault::resolve_base(settings_base, default_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_lock_errors_are_recognized() {
        assert!(is_transient_lock_error(
            &"error returned from database: (code: 5) database is locked"
        ));
        assert!(is_transient_lock_error(
            &"error returned from database: (code: 6) database table is locked"
        ));
        assert!(!is_transient_lock_error(
            &"unable to open database file: /tmp/app.db"
        ));
    }

    #[test]
    fn newer_schema_errors_are_recognized() {
        // Rendered form of MigrateError::SchemaFromNewerApp after crossing the
        // plugin setup boundary as a string.
        assert!(is_newer_schema_error(
            &"plugin db failed: the database was created by a newer version of Mitschnitt: it requires migration 20260901000000, but this build only includes migrations up to 20260816100100"
        ));
        assert!(!is_newer_schema_error(
            &"unable to open database file: /tmp/app.db"
        ));
    }

    #[test]
    fn dev_uses_an_isolated_persistent_database() {
        let db_dir = desktop_db_dir("com.hyprnote.dev").unwrap();

        assert!(db_dir.ends_with("com.hyprnote.dev"));
    }

    // `resolve_db_dir` just forwards to `anlg_storage::vault::resolve_base`,
    // whose own precedence (env var, then `global.json`, then the fallback)
    // is covered in `crates/storage`. What is specific to this wiring -- and
    // what the vault-move bug lived in -- is which two paths get passed in:
    // the override lookup must read `settings_base` (fixed for the life of
    // the install), and only the fallback may be the legacy-adjusted
    // `default_dir`. These tests pin that down without touching the real
    // `dirs::data_dir()`, using a temp dir and no env var so they cannot
    // collide with a real install or with a parallel test that sets
    // `CHAR_VAULT_BASE`.
    #[test]
    fn resolve_db_dir_falls_back_to_default_without_an_override() {
        let temp = tempfile::tempdir().unwrap();
        let settings_base = temp.path().join("settings");
        let default_dir = temp.path().join("default");
        std::fs::create_dir_all(&settings_base).unwrap();

        // No global.json at all yet -- the common case for every install
        // that has never moved its storage location.
        let resolved = resolve_db_dir(&settings_base, &default_dir);

        assert_eq!(resolved, default_dir);
    }

    #[test]
    fn resolve_db_dir_follows_a_moved_vault_path() {
        let temp = tempfile::tempdir().unwrap();
        let settings_base = temp.path().join("settings");
        let default_dir = settings_base.clone();
        let moved_to = temp.path().join("moved-vault");
        std::fs::create_dir_all(&settings_base).unwrap();
        std::fs::create_dir_all(&moved_to).unwrap();

        std::fs::write(
            settings_base.join(anlg_storage::global::VAULT_CONFIG_FILENAME),
            serde_json::json!({ "vault_path": moved_to.to_string_lossy() }).to_string(),
        )
        .unwrap();

        let resolved = resolve_db_dir(&settings_base, &default_dir);

        // This is the fix: before it, `desktop_db_dir` ignored the override
        // entirely and app.db stayed at `default_dir` forever.
        assert_eq!(resolved, moved_to);
    }

    #[test]
    fn resolve_db_dir_reads_the_override_from_settings_base_not_default_dir() {
        // The legacy-folder branch in `desktop_db_dir` can make `default_dir`
        // different from `settings_base`. `global.json` must still be read
        // from `settings_base` -- the settings plugin never writes it
        // anywhere else -- so a `vault_path` sitting in `default_dir` alone
        // must NOT be picked up.
        let temp = tempfile::tempdir().unwrap();
        let settings_base = temp.path().join("settings");
        let default_dir = temp.path().join("legacy-default");
        let decoy_target = temp.path().join("decoy");
        std::fs::create_dir_all(&settings_base).unwrap();
        std::fs::create_dir_all(&default_dir).unwrap();
        std::fs::create_dir_all(&decoy_target).unwrap();

        std::fs::write(
            default_dir.join(anlg_storage::global::VAULT_CONFIG_FILENAME),
            serde_json::json!({ "vault_path": decoy_target.to_string_lossy() }).to_string(),
        )
        .unwrap();

        let resolved = resolve_db_dir(&settings_base, &default_dir);

        assert_eq!(resolved, default_dir);
    }
}
