//! Where the readable folders live, and moving them when that changes.
//!
//! Mitschnitt-Fork (F16c). The mirror may sit outside the application data
//! folder -- a synced folder is the point, so a meeting is readable on the phone
//! and on the second machine. **Only the readable side moves.** The database and
//! `sessions/` stay local: a sync client that gets its hands on an open SQLite
//! file corrupts it, and the machine room is not something a person browses.
//!
//! The choice is stored like every other setting, in the `app_settings` table
//! under [`ROOT_SETTING_KEY`], as a JSON string. Empty means "the default place".

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::{Error, Result};

/// Same key the frontend writes through `setSettingValue`.
pub const ROOT_SETTING_KEY: &str = "mirror_root";

/// How many folder names a report carries by name. Beyond that only the count
/// grows: a failure list is meant to be read by a person, not scrolled.
pub const MOVE_REPORT_LIMIT: usize = 20;

/// Reads the chosen mirror root out of the settings table.
///
/// Read from `app_settings` only, never from `synced_preferences`: this is a
/// path on one machine, and a synced value would send the other machine's
/// folder here.
pub async fn read_root_setting(pool: &SqlitePool) -> Result<Option<PathBuf>> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT value_json FROM app_settings WHERE id = ?")
            .bind(ROOT_SETTING_KEY)
            .fetch_optional(pool)
            .await?
            .flatten();

    let Some(raw) = raw else {
        return Ok(None);
    };

    // Values are stored JSON-encoded. A value that is not a string (an upstream
    // change, a hand-edited row) is not a path, and treating it as one would
    // point the mirror at a nonsense folder -- so it degrades to the default.
    let path = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default();

    let path = path.trim();
    if path.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(path)))
}

/// Stores the chosen mirror root.
///
/// The one writer. The choice used to be written by the frontend after the
/// folders had travelled, which made the move and the setting two steps with a
/// gap between them: an app that died in the gap kept pointing at the old place
/// while the meetings were already at the new one, and the watcher filled the
/// old place back up. Same statement the frontend's `setSettingValue` uses, so
/// the row looks the same whichever way it was written.
///
/// An empty path means "the default place".
pub async fn write_root_setting(pool: &SqlitePool, root: Option<&Path>) -> Result<()> {
    let value = root
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();

    sqlx::query(
        "INSERT INTO app_settings (id, value_json, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           value_json = excluded.value_json,
           updated_at = excluded.updated_at",
    )
    .bind(ROOT_SETTING_KEY)
    .bind(serde_json::Value::String(value).to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// What a move did, and what it could not do.
///
/// A partial move is the dangerous outcome, so it is a value the caller has to
/// look at rather than a log line: `failed` non-empty means folders stayed
/// behind and a person has to be told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MoveReport {
    pub moved: usize,
    /// Folder name and the reason it stayed where it was, at most
    /// [`MOVE_REPORT_LIMIT`] of them.
    pub failed: Vec<(String, String)>,
    /// How many failed in total. Separate from `failed.len()` so a run with
    /// hundreds of failures reports the true size instead of the truncated one
    /// -- a number that quietly stops counting is how a bad migration reads as
    /// a small one.
    pub failed_total: usize,
}

impl MoveReport {
    pub fn is_complete(&self) -> bool {
        self.failed_total == 0
    }
}

/// Moves every mirror folder from one root to another. All of them, or none.
///
/// Entry by entry rather than one big directory move: a rename is tried first
/// (instant, same volume) and a copy-then-delete is the fallback across volumes,
/// which is exactly the Nextcloud case.
///
/// **A half-finished move is the outcome this refuses to produce.** It is the
/// two-places problem in its worst form: some meetings here, some there, and
/// whichever root the app then points at, the other one holds meetings nobody
/// looks at again. So every entry is checked before a single one is touched,
/// and anything that still fails mid-way is rolled back to where it started.
/// The invariant a caller may rely on: `failed_total == 0` means everything
/// travelled, and anything else means nothing did -- unless the rollback itself
/// failed, which is reported by name as its own failure.
///
/// Nothing is deleted at the source until its copy is complete, so even a
/// power cut leaves a duplicate, never a hole.
pub fn move_root(old_root: &Path, new_root: &Path) -> Result<MoveReport> {
    move_root_with(old_root, new_root, true)
}

/// The body of [`move_root`], with the rename shortcut as a switch -- see
/// [`move_entry_with`] for why the tests need it.
fn move_root_with(old_root: &Path, new_root: &Path, rename_allowed: bool) -> Result<MoveReport> {
    let mut report = MoveReport::default();

    // Not a string comparison: `~/mirror`, `~/mirror/`, `~/x/../mirror` and a
    // symlink to it are one folder, and reading them as four would turn "there
    // is nothing to do" into "every folder failed to move".
    if is_same_place(old_root, new_root) || !old_root.is_dir() {
        return Ok(report);
    }

    std::fs::create_dir_all(new_root)
        .map_err(|error| Error::io(format!("create {}", new_root.display()), error))?;

    let entries = std::fs::read_dir(old_root)
        .map_err(|error| Error::io(format!("read {}", old_root.display()), error))?;

    // Sorted, so a run is reproducible: the order the filesystem hands back is
    // its own business, and a report that names a different folder each time is
    // not something a person can act on.
    let mut planned: Vec<(String, PathBuf, PathBuf)> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let target = new_root.join(&name);
            (name, entry.path(), target)
        })
        .collect();
    planned.sort_by(|left, right| left.0.cmp(&right.0));

    // Look before touching. The one failure that actually happens in practice
    // is a name already taken at the new place, and finding it after three
    // folders have travelled is finding it too late.
    let blocked: Vec<(String, String)> = planned
        .iter()
        .filter(|(_, _, target)| target.exists())
        .map(|(name, _, target)| {
            (
                name.clone(),
                format!("{} already exists at the new place", target.display()),
            )
        })
        .collect();
    if !blocked.is_empty() {
        return Ok(report_of(blocked));
    }

    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (name, source, target) in planned {
        match move_entry_with(&source, &target, rename_allowed) {
            Ok(()) => {
                report.moved += 1;
                done.push((source, target));
            }
            Err(reason) => {
                let mut failures = vec![(name, reason)];
                failures.extend(roll_back(done, rename_allowed));
                return Ok(report_of(failures));
            }
        }
    }

    Ok(report)
}

/// Puts already-moved folders back where they came from.
///
/// Returns what could not be put back -- those are the folders that really are
/// in two places now, and saying so is the only honest thing left to do.
fn roll_back(done: Vec<(PathBuf, PathBuf)>, rename_allowed: bool) -> Vec<(String, String)> {
    done.into_iter()
        .filter_map(|(source, target)| {
            move_entry_with(&target, &source, rename_allowed)
                .err()
                .map(|reason| {
                    (
                        source
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| source.display().to_string()),
                        format!("moved to the new place and could not be moved back: {reason}"),
                    )
                })
        })
        .collect()
}

fn report_of(failures: Vec<(String, String)>) -> MoveReport {
    MoveReport {
        moved: 0,
        failed_total: failures.len(),
        failed: failures.into_iter().take(MOVE_REPORT_LIMIT).collect(),
    }
}

/// Whether two paths name the same folder, `..` and symlinks included.
fn is_same_place(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        // One of them does not exist yet, so it cannot be the other one.
        _ => false,
    }
}

/// The body of [`move_entry`], with the rename shortcut as a switch.
///
/// The switch exists for the tests: inside one tempdir a rename always works,
/// so the copy fallback -- the path a move onto a synced folder on another
/// volume takes, every time -- could otherwise never be reached from a test.
fn move_entry_with(
    source: &Path,
    target: &Path,
    rename_allowed: bool,
) -> std::result::Result<(), String> {
    if target.exists() {
        return Err(format!(
            "{} already exists at the new place",
            target.display()
        ));
    }

    if rename_allowed && std::fs::rename(source, target).is_ok() {
        return Ok(());
    }

    // Different volume. Copy first, and only remove the original once the copy
    // stands -- a failure here must not be able to lose a meeting.
    if let Err(error) = copy_tree(source, target) {
        // The target did not exist a moment ago (checked above), so whatever is
        // there now is this half-written copy. Left behind it would block every
        // later attempt with "already exists at the new place" -- a permanent
        // failure caused by a temporary one.
        let _ = remove_tree(target);
        return Err(error.to_string());
    }
    remove_tree(source)
        .map_err(|error| format!("copied, but the old copy could not be removed: {error}"))
}

fn copy_tree(source: &Path, target: &Path) -> std::io::Result<()> {
    if source.is_file() {
        return std::fs::copy(source, target).map(|_| ());
    }
    if !source.is_dir() {
        // Whatever is left is a broken symlink, a socket or a device node --
        // not something to interpret. A *live* symlink never reaches this line:
        // `is_file` and `is_dir` follow it, so it travels as the file or folder
        // it points at. That is the right outcome for a mirror (the meeting
        // arrives), and it is worth stating, because the reverse reading of this
        // branch -- "symlinks are refused" -- was wrong and would have hidden
        // the one case that does bite: `remove_tree` then sees a symlink whose
        // target is a folder, calls `remove_dir_all` on it and fails with
        // ENOTDIR, which the report names as "copied, but the old copy could
        // not be removed".
        return Err(std::io::Error::other("not a file or folder"));
    }

    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        copy_tree(&entry.path(), &target.join(entry.file_name()))?;
    }
    Ok(())
}

fn remove_tree(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn every_folder_travels_and_the_old_place_is_left_empty() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("mirror");
        let new = temp.path().join("Nextcloud").join("Mitschnitt");
        write(&old.join("2026-05-05_standup_s1/summary.md"), "# Standup");
        write(
            &old.join("2026-05-06_jour-fixe_s2/summary.md"),
            "# Jour Fixe",
        );

        let report = move_root(&old, &new).unwrap();

        assert_eq!(report.moved, 2);
        assert!(report.is_complete());
        assert_eq!(
            std::fs::read_to_string(new.join("2026-05-05_standup_s1/summary.md")).unwrap(),
            "# Standup"
        );
        assert!(!old.join("2026-05-05_standup_s1").exists());
    }

    // Falsifier 3. A folder that cannot travel has to appear in the report by
    // name; a migration that swallows one is the silent half-move.
    //
    // And the folder next to it stays put as well: one blocked name means the
    // whole move does not happen. Moving the others anyway is what leaves
    // meetings in two places -- whichever root the app is then pointed at, the
    // other one holds meetings nobody sees again.
    #[test]
    fn one_folder_that_cannot_travel_keeps_every_other_folder_at_home() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("mirror");
        let new = temp.path().join("ziel");
        write(&old.join("kann-weg/summary.md"), "ok");
        write(&old.join("blockiert/summary.md"), "ok");
        // Something is already sitting at the target name.
        write(&new.join("blockiert/summary.md"), "fremd");

        let report = move_root(&old, &new).unwrap();

        assert_eq!(report.moved, 0);
        assert!(!report.is_complete());
        assert_eq!(report.failed_total, 1);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, "blockiert");
        assert!(
            old.join("blockiert").is_dir(),
            "the folder was reported as failed but removed anyway"
        );
        assert!(
            old.join("kann-weg/summary.md").is_file(),
            "a meeting travelled although the move as a whole could not"
        );
        assert!(
            !new.join("kann-weg").exists(),
            "a meeting was left at the new place while the app keeps using the old one"
        );
    }

    // The same folder named differently is not a move. Without canonicalising,
    // `mirror/../mirror` reads as a new root, every entry collides with itself
    // and the report claims every meeting failed to travel.
    #[test]
    fn a_root_named_the_long_way_round_is_still_the_same_root() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("mirror");
        write(&old.join("a/summary.md"), "ok");
        let same_place = temp.path().join("mirror/../mirror");

        assert_eq!(move_root(&old, &same_place).unwrap(), MoveReport::default());
        assert!(old.join("a/summary.md").is_file());
    }

    // A copy that dies half way must not leave its wreckage at the new place:
    // the next attempt would find the name taken and report "already exists" --
    // a permanent failure caused by a temporary one, and no way out from the
    // app.
    #[cfg(unix)]
    #[test]
    fn a_copy_that_fails_does_not_leave_wreckage_blocking_the_next_attempt() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("mirror");
        let new = temp.path().join("ziel");
        write(&old.join("meeting/summary.md"), "ok");
        write(&old.join("meeting/geheim.md"), "unlesbar");
        let unreadable = old.join("meeting/geheim.md");
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

        let report = move_entry_with(&old.join("meeting"), &new.join("meeting"), false);
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(report.is_err(), "an unreadable file was copied anyway");
        assert!(
            !new.join("meeting").exists(),
            "the half-written copy stayed at the new place and blocks every later attempt"
        );
        assert!(
            old.join("meeting/summary.md").is_file(),
            "the original was touched although the copy never completed"
        );
    }

    // A move that fails half way puts back what it already moved, so the app
    // and the meetings stay in one place together.
    #[cfg(unix)]
    #[test]
    fn a_move_that_fails_half_way_puts_the_travelled_folders_back() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("mirror");
        let new = temp.path().join("ziel");
        write(&old.join("a-reist/summary.md"), "ok");
        write(&old.join("b-bleibt/summary.md"), "ok");
        // Folders are walked in name order, so "a" travels before "b" fails.
        // "b" fails because one of its files cannot be read -- which only
        // matters on the copy path, the one a move across volumes always takes.
        let stuck = old.join("b-bleibt");
        let unreadable = stuck.join("geheim.md");
        write(&unreadable, "unlesbar");
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

        let report = move_root_with(&old, &new, false);
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();
        let report = report.unwrap();

        assert!(!report.is_complete());
        assert_eq!(report.moved, 0, "the report claims folders travelled");
        assert!(stuck.is_dir());
        assert!(
            old.join("a-reist/summary.md").is_file(),
            "the folder that travelled was not put back, so the meetings are in two places"
        );
        assert!(!new.join("a-reist").exists());
    }

    // A long failure list is truncated for reading, but the count must not be:
    // a report that says "3 failed" when 40 did turns a broken migration into a
    // small one in the reader's head.
    #[test]
    fn the_named_list_is_capped_but_the_count_is_not() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("mirror");
        let new = temp.path().join("ziel");
        let blocked = MOVE_REPORT_LIMIT + 5;
        for index in 0..blocked {
            write(&old.join(format!("ordner-{index}/summary.md")), "ok");
            write(&new.join(format!("ordner-{index}/summary.md")), "fremd");
        }

        let report = move_root(&old, &new).unwrap();

        assert_eq!(report.moved, 0);
        assert_eq!(report.failed_total, blocked);
        assert_eq!(report.failed.len(), MOVE_REPORT_LIMIT);
    }

    #[test]
    fn moving_a_root_onto_itself_does_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("mirror");
        write(&old.join("a/summary.md"), "ok");

        assert_eq!(move_root(&old, &old).unwrap(), MoveReport::default());
        assert!(old.join("a/summary.md").is_file());
    }

    #[test]
    fn a_root_that_was_never_written_is_not_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let report = move_root(&temp.path().join("nie-da"), &temp.path().join("neu")).unwrap();
        assert_eq!(report, MoveReport::default());
    }
}
