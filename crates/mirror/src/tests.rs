use std::path::Path;

use anlg_db_core::Db;
use sqlx::SqlitePool;

use super::*;

async fn test_db() -> Db {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_app::prepare_schema(&db).await.unwrap();
    db
}

async fn insert_session(pool: &SqlitePool, id: &str, title: &str, started_at: &str) {
    sqlx::query(
        "INSERT INTO sessions (id, title, started_at, updated_at, created_at)
         VALUES (?, ?, ?, '2026-05-05T08:00:00Z', '2026-05-05T08:00:00Z')",
    )
    .bind(id)
    .bind(title)
    .bind(started_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_summary(pool: &SqlitePool, id: &str, session_id: &str, text: &str) {
    let body = serde_json::json!({
        "type": "doc",
        "content": [{"type": "paragraph", "content": [{"type": "text", "text": text}]}]
    })
    .to_string();

    sqlx::query(
        "INSERT INTO session_documents
         (id, session_id, kind, title, body_format, body, updated_at, created_at)
         VALUES (?, ?, 'summary', 'Summary', 'prosemirror_json', ?,
                 '2026-05-05T08:00:00Z', '2026-05-05T08:00:00Z')",
    )
    .bind(id)
    .bind(session_id)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_transcript(pool: &SqlitePool, id: &str, session_id: &str) {
    let words = serde_json::json!([
        {"id": "w1", "text": " Moin", "start_ms": 0, "end_ms": 500, "channel": 0},
        {"id": "w2", "text": " zusammen.", "start_ms": 500, "end_ms": 1000, "channel": 0},
        {"id": "w3", "text": " Moin.", "start_ms": 61000, "end_ms": 61500, "channel": 1}
    ])
    .to_string();

    sqlx::query(
        "INSERT INTO transcripts
         (id, session_id, source, provider, model, started_at_ms, words_json,
          speaker_hints_json, metadata_json, updated_at, created_at)
         VALUES (?, ?, 'batch_transcription', 'soniqo', 'soniqo-parakeet-batch', 0, ?,
                 '[]', '{}', '2026-05-05T08:00:00Z', '2026-05-05T08:00:00Z')",
    )
    .bind(id)
    .bind(session_id)
    .bind(words)
    .execute(pool)
    .await
    .unwrap();
}

fn paths(base: &Path) -> MirrorPaths {
    MirrorPaths::for_app_data_dir(base)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("missing {}", path.display()))
}

fn only_mirror_dir(root: &Path) -> std::path::PathBuf {
    let mut dirs: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    assert_eq!(dirs.len(), 1, "expected exactly one mirror folder");
    dirs.pop().unwrap()
}

// The app's background watcher only wakes on these table names. A typo, or a
// table renamed by an upstream migration, would make it wake on nothing and the
// mirror would silently stop following the database -- so the names are checked
// against the real schema, not against a second copy of the same list.
#[tokio::test]
async fn every_rendered_table_exists_in_the_schema() {
    let db = test_db().await;
    for table in RENDERED_TABLES {
        let found: Option<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
                .bind(table)
                .fetch_optional(db.pool())
                .await
                .unwrap();
        assert_eq!(found.as_deref(), Some(table), "no such table: {table}");
    }
}

#[tokio::test]
async fn a_session_is_mirrored_into_three_readable_files() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());

    insert_session(db.pool(), "s1", "Jour Fixe Grün", "2026-05-05T08:00:00Z").await;
    insert_summary(db.pool(), "d1", "s1", "Wir bauen den Spiegel.").await;
    insert_transcript(db.pool(), "t1", "s1").await;

    let outcome = sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();
    assert_eq!(outcome, SyncOutcome::Written);

    let dir = only_mirror_dir(&paths.root);
    assert!(
        dir.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("2026-05-05_jour-fixe-gruen_"),
        "unreadable folder name: {}",
        dir.display()
    );

    let transcript = read(&dir.join(TRANSCRIPT_FILE));
    assert!(transcript.contains("Moin zusammen."), "{transcript}");
    assert!(transcript.contains("[00:00:00]"), "{transcript}");
    // The second channel starts a minute in; a timecode proves the mirror keeps
    // the clock, not just the words.
    assert!(transcript.contains("[00:01:01]"), "{transcript}");

    let summary = read(&dir.join(SUMMARY_FILE));
    assert!(summary.contains("## Summary"), "{summary}");
    assert!(summary.contains("Wir bauen den Spiegel."), "{summary}");

    let meta: serde_json::Value = serde_json::from_str(&read(&dir.join(META_FILE))).unwrap();
    assert_eq!(meta["session_id"], "s1");
    assert_eq!(meta["title"], "Jour Fixe Grün");
    assert_eq!(meta["transcripts"][0]["model"], "soniqo-parakeet-batch");
    assert_eq!(meta["audio"]["session_folder"], "sessions/s1");
    assert!(meta["source_fingerprint"].as_str().unwrap().len() == 64);
}

// Found on real data: the demo session's only participant has no name, and
// handing that empty name to the labeler produced `**[00:00:00] **` -- a
// heading that reads as a broken file rather than as an unnamed speaker.
#[tokio::test]
async fn a_nameless_participant_does_not_produce_an_empty_speaker_label() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());

    insert_session(db.pool(), "s1", "Demo", "2026-05-05T08:00:00Z").await;
    insert_transcript(db.pool(), "t1", "s1").await;
    sqlx::query("INSERT INTO humans (id, name) VALUES ('h1', '')")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO session_participants (id, session_id, human_id, source)
         VALUES ('p1', 's1', 'h1', 'manual')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    // The transcript owner is what makes a channel "self"; without it the
    // fallback path under test is never reached.
    sqlx::query("UPDATE transcripts SET owner_user_id = 'h1' WHERE id = 't1'")
        .execute(db.pool())
        .await
        .unwrap();

    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();
    let transcript = read(&only_mirror_dir(&paths.root).join(TRANSCRIPT_FILE));

    assert!(
        !transcript.contains("] **"),
        "empty speaker label:\n{transcript}"
    );
    assert!(transcript.contains("[00:00:00] You"), "{transcript}");
}

#[tokio::test]
async fn an_unchanged_session_is_not_rewritten() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;

    assert_eq!(
        sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
            .await
            .unwrap(),
        SyncOutcome::Written
    );
    assert_eq!(
        sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
            .await
            .unwrap(),
        SyncOutcome::UpToDate
    );
    // ...but a forced run still rewrites, which is what `--force` is for.
    assert_eq!(
        sync_session(db.pool(), &paths, "s1", true, &RecorderView::Unavailable)
            .await
            .unwrap(),
        SyncOutcome::Written
    );
}

#[tokio::test]
async fn a_changed_summary_reaches_the_mirror() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    insert_summary(db.pool(), "d1", "s1", "Erste Fassung.").await;
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    sqlx::query(
        "UPDATE session_documents
         SET body = ?, updated_at = '2026-05-05T09:00:00Z' WHERE id = 'd1'",
    )
    .bind(
        serde_json::json!({
            "type": "doc",
            "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Zweite Fassung."}]}]
        })
        .to_string(),
    )
    .execute(db.pool())
    .await
    .unwrap();

    assert_eq!(
        sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
            .await
            .unwrap(),
        SyncOutcome::Written
    );
    let summary = read(&only_mirror_dir(&paths.root).join(SUMMARY_FILE));
    assert!(summary.contains("Zweite Fassung."), "{summary}");
    assert!(!summary.contains("Erste Fassung."), "{summary}");
}

#[tokio::test]
async fn a_renamed_meeting_moves_its_folder_instead_of_growing_a_twin() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Alter Titel", "2026-05-05T08:00:00Z").await;
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    sqlx::query("UPDATE sessions SET title = 'Neuer Titel', updated_at = '2026-05-05T09:00:00Z' WHERE id = 's1'")
        .execute(db.pool())
        .await
        .unwrap();
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    let dir = only_mirror_dir(&paths.root);
    assert!(
        dir.file_name()
            .unwrap()
            .to_string_lossy()
            .contains("neuer-titel"),
        "{}",
        dir.display()
    );
}

#[tokio::test]
async fn the_audit_reports_a_missing_mirror() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;

    let report = audit(db.pool(), &paths).await.unwrap();
    assert!(report.has_gaps());
    assert_eq!(report.sessions[0].state, MirrorState::Missing);
}

#[tokio::test]
async fn the_audit_reports_a_stale_mirror() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    assert!(!audit(db.pool(), &paths).await.unwrap().has_gaps());

    sqlx::query("UPDATE sessions SET title = 'Standup (neu)', updated_at = '2026-05-05T10:00:00Z' WHERE id = 's1'")
        .execute(db.pool())
        .await
        .unwrap();

    let report = audit(db.pool(), &paths).await.unwrap();
    assert_eq!(report.sessions[0].state, MirrorState::Stale);
    assert!(report.has_gaps());
}

// A folder that exists but lost a file must not read as "current" -- that is
// exactly the two-truths situation the audit exists to prevent.
#[tokio::test]
async fn a_mirror_missing_a_file_is_reported_and_repaired() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    let transcript = only_mirror_dir(&paths.root).join(TRANSCRIPT_FILE);
    std::fs::remove_file(&transcript).unwrap();

    let report = audit(db.pool(), &paths).await.unwrap();
    assert!(matches!(
        report.sessions[0].state,
        MirrorState::Unreadable(_)
    ));

    // And the next unforced sync puts it back rather than declaring victory.
    assert_eq!(
        sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
            .await
            .unwrap(),
        SyncOutcome::Written
    );
    assert!(transcript.is_file());
}

#[tokio::test]
async fn a_corrupt_meta_file_is_reported_not_swallowed() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    std::fs::write(only_mirror_dir(&paths.root).join(META_FILE), "{not json").unwrap();

    let report = audit(db.pool(), &paths).await.unwrap();
    // The folder can no longer be attributed to s1, so s1 reads as missing and
    // the folder itself surfaces as an orphan. Either way it is reported.
    assert!(report.has_gaps());
    assert_eq!(report.orphans.len(), 1);
}

#[tokio::test]
async fn a_mirror_whose_session_is_gone_surfaces_as_an_orphan() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    sqlx::query("UPDATE sessions SET deleted_at = '2026-05-06T00:00:00Z' WHERE id = 's1'")
        .execute(db.pool())
        .await
        .unwrap();

    let report = audit(db.pool(), &paths).await.unwrap();
    assert_eq!(report.orphans.len(), 1);
    assert!(report.sessions.is_empty());
    // Reported, never deleted.
    assert!(only_mirror_dir(&paths.root).is_dir());
}

#[tokio::test]
async fn sync_all_covers_every_session_and_is_idempotent() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Eins", "2026-05-05T08:00:00Z").await;
    insert_session(db.pool(), "s2", "Zwei", "2026-05-06T08:00:00Z").await;

    let first = sync_all(db.pool(), &paths, false, &RecorderView::Unavailable)
        .await
        .unwrap();
    assert_eq!(first.written.len(), 2);
    assert_eq!(first.up_to_date, 0);

    let second = sync_all(db.pool(), &paths, false, &RecorderView::Unavailable)
        .await
        .unwrap();
    assert!(second.written.is_empty());
    assert_eq!(second.up_to_date, 2);
    assert!(!audit(db.pool(), &paths).await.unwrap().has_gaps());
}

#[tokio::test]
async fn a_session_that_no_longer_exists_is_not_mirrored() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());

    assert_eq!(
        sync_session(db.pool(), &paths, "nope", false, &RecorderView::Unavailable)
            .await
            .unwrap(),
        SyncOutcome::SessionGone
    );
    assert!(!paths.root.join("nope").exists());
}

// The mirror is a projection, never a source. If anything in this crate ever
// starts writing to the database this fails.
#[tokio::test]
async fn mirroring_never_writes_to_the_database() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    insert_summary(db.pool(), "d1", "s1", "Text.").await;
    insert_transcript(db.pool(), "t1", "s1").await;

    let before: (String, String, String) = sqlx::query_as(
        "SELECT (SELECT group_concat(id || updated_at) FROM sessions),
                (SELECT group_concat(id || updated_at) FROM session_documents),
                (SELECT group_concat(id || updated_at) FROM transcripts)",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();

    sync_all(db.pool(), &paths, true, &RecorderView::Unavailable)
        .await
        .unwrap();
    audit(db.pool(), &paths).await.unwrap();

    let after: (String, String, String) = sqlx::query_as(
        "SELECT (SELECT group_concat(id || updated_at) FROM sessions),
                (SELECT group_concat(id || updated_at) FROM session_documents),
                (SELECT group_concat(id || updated_at) FROM transcripts)",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();

    assert_eq!(before, after);
}

// --- F16: one folder per meeting -------------------------------------------

/// A recording that is over. Dated back, because the mirror deliberately skips
/// a recording that was written a moment ago -- that is what a meeting being
/// recorded right now looks like on disk.
fn write_recording(sessions_root: &Path, session_id: &str, name: &str, bytes: &[u8]) {
    let path = write_growing_recording(sessions_root, session_id, name, bytes);
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(600))
        .unwrap();
}

/// A recording still being written into: the recorder flushes once a second, so
/// its modification time is always now.
fn write_growing_recording(
    sessions_root: &Path,
    session_id: &str,
    name: &str,
    bytes: &[u8],
) -> std::path::PathBuf {
    let dir = sessions_root.join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

// "Delete recording" has to mean it. The hard link keeps the bytes alive under
// the other name, and on a copy there is a second file outright -- so until F16b
// the app said "deleted" while the recording sat in the meeting folder, and with
// a synced root it had already been uploaded.
#[tokio::test]
async fn deleting_the_recording_on_purpose_empties_both_places() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    write_recording(&paths.sessions_root, "s1", "audio.mp3", b"tondaten");
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    assert!(forget_audio(&paths, "s1").unwrap());

    assert!(
        !only_mirror_dir(&paths.root).join("audio.mp3").exists(),
        "the recording is still in the folder the person looks at"
    );
    // The readable files themselves are none of this call's business.
    assert!(only_mirror_dir(&paths.root).join(SUMMARY_FILE).is_file());
}

// The other half of the same decision, and the one that would be expensive to
// get wrong: a mirror pass that finds no recording in the machine room leaves
// the human folder alone, because that copy can be the last one in existence.
//
// Note what this does NOT say. Since F16b the retention deadline deletes from
// both places -- it goes through `forget_audio`, because the person set the
// deadline. This test is about the mirror's own housekeeping, which is a
// different caller. The name says "a vanished source", not "retention", because
// the old name taught the wrong product behaviour.
#[tokio::test]
async fn a_vanished_source_never_takes_the_human_copy_with_it() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    write_recording(&paths.sessions_root, "s1", "audio.mp3", b"tondaten");
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    // What retention does: the machine room, and only the machine room.
    std::fs::remove_file(paths.sessions_root.join("s1").join("audio.mp3")).unwrap();
    sync_session(db.pool(), &paths, "s1", true, &RecorderView::Unavailable)
        .await
        .unwrap();

    assert_eq!(
        std::fs::read(only_mirror_dir(&paths.root).join("audio.mp3")).unwrap(),
        b"tondaten",
        "retention took the last copy of the recording with it"
    );
}

// A deleted meeting must not keep a folder with its transcript, its summary and
// its recording in it. The audit reports such a folder as an orphan and
// deliberately never removes it -- right for a meeting that merely vanished from
// the database, wrong for one the person deleted.
#[tokio::test]
async fn deleting_a_meeting_takes_its_readable_folder_with_it() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    insert_summary(db.pool(), "d1", "s1", "Vertraulich.").await;
    write_recording(&paths.sessions_root, "s1", "audio.mp3", b"tondaten");
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();
    let folder = only_mirror_dir(&paths.root);

    assert!(forget_session(&paths, "s1").unwrap());

    assert!(
        !folder.exists(),
        "the deleted meeting kept its transcript, summary and recording"
    );
    // Saying so twice is not an error: the undo window and the finalizer can
    // both reach this.
    assert!(!forget_session(&paths, "s1").unwrap());
}

// A rename that cannot happen must not leave a second folder for the same
// meeting behind. The index keys on the session id, so of two such folders it
// keeps one and the other belongs to nothing: not the live folder, and not an
// orphan either, because its meeting exists. Nobody would ever see it again --
// with the transcript, the summary and the recording still in it.
#[tokio::test]
async fn a_rename_that_cannot_happen_does_not_leave_a_second_folder_behind() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();
    let first = only_mirror_dir(&paths.root);

    // Rename the meeting, and put something at the name its folder wants.
    sqlx::query(
        "UPDATE sessions SET title = 'Jour Fixe', updated_at = '2026-05-05T09:00:00Z'
         WHERE id = 's1'",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let blocked = paths.root.join("2026-05-05_jour-fixe_s1");
    std::fs::create_dir_all(&blocked).unwrap();

    let outcome = sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable).await;

    assert!(outcome.is_err(), "the blocked rename passed as a success");
    assert!(
        !blocked.join(SUMMARY_FILE).exists(),
        "the meeting was written to the new name anyway, so it now has two folders"
    );
    assert!(
        first.join(SUMMARY_FILE).is_file(),
        "the folder that could not be renamed lost its contents"
    );

    // And once the obstacle is gone the next pass finishes the rename.
    std::fs::remove_dir(&blocked).unwrap();
    assert_eq!(
        sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
            .await
            .unwrap(),
        SyncOutcome::Written
    );
    assert_eq!(only_mirror_dir(&paths.root), blocked);
}

// Two folders claiming one meeting can still arrive from outside -- a restored
// backup, a copy made by hand, a sync client resurrecting a folder. The index
// can only own one of them, so the other has to be reported, or it is a full
// copy of a meeting that no cleanup can see.
#[tokio::test]
async fn a_second_folder_for_the_same_meeting_is_reported_not_ignored() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    let live = only_mirror_dir(&paths.root);
    let twin = paths.root.join("2026-05-05_standup-alt_s1");
    std::fs::create_dir_all(&twin).unwrap();
    for name in [TRANSCRIPT_FILE, SUMMARY_FILE, META_FILE] {
        std::fs::copy(live.join(name), twin.join(name)).unwrap();
    }

    let report = audit(db.pool(), &paths).await.unwrap();

    assert!(
        report
            .orphans
            .iter()
            .any(|name| name == "2026-05-05_standup-alt_s1"),
        "the second folder for this meeting is invisible to every cleanup: {:?}",
        report.orphans
    );
}

// One folder the app cannot write into -- Nextcloud briefly gone, a permission
// lost, a full disk -- must cost that one meeting and no other. Aborting the
// pass at the first error means every meeting behind it silently stops being
// mirrored, and the only trace is a log line.
#[cfg(unix)]
#[tokio::test]
async fn one_unwritable_folder_does_not_stop_the_meetings_behind_it() {
    use std::os::unix::fs::PermissionsExt as _;

    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    // Sessions are walked in id order, so the broken one is walked first.
    insert_session(db.pool(), "s1", "Kaputt", "2026-05-05T08:00:00Z").await;
    insert_session(db.pool(), "s2", "Zwei", "2026-05-06T08:00:00Z").await;
    insert_session(db.pool(), "s3", "Drei", "2026-05-07T08:00:00Z").await;
    assert_eq!(
        sync_all(db.pool(), &paths, false, &RecorderView::Unavailable)
            .await
            .unwrap()
            .written
            .len(),
        3
    );

    for id in ["s1", "s2", "s3"] {
        write_recording(&paths.sessions_root, id, "audio.mp3", b"tondaten");
    }
    let broken = paths.root.join("2026-05-05_kaputt_s1");
    assert!(
        broken.is_dir(),
        "the folder under test is not where it was expected"
    );
    std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o500)).unwrap();

    let report = sync_all(db.pool(), &paths, false, &RecorderView::Unavailable).await;
    std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o755)).unwrap();
    let report = report.expect("one unwritable folder took the whole pass down");

    assert!(
        paths
            .root
            .join("2026-05-06_zwei_s2")
            .join("audio.mp3")
            .is_file()
            && paths
                .root
                .join("2026-05-07_drei_s3")
                .join("audio.mp3")
                .is_file(),
        "the meetings behind the broken one were never mirrored"
    );
    assert_eq!(
        report
            .failed
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        vec!["s1"],
        "the broken meeting was swallowed instead of reported"
    );
}

// Falsifier 1. A finished meeting whose readable folder has no recording in it
// is exactly the two-places problem F16 exists to end.
#[tokio::test]
async fn a_finished_meeting_carries_its_recording_in_the_human_folder() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    write_recording(&paths.sessions_root, "s1", "audio.mp3", b"tondaten");

    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    let recording = only_mirror_dir(&paths.root).join("audio.mp3");
    assert_eq!(
        std::fs::read(&recording).unwrap(),
        b"tondaten",
        "the readable folder has no playable recording"
    );
}

// The whole point of the hard link: two names, one set of bytes. A copy would
// double the disk footprint of every meeting.
#[tokio::test]
async fn the_recording_is_shared_with_the_machine_room_not_duplicated() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    write_recording(&paths.sessions_root, "s1", "audio.mp3", b"tondaten");
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    let source = paths.sessions_root.join("s1").join("audio.mp3");
    let mirrored = only_mirror_dir(&paths.root).join("audio.mp3");

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            std::fs::metadata(&source).unwrap().ino(),
            std::fs::metadata(&mirrored).unwrap().ino()
        );
    }

    // The same claim without an inode: one set of bytes means a write through
    // one name is visible under the other. Rewriting in place keeps the file
    // itself, so a second copy would still read the old take. This is the half
    // that can go red on any platform -- the length assertion this test used to
    // end on could not.
    std::fs::write(&source, b"neue aufnahme").unwrap();
    assert_eq!(
        std::fs::read(&mirrored).unwrap(),
        b"neue aufnahme",
        "the human folder holds a second copy, not the same bytes"
    );
}

// The link has to survive a pass that changes nothing in the database. Until
// F16 the fingerprint short-circuit returned before touching the folder, so a
// recording finished after the first mirror pass would never have arrived.
#[tokio::test]
async fn a_recording_that_appears_after_the_first_pass_still_arrives() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;

    assert_eq!(
        sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
            .await
            .unwrap(),
        SyncOutcome::Written
    );
    assert!(!only_mirror_dir(&paths.root).join("audio.mp3").exists());

    write_recording(&paths.sessions_root, "s1", "audio.mp3", b"spaeter");

    assert_eq!(
        sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
            .await
            .unwrap(),
        SyncOutcome::UpToDate
    );
    assert_eq!(
        std::fs::read(only_mirror_dir(&paths.root).join("audio.mp3")).unwrap(),
        b"spaeter"
    );
}

// F16d, through the whole pass rather than through `audio::ensure` alone: the
// text of a meeting in progress may travel (it is written from the database and
// stands on its own), the recording may not. A quiet file is not a finished
// meeting.
#[tokio::test]
async fn a_pass_writes_the_text_of_a_running_meeting_but_not_its_recording() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    // Settled for a full minute: every signal the file can give says "done".
    write_recording(&paths.sessions_root, "s1", "audio.wav", b"halbes-meeting");

    assert_eq!(
        sync_session(
            db.pool(),
            &paths,
            "s1",
            false,
            &RecorderView::holding(["s1"])
        )
        .await
        .unwrap(),
        SyncOutcome::Written
    );

    let dir = only_mirror_dir(&paths.root);
    assert!(dir.join(TRANSCRIPT_FILE).is_file());
    assert!(
        !dir.join("audio.wav").exists(),
        "the recording of a meeting still being recorded reached the readable folder"
    );

    // And it arrives on the pass after the recorder lets go.
    sync_session(db.pool(), &paths, "s1", true, &RecorderView::idle())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(only_mirror_dir(&paths.root).join("audio.wav")).unwrap(),
        b"halbes-meeting"
    );
}

// Falsifier 4, in its still-true form: whenever the machine-room recording is
// gone for a reason nobody routed through `forget_audio`, the readable folder is
// the only place the meeting still sounds like anything -- and a pass must not
// tidy it away. (The retention deadline is not that case; it deletes from both
// places on purpose, see `forget_audio`.)
#[tokio::test]
async fn a_pass_over_a_missing_recording_leaves_the_human_one_playable() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;
    write_recording(&paths.sessions_root, "s1", "audio.mp3", b"tondaten");
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    std::fs::remove_file(paths.sessions_root.join("s1").join("audio.mp3")).unwrap();
    sync_session(db.pool(), &paths, "s1", true, &RecorderView::Unavailable)
        .await
        .unwrap();

    assert_eq!(
        std::fs::read(only_mirror_dir(&paths.root).join("audio.mp3")).unwrap(),
        b"tondaten"
    );
}

// A rename moves the folder, and the recording has to ride along -- otherwise
// renaming a meeting would strand its audio under the old name.
#[tokio::test]
async fn renaming_a_meeting_takes_the_recording_along() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Alter Titel", "2026-05-05T08:00:00Z").await;
    write_recording(&paths.sessions_root, "s1", "audio.mp3", b"tondaten");
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    sqlx::query("UPDATE sessions SET title = 'Neuer Titel', updated_at = '2026-05-05T09:00:00Z' WHERE id = 's1'")
        .execute(db.pool())
        .await
        .unwrap();
    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    let dir = only_mirror_dir(&paths.root);
    assert!(dir.to_string_lossy().contains("neuer-titel"), "{dir:?}");
    assert_eq!(std::fs::read(dir.join("audio.mp3")).unwrap(), b"tondaten");
}

// The door every "Show in Finder" goes through.
#[tokio::test]
async fn the_folder_lookup_names_the_readable_place_and_nothing_before_it() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;

    assert_eq!(session_folder(&paths, "s1").unwrap(), None);

    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();
    let found = session_folder(&paths, "s1").unwrap().unwrap();

    assert_eq!(found, only_mirror_dir(&paths.root));
    assert!(
        !found.starts_with(&paths.sessions_root),
        "the lookup points into the machine room: {}",
        found.display()
    );
}

// A chosen root has to be the root that is written; a mirror that keeps writing
// to the default place would leave the chosen folder empty forever.
#[tokio::test]
async fn a_chosen_root_is_where_the_folders_are_written() {
    let db = test_db().await;
    let temp = tempfile::tempdir().unwrap();
    let chosen = temp.path().join("Nextcloud").join("Mitschnitt");
    let paths = MirrorPaths::with_root(chosen.clone(), temp.path());
    insert_session(db.pool(), "s1", "Standup", "2026-05-05T08:00:00Z").await;

    sync_session(db.pool(), &paths, "s1", false, &RecorderView::Unavailable)
        .await
        .unwrap();

    assert!(only_mirror_dir(&chosen).join(SUMMARY_FILE).is_file());
    assert!(
        !MirrorPaths::default_root(temp.path()).exists(),
        "the default place was written to as well"
    );
}

#[tokio::test]
async fn an_unset_root_setting_means_the_default_place() {
    let db = test_db().await;
    assert_eq!(read_root_setting(db.pool()).await.unwrap(), None);

    sqlx::query("INSERT INTO app_settings (id, value_json) VALUES (?, ?)")
        .bind(ROOT_SETTING_KEY)
        .bind("\"\"")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(read_root_setting(db.pool()).await.unwrap(), None);
}

#[tokio::test]
async fn a_stored_root_setting_is_read_as_a_path() {
    let db = test_db().await;
    sqlx::query("INSERT INTO app_settings (id, value_json) VALUES (?, ?)")
        .bind(ROOT_SETTING_KEY)
        .bind("\"/Users/mads/Nextcloud/Mitschnitt\"")
        .execute(db.pool())
        .await
        .unwrap();

    assert_eq!(
        read_root_setting(db.pool()).await.unwrap(),
        Some(std::path::PathBuf::from(
            "/Users/mads/Nextcloud/Mitschnitt"
        ))
    );
}

// A value that is not a string is not a path. Pointing the mirror at `true`
// would create a folder called "true" next to the meetings.
#[tokio::test]
async fn a_root_setting_that_is_not_a_path_degrades_to_the_default() {
    let db = test_db().await;
    sqlx::query("INSERT INTO app_settings (id, value_json) VALUES (?, ?)")
        .bind(ROOT_SETTING_KEY)
        .bind("true")
        .execute(db.pool())
        .await
        .unwrap();

    assert_eq!(read_root_setting(db.pool()).await.unwrap(), None);
}
