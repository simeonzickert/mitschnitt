//! Moves the vault base -- including the database -- to a new location.
//!
//! Mitschnitt-Fork. The storage-location setting (`Settings` general tab)
//! used to move `sessions/`, `humans/`, `chats/`, ... via
//! `tauri-plugin-settings`'s `move_vault`, but never `app.db`: `db.rs`
//! resolved the database's directory independently of the vault-path
//! override, so the one file that gives every other vault item its meaning
//! stayed behind at the old location forever. A user who trusted the button
//! and deleted the old folder afterward would have deleted everything.
//! `db::desktop_db_dir` now follows the override too (see its doc comment),
//! which closes half the bug; this module closes the other half by actually
//! copying `app.db` to the new location as part of the same move, and
//! removes `tauri-plugin-settings`'s now-superseded `move_vault` so nothing
//! can call the incomplete version again by accident.
//!
//! ## Why a restart, not a hot swap
//!
//! `app.db` is open for the entire life of the process: the connection pool
//! is created once in `main()`, before any window exists, and shared via
//! `Arc<anlg_db_core::Db>` into `tauri_plugin_db` and every other plugin that
//! touches the database. There is no seam in that pool to swap out from
//! under all of its holders at once -- doing that safely would mean giving
//! `Db` interior mutability over its own connection pool and auditing every
//! caller for the assumption that today holds everywhere: once you have an
//! `Arc<Db>`, the pool inside it is good until the process exits. That is a
//! bigger, riskier change than a one-day fix should make.
//!
//! So this move does not try to keep the current process's database usable
//! afterward. It checkpoints and closes the pool (`Db::checkpoint_and_close`,
//! see its doc comment for why closing is unavoidable rather than a
//! simplification), copies the now-self-contained `app.db` file, and leaves
//! the pool closed. The frontend already relaunches the app right after a
//! successful move (`scheduleAutomaticRelaunch` in `storage-location.tsx`,
//! wired to `settingsCommands` before this module existed -- it flushes,
//! calls `moveVault`, then relaunches unconditionally), so the closed pool
//! is only ever a problem for the seconds between this command returning and
//! the relaunch actually happening. On the failure branch reached only after
//! the pool is already closed (the database file copy itself), that
//! assumption does not hold -- see the comment on that branch below.
//!
//! ## What is NOT moved, and why that is fine
//!
//! The Markdown mirror (`mirror.rs`, default location
//! `<db dir>/mirror`) is not a vault item and is not copied here. It is a
//! derived cache the watcher rebuilds from the database on the next pass
//! (`anlg_mirror::sync_all` walks every meeting in the database, not just
//! ones that changed since some checkpoint), so once the database has safely
//! arrived at the new location, the mirror reappears there on its own after
//! the restart. Copying stale derived files would only be extra work for no
//! benefit. The old mirror folder is still removed as part of old-location
//! cleanup below, so it does not linger as an orphan.

use tauri::Manager as _;
use tauri_plugin_settings::SettingsPluginExt;

use crate::db;

/// SQLite's own sidecar files. Not vault items (`fs.rs`'s `VAULT_FILES`
/// never listed them, correctly): `checkpoint_database` empties the WAL
/// before the file copy runs, so a moved database needs none of these to be
/// usable. Listed here only so old-location cleanup does not leave them
/// behind as orphans after a successful move.
const DB_SIDECAR_SUFFIXES: &[&str] = &["-wal", "-shm", "-journal"];

/// Prefixes the one failure branch (the database file copy, after the pool
/// is already closed) whose error the frontend must treat differently from
/// every other error this command can return: on every other branch the
/// pool is untouched and the app stays fully usable, so the frontend shows
/// the error and lets the person retry; only on THIS branch is a relaunch
/// required to make the app usable again at all, closed pool or not. A
/// dedicated error variant would say this more cleanly than a string prefix,
/// but would also mean redoing the generated TypeScript binding by hand
/// while the workstation this was built on cannot run this crate's own test
/// binary to regenerate it for real (see the PR/handoff notes) -- not a
/// trade worth making today. Checked in `storage-location.tsx`.
const DATABASE_POOL_CLOSED_MARKER: &str = "DATABASE_POOL_CLOSED: ";

/// Verschiebt den Speicherort samt Datenbank an einen neuen Ort.
///
/// Ersetzt `tauri-plugin-settings`s `moveVault`, das `app.db` nie mitnahm --
/// siehe den Modulkopf fuer die Begruendung.
// Dieser Doc-Kommentar wandert von hier ins erzeugte TypeScript. Wer ihn dort
// von Hand aendert, verliert die Aenderung beim naechsten `export_types`.
#[tauri::command]
#[specta::specta]
pub async fn move_vault<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    new_path: String,
) -> Result<(), String> {
    let new_path = camino::Utf8PathBuf::from(&new_path);
    let identifier = app.config().identifier.clone();

    let old_vault_base = app.settings().vault_base().map_err(|error| error.to_string())?;
    if new_path == old_vault_base {
        return Ok(());
    }

    anlg_storage::vault::validate_vault_base_change(old_vault_base.as_ref(), new_path.as_ref())
        .map_err(|error| error.to_string())?;
    if !anlg_storage::vault::fs::is_empty_or_missing_dir(new_path.as_ref())
        .map_err(|error| error.to_string())?
    {
        return Err(anlg_storage::Error::VaultBaseIsNotEmpty.to_string());
    }
    anlg_storage::vault::ensure_vault_dir(new_path.as_ref()).map_err(|error| error.to_string())?;

    // Resolved the same way the app resolves it at startup (`db::desktop_db_dir`),
    // which additionally knows about the rare legacy-folder case
    // `vault_base()` does not (see that function's doc comment). In the
    // overwhelming majority of installs this is identical to `old_vault_base`.
    let old_db_dir = db::desktop_db_dir(&identifier)
        .ok_or_else(|| "application data directory is unavailable".to_string())?;

    // Held for the whole move: the background watcher in `mirror.rs` reads
    // and writes through this exact same `MirrorGate` (`change_root`) so a
    // pass cannot run against a location this move has already left, or run
    // its own sync against the database while this move is checkpointing it.
    let gate = app
        .try_state::<crate::mirror::MirrorGate>()
        .map(|gate| gate.inner().clone());
    let _held = match &gate {
        Some(gate) => Some(gate.hold().await),
        None => None,
    };

    // 1. Copy every vault item other than the database. On failure the
    // target never had anything of the user's in it (validated above), so
    // cleaning it up is safe, and nothing below this point has touched the
    // database or the old location yet.
    if let Err(error) =
        anlg_storage::vault::fs::copy_vault_items(old_vault_base.as_ref(), new_path.as_ref()).await
    {
        let _ = anlg_storage::vault::fs::remove_vault_items(new_path.as_ref()).await;
        return Err(error.to_string());
    }

    // 2. The database. See the module doc for why this checkpoints and
    // closes the pool rather than trying to keep it open.
    if let Err(error) = checkpoint_database(&app).await {
        let _ = anlg_storage::vault::fs::remove_vault_items(new_path.as_ref()).await;
        return Err(error);
    }

    let old_db_path = old_db_dir.join(db::DB_FILENAME);
    let new_db_path = new_path.as_std_path().join(db::DB_FILENAME);
    if let Err(error) = tokio::fs::copy(&old_db_path, &new_db_path).await {
        // The pool closed in `checkpoint_database` above; there is no path
        // back to a working database through this process from here, no
        // matter what this branch does. What it CAN still guarantee is that
        // the old location is untouched: nothing below this point runs, so
        // config keeps naming the old vault path and nothing was deleted
        // there. A restart recovers cleanly -- it is just no longer optional
        // the way it is on every branch above.
        let _ = anlg_storage::vault::fs::remove_vault_items(new_path.as_ref()).await;
        let _ = tokio::fs::remove_file(&new_db_path).await;
        return Err(format!(
            "{DATABASE_POOL_CLOSED_MARKER}failed to copy the database to the new \
             location: {error}. Your data is unchanged at the old location, but \
             Mitschnitt must be restarted before it can be used again."
        ));
    }

    // 3. Persist the new location. Only from here does the app consider the
    // move real; every branch above returned without writing to config.
    app.settings()
        .set_vault_base(new_path)
        .await
        .map_err(|error| error.to_string())?;

    // 4. Best-effort cleanup of the old location, mirroring the non-db
    // items' existing "best effort, do not fail an already-succeeded move"
    // treatment above. See the module doc for why the mirror folder is
    // cleaned up here instead of being added to fs.rs's VAULT_* lists.
    let _ = anlg_storage::vault::fs::remove_vault_items(old_vault_base.as_ref()).await;
    let _ = remove_database_files(&old_db_dir).await;
    let _ = remove_dir_all_best_effort(&old_db_dir.join(anlg_mirror::MIRROR_DIR)).await;

    Ok(())
}

/// Checkpoints and closes the app's database pool, translating the result
/// into the plain `String` error this module's command returns.
async fn checkpoint_database<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    let db = app
        .try_state::<std::sync::Arc<anlg_db_core::Db>>()
        .map(|db| db.inner().clone())
        .ok_or_else(|| "the database is not open".to_string())?;

    db.checkpoint_and_close().await.map_err(|error| {
        format!(
            "could not safely prepare the database to move: {error}. \
             Nothing was changed; try again in a moment."
        )
    })
}

/// Removes `app.db` and its sidecar files from `dir`, best-effort.
async fn remove_database_files(dir: &std::path::Path) -> std::io::Result<()> {
    let db_path = dir.join(db::DB_FILENAME);
    if db_path.is_file() {
        tokio::fs::remove_file(&db_path).await?;
    }
    for suffix in DB_SIDECAR_SUFFIXES {
        let sidecar = dir.join(format!("{}{suffix}", db::DB_FILENAME));
        if sidecar.is_file() {
            tokio::fs::remove_file(&sidecar).await?;
        }
    }
    Ok(())
}

async fn remove_dir_all_best_effort(dir: &std::path::Path) -> std::io::Result<()> {
    if dir.is_dir() {
        tokio::fs::remove_dir_all(dir).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn remove_database_files_removes_the_database_and_its_sidecars() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        std::fs::write(dir.join(db::DB_FILENAME), b"db").unwrap();
        std::fs::write(dir.join(format!("{}-wal", db::DB_FILENAME)), b"wal").unwrap();
        std::fs::write(dir.join(format!("{}-shm", db::DB_FILENAME)), b"shm").unwrap();
        std::fs::write(dir.join(format!("{}-journal", db::DB_FILENAME)), b"journal").unwrap();
        std::fs::write(dir.join("unrelated.txt"), b"stays").unwrap();

        remove_database_files(dir).await.unwrap();

        assert!(!dir.join(db::DB_FILENAME).exists());
        assert!(!dir.join(format!("{}-wal", db::DB_FILENAME)).exists());
        assert!(!dir.join(format!("{}-shm", db::DB_FILENAME)).exists());
        assert!(!dir.join(format!("{}-journal", db::DB_FILENAME)).exists());
        assert!(
            dir.join("unrelated.txt").exists(),
            "cleanup must not touch files it does not own"
        );
    }

    #[tokio::test]
    async fn remove_database_files_tolerates_a_missing_database() {
        let temp = tempfile::tempdir().unwrap();

        // No app.db and no sidecars were ever created here (e.g. the vault
        // was never actually used before being moved). Must not error.
        remove_database_files(temp.path()).await.unwrap();
    }

    #[tokio::test]
    async fn remove_dir_all_best_effort_tolerates_a_missing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("mirror");

        // The default case for a vault that has never accumulated a mirror
        // pass, or one already using a custom (unrelated) mirror root.
        remove_dir_all_best_effort(&missing).await.unwrap();
    }

    #[tokio::test]
    async fn remove_dir_all_best_effort_removes_an_existing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let mirror_dir = temp.path().join("mirror");
        std::fs::create_dir_all(mirror_dir.join("nested")).unwrap();
        std::fs::write(mirror_dir.join("nested").join("meeting.md"), b"# Meeting").unwrap();

        remove_dir_all_best_effort(&mirror_dir).await.unwrap();

        assert!(!mirror_dir.exists());
    }
}
