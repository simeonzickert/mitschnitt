use super::*;

#[tokio::test]
async fn sent_disclosure_cannot_be_stored_as_a_consent_source() {
    let db = test_db().await;
    let error = sqlx::query(
        "INSERT INTO session_participant_consent (
           session_id, participant_key, status, source, updated_at
         ) VALUES (
           'session-1', 'ada', 'consented', 'disclosure_sent', '2026-08-21T00:00:00Z'
         )",
    )
    .execute(db.pool())
    .await
    .unwrap_err();
    assert!(error.to_string().contains("CHECK"));
}
