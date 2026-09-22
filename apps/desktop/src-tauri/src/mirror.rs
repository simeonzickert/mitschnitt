//! Keeps the readable Markdown mirror in step with the database.
//!
//! Mitschnitt-Fork. A meeting that only exists inside a multi-hundred-megabyte
//! SQLite file is not data the owner can see, so every meeting is also written
//! as Markdown next to the database. This watcher is what makes the mirror
//! current rather than a one-off export: it re-syncs after a transcript
//! finishes, a summary is generated or edited, or a title changes.
//!
//! One way only. Nothing here reads the mirror back into the database; that
//! road runs through `session_proposals`, where a human accepts or declines.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anlg_mirror::{MirrorPaths, RENDERED_TABLES, RecorderView};

/// A finished transcription writes in bursts; waiting a moment collapses the
/// burst into a single pass instead of rewriting the same folder repeatedly.
const DEBOUNCE: Duration = Duration::from_millis(750);
const RETRY_INTERVAL: Duration = Duration::from_secs(60);

/// The settings table, on top of the rendered ones: the mirror root is a
/// setting, so a change to it has to wake the watcher the same way a new
/// transcript does. Without it the app would keep writing to the old place
/// until the next restart.
const SETTINGS_TABLE: &str = "app_settings";

/// Where the app data folder is. Held rather than resolved per call so the
/// commands and the watcher cannot drift onto two different machine rooms.
pub struct MirrorHome(pub PathBuf);

/// Held while the readable folders are moving to a new root.
///
/// Mitschnitt-Fork. Without it the watcher and the move fight over the same
/// folders. A move across volumes takes minutes; the watcher meanwhile resolves
/// the root it still knows, finds the folders gone, reads that as "every meeting
/// is missing" and writes them all back at the old place -- recordings included.
/// Measured, not feared: three folders moved away, three folders back on the very
/// next pass.
///
/// So the watcher waits while the move runs, and the move writes the setting
/// before it lets go. Ordering the two steps was never enough; it only made the
/// window small.
#[derive(Clone, Default)]
pub struct MirrorGate(Arc<tokio::sync::Mutex<()>>);

impl MirrorGate {
    // `pub(crate)`, not private: `vault_move`'s database-carrying move holds
    // this gate too, for the same reason `change_root` does -- so the watcher
    // cannot run a pass against a location the move has already left or not
    // yet finished arriving at.
    pub(crate) async fn hold(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.0.lock().await
    }
}

/// The current mirror paths: the root as chosen in the settings, the machine
/// room always local.
///
/// Read fresh on every pass instead of captured at startup, so choosing a new
/// folder takes effect without a restart.
pub async fn current_paths(db: &anlg_db_core::Db, app_data_dir: &std::path::Path) -> MirrorPaths {
    match anlg_mirror::read_root_setting(db.pool()).await {
        Ok(Some(root)) => MirrorPaths::with_root(root, app_data_dir),
        Ok(None) => MirrorPaths::for_app_data_dir(app_data_dir),
        // An unreadable setting must not stop the mirror; the default place is
        // always writable and losing the choice is visible, losing the mirror
        // is not.
        Err(error) => {
            tracing::error!(%error, "could not read the mirror folder setting; using the default");
            MirrorPaths::for_app_data_dir(app_data_dir)
        }
    }
}

pub fn spawn(db: Arc<anlg_db_core::Db>, app_data_dir: PathBuf, gate: MirrorGate) {
    tauri::async_runtime::spawn(async move {
        run(db, app_data_dir, gate).await;
    });
}

async fn run(db: Arc<anlg_db_core::Db>, app_data_dir: PathBuf, gate: MirrorGate) {
    // Subscribed before the first pass so a write during the backfill is not
    // lost between the two.
    let mut changes = db.change_notifier().subscribe();

    loop {
        {
            // Resolving the root and using it happen under the same hold, so a
            // move cannot slip in between and have this pass write into a place
            // the app has already left.
            let _held = gate.hold().await;
            let paths = current_paths(&db, &app_data_dir).await;
            sync(&db, &paths).await;
        }
        if !wait_for_relevant_change(&mut changes).await {
            break;
        }
    }
}

/// Blocks until something happened that could move a mirror file. Returns
/// `false` only when the change feed is gone and the watcher should stop.
///
/// The filtering has to happen *here* rather than around the sync call: an
/// earlier version skipped an unrelated table with `continue`, which continued
/// the outer loop and re-synced anyway -- so a constantly-written table like
/// `search_index_dirty` would have driven a full mirror pass with no debounce.
async fn wait_for_relevant_change(
    changes: &mut tokio::sync::broadcast::Receiver<anlg_db_change::TableChange>,
) -> bool {
    use tokio::sync::broadcast::error::RecvError;

    loop {
        tokio::select! {
            change = changes.recv() => match change {
                Ok(change) if RENDERED_TABLES.contains(&change.table.as_str())
                    || change.table == SETTINGS_TABLE => {
                    tokio::time::sleep(DEBOUNCE).await;
                    // Collapse the rest of the burst so one finished
                    // transcription does not trigger one pass per written row.
                    while changes.try_recv().is_ok() {}
                    return true;
                }
                Ok(_) => {}
                // A lagged receiver may have dropped a relevant change, so the
                // safe reading is "something happened".
                Err(RecvError::Lagged(_)) => return true,
                Err(RecvError::Closed) => return false,
            },
            // Backstop, so a missed notification costs a delay and not the
            // mirror.
            _ = tokio::time::sleep(RETRY_INTERVAL) => return true,
        }
    }
}

/// What the recorder says, in the shape the mirror wants.
///
/// Mitschnitt-Fork (F16d). A recording whose file stopped changing is not the
/// same thing as a finished meeting -- a muted microphone, a lost device or an
/// encode in flight all look identical from the outside -- so the mirror asks
/// the recorder first and uses the fifteen-second file window only as a second
/// net.
///
/// An unanswered actor becomes [`RecorderView::Unavailable`], which is honest
/// rather than strong: the pass then falls back to the file-quiet window alone,
/// exactly as the mirror worked before it could ask. Saying so out loud because
/// it is easy to read the variant name as a stronger guard than it is -- the
/// residual hole is a meeting the recorder holds, whose file has been quiet for
/// fifteen seconds, on a pass where the actor did not answer within 100 ms. The
/// next pass a minute later asks again.
///
/// Not treated as "skip the audio entirely" on purpose: that is what the CLI
/// would then do on every run, and it would never mirror a recording again.
///
/// Asked once per pass, not once per meeting, so a recording that starts while
/// a pass is walking the library is not in the answer. The file window catches
/// that one: a recording that just started was written seconds ago. The two
/// windows only fail together for a meeting that begins mid-pass AND stays
/// quiet on disk for fifteen seconds, and the pass a minute later sees it.
async fn recorder_view() -> RecorderView {
    match tauri_plugin_transcription::held_session_ids().await {
        Some(ids) => RecorderView::holding(ids),
        None => RecorderView::Unavailable,
    }
}

async fn sync(db: &anlg_db_core::Db, paths: &MirrorPaths) {
    let recorder = recorder_view().await;
    match anlg_mirror::sync_all(db.pool(), paths, false, &recorder).await {
        Ok(report) => {
            // One meeting failing no longer ends the pass, so the failures have
            // to be said out loud here or they are said nowhere.
            for (session_id, reason) in &report.failed {
                tracing::error!(%session_id, %reason, "could not mirror this meeting");
            }
            // Said out loud because "the recorder still has it" and "the
            // recorder never let go" look the same from here, and the second one
            // is otherwise invisible for ever.
            if !report.recording_held.is_empty() {
                tracing::debug!(
                    held = report.recording_held.len(),
                    sessions = ?report.recording_held,
                    "left these recordings alone; the recorder still holds them"
                );
            }
            if !report.written.is_empty() {
                tracing::debug!(
                    written = report.written.len(),
                    up_to_date = report.up_to_date,
                    "refreshed the markdown mirror"
                );
            }
        }
        // A failing mirror must never take the app down: the database stays the
        // system of record, and `anarlog mirror status` reports the gap.
        Err(error) => tracing::error!(%error, "failed to refresh the markdown mirror"),
    }
}

/// What a folder change did. Mirrors `anlg_mirror::MoveReport`, as a shape the
/// frontend can render.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MirrorMoveReport {
    pub moved: usize,
    /// Folder names that stayed at the old place, with the reason -- at most a
    /// readable handful. Non-empty means a person has to be told: a
    /// half-finished move that nobody sees is exactly the two-places problem
    /// again.
    pub failed: Vec<MirrorMoveFailure>,
    /// How many failed altogether, which can be more than `failed` names.
    pub failed_total: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MirrorMoveFailure {
    pub folder: String,
    pub reason: String,
}

fn home<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    use tauri::Manager as _;
    app.try_state::<MirrorHome>()
        .map(|home| home.0.clone())
        .ok_or_else(|| "application data directory is unavailable".to_string())
}

async fn db_of<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Arc<anlg_db_core::Db>, String> {
    use tauri::Manager as _;
    app.try_state::<Arc<anlg_db_core::Db>>()
        .map(|db| db.inner().clone())
        .ok_or_else(|| "the database is not open".to_string())
}

/// The folder this meeting lives in for a person.
///
/// Mitschnitt-Fork (F16b). Every "Show in Finder" resolves through here rather
/// than through `sessionDir`, which points into `sessions/<uuid>` -- a folder
/// with a uuid for a name and no transcript in it.
///
/// Returns `null` while the meeting has not been mirrored yet (the watcher runs
/// a moment behind a brand new session). The caller decides what to say; opening
/// the machine room instead would quietly reintroduce the second place.
#[tauri::command]
#[specta::specta]
pub async fn mirror_session_folder<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    session_id: String,
) -> Result<Option<String>, String> {
    let app_data_dir = home(&app)?;
    let db = db_of(&app).await?;
    let paths = current_paths(&db, &app_data_dir).await;

    anlg_mirror::session_folder(&paths, &session_id)
        .map(|found| found.map(|path| path.to_string_lossy().to_string()))
        .map_err(|error| error.to_string())
}

/// Removes the recording from the meeting folder, for a deletion the person
/// asked for.
///
/// Called next to the machine-room delete, never from the retention service:
/// see `anlg_mirror::forget_audio` for why that line is where it is.
#[tauri::command]
#[specta::specta]
pub async fn mirror_forget_audio<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    session_id: String,
) -> Result<bool, String> {
    let app_data_dir = home(&app)?;
    let db = db_of(&app).await?;
    let paths = current_paths(&db, &app_data_dir).await;

    anlg_mirror::forget_audio(&paths, &session_id).map_err(|error| error.to_string())
}

/// Removes the whole meeting folder, once a deletion is final.
#[tauri::command]
#[specta::specta]
pub async fn mirror_forget_session<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    session_id: String,
) -> Result<bool, String> {
    let app_data_dir = home(&app)?;
    let db = db_of(&app).await?;
    let paths = current_paths(&db, &app_data_dir).await;

    anlg_mirror::forget_session(&paths, &session_id).map_err(|error| error.to_string())
}

/// The folder the readable copies currently live in.
#[tauri::command]
#[specta::specta]
pub async fn mirror_root<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<String, String> {
    let app_data_dir = home(&app)?;
    let db = db_of(&app).await?;
    Ok(current_paths(&db, &app_data_dir)
        .await
        .root
        .to_string_lossy()
        .to_string())
}

/// Changes where the readable folders live: moves them and records the choice.
///
/// **One command, one writer.** This used to be two steps -- Rust moved the
/// folders, then the frontend wrote the setting -- and the gap between them was
/// long enough to matter: an app that died in it, or a watcher pass that ran
/// through it, put the old place back into use while the meetings were already
/// at the new one. The setting is now written here, under the same hold as the
/// move (see [`MirrorGate`]), so there is no moment in which the two disagree.
///
/// The setting is written only when every folder arrived. `move_root` moves all
/// of them or none, so refusing to switch on a failure leaves the meetings and
/// the app together at the old place, and the person can try again.
///
/// `new_root` empty means "back to the default place".
#[tauri::command]
#[specta::specta]
pub async fn mirror_move_root<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    new_root: String,
) -> Result<MirrorMoveReport, String> {
    use tauri::Manager as _;
    let app_data_dir = home(&app)?;
    let db = db_of(&app).await?;
    let gate = app
        .try_state::<MirrorGate>()
        .map(|gate| gate.inner().clone())
        .ok_or_else(|| "the mirror is not running".to_string())?;

    change_root(&db, &app_data_dir, &gate, new_root).await
}

async fn change_root(
    db: &Arc<anlg_db_core::Db>,
    app_data_dir: &std::path::Path,
    gate: &MirrorGate,
    new_root: String,
) -> Result<MirrorMoveReport, String> {
    let chosen = match new_root.trim() {
        "" => MirrorPaths::default_root(app_data_dir),
        picked => PathBuf::from(picked),
    };

    let _held = gate.hold().await;
    let old_root = current_paths(db, app_data_dir).await.root;
    reject_impossible_root(app_data_dir, &old_root, &chosen)?;

    let report = {
        let old_root = old_root.clone();
        let chosen = chosen.clone();
        tokio::task::spawn_blocking(move || anlg_mirror::move_root(&old_root, &chosen))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?
    };

    if report.is_complete() {
        let default_place = MirrorPaths::default_root(app_data_dir);
        let stored = (chosen != default_place).then_some(chosen.as_path());
        anlg_mirror::write_root_setting(db.pool(), stored)
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(MirrorMoveReport {
        moved: report.moved,
        failed_total: report.failed_total,
        failed: report
            .failed
            .into_iter()
            .map(|(folder, reason)| MirrorMoveFailure { folder, reason })
            .collect(),
    })
}

/// The two roots the app must refuse before anything is touched.
///
/// The machine room may not end up inside the readable root: the database is
/// not something a sync client may open, and a root that contains it would drag
/// it along.
///
/// And the new root may not be nested in the current one, in either direction.
/// The folder dialog opens in the current root, so a subfolder of it is the
/// nearest thing to hand -- and it is unbuildable: the new root is created
/// first, then the old root is read, so the move walks into its own target and
/// copies it into itself until the path runs out of room. Measured before the
/// check existed: 78 levels deep, about 140 folders of debris, no data lost and
/// nothing usable either. The other direction leaves the emptied old root
/// sitting inside the new one.
fn reject_impossible_root(
    app_data_dir: &std::path::Path,
    old_root: &std::path::Path,
    new_root: &std::path::Path,
) -> Result<(), String> {
    let home = resolve(app_data_dir);
    let old = resolve(old_root);
    let new = resolve(new_root);

    if home.starts_with(&new) {
        return Err("mirror_root_contains_app_data".to_string());
    }
    if old != new && (new.starts_with(&old) || old.starts_with(&new)) {
        return Err("mirror_root_nested_in_current".to_string());
    }
    Ok(())
}

/// A path in a shape two paths can be compared in.
///
/// `starts_with` is a comparison of components, so `~/mirror/../Nextcloud` and a
/// symlinked home would defeat it while naming the very folder the check is
/// there to catch. The chosen folder may not exist yet, so what exists of it is
/// resolved and the rest appended.
fn resolve(path: &std::path::Path) -> PathBuf {
    use std::path::Component;

    if let Ok(real) = path.canonicalize() {
        return real;
    }

    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut folded = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                folded.pop();
            }
            Component::CurDir => {}
            other => folded.push(other),
        }
    }

    let mut existing = folded.clone();
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name().map(std::ffi::OsStr::to_os_string) else {
            break;
        };
        missing.push(name);
        if !existing.pop() {
            break;
        }
    }

    match existing.canonicalize() {
        Ok(mut resolved) => {
            for name in missing.into_iter().rev() {
                resolved.push(name);
            }
            resolved
        }
        Err(_) => folded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert_session(pool: &sqlx::SqlitePool, id: &str, title: &str) {
        sqlx::query(
            "INSERT INTO sessions (id, title, started_at, updated_at, created_at)
             VALUES (?, ?, '2026-05-05T08:00:00Z', ?, '2026-05-05T08:00:00Z')",
        )
        .bind(id)
        .bind(title)
        .bind(format!("2026-05-05T08:00:00.{}Z", rand_suffix()))
        .execute(pool)
        .await
        .unwrap();
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000
    }

    /// Polls instead of sleeping a fixed amount, so the test measures the
    /// watcher rather than the machine it runs on.
    async fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if condition() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }

    fn mirror_contains(root: &std::path::Path, needle: &str) -> bool {
        std::fs::read_dir(root)
            .map(|entries| {
                entries.filter_map(std::result::Result::ok).any(|entry| {
                    std::fs::read_to_string(entry.path().join(anlg_mirror::SUMMARY_FILE))
                        .map(|body| body.contains(needle))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    }

    async fn open_db(dir: &std::path::Path) -> Arc<anlg_db_core::Db> {
        let db = Arc::new(
            anlg_db_core::Db::connect_local_plain(&dir.join("app.db"))
                .await
                .unwrap(),
        );
        anlg_db_app::prepare_schema(&db).await.unwrap();
        db
    }

    fn meeting_folders(root: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(root)
            .map(|entries| {
                entries
                    .filter_map(std::result::Result::ok)
                    .filter(|entry| entry.path().is_dir())
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    // The point of the watcher: nobody runs a command. A change in the database
    // has to reach the readable copy on its own, or the mirror is a one-off
    // export that silently rots.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_watcher_follows_a_change_without_being_asked() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path()).await;
        let paths = MirrorPaths::for_app_data_dir(dir.path());
        let app_data_dir = dir.path().to_path_buf();

        insert_session(db.pool(), "s1", "Erster Titel").await;

        let watcher = tokio::spawn({
            let db = db.clone();
            let app_data_dir = app_data_dir.clone();
            async move { run(db, app_data_dir, MirrorGate::default()).await }
        });

        assert!(
            wait_until(|| mirror_contains(&paths.root, "Erster Titel")).await,
            "the watcher never wrote the first mirror"
        );

        // Now change the meeting the way the app does, and touch nothing else.
        sqlx::query("UPDATE sessions SET title = 'Zweiter Titel', updated_at = '2026-05-05T09:00:00Z' WHERE id = 's1'")
            .execute(db.pool())
            .await
            .unwrap();

        assert!(
            wait_until(|| mirror_contains(&paths.root, "Zweiter Titel")).await,
            "the watcher did not follow the change"
        );

        watcher.abort();
    }

    // The whole point of doing the move and the setting in one command. As two
    // steps the watcher woke up in between, resolved the root it still knew,
    // found it empty, read that as "every meeting is missing" and wrote them all
    // back at the old place -- so a person who moved their meetings to Nextcloud
    // had them in both places a second later.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_finished_move_leaves_nothing_behind_for_the_watcher_to_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path()).await;
        let app_data_dir = dir.path().to_path_buf();
        let old_root = MirrorPaths::default_root(&app_data_dir);
        let new_root = dir.path().join("Nextcloud").join("Mitschnitt");
        let gate = MirrorGate::default();

        insert_session(db.pool(), "s1", "Jour Fixe").await;
        let watcher = tokio::spawn({
            let (db, app_data_dir, gate) = (db.clone(), app_data_dir.clone(), gate.clone());
            async move { run(db, app_data_dir, gate).await }
        });
        assert!(
            wait_until(|| !meeting_folders(&old_root).is_empty()).await,
            "the watcher never wrote the first mirror"
        );

        let report = change_root(
            &db,
            &app_data_dir,
            &gate,
            new_root.to_string_lossy().to_string(),
        )
        .await
        .unwrap();
        assert_eq!(report.moved, 1);
        assert_eq!(report.failed_total, 0);

        // Wake the watcher the way the app does. The new title can only appear
        // through a watcher pass, so waiting for it waits for a real pass --
        // waiting for the folder itself would be satisfied by the move already.
        sqlx::query("UPDATE sessions SET title = 'Jour Fixe Grün', updated_at = '2026-05-05T10:00:00Z' WHERE id = 's1'")
            .execute(db.pool())
            .await
            .unwrap();
        assert!(
            wait_until(|| mirror_contains(&new_root, "Jour Fixe Grün")).await,
            "the watcher did not follow the meetings to the new place"
        );
        assert_eq!(
            meeting_folders(&old_root),
            Vec::<String>::new(),
            "the watcher filled the old place back up, so the meetings are in two places"
        );

        watcher.abort();
    }

    // While the folders travel -- minutes, across volumes -- the watcher must
    // not run. It would resolve the root the app still knows, find the folders
    // gone and write every meeting back there.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_watcher_waits_while_the_folders_are_travelling() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path()).await;
        let app_data_dir = dir.path().to_path_buf();
        let root = MirrorPaths::default_root(&app_data_dir);
        let gate = MirrorGate::default();

        insert_session(db.pool(), "s1", "Standup").await;
        let watcher = tokio::spawn({
            let (db, app_data_dir, gate) = (db.clone(), app_data_dir.clone(), gate.clone());
            async move { run(db, app_data_dir, gate).await }
        });
        assert!(wait_until(|| !meeting_folders(&root).is_empty()).await);

        let held = gate.hold().await;
        std::fs::remove_dir_all(&root).unwrap();
        sqlx::query("UPDATE sessions SET title = 'Standup Neu', updated_at = '2026-05-05T11:00:00Z' WHERE id = 's1'")
            .execute(db.pool())
            .await
            .unwrap();

        // Long enough for a pass to have happened: the debounce plus room.
        tokio::time::sleep(DEBOUNCE * 4).await;
        assert!(
            !root.exists(),
            "the watcher wrote into the root while it was being moved away"
        );

        drop(held);
        assert!(
            wait_until(|| !meeting_folders(&root).is_empty()).await,
            "the watcher never resumed after the move"
        );

        watcher.abort();
    }

    // The folder dialog opens in the current root, so a subfolder of it is the
    // nearest thing to hand -- and the move would then walk into its own target
    // and copy it into itself until the path ran out of room.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_folder_inside_the_current_root_is_refused_before_anything_moves() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path()).await;
        let app_data_dir = dir.path().to_path_buf();
        let root = MirrorPaths::default_root(&app_data_dir);
        let gate = MirrorGate::default();

        insert_session(db.pool(), "s1", "Standup").await;
        anlg_mirror::sync_all(
            db.pool(),
            &MirrorPaths::for_app_data_dir(&app_data_dir),
            false,
            &RecorderView::idle(),
        )
        .await
        .unwrap();
        let before = meeting_folders(&root);
        assert_eq!(before.len(), 1);

        let inside = root.join("Meetings");
        let error = change_root(
            &db,
            &app_data_dir,
            &gate,
            inside.to_string_lossy().to_string(),
        )
        .await
        .expect_err("a folder inside the current root was accepted");

        assert_eq!(error, "mirror_root_nested_in_current");
        assert!(!inside.exists(), "the refused folder was created anyway");
        assert_eq!(meeting_folders(&root), before);

        // And the other direction: the current root inside the chosen one would
        // leave the emptied old root sitting in the new one. Only reachable once
        // the root has left the app data folder, because every parent of the
        // default root contains the machine room and is refused for that instead.
        let synced = dir.path().join("Nextcloud");
        change_root(
            &db,
            &app_data_dir,
            &gate,
            synced.join("Mitschnitt").to_string_lossy().to_string(),
        )
        .await
        .unwrap();

        assert_eq!(
            change_root(
                &db,
                &app_data_dir,
                &gate,
                synced.to_string_lossy().to_string()
            )
            .await
            .expect_err("the parent of the current root was accepted"),
            "mirror_root_nested_in_current"
        );
    }
}
