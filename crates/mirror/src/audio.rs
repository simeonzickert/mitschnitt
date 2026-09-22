//! Puts the recording into the human folder.
//!
//! Mitschnitt-Fork (F16). Until now a meeting lived in two places: the readable
//! folder held transcript, summary and metadata, while the recording stayed
//! behind in `sessions/<uuid>`. Two places confuse people, so the recording
//! joins the readable folder -- as a **hard link**, so the bytes exist once on
//! disk and the machine room keeps the path every other part of the app resolves
//! by convention.
//!
//! A hard link only works inside one volume, so there is a plain copy as a
//! fallback. **The fallback is rarer than it sounds, and it was described wrongly
//! here until it was measured.** A synced folder in the home directory --
//! Nextcloud, iCloud, Dropbox -- is on macOS the same APFS volume as the
//! application data folder, and the link goes straight through: measured on the
//! target machine, `~/Nextcloud` and `~/Library/Application Support/...` both
//! report `dev=16777229`, both `/dev/disk3s5` on `/System/Volumes/Data`, and a
//! hard link across that boundary succeeds. The copy path is for external disks
//! and network volumes. There is still no volume guessing: the link is attempted
//! and the error decides, because device ids would be wrong exactly on the cases
//! that matter (bind mounts, network shares).
//!
//! **Only a finished recording travels**, and that is decided twice over. While
//! a meeting is being recorded, the app writes into `audio.wav` and flushes it
//! once a second. Linking that file puts a growing recording into the synced
//! folder, so a sync client uploads a meeting while it is still happening and
//! re-uploads it after every flush; a copy made from it is a truncated meeting.
//!
//! The first test asks the recorder whether it still holds this meeting
//! ([`RecorderView`]); the second asks the file whether it has stopped changing
//! ([`has_settled`]). Both are needed. The recorder knows about a meeting whose
//! bytes are quiet for other reasons -- muted microphone, lost device, an encode
//! in flight -- which the file cannot tell from a finished recording. The file
//! knows about writers the recorder never hears about: an import, a sync client
//! pulling a file down. Either one says "not yet", and the recording arrives on
//! a later pass instead. Neither is an error.
//!
//! The recording is not stable over its lifetime either, which is why this runs
//! on every pass and not only when a Markdown file is rewritten:
//!
//! * a finished recording is encoded from `audio.wav` to `audio.mp3` -- new
//!   name, new inode, the old link would point at a deleted file;
//! * "keep recording" decodes the mp3 back to a wav and deletes the mp3;
//! * an import writes the file afresh.
//!
//! And it never deletes on its own. Nothing in this file removes a recording
//! from the human folder except [`forget`], and [`forget`] is only ever reached
//! from a deletion somebody asked for -- either by hand, or by the retention
//! deadline they set. **The deadline is one of those since F16b**: the app calls
//! `mirror_forget_audio` on the same path it clears the machine room with, so
//! the recording goes from both places at once. That is the app's decision and
//! it is deliberate; a deadline that deletes only half is a lie. It is written
//! here because this file used to say the opposite -- that automatic retention
//! cleared the machine room only and the human copy was the last one standing --
//! and a stale sentence in the place a person looks first is worse than none.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Which meetings the recorder is holding right now.
///
/// Mitschnitt-Fork (F16d). File quiet used to be the only test for "this
/// recording is finished", and it answers a different question: it says the
/// bytes stopped moving, not that the meeting is over. A recording the recorder
/// still holds can sit unchanged for minutes -- the microphone muted or gone,
/// the device switched, a stalled live transcription, or the encode from
/// `audio.wav` to `audio.mp3` while the old wav lies untouched beside it. Every
/// one of those is a meeting in progress that the fifteen-second window reads as
/// done, and mirroring it puts half a meeting into a synced folder.
///
/// So the recorder is asked. The file-quiet window stays as the second net, for
/// the cases the recorder cannot see: an import writing a file, a sync client
/// still pulling one down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecorderView {
    /// The recorder answered. These sessions are being recorded or finalised;
    /// everything else is finished as far as the recorder is concerned.
    Held(HashSet<String>),
    /// Nobody could ask the recorder -- the CLI runs in its own process, and in
    /// the app the actor can time out. Only the file-quiet window applies, which
    /// is what this crate did before it could ask at all.
    Unavailable,
}

impl RecorderView {
    /// The recorder answered and holds nothing.
    pub fn idle() -> Self {
        Self::Held(HashSet::new())
    }

    /// The recorder answered and holds exactly these sessions.
    pub fn holding<I, S>(session_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Held(session_ids.into_iter().map(Into::into).collect())
    }

    /// Whether this meeting is still in the recorder's hands.
    ///
    /// [`Self::Unavailable`] answers `false`, deliberately: an unanswered
    /// question must not stop the mirror, or a missing recorder would mean no
    /// recording ever reaches a readable folder again. The file-quiet window
    /// carries that case.
    pub fn holds(&self, session_id: &str) -> bool {
        match self {
            Self::Held(ids) => ids.contains(session_id),
            Self::Unavailable => false,
        }
    }
}

/// The names the app gives a session recording. Same list as
/// `fs_sync_core::audio`, kept here as three literals rather than as a
/// dependency on that crate: it drags in the audio codecs, and its lookup
/// rejects any session id that is not a uuid.
const AUDIO_FILE_NAMES: [&str; 3] = ["audio.mp3", "audio.wav", "audio.ogg"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOutcome {
    /// The human folder already held exactly this recording.
    AlreadyThere,
    /// Hard link created: one set of bytes, two names.
    Linked,
    /// Same volume was not available, so the bytes were copied.
    Copied,
    /// The machine room has no recording for this session. Whatever the human
    /// folder holds is left alone -- it may be the last copy in existence.
    NoSource,
    /// The recording is still being written, so it is not a recording yet. Not
    /// an error: the next pass after it stops changing picks it up.
    StillBeingWritten,
    /// The recorder still holds this meeting. The file may well have stopped
    /// changing -- a muted microphone, a stalled transcription, an encode in
    /// flight -- and that is exactly the case the file-quiet window gets wrong.
    /// Not an error: the pass after the recorder lets go picks it up.
    StillRecording,
}

/// How long a recording has to have stopped changing before it counts as one.
///
/// The recorder flushes to disk once a second (`FLUSH_INTERVAL` in
/// `listener-core`), so anything untouched for this long is not being recorded
/// into. Generous on purpose: being a minute late with a finished recording
/// costs a wait, being early puts a live meeting into a synced folder.
const SETTLE_FOR: std::time::Duration = std::time::Duration::from_secs(15);

/// The session folder in the machine room.
///
/// Sessions can be filed into sub-folders, so the direct path is tried first and
/// a recursive search second -- the same order `fs_sync_core::find_session_dir`
/// uses, minus its uuid check.
pub fn session_dir(sessions_root: &Path, session_id: &str) -> PathBuf {
    let direct = sessions_root.join(session_id);
    if direct.is_dir() {
        return direct;
    }
    find_recursively(sessions_root, session_id).unwrap_or(direct)
}

fn find_recursively(dir: &Path, session_id: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|name| name == session_id) {
            return Some(path);
        }
        if let Some(found) = find_recursively(&path, session_id) {
            return Some(found);
        }
    }
    None
}

/// The recording the machine room currently holds, if any.
pub fn source_path(sessions_root: &Path, session_id: &str) -> Option<PathBuf> {
    let dir = session_dir(sessions_root, session_id);
    AUDIO_FILE_NAMES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
}

/// Makes the human folder carry the current recording.
pub fn ensure(
    mirror_dir: &Path,
    sessions_root: &Path,
    session_id: &str,
    recorder: &RecorderView,
) -> Result<AudioOutcome> {
    ensure_with(mirror_dir, sessions_root, session_id, recorder, true)
}

/// The body of [`ensure`], with the hard link as a switch.
///
/// The switch exists for the tests: a hard link works whenever both roots share
/// a volume, and a test that puts them in one tempdir can therefore never reach
/// the copy fallback -- which is exactly the path a mirror on a synced folder
/// takes on every single pass. Taking the link away reproduces that path without
/// mounting a second volume.
fn ensure_with(
    mirror_dir: &Path,
    sessions_root: &Path,
    session_id: &str,
    recorder: &RecorderView,
    hard_link_allowed: bool,
) -> Result<AudioOutcome> {
    // Asked before the file is even looked at: the recorder's answer is about
    // the meeting, the file's modification time is about the bytes, and only the
    // first one knows the difference between a finished recording and a quiet
    // one.
    // Nothing else in this function runs while a meeting is held -- no staged
    // leftover is cleared, no superseded name is pruned. That is deliberate: all
    // of it is housekeeping around a recording that is about to change again
    // anyway, and it happens on the first pass after the recorder lets go.
    if recorder.holds(session_id) {
        return Ok(AudioOutcome::StillRecording);
    }
    let Some(source) = source_path(sessions_root, session_id) else {
        return Ok(AudioOutcome::NoSource);
    };
    let Some(file_name) = source.file_name() else {
        return Ok(AudioOutcome::NoSource);
    };
    if !has_settled(&source)? {
        return Ok(AudioOutcome::StillBeingWritten);
    }
    let target = mirror_dir.join(file_name);

    if target.is_file() && already_current(&source, &target)? {
        prune_superseded(mirror_dir, &target)?;
        return Ok(AudioOutcome::AlreadyThere);
    }

    std::fs::create_dir_all(mirror_dir)
        .map_err(|error| Error::io(format!("create {}", mirror_dir.display()), error))?;

    // The replacement is built beside the target and moved onto it, never into
    // it. Writing in place would mean deleting the recording first -- `hard_link`
    // refuses an existing target and `copy` truncates one -- and every failure
    // after that point would leave the human folder without the recording it had
    // a moment ago. A leftover from a killed run is cleared rather than trusted.
    let staged = staged_path(&target, file_name);
    remove_if_present(&staged)?;

    let outcome = if hard_link_allowed && std::fs::hard_link(&source, &staged).is_ok() {
        AudioOutcome::Linked
    } else {
        remove_if_present(&staged)?;
        if let Err(error) = copy_into_place(&source, &staged) {
            remove_if_present(&staged)?;
            return Err(error);
        }
        AudioOutcome::Copied
    };

    if let Err(error) = std::fs::rename(&staged, &target) {
        remove_if_present(&staged)?;
        return Err(Error::io(format!("replace {}", target.display()), error));
    }

    prune_superseded(mirror_dir, &target)?;
    Ok(outcome)
}

/// Removes the recording from the human folder. Returns whether there was one.
///
/// **Only ever on a deletion somebody asked for.** The rule for this crate is a
/// rule about who asked, not about which file: such a deletion reaches both
/// places, because "deleted" that leaves the recording sitting in the meeting
/// folder -- and, with a synced root, on a server -- is a lie the app tells.
///
/// A retention deadline counts as asked for. The app routes it here on purpose
/// (`forgetMirroredAudio` in `session/attachments.ts`), because the person set
/// the deadline. What must never reach this function is the mirror's own
/// housekeeping: a pass that finds no recording in the machine room leaves the
/// human folder alone, because that copy may be the only one left. That is the
/// separation the tests guard, and it is a separation between *callers*, not
/// between "by hand" and "automatic".
pub fn forget(mirror_dir: &Path) -> Result<bool> {
    let mut removed = false;
    for name in AUDIO_FILE_NAMES {
        let path = mirror_dir.join(name);
        if path.is_file() {
            std::fs::remove_file(&path)
                .map_err(|error| Error::io(format!("remove {}", path.display()), error))?;
            removed = true;
        }
    }
    Ok(removed)
}

/// The recording under a name the app has since stopped using.
///
/// An encode changed `audio.wav` into `audio.mp3`; the old name would sit next
/// to the new one and the folder would hold two recordings, one of them dead.
/// Runs after the current recording is in place, never before: pruning first
/// would let a failed write leave the folder with no recording at all. And only
/// while a source exists, so a retention delete can never take the human copy
/// with it.
fn prune_superseded(mirror_dir: &Path, target: &Path) -> Result<()> {
    for name in AUDIO_FILE_NAMES {
        let stale = mirror_dir.join(name);
        if stale != *target && stale.is_file() {
            std::fs::remove_file(&stale)
                .map_err(|error| Error::io(format!("remove {}", stale.display()), error))?;
        }
    }
    Ok(())
}

/// Copies the recording and stamps the source's modification time onto it.
///
/// The stamp is not cosmetic, it is what makes [`already_current`] work on the
/// copy path: a copy shares neither device nor inode with its source, so
/// sameness can only be read off length plus time. Without it every pass would
/// copy the whole recording again -- and the watcher walks every session once a
/// minute, so on a mirror root inside a synced folder that is the entire library
/// re-uploaded, every minute, for as long as the app runs.
///
/// A filesystem that refuses the stamp costs repeated copying, not a wrong file,
/// so it is not worth failing the mirror over.
fn copy_into_place(source: &Path, staged: &Path) -> Result<()> {
    std::fs::copy(source, staged).map_err(|error| {
        Error::io(
            format!("copy {} to {}", source.display(), staged.display()),
            error,
        )
    })?;

    if let Ok(modified) = std::fs::metadata(source).and_then(|meta| meta.modified())
        && let Ok(file) = std::fs::OpenOptions::new().write(true).open(staged)
    {
        let _ = file.set_modified(modified);
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::io(format!("remove {}", path.display()), error)),
    }
}

/// A sibling of the target, so the move onto it is a rename within one folder
/// and therefore atomic. Hidden, and a fixed name rather than a unique one: the
/// app is the only writer, and a fixed name is the one a later run can clean up.
fn staged_path(target: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    target.with_file_name(format!(".{}.part", file_name.to_string_lossy()))
}

/// Whether the recording has stopped changing.
///
/// The second of the two tests, and on its own it was never enough: a quiet file
/// is not a finished meeting, only a meeting nobody is writing to this second
/// (see [`RecorderView`]). What this one is for is the writers the recorder does
/// not know about -- a file being imported, a file a sync client is still
/// pulling down -- which share one symptom: the file was written a moment ago.
///
/// A file with no readable modification time is let through: the filesystems
/// that cannot answer are the ones where nothing is being recorded either.
fn has_settled(source: &Path) -> Result<bool> {
    let meta = std::fs::metadata(source)
        .map_err(|error| Error::io(format!("read {}", source.display()), error))?;
    let Ok(modified) = meta.modified() else {
        return Ok(true);
    };
    // A modification time in the future (a clock jump, a copied-in file) reads
    // as an error here; waiting is the harmless way to be wrong.
    Ok(modified
        .elapsed()
        .map(|age| age >= SETTLE_FOR)
        .unwrap_or(false))
}

/// Whether the human folder already holds this exact recording.
///
/// Two answers, because there are two ways the recording gets there. A hard link
/// shares device and inode with its source, which is the only check that
/// distinguishes a live link from a copy that happens to be the same size. A
/// copy shares neither, so it is judged by length plus the modification time
/// [`copy_into_place`] stamps onto it -- an answer that is only trustworthy
/// because this crate writes that time itself.
fn already_current(source: &Path, target: &Path) -> Result<bool> {
    let source_meta = std::fs::metadata(source)
        .map_err(|error| Error::io(format!("read {}", source.display()), error))?;
    let target_meta = std::fs::metadata(target)
        .map_err(|error| Error::io(format!("read {}", target.display()), error))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if source_meta.dev() == target_meta.dev() && source_meta.ino() == target_meta.ino() {
            return Ok(true);
        }
    }

    if source_meta.len() != target_meta.len() {
        return Ok(false);
    }
    Ok(match (source_meta.modified(), target_meta.modified()) {
        (Ok(source_time), Ok(target_time)) => source_time == target_time,
        // A filesystem without modification times cannot answer the question,
        // and copying again is the harmless way to be wrong.
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recording that is over: written, then dated back past the settle
    /// window, because a file written this instant is indistinguishable from a
    /// meeting being recorded right now -- which is the point of `has_settled`.
    fn write_recording(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = write_growing_recording(dir, name, bytes);
        settle(&path);
        path
    }

    /// A recording still being written into: the recorder flushes once a
    /// second, so its modification time is always now.
    fn write_growing_recording(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn settle(path: &Path) {
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(std::time::SystemTime::now() - SETTLE_FOR * 2)
            .unwrap();
    }

    // Both roots live under one tempdir, so the hard link path is the one under
    // test rather than the copy fallback.
    fn roots() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let sessions = temp.path().join("sessions");
        let mirror = temp.path().join("mirror").join("2026-05-05_standup_s1");
        std::fs::create_dir_all(&mirror).unwrap();
        (temp, sessions, mirror)
    }

    // F16d, the case the file-quiet window gets wrong. The recorder still holds
    // this meeting -- muted microphone, lost device, a stalled transcription --
    // and its file has not moved for a minute. Everything the file can say says
    // "finished", and it is not.
    #[test]
    fn a_meeting_the_recorder_still_holds_does_not_travel_however_quiet_its_file_is() {
        let (_temp, sessions, mirror) = roots();
        let source = write_growing_recording(&sessions.join("s1"), "audio.wav", b"halbes-meeting");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&source)
            .unwrap();
        file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(60))
            .unwrap();
        drop(file);

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::holding(["s1"])).unwrap(),
            AudioOutcome::StillRecording,
            "a meeting the recorder still holds was mirrored because its file was quiet"
        );
        assert!(
            !mirror.join("audio.wav").exists(),
            "half a meeting is sitting in the readable folder"
        );
    }

    // The other side of the same switch: once the recorder has let go, the
    // recording travels. Without this the guard could be a blanket refusal and
    // the test above would still pass.
    #[test]
    fn a_meeting_the_recorder_has_let_go_of_travels() {
        let (_temp, sessions, mirror) = roots();
        write_recording(&sessions.join("s1"), "audio.mp3", b"tondaten");

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::idle()).unwrap(),
            AudioOutcome::Linked
        );
        assert_eq!(
            std::fs::read(mirror.join("audio.mp3")).unwrap(),
            b"tondaten"
        );
    }

    // A meeting being recorded right now fails both tests, and the outcome has
    // to name the recorder rather than the file -- otherwise a report cannot
    // tell "still going" from "a sync client is still pulling this down", and
    // the two want different answers from a person.
    #[test]
    fn a_running_meeting_is_refused_by_the_recorder_not_by_the_clock() {
        let (_temp, sessions, mirror) = roots();
        write_growing_recording(&sessions.join("s1"), "audio.wav", b"laeuft-noch");

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::holding(["s1"])).unwrap(),
            AudioOutcome::StillRecording
        );
        assert!(!mirror.join("audio.wav").exists());
    }

    // The guard is per meeting, not per pass. One meeting being recorded must
    // not stop every other meeting's recording from reaching its folder.
    #[test]
    fn one_running_meeting_does_not_hold_up_the_others() {
        let (_temp, sessions, mirror) = roots();
        write_recording(&sessions.join("s2"), "audio.mp3", b"tondaten");

        assert_eq!(
            ensure(&mirror, &sessions, "s2", &RecorderView::holding(["s1"])).unwrap(),
            AudioOutcome::Linked
        );
    }

    // An unanswered recorder must not stop the mirror altogether: the CLI can
    // never ask, and in the app the actor can time out. The file-quiet window
    // carries that case, which is what this crate did before it could ask.
    #[test]
    fn an_unanswered_recorder_falls_back_to_the_file_and_not_to_a_standstill() {
        let (_temp, sessions, mirror) = roots();
        write_recording(&sessions.join("s1"), "audio.mp3", b"tondaten");

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::Linked
        );

        let (_temp2, sessions2, mirror2) = roots();
        write_growing_recording(&sessions2.join("s1"), "audio.wav", b"laeuft-noch");
        assert_eq!(
            ensure(&mirror2, &sessions2, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::StillBeingWritten
        );
    }

    #[test]
    fn the_recording_arrives_as_a_hard_link_not_a_second_copy() {
        let (_temp, sessions, mirror) = roots();
        let source = write_recording(&sessions.join("s1"), "audio.mp3", b"tondaten");

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::Linked
        );

        let target = mirror.join("audio.mp3");
        assert_eq!(std::fs::read(&target).unwrap(), b"tondaten");
        assert!(
            already_current(&source, &target).unwrap(),
            "the human folder holds a second copy instead of the same bytes"
        );
    }

    // The dev+ino check has to make the second pass a no-op; without it every
    // sync would unlink and relink the recording.
    #[test]
    fn a_second_pass_leaves_the_link_alone() {
        let (_temp, sessions, mirror) = roots();
        write_recording(&sessions.join("s1"), "audio.mp3", b"tondaten");

        ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap();
        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::AlreadyThere
        );
    }

    // The measured failure mode: finishing a recording encodes `audio.wav` into
    // a fresh `audio.mp3` and deletes the wav. A link made before that points at
    // a deleted file, and the old name would linger in the human folder.
    #[test]
    fn an_encode_to_mp3_renews_the_link_and_takes_the_old_name_with_it() {
        let (_temp, sessions, mirror) = roots();
        let session = sessions.join("s1");
        write_recording(&session, "audio.wav", b"rohton");
        ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap();
        assert!(mirror.join("audio.wav").is_file());

        std::fs::remove_file(session.join("audio.wav")).unwrap();
        let encoded = write_recording(&session, "audio.mp3", b"encoded");

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::Linked
        );
        assert!(
            !mirror.join("audio.wav").exists(),
            "the superseded recording stayed behind in the human folder"
        );
        assert!(already_current(&encoded, &mirror.join("audio.mp3")).unwrap());
    }

    // A file written afresh under the same name is a different inode. Comparing
    // names only would leave the human folder pointing at the previous take.
    #[test]
    fn a_rewritten_recording_under_the_same_name_is_relinked() {
        let (_temp, sessions, mirror) = roots();
        let session = sessions.join("s1");
        write_recording(&session, "audio.mp3", b"erste aufnahme");
        ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap();

        std::fs::remove_file(session.join("audio.mp3")).unwrap();
        write_recording(&session, "audio.mp3", b"zweite aufnahme");

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::Linked
        );
        assert_eq!(
            std::fs::read(mirror.join("audio.mp3")).unwrap(),
            b"zweite aufnahme"
        );
    }

    // Falsifier 4. Retention deletes the machine-room recording; the human copy
    // is then the only one left and must survive every later pass.
    #[test]
    fn a_deleted_source_never_takes_the_human_copy_with_it() {
        let (_temp, sessions, mirror) = roots();
        write_recording(&sessions.join("s1"), "audio.mp3", b"tondaten");
        ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap();

        std::fs::remove_file(sessions.join("s1").join("audio.mp3")).unwrap();

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::NoSource
        );
        assert_eq!(
            std::fs::read(mirror.join("audio.mp3")).unwrap(),
            b"tondaten",
            "the human copy died with its source"
        );
    }

    // The copy path is what a mirror root on a synced folder takes, and the
    // watcher walks every session once a minute. If the second pass copies
    // again, a Nextcloud library is deleted and re-uploaded in full, every
    // minute, forever.
    #[test]
    fn a_second_pass_on_the_copy_path_does_not_copy_again() {
        let (_temp, sessions, mirror) = roots();
        write_recording(&sessions.join("s1"), "audio.mp3", b"tondaten");

        assert_eq!(
            ensure_with(&mirror, &sessions, "s1", &RecorderView::Unavailable, false).unwrap(),
            AudioOutcome::Copied
        );
        let target = mirror.join("audio.mp3");
        let after_first = std::fs::metadata(&target).unwrap().modified().unwrap();

        assert_eq!(
            ensure_with(&mirror, &sessions, "s1", &RecorderView::Unavailable, false).unwrap(),
            AudioOutcome::AlreadyThere,
            "the copy was written a second time"
        );
        assert_eq!(
            std::fs::metadata(&target).unwrap().modified().unwrap(),
            after_first,
            "the file was replaced although nothing about the recording changed"
        );
    }

    // The other direction of the same check: a freshness test that never
    // notices a change would be worse than copying every time.
    #[test]
    fn a_changed_recording_is_copied_again() {
        let (_temp, sessions, mirror) = roots();
        let session = sessions.join("s1");
        write_recording(&session, "audio.mp3", b"erste aufnahme");
        ensure_with(&mirror, &sessions, "s1", &RecorderView::Unavailable, false).unwrap();

        std::fs::remove_file(session.join("audio.mp3")).unwrap();
        write_recording(&session, "audio.mp3", b"zweite, laengere aufnahme");

        assert_eq!(
            ensure_with(&mirror, &sessions, "s1", &RecorderView::Unavailable, false).unwrap(),
            AudioOutcome::Copied
        );
        assert_eq!(
            std::fs::read(mirror.join("audio.mp3")).unwrap(),
            b"zweite, laengere aufnahme"
        );
    }

    // A copy that cannot be made must not have cost the recording that was
    // already there. Deleting first and writing second leaves a window in which
    // the human folder holds no recording at all -- and if the write fails, the
    // window never closes.
    #[test]
    fn a_failing_copy_leaves_the_recording_that_was_already_there() {
        use std::os::unix::fs::PermissionsExt as _;

        let (_temp, sessions, mirror) = roots();
        let session = sessions.join("s1");
        write_recording(&session, "audio.mp3", b"gute aufnahme");
        ensure_with(&mirror, &sessions, "s1", &RecorderView::Unavailable, false).unwrap();

        // A new take the machine cannot read: same name, different bytes.
        let source = session.join("audio.mp3");
        std::fs::remove_file(&source).unwrap();
        write_recording(&session, "audio.mp3", b"neue, unlesbare aufnahme");
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o000)).unwrap();

        let result = ensure_with(&mirror, &sessions, "s1", &RecorderView::Unavailable, false);
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(result.is_err(), "an unreadable source was reported as done");
        assert_eq!(
            std::fs::read(mirror.join("audio.mp3")).unwrap(),
            b"gute aufnahme",
            "the recording in the human folder was deleted for a replacement that never arrived"
        );
    }

    // The three tests above take the hard link away to reach the copy path.
    // This one reaches it the way a person does, on a second volume, through the
    // public entry point -- so the switch above is a stand-in for something real
    // rather than for a branch only a test can enter. Off by default because it
    // needs a mounted volume:
    //
    //   hdiutil create -size 20m -fs APFS -volname MitschnittProbe "$TMPDIR/probe.dmg"
    //   hdiutil attach "$TMPDIR/probe.dmg"
    //   MITSCHNITT_OTHER_VOLUME=/Volumes/MitschnittProbe \
    //     cargo test -p mirror --lib -- --ignored a_second_volume
    //   hdiutil detach /Volumes/MitschnittProbe
    #[test]
    #[ignore = "needs a second volume; see the comment above"]
    fn a_second_volume_takes_the_copy_path_and_settles_after_one_pass() {
        let Ok(volume) = std::env::var("MITSCHNITT_OTHER_VOLUME") else {
            panic!("set MITSCHNITT_OTHER_VOLUME to a mounted volume");
        };
        let temp = tempfile::tempdir().unwrap();
        let sessions = temp.path().join("sessions");
        let mirror = PathBuf::from(volume).join("mitschnitt-probe/2026-05-05_standup_s1");
        let _ = std::fs::remove_dir_all(mirror.parent().unwrap());
        std::fs::create_dir_all(&mirror).unwrap();
        write_recording(&sessions.join("s1"), "audio.mp3", b"tondaten");

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::Copied,
            "a second volume did not fall back to a copy"
        );
        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::AlreadyThere,
            "the copy on the second volume was written again"
        );

        std::fs::remove_dir_all(mirror.parent().unwrap()).unwrap();
    }

    // The real path on the target machine: the synced folder and the machine
    // room share a volume, so the link goes through -- and a link made while the
    // meeting is still being recorded puts the growing file straight into the
    // sync client's hands. It uploads a meeting that is still happening, and
    // re-uploads it after every flush, once a second.
    #[test]
    fn a_meeting_still_being_recorded_does_not_reach_the_human_folder() {
        let (_temp, sessions, mirror) = roots();
        write_growing_recording(&sessions.join("s1"), "audio.wav", b"waechst noch");

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::StillBeingWritten
        );
        assert!(
            !mirror.join("audio.wav").exists(),
            "a meeting that is still being recorded was put into the synced folder"
        );
    }

    // ...and it is not an error, so a meeting in progress must not make the pass
    // report a failure. It arrives on the first pass after the recording stops.
    #[test]
    fn the_recording_arrives_once_it_stops_changing() {
        let (_temp, sessions, mirror) = roots();
        let source = write_growing_recording(&sessions.join("s1"), "audio.wav", b"waechst noch");
        ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap();

        settle(&source);

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::Linked
        );
        assert_eq!(
            std::fs::read(mirror.join("audio.wav")).unwrap(),
            b"waechst noch"
        );
    }

    // The same guard covers a source being written for other reasons: an encode
    // in progress, an import, a sync client still pulling the file down. A copy
    // taken from any of them is a truncated meeting.
    #[test]
    fn a_half_written_source_is_not_copied_across() {
        let (_temp, sessions, mirror) = roots();
        write_growing_recording(&sessions.join("s1"), "audio.mp3", b"halb geschrieben");

        assert_eq!(
            ensure_with(&mirror, &sessions, "s1", &RecorderView::Unavailable, false).unwrap(),
            AudioOutcome::StillBeingWritten
        );
        assert!(!mirror.join("audio.mp3").exists());
    }

    #[test]
    fn a_session_without_a_recording_is_not_an_error() {
        let (_temp, sessions, mirror) = roots();
        std::fs::create_dir_all(sessions.join("s1")).unwrap();

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::NoSource
        );
    }

    // Sessions filed into a folder are not at `sessions/<id>`; a lookup that
    // only tries the direct path would report "no recording" for them.
    #[test]
    fn a_session_filed_into_a_folder_is_still_found() {
        let (_temp, sessions, mirror) = roots();
        write_recording(&sessions.join("Kunden").join("s1"), "audio.mp3", b"ton");

        assert_eq!(
            ensure(&mirror, &sessions, "s1", &RecorderView::Unavailable).unwrap(),
            AudioOutcome::Linked
        );
    }
}
