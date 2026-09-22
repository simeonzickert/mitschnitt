//! The Markdown mirror.
//!
//! Next to the SQLite database the app writes a `mirror/` folder holding one
//! readable folder per meeting: the transcript as Markdown, the notes and
//! summary as Markdown, the metadata as JSON, and -- since F16 -- the recording
//! itself, as a hard link into `sessions/` rather than a second copy (see
//! [`audio`]). One meeting, one folder: `sessions/<uuid>` is the machine room
//! and no longer a place a person is ever sent.
//!
//! **The direction is one way.** Nothing in this crate reads the mirror back
//! into the database, and nothing may be added that does. Suggestions from
//! outside travel the other road, through `session_proposals`
//! (`crates/agent-access/src/proposals.rs`), where a human accepts or declines
//! them in the app.
//!
//! A mirror can go stale, so staleness is a first-class result: [`audit`]
//! reports which sessions are missing, out of date, or unreadable, and which
//! mirror folders no longer belong to any session.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

pub mod audio;
mod layout;
mod render;
mod root;
mod source;
mod speaker;

pub use audio::{AudioOutcome, RecorderView};
pub use layout::{META_FILE, SUMMARY_FILE, TRANSCRIPT_FILE};
pub use render::MirrorFiles;
pub use root::{
    MOVE_REPORT_LIMIT, MoveReport, ROOT_SETTING_KEY, move_root, read_root_setting,
    write_root_setting,
};
pub use source::{MIRROR_FORMAT_VERSION, MirrorSource};

pub const MIRROR_DIR: &str = "mirror";

/// The tables a rendered mirror is built from.
///
/// Lives here rather than in the app because this crate is the one that knows
/// what it reads: a watcher elsewhere can only stay correct if it takes its
/// list from the renderer. A change to any other table cannot move a mirror
/// file, so it does not need to wake anyone.
pub const RENDERED_TABLES: [&str; 5] = [
    "sessions",
    "session_documents",
    "transcripts",
    "session_participants",
    "action_items",
];

const SESSIONS_DIR: &str = "sessions";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Where the mirror lives and where the audio it points at lives.
#[derive(Debug, Clone)]
pub struct MirrorPaths {
    pub root: PathBuf,
    pub sessions_root: PathBuf,
}

impl MirrorPaths {
    /// `app_data_dir` is the folder holding `app.db` -- for the fork that is
    /// `.../media.zickert.mitschnitt` resolved through
    /// `anlg_storage::global::resolve_app_folder`, never an upstream folder.
    pub fn for_app_data_dir(app_data_dir: &Path) -> Self {
        Self::with_root(Self::default_root(app_data_dir), app_data_dir)
    }

    /// The mirror somewhere else -- a synced folder, say. Only the readable
    /// side moves: `sessions_root` and the database stay on this machine, so a
    /// sync client never gets to touch SQLite.
    pub fn with_root(root: PathBuf, app_data_dir: &Path) -> Self {
        Self {
            root,
            sessions_root: app_data_dir.join(SESSIONS_DIR),
        }
    }

    pub fn default_root(app_data_dir: &Path) -> PathBuf {
        app_data_dir.join(MIRROR_DIR)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcome {
    Written,
    UpToDate,
    SessionGone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirrorState {
    /// Mirror exists and was built from the session as it stands now.
    Current,
    /// No mirror folder for this session at all.
    Missing,
    /// A mirror exists but the session moved on since it was written.
    Stale,
    /// A mirror folder exists but its metadata cannot be trusted.
    Unreadable(String),
}

#[derive(Debug, Clone)]
pub struct SessionAudit {
    pub session_id: String,
    pub title: String,
    pub state: MirrorState,
}

#[derive(Debug, Clone, Default)]
pub struct AuditReport {
    pub sessions: Vec<SessionAudit>,
    /// Mirror folders whose session no longer exists in the database. Reported,
    /// never deleted -- removing a human-readable copy of a meeting is not a
    /// decision a background job gets to make.
    pub orphans: Vec<String>,
}

impl AuditReport {
    pub fn gaps(&self) -> impl Iterator<Item = &SessionAudit> {
        self.sessions
            .iter()
            .filter(|entry| entry.state != MirrorState::Current)
    }

    pub fn has_gaps(&self) -> bool {
        self.gaps().next().is_some() || !self.orphans.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    pub written: Vec<String>,
    pub up_to_date: usize,
    pub orphans: Vec<String>,
    /// Meetings whose recording was left in the machine room because the
    /// recorder still holds them (Mitschnitt-Fork, F16d).
    ///
    /// Carried out of the pass rather than dropped, because the normal case and
    /// the stuck case look identical from inside: a meeting being recorded shows
    /// up here for a few passes, and a finalisation that wedges shows up here
    /// for ever. `finalizing_sessions` is only swept when the next session
    /// starts, so a supervisor that neither finishes nor fails keeps its meeting
    /// held indefinitely -- and the pass would otherwise report `UpToDate` with
    /// no recording in the folder and no trace anywhere.
    pub recording_held: Vec<String>,
    /// Session id and reason for every meeting this pass could not mirror.
    ///
    /// A pass walks every meeting, so one unwritable folder -- a synced folder
    /// briefly gone, a lost permission, a full disk -- used to abort the run and
    /// take every meeting behind it with it, leaving nothing but a log line. A
    /// failure belongs to one meeting and is carried out of the pass as a value
    /// the caller has to look at.
    pub failed: Vec<(String, String)>,
}

/// Mirrors one session. Returns [`SyncOutcome::UpToDate`] without touching the
/// disk when the fingerprint already matches, unless `force` is set.
pub async fn sync_session(
    pool: &SqlitePool,
    paths: &MirrorPaths,
    session_id: &str,
    force: bool,
    recorder: &RecorderView,
) -> Result<SyncOutcome> {
    let index = read_index(&paths.root)?;
    sync_one(pool, paths, session_id, force, &index, recorder).await
}

/// Mirrors every session that is missing or stale (or all of them, with
/// `force`). This is what both the CLI and the app's background watcher call,
/// so there is exactly one definition of "bring the mirror up to date".
pub async fn sync_all(
    pool: &SqlitePool,
    paths: &MirrorPaths,
    force: bool,
    recorder: &RecorderView,
) -> Result<SyncReport> {
    let index = read_index(&paths.root)?;
    let sessions = source::list_session_ids(pool).await?;

    let mut report = SyncReport::default();
    for (session_id, _) in &sessions {
        // Per meeting, not per pass: a folder the app cannot write into is one
        // meeting's problem, and letting it end the run would silently stop
        // mirroring every meeting behind it.
        if recorder.holds(session_id) {
            report.recording_held.push(session_id.clone());
        }
        match sync_one(pool, paths, session_id, force, &index, recorder).await {
            Ok(SyncOutcome::Written) => report.written.push(session_id.clone()),
            Ok(SyncOutcome::UpToDate) => report.up_to_date += 1,
            Ok(SyncOutcome::SessionGone) => {}
            Err(error) => report.failed.push((session_id.clone(), error.to_string())),
        }
    }

    let live: Vec<String> = sessions.into_iter().map(|(id, _)| id).collect();
    report.orphans = orphans(&index, &live);
    Ok(report)
}

/// Reports which sessions have no mirror or a mirror that has fallen behind.
///
/// Without this the mirror would quietly become a second, wrong truth, which is
/// worse than having no mirror at all.
pub async fn audit(pool: &SqlitePool, paths: &MirrorPaths) -> Result<AuditReport> {
    let index = read_index(&paths.root)?;
    let sessions = source::list_session_ids(pool).await?;

    let mut report = AuditReport::default();
    for (session_id, title) in &sessions {
        let state = match index.get(session_id) {
            None => MirrorState::Missing,
            Some(entry) => match &entry.fingerprint {
                Err(reason) => MirrorState::Unreadable(reason.clone()),
                Ok(recorded) => {
                    let Some(current) = source::load(pool, session_id).await? else {
                        continue;
                    };
                    if !mirror_files_present(&paths.root.join(&entry.dir_name)) {
                        MirrorState::Unreadable("a mirror file is missing".to_string())
                    } else if *recorded == current.fingerprint {
                        MirrorState::Current
                    } else {
                        MirrorState::Stale
                    }
                }
            },
        };

        report.sessions.push(SessionAudit {
            session_id: session_id.clone(),
            title: title.clone(),
            state,
        });
    }

    let live: Vec<String> = sessions.into_iter().map(|(id, _)| id).collect();
    report.orphans = orphans(&index, &live);
    Ok(report)
}

async fn sync_one(
    pool: &SqlitePool,
    paths: &MirrorPaths,
    session_id: &str,
    force: bool,
    index: &Index,
    recorder: &RecorderView,
) -> Result<SyncOutcome> {
    let Some(source) = source::load(pool, session_id).await? else {
        return Ok(SyncOutcome::SessionGone);
    };

    let existing = index.get(session_id);
    let up_to_date = existing.is_some_and(|entry| {
        entry.fingerprint.as_deref() == Ok(source.fingerprint.as_str())
            && mirror_files_present(&paths.root.join(&entry.dir_name))
    });
    if up_to_date && !force {
        // The recording moves without the database moving: finishing a meeting
        // turns `audio.wav` into a fresh `audio.mp3`, and the fingerprint knows
        // nothing about it. So the link is checked on every pass, not only when
        // a Markdown file is rewritten.
        if let Some(entry) = existing {
            audio::ensure(
                &paths.root.join(&entry.dir_name),
                &paths.sessions_root,
                session_id,
                recorder,
            )?;
        }
        return Ok(SyncOutcome::UpToDate);
    }

    let files = render::render(
        &source,
        &paths.sessions_root,
        &chrono::Utc::now().to_rfc3339(),
    );
    let target = paths.root.join(&files.dir_name);

    // A renamed meeting changes the folder name. Moving the old folder keeps
    // one mirror per session instead of leaving a stale twin behind.
    //
    // A rename that cannot happen ends the pass for this meeting rather than
    // carrying on and writing the new folder anyway. Carrying on is what creates
    // the twin: two folders with the same session id in their metadata, of which
    // the index can only keep one, and the other belongs to no cleanup -- not
    // the live folder, and not an orphan either, because its meeting still
    // exists. It holds a full copy of the meeting and nothing ever looks at it
    // again. Ending here costs one pass; the next one tries the rename again,
    // and `sync_all` collects the reason so it is said out loud meanwhile.
    if let Some(entry) = existing
        && entry.dir_name != files.dir_name
        && paths.root.join(&entry.dir_name).is_dir()
    {
        let old = paths.root.join(&entry.dir_name);
        if target.exists() {
            return Err(Error::io(
                format!(
                    "cannot rename {} to {}: something is already there",
                    old.display(),
                    target.display()
                ),
                std::io::Error::from(std::io::ErrorKind::AlreadyExists),
            ));
        }
        std::fs::rename(&old, &target)
            .map_err(|error| Error::io(format!("rename {}", old.display()), error))?;
    }

    std::fs::create_dir_all(&target)
        .map_err(|error| Error::io(format!("create {}", target.display()), error))?;

    write_atomically(&target.join(TRANSCRIPT_FILE), &files.transcript_md)?;
    write_atomically(&target.join(SUMMARY_FILE), &files.summary_md)?;
    write_atomically(&target.join(META_FILE), &files.meta_json)?;
    audio::ensure(&target, &paths.sessions_root, session_id, recorder)?;

    Ok(SyncOutcome::Written)
}

/// The one folder a person is sent to for this meeting, if it has been written.
///
/// This is what every "Show in Finder" resolves through: the machine room is
/// not a place anyone should be shown.
pub fn session_folder(paths: &MirrorPaths, session_id: &str) -> Result<Option<PathBuf>> {
    let index = read_index(&paths.root)?;
    Ok(index
        .get(session_id)
        .map(|entry| paths.root.join(&entry.dir_name)))
}

/// Removes the recording from the readable folder. Returns whether there was
/// one to remove.
///
/// For a deletion **a person asked for**, and nothing else. Since F16 the
/// recording lives in the readable folder too -- as a hard link, so deleting it
/// in the machine room leaves the bytes alive under the other name, and on a
/// copy (a mirror root on another volume) there is a whole second file. Either
/// way the app said "deleted" while the recording sat in the meeting folder, and
/// with a synced root it had already been uploaded.
///
/// The retention deadline is such a deletion and does call this, since F16b: the
/// person set the deadline, and a deadline that empties the machine room while
/// the recording stays in the meeting folder deletes nothing at all. What must
/// not call this is the mirror's own housekeeping -- a pass that finds no
/// recording in `sessions/` leaves the human folder alone, because that copy can
/// be the last one in existence. The line runs between callers, not between "by
/// hand" and "automatic".
pub fn forget_audio(paths: &MirrorPaths, session_id: &str) -> Result<bool> {
    let Some(dir) = session_folder(paths, session_id)? else {
        return Ok(false);
    };
    audio::forget(&dir)
}

/// Removes the whole readable folder for a meeting the person deleted.
///
/// Without this a deleted meeting keeps its transcript, its summary and its
/// recording in the folder a person actually looks at; the audit reported the
/// folder as an orphan and deliberately never touched it, which is right for a
/// folder whose meeting merely vanished from the database and wrong for one the
/// person deleted on purpose.
pub fn forget_session(paths: &MirrorPaths, session_id: &str) -> Result<bool> {
    let Some(dir) = session_folder(paths, session_id)? else {
        return Ok(false);
    };
    if !dir.is_dir() {
        return Ok(false);
    }
    std::fs::remove_dir_all(&dir)
        .map_err(|error| Error::io(format!("remove {}", dir.display()), error))?;
    Ok(true)
}

/// Written to a sibling temp file and renamed, so a reader never sees half a
/// transcript and a crash never leaves a truncated file behind.
fn write_atomically(path: &Path, contents: &str) -> Result<()> {
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, contents)
        .map_err(|error| Error::io(format!("write {}", temp.display()), error))?;
    std::fs::rename(&temp, path)
        .map_err(|error| Error::io(format!("replace {}", path.display()), error))
}

fn mirror_files_present(dir: &Path) -> bool {
    [TRANSCRIPT_FILE, SUMMARY_FILE, META_FILE]
        .iter()
        .all(|name| dir.join(name).is_file())
}

#[derive(Debug, Clone)]
struct IndexEntry {
    dir_name: String,
    /// `Err` carries why the folder's metadata could not be trusted.
    fingerprint: std::result::Result<String, String>,
}

#[derive(Debug, Clone, Default)]
struct Index {
    by_session: HashMap<String, IndexEntry>,
    /// Folders whose session id was already claimed by another folder.
    ///
    /// A meeting has exactly one readable folder, and a rename moves it. When
    /// the move does not happen -- a sync client holding the old folder, a name
    /// already taken at the new one -- two folders end up carrying the same
    /// session id. The map keeps one of them, and the other used to vanish
    /// completely: not the live folder, so no pass ever wrote to it, and not an
    /// orphan either, because its session is alive. Invisible to every cleanup
    /// there is, holding a full copy of a meeting. They are reported as orphans,
    /// which is where a person looks for folders nothing owns.
    shadowed: Vec<String>,
}

impl Index {
    fn get(&self, session_id: &str) -> Option<&IndexEntry> {
        self.by_session.get(session_id)
    }
}

/// Maps session id to mirror folder by reading each folder's `meta.json`.
///
/// The folder name is not the key: it carries the meeting title, so it changes
/// when a meeting is renamed. The id inside the metadata is the stable link.
fn read_index(root: &Path) -> Result<Index> {
    let mut index = Index::default();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(index),
        Err(error) => return Err(Error::io(format!("read {}", root.display()), error)),
    };

    // Sorted, so which of two folders claiming one meeting is kept does not
    // depend on the order the filesystem happens to hand them back.
    let mut entries: Vec<_> = entries
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let meta_path = entry.path().join(META_FILE);

        let (session_id, fingerprint) = match std::fs::read_to_string(&meta_path) {
            Err(_) => (None, Err("meta.json is missing or unreadable".to_string())),
            Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Err(error) => (None, Err(format!("meta.json is not valid JSON: {error}"))),
                Ok(value) => {
                    let session_id = value
                        .get("session_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                    let fingerprint = value
                        .get("source_fingerprint")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| "meta.json has no source_fingerprint".to_string());
                    (session_id, fingerprint)
                }
            },
        };

        // A folder with no readable session id cannot be attributed, so it is
        // reported as an orphan rather than silently ignored.
        let key = session_id.unwrap_or_else(|| format!("?{dir_name}"));
        if let Some(displaced) = index.by_session.insert(
            key,
            IndexEntry {
                dir_name: dir_name.clone(),
                fingerprint,
            },
        ) {
            index.shadowed.push(displaced.dir_name);
        }
    }

    Ok(index)
}

fn orphans(index: &Index, live_session_ids: &[String]) -> Vec<String> {
    let mut orphans: Vec<String> = index
        .by_session
        .iter()
        .filter(|(session_id, _)| !live_session_ids.contains(session_id))
        .map(|(_, entry)| entry.dir_name.clone())
        .collect();
    orphans.extend(index.shadowed.iter().cloned());
    orphans.sort();
    orphans
}

#[cfg(test)]
mod tests;
