use super::*;
use sqlx::Row;

#[tokio::test]
async fn acknowledged_search_generations_remain_monotonic() {
    let db = test_db().await;
    sqlx::query("INSERT INTO sessions (id, title) VALUES ('session-1', 'One')")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE search_index_dirty
         SET acknowledged_generation = generation
         WHERE entity_type = 'session' AND entity_id = 'session-1'",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE sessions SET title = 'Two' WHERE id = 'session-1'")
        .execute(db.pool())
        .await
        .unwrap();

    let (generation, acknowledged_generation): (i64, i64) = sqlx::query_as(
        "SELECT generation, acknowledged_generation
         FROM search_index_dirty
         WHERE entity_type = 'session' AND entity_id = 'session-1'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(generation, 2);
    assert_eq!(acknowledged_generation, 1);
}

#[tokio::test]
async fn active_transcript_journal_defers_search_until_final_compaction() {
    let db = test_db().await;
    sqlx::query("INSERT INTO sessions (id, title) VALUES ('session-1', 'One')")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO transcripts (id, session_id, words_json)
         VALUES ('transcript-1', 'session-1', '[]')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE search_index_dirty
         SET acknowledged_generation = generation
         WHERE entity_type = 'session' AND entity_id = 'session-1'",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let baseline_generation: i64 = sqlx::query_scalar(
        "SELECT generation FROM search_index_dirty
         WHERE entity_type = 'session' AND entity_id = 'session-1'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();

    let mut transaction = db.pool().begin().await.unwrap();
    sqlx::query(
        "INSERT INTO transcript_live_state (transcript_id, next_sequence)
         VALUES ('transcript-1', 120)",
    )
    .execute(&mut *transaction)
    .await
    .unwrap();
    for sequence in 0..120 {
        sqlx::query(
            "INSERT INTO transcript_live_deltas (id, transcript_id, sequence)
             VALUES (?, 'transcript-1', ?)",
        )
        .bind(format!("delta-{sequence}"))
        .bind(sequence)
        .execute(&mut *transaction)
        .await
        .unwrap();
    }
    transaction.commit().await.unwrap();

    let generation_during_capture: i64 = sqlx::query_scalar(
        "SELECT generation FROM search_index_dirty
         WHERE entity_type = 'session' AND entity_id = 'session-1'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(generation_during_capture, baseline_generation);

    sqlx::query(
        "UPDATE transcripts
         SET words_json = '[{\"text\":\"final\"}]', content_revision = content_revision + 1
         WHERE id = 'transcript-1'",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let (final_generation, acknowledged_generation): (i64, i64) = sqlx::query_as(
        "SELECT generation, acknowledged_generation
         FROM search_index_dirty
         WHERE entity_type = 'session' AND entity_id = 'session-1'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(final_generation, baseline_generation + 1);
    assert_eq!(acknowledged_generation, baseline_generation);
}

#[tokio::test]
async fn search_index_queue_coalesces_changes_and_tracks_session_moves() {
    let db = test_db().await;

    sqlx::query(
        "INSERT INTO sessions (id, title) VALUES ('session-1', 'One'), ('session-2', 'Two')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("DELETE FROM search_index_dirty")
        .execute(db.pool())
        .await
        .unwrap();

    sqlx::query(
            "INSERT INTO session_documents (id, session_id, body) VALUES ('document-1', 'session-1', 'one')",
        )
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE session_documents SET body = 'two' WHERE id = 'document-1'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE session_documents SET session_id = 'session-2' WHERE id = 'document-1'")
        .execute(db.pool())
        .await
        .unwrap();

    sqlx::query(
            "INSERT INTO transcripts (id, session_id, words_json) VALUES ('transcript-1', 'session-1', '[]')",
        )
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE transcripts SET session_id = 'session-2' WHERE id = 'transcript-1'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM transcripts WHERE id = 'transcript-1'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM session_documents WHERE id = 'document-1'")
        .execute(db.pool())
        .await
        .unwrap();

    let rows = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT entity_type, entity_id, generation
             FROM search_index_dirty
             ORDER BY entity_type, entity_id",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();

    assert_eq!(
        rows,
        vec![
            ("session".to_string(), "session-1".to_string(), 5),
            ("session".to_string(), "session-2".to_string(), 4),
        ]
    );
}

#[tokio::test]
async fn search_index_queue_tracks_entity_lifecycle_and_starts_unversioned() {
    let db = test_db().await;

    let projection_version: i64 = sqlx::query_scalar(
        "SELECT projection_version FROM search_index_state WHERE id = 'default'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(projection_version, 0);

    sqlx::query("INSERT INTO sessions (id, title) VALUES ('session-1', 'One')")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO humans (id, name) VALUES ('human-1', 'Ada')")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO organizations (id, name) VALUES ('organization-1', 'Acme')")
        .execute(db.pool())
        .await
        .unwrap();

    sqlx::query("UPDATE sessions SET deleted_at = '2026-07-14T00:00:00Z' WHERE id = 'session-1'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE humans SET memo = 'Updated' WHERE id = 'human-1'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM organizations WHERE id = 'organization-1'")
        .execute(db.pool())
        .await
        .unwrap();

    let rows = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT entity_type, entity_id, generation
             FROM search_index_dirty
             ORDER BY entity_type, entity_id",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();

    assert_eq!(
        rows,
        vec![
            ("human".to_string(), "human-1".to_string(), 2),
            ("organization".to_string(), "organization-1".to_string(), 2,),
            ("session".to_string(), "session-1".to_string(), 2),
        ]
    );
}
