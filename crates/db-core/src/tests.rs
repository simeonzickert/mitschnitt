use super::*;

#[tokio::test]
async fn connect_local_plain_creates_parent_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("nonexistent").join("nested").join("app.db");
    let db = Db::connect_local_plain(&db_path).await.unwrap();
    assert!(db_path.exists());
    drop(db);
}

#[tokio::test]
async fn connect_local_read_write_does_not_create_missing_database() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("missing.db");

    let result = Db::connect_local_read_write(&db_path).await;

    assert!(result.is_err());
    assert!(!db_path.exists());
}

#[tokio::test]
async fn connect_local_read_write_accepts_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("app.db");
    let created = Db::connect_local_plain(&db_path).await.unwrap();
    sqlx::query("CREATE TABLE records (id TEXT PRIMARY KEY NOT NULL)")
        .execute(created.pool())
        .await
        .unwrap();
    created.pool().close().await;

    let writable = Db::connect_local_read_write(&db_path).await.unwrap();
    sqlx::query("INSERT INTO records (id) VALUES ('written')")
        .execute(writable.pool())
        .await
        .unwrap();
    let rows: Vec<String> = sqlx::query_scalar("SELECT id FROM records")
        .fetch_all(writable.pool())
        .await
        .unwrap();
    writable.pool().close().await;

    assert_eq!(rows, vec!["written"]);
}

#[tokio::test]
async fn connect_local_read_only_does_not_create_missing_database() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("missing.db");

    let result = Db::connect_local_read_only(&db_path).await;

    assert!(result.is_err());
    assert!(!db_path.exists());
}

#[tokio::test]
async fn connect_local_read_only_rejects_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("app.db");
    let writable = Db::connect_local_plain(&db_path).await.unwrap();
    sqlx::query("CREATE TABLE records (id TEXT PRIMARY KEY NOT NULL)")
        .execute(writable.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO records (id) VALUES ('existing')")
        .execute(writable.pool())
        .await
        .unwrap();
    writable.pool().close().await;

    let read_only = Db::connect_local_read_only(&db_path).await.unwrap();
    let rows: Vec<String> = sqlx::query_scalar("SELECT id FROM records ORDER BY id")
        .fetch_all(read_only.pool())
        .await
        .unwrap();
    let query_only: i64 = sqlx::query_scalar("PRAGMA query_only")
        .fetch_one(read_only.pool())
        .await
        .unwrap();
    let write_result = sqlx::query("INSERT INTO records (id) VALUES ('rejected')")
        .execute(read_only.pool())
        .await;

    assert_eq!(rows, vec!["existing"]);
    assert_eq!(query_only, 1);
    assert!(write_result.is_err());
}

#[tokio::test]
async fn open_applies_requested_pragmas() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("app.db");

    let db = Db::open(DbOpenOptions {
        storage: DbStorage::Local(&db_path),
        journal_mode_wal: true,
        foreign_keys: true,
        max_connections: Some(1),
    })
    .await
    .unwrap();

    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(db.pool())
        .await
        .unwrap();
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(db.pool())
        .await
        .unwrap();
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(db.pool())
        .await
        .unwrap();

    assert_eq!(foreign_keys, 1);
    assert_eq!(journal_mode.to_lowercase(), "wal");
    assert_eq!(busy_timeout, SQLITE_BUSY_TIMEOUT.as_millis() as i64);
}

#[tokio::test]
async fn emits_table_changes_for_local_writes() {
    let db = Db::connect_memory_plain().await.unwrap();
    let notifier = db.change_notifier();
    sqlx::query("CREATE TABLE test_events (id TEXT PRIMARY KEY NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();

    let mut changes = notifier.subscribe();
    let before = notifier.current_seq();

    sqlx::query("INSERT INTO test_events (id) VALUES ('a')")
        .execute(db.pool())
        .await
        .unwrap();

    let change = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(change.table, "test_events");
    assert_eq!(change.kind, anlg_db_change::TableChangeKind::Insert);
    assert!(change.seq > before);
    assert_eq!(notifier.current_seq(), change.seq);
    assert_eq!(notifier.latest_table_seq("test_events"), Some(change.seq));
}

#[tokio::test]
async fn emits_table_changes_only_after_commit() {
    let db = Db::connect_memory_plain().await.unwrap();
    let notifier = db.change_notifier();
    sqlx::query("CREATE TABLE test_events (id TEXT PRIMARY KEY NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();

    let mut changes = notifier.subscribe();
    let mut tx = db.pool().begin().await.unwrap();

    sqlx::query("INSERT INTO test_events (id) VALUES ('a')")
        .execute(&mut *tx)
        .await
        .unwrap();

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), changes.recv())
            .await
            .is_err()
    );

    tx.commit().await.unwrap();

    let change = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(change.table, "test_events");
    assert_eq!(change.kind, anlg_db_change::TableChangeKind::Insert);
    assert_eq!(notifier.latest_table_seq("test_events"), Some(change.seq));
}

#[tokio::test]
async fn rollback_clears_pending_table_changes() {
    let db = Db::connect_memory_plain().await.unwrap();
    let notifier = db.change_notifier();
    sqlx::query("CREATE TABLE test_events (id TEXT PRIMARY KEY NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();

    let mut changes = notifier.subscribe();
    let mut tx = db.pool().begin().await.unwrap();

    sqlx::query("INSERT INTO test_events (id) VALUES ('a')")
        .execute(&mut *tx)
        .await
        .unwrap();

    tx.rollback().await.unwrap();

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), changes.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn coalesces_multiple_writes_in_a_transaction() {
    let db = Db::connect_memory_plain().await.unwrap();
    let notifier = db.change_notifier();
    sqlx::query("CREATE TABLE test_events (id TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();

    let mut changes = notifier.subscribe();
    let mut tx = db.pool().begin().await.unwrap();

    sqlx::query("INSERT INTO test_events (id, value) VALUES ('a', 'before')")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE test_events SET value = 'after' WHERE id = 'a'")
        .execute(&mut *tx)
        .await
        .unwrap();

    tx.commit().await.unwrap();

    let change = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(change.table, "test_events");
    assert_eq!(change.kind, anlg_db_change::TableChangeKind::Update);
    assert_eq!(notifier.latest_table_seq("test_events"), Some(change.seq));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), changes.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn emits_update_and_delete_table_changes() {
    let db = Db::connect_memory_plain().await.unwrap();
    let notifier = db.change_notifier();
    sqlx::query("CREATE TABLE test_events (id TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO test_events (id, value) VALUES ('a', 'before')")
        .execute(db.pool())
        .await
        .unwrap();

    let mut changes = notifier.subscribe();

    sqlx::query("UPDATE test_events SET value = 'after' WHERE id = 'a'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM test_events WHERE id = 'a'")
        .execute(db.pool())
        .await
        .unwrap();

    let update = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();
    let delete = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(update.table, "test_events");
    assert_eq!(update.kind, anlg_db_change::TableChangeKind::Update);
    assert_eq!(delete.table, "test_events");
    assert_eq!(delete.kind, anlg_db_change::TableChangeKind::Delete);
    assert!(delete.seq > update.seq);
    assert_eq!(notifier.latest_table_seq("test_events"), Some(delete.seq));
}

#[tokio::test]
async fn emits_table_changes_across_multiple_connections() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("app.db");

    let db = Db::open(DbOpenOptions {
        storage: DbStorage::Local(&db_path),
        journal_mode_wal: true,
        foreign_keys: true,
        max_connections: Some(4),
    })
    .await
    .unwrap();
    let notifier = db.change_notifier();
    sqlx::query("CREATE TABLE multi_conn_events (id TEXT PRIMARY KEY NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();

    let mut changes = notifier.subscribe();
    let mut conn_a = db.pool().acquire().await.unwrap();
    let mut conn_b = db.pool().acquire().await.unwrap();

    sqlx::query("INSERT INTO multi_conn_events (id) VALUES ('a')")
        .execute(&mut *conn_a)
        .await
        .unwrap();
    sqlx::query("INSERT INTO multi_conn_events (id) VALUES ('b')")
        .execute(&mut *conn_b)
        .await
        .unwrap();

    let first = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();
    let second = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(first.table, "multi_conn_events");
    assert_eq!(second.table, "multi_conn_events");
    assert_ne!(first.seq, second.seq);
}

#[tokio::test]
async fn tracks_monotonic_change_sequences_per_table() {
    let db = Db::connect_memory_plain().await.unwrap();
    let notifier = db.change_notifier();
    sqlx::query("CREATE TABLE test_events (id TEXT PRIMARY KEY NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("CREATE TABLE other_events (id TEXT PRIMARY KEY NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();

    let start = notifier.current_seq();
    let mut changes = notifier.subscribe();

    sqlx::query("INSERT INTO test_events (id) VALUES ('a')")
        .execute(db.pool())
        .await
        .unwrap();
    let first = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();

    sqlx::query("INSERT INTO test_events (id) VALUES ('b')")
        .execute(db.pool())
        .await
        .unwrap();
    let second = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();

    sqlx::query("INSERT INTO other_events (id) VALUES ('c')")
        .execute(db.pool())
        .await
        .unwrap();
    let third = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();

    assert!(first.seq > start);
    assert!(second.seq > first.seq);
    assert!(third.seq > second.seq);
    assert_eq!(notifier.current_seq(), third.seq);
    assert_eq!(notifier.latest_table_seq("test_events"), Some(second.seq));
    assert_eq!(notifier.latest_table_seq("other_events"), Some(third.seq));
    assert_eq!(notifier.latest_table_seq("missing_events"), None);
}

#[tokio::test]
async fn notifier_survives_db_drop() {
    let db = Db::connect_memory_plain().await.unwrap();
    let notifier = db.change_notifier().clone();
    sqlx::query("CREATE TABLE retained_events (id TEXT PRIMARY KEY NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();

    let pool = db.pool().clone();
    let mut changes = notifier.subscribe();
    drop(db);

    sqlx::query("INSERT INTO retained_events (id) VALUES ('a')")
        .execute(&pool)
        .await
        .unwrap();

    let change = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(change.table, "retained_events");
    assert_eq!(change.kind, anlg_db_change::TableChangeKind::Insert);
    assert_eq!(
        notifier.latest_table_seq("retained_events"),
        Some(change.seq)
    );
}

#[tokio::test]
async fn open_memory_clamps_max_connections_to_one() {
    let db = Db::open(DbOpenOptions {
        storage: DbStorage::Memory,
        journal_mode_wal: false,
        foreign_keys: true,
        max_connections: Some(4),
    })
    .await
    .unwrap();

    let _conn = db.pool().acquire().await.unwrap();

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), db.pool().acquire())
            .await
            .is_err()
    );
}

// `checkpoint_and_close` is the mechanism the Mitschnitt fork's vault move
// leans on to make a plain file copy of app.db safe while the app is
// running: fold the WAL into the main file, then close the pool so nothing
// can write to either afterwards. These tests exercise that mechanism
// directly against a real WAL-mode database and a real file on disk, not a
// mock -- the two things that would make a moved database unusable are
// exactly what they check: leftover data still sitting in the `-wal`
// sidecar, and a connection that can still write after the function
// returns.

#[tokio::test]
async fn checkpoint_and_close_truncates_the_wal_file() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("app.db");
    let wal_path = tmp.path().join("app.db-wal");
    let db = Db::connect_local(&db_path).await.unwrap();

    sqlx::query("CREATE TABLE records (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();
    for value in ["a", "b", "c"] {
        sqlx::query("INSERT INTO records (value) VALUES (?)")
            .bind(value)
            .execute(db.pool())
            .await
            .unwrap();
    }

    // WAL mode writes go to the sidecar first; a plain file copy of
    // `db_path` taken right now would be missing every row just inserted.
    let wal_size_before_checkpoint = std::fs::metadata(&wal_path).unwrap().len();
    assert!(
        wal_size_before_checkpoint > 0,
        "the writes above should have landed in the WAL sidecar, not (yet) in app.db itself"
    );

    db.checkpoint_and_close().await.unwrap();

    // TRUNCATE folds everything back into app.db and empties the sidecar. In
    // this case closing the pool goes one step further still: SQLite removes
    // the now-empty `-wal`/`-shm` files entirely once the last connection to
    // the database closes, so "gone" is the stronger and equally acceptable
    // form of "no leftover data outside app.db" -- either way there is
    // nothing left for a plain copy of app.db to miss.
    match std::fs::metadata(&wal_path) {
        Ok(metadata) => assert_eq!(
            metadata.len(),
            0,
            "checkpoint(TRUNCATE) should have emptied the WAL sidecar"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("unexpected error reading the WAL sidecar: {error}"),
    }

    let reopened = Db::connect_local_plain(&db_path).await.unwrap();
    let values: Vec<String> = sqlx::query_scalar("SELECT value FROM records ORDER BY id")
        .fetch_all(reopened.pool())
        .await
        .unwrap();
    reopened.pool().close().await;

    assert_eq!(values, vec!["a", "b", "c"]);
}

#[tokio::test]
async fn checkpoint_and_close_leaves_the_pool_unusable() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("app.db");
    let db = Db::connect_local(&db_path).await.unwrap();

    db.checkpoint_and_close().await.unwrap();

    let acquired = db.pool().acquire().await;
    assert!(
        matches!(acquired, Err(sqlx::Error::PoolClosed)),
        "a connection acquired after checkpoint_and_close should be refused, \
         not silently allowed to write into a file that is about to be copied: {acquired:?}"
    );
}

// The mutation that matters here is not in this function at all: an earlier
// draft skipped the explicit `PRAGMA wal_checkpoint` entirely and closed the
// pool directly, on the theory that SQLite already checkpoints on its own
// when the last connection to a WAL database closes. That theory is true --
// `checkpoint_and_close_truncates_the_wal_file` above still passes without
// the explicit checkpoint, because closing was already enough in that test's
// uncontended case. What the plain `close()` alone does NOT give is this: if
// SQLite's own implicit checkpoint cannot get the lock it needs (a reader
// still has the old snapshot open), it just leaves the WAL as-is and closes
// quietly -- no error, nothing to check, and a caller who copies the file
// right after gets an incomplete snapshot with no indication anything was
// wrong. That silent failure is exactly what the explicit checkpoint's
// `busy` check turns into a reported one. This test is the one that
// actually depends on the explicit checkpoint existing.
#[tokio::test]
async fn checkpoint_and_close_reports_busy_instead_of_silently_skipping_it() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("app.db");
    let db = Db::connect_local(&db_path).await.unwrap();

    sqlx::query("CREATE TABLE records (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO records (value) VALUES ('a')")
        .execute(db.pool())
        .await
        .unwrap();

    // A second connection with an open read transaction on the old snapshot
    // is exactly what a TRUNCATE checkpoint cannot get past: SQLite needs
    // every reader off the WAL before it can reset it. Left open on purpose
    // -- committing it would let the checkpoint through and defeat the test.
    let reader = Db::connect_local(&db_path).await.unwrap();
    let mut reader_conn = reader.pool().acquire().await.unwrap();
    sqlx::query("BEGIN;")
        .execute(&mut *reader_conn)
        .await
        .unwrap();
    let _open_snapshot: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM records")
        .fetch_one(&mut *reader_conn)
        .await
        .unwrap();

    let result = db.checkpoint_and_close().await;

    assert!(
        result.is_err(),
        "a checkpoint blocked by a live reader must be reported as an error, \
         not silently treated as success: {result:?}"
    );

    // And the failure must not have closed the pool regardless -- the
    // database is still fully intact and usable through `db`, exactly as
    // the doc comment on `checkpoint_and_close` promises.
    sqlx::query("INSERT INTO records (value) VALUES ('b')")
        .execute(db.pool())
        .await
        .expect("the pool must stay open when the checkpoint failed, not only when it succeeds");

    drop(reader_conn);
    reader.pool().close().await;
    db.pool().close().await;
}

/// Demonstration, not a mutation-tested unit: creates a database with rows,
/// runs the exact sequence the vault move uses (checkpoint, close, plain
/// file copy) with no Tauri or app-crate machinery involved, opens the copy
/// at its new path, and shows the rows are there.
#[tokio::test]
async fn a_checkpointed_database_survives_a_plain_file_copy_to_a_new_location() {
    let tmp = tempfile::tempdir().unwrap();
    let old_dir = tmp.path().join("old-vault");
    let new_dir = tmp.path().join("new-vault");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();
    let old_db_path = old_dir.join("app.db");
    let new_db_path = new_dir.join("app.db");

    let db = Db::connect_local(&old_db_path).await.unwrap();
    sqlx::query("CREATE TABLE meetings (id INTEGER PRIMARY KEY, title TEXT NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();
    for title in ["Standup", "Kundengespraech NOR", "1:1 mit Nina"] {
        sqlx::query("INSERT INTO meetings (title) VALUES (?)")
            .bind(title)
            .execute(db.pool())
            .await
            .unwrap();
    }

    db.checkpoint_and_close().await.unwrap();
    tokio::fs::copy(&old_db_path, &new_db_path).await.unwrap();

    let moved = Db::connect_local_plain(&new_db_path).await.unwrap();
    let titles: Vec<String> = sqlx::query_scalar("SELECT title FROM meetings ORDER BY id")
        .fetch_all(moved.pool())
        .await
        .unwrap();
    moved.pool().close().await;

    assert_eq!(titles, vec!["Standup", "Kundengespraech NOR", "1:1 mit Nina"]);
    // The old file is untouched by this test on purpose -- cleaning it up
    // after a successful copy is the caller's job (`vault_move`), not this
    // function's; verifying that stays with the caller's own test.
    assert!(old_db_path.exists());
}
