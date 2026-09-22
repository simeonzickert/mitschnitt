use super::*;

#[tokio::test]
async fn attachment_cloud_sync_intent_is_a_synced_additive_column() {
    let db = test_db().await;
    sqlx::query(
        "INSERT INTO session_attachments (id, workspace_id, session_id)
             VALUES ('attachment-intent', 'workspace-1', 'session-1')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let enabled: i64 = sqlx::query_scalar(
        "SELECT cloud_sync_enabled FROM session_attachments
             WHERE id = 'attachment-intent'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(enabled, 0);

    let invalid = sqlx::query(
        "UPDATE session_attachments SET cloud_sync_enabled = 2
             WHERE id = 'attachment-intent'",
    )
    .execute(db.pool())
    .await;
    assert!(invalid.is_err());
}

// Mitschnitt-Fork (ISA N8, ZICK-251). Zwilling zu
// apps/desktop/src/session/queries/note-attachments.ts: dort ist die
// Datenbank ein Mock, hier laeuft derselbe SQL-Text gegen ein echtes SQLite.
// Wer eine Seite aendert, aendert beide.
const NOTE_ATTACHMENT_REFERENCED: &str = "
  SELECT 1
  FROM session_documents AS document
  WHERE document.session_id = session_attachments.session_id
    AND document.deleted_at IS NULL
    AND CASE
      WHEN document.body_format = 'markdown' THEN instr(
        document.body,
        'attachment://' || substr(
          session_attachments.relative_path,
          length('attachments/') + 1
        )
      ) > 0
      ELSE EXISTS (
        SELECT 1
        FROM json_tree(
          CASE WHEN json_valid(document.body) THEN document.body ELSE '{}' END
        ) AS node
        WHERE node.type = 'object'
          AND json_extract(node.value, '$.type') IN ('image', 'fileAttachment')
          AND json_extract(node.value, '$.attrs.attachmentId') = substr(
            session_attachments.relative_path,
            length('attachments/') + 1
          )
      )
    END
";

async fn reconcile_note_attachments(pool: &sqlx::SqlitePool, session_id: &str, now: &str) {
    let tombstone = format!(
        "UPDATE session_attachments
         SET deleted_at = ?, updated_at = ?
         WHERE session_id = ?
           AND source_type = 'note_upload'
           AND deleted_at IS NULL
           AND NOT EXISTS ({NOTE_ATTACHMENT_REFERENCED})"
    );
    let restore = format!(
        "UPDATE session_attachments
         SET deleted_at = NULL, updated_at = ?
         WHERE session_id = ?
           AND source_type = 'note_upload'
           AND deleted_at IS NOT NULL
           AND EXISTS (
             SELECT 1 FROM sessions
             WHERE sessions.id = session_attachments.session_id
               AND sessions.deleted_at IS NULL
           )
           AND EXISTS ({NOTE_ATTACHMENT_REFERENCED})"
    );
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(tombstone))
        .bind(now)
        .bind(now)
        .bind(session_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(restore))
        .bind(now)
        .bind(session_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

fn note_with_pictures(ids: &[&str]) -> String {
    note_with_nodes(
        &ids.iter()
            .map(|id| {
                format!(
                    r#"{{"type":"image","attrs":{{"attachmentId":{}}}}}"#,
                    serde_json::to_string(id).unwrap()
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn note_with_nodes(nodes: &[String]) -> String {
    let content = nodes.join(",");
    format!(r#"{{"type":"doc","content":[{{"type":"paragraph"}},{content}]}}"#)
}

async fn write_note(pool: &sqlx::SqlitePool, session_id: &str, body: &str) {
    write_document(pool, session_id, "prosemirror_json", body).await;
}

async fn write_document(pool: &sqlx::SqlitePool, session_id: &str, format: &str, body: &str) {
    sqlx::query(
        "INSERT INTO session_documents (id, session_id, kind, body_format, body)
         VALUES (?, ?, 'note', ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           body_format = excluded.body_format, body = excluded.body, deleted_at = NULL",
    )
    .bind(session_id)
    .bind(session_id)
    .bind(format)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_session_with_picture(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT INTO sessions (id, workspace_id, title) VALUES ('session-1', 'w', 'Planning');
         INSERT INTO session_attachments (id, workspace_id, session_id, relative_path, source_type)
         VALUES ('row-a', 'w', 'session-1', 'attachments/a.png', 'note_upload')",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn tombstones(pool: &sqlx::SqlitePool) -> Vec<(String, Option<String>)> {
    sqlx::query_as(
        "SELECT id, deleted_at FROM session_attachments
         WHERE session_id = 'session-1' ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

// Zwei Bilder in der Notiz, eines wird herausgenommen: nur dessen Zeile
// bekommt den Tombstone; das andere und die Tonaufnahme (anderer
// source_type) bleiben unberuehrt. Kommt das Bild zurueck (Cmd+Z), faellt
// der Tombstone wieder. Die Kennung mit Anfuehrungszeichen prueft, dass die
// Suche ueber json_tree geht und nicht ueber einen Teilstring.
#[tokio::test]
async fn note_attachment_reconciliation_tombstones_only_what_no_document_references() {
    let db = test_db().await;
    let pool = db.pool();
    sqlx::query(
        "INSERT INTO sessions (id, workspace_id, title) VALUES ('session-1', 'w', 'Planning');
         INSERT INTO session_attachments (id, workspace_id, session_id, relative_path, source_type)
         VALUES
           ('row-a', 'w', 'session-1', 'attachments/a.png', 'note_upload'),
           ('row-b', 'w', 'session-1', 'attachments/b \"quoted\".png', 'note_upload'),
           ('session-audio:session-1', 'w', 'session-1', 'audio.mp3', 'session_audio')",
    )
    .execute(pool)
    .await
    .unwrap();
    write_note(
        pool,
        "session-1",
        &note_with_pictures(&["a.png", "b \"quoted\".png"]),
    )
    .await;

    reconcile_note_attachments(pool, "session-1", "2026-09-02T04:00:00.000Z").await;
    assert_eq!(
        tombstones(pool).await,
        vec![
            ("row-a".into(), None),
            ("row-b".into(), None),
            ("session-audio:session-1".into(), None),
        ],
        "beide Bilder sind referenziert, nichts darf fallen"
    );

    // Bild a raus.
    write_note(
        pool,
        "session-1",
        &note_with_pictures(&["b \"quoted\".png"]),
    )
    .await;
    reconcile_note_attachments(pool, "session-1", "2026-09-02T04:01:00.000Z").await;
    assert_eq!(
        tombstones(pool).await,
        vec![
            ("row-a".into(), Some("2026-09-02T04:01:00.000Z".into())),
            ("row-b".into(), None),
            ("session-audio:session-1".into(), None),
        ],
        "nur das entfernte Bild traegt den Tombstone"
    );

    // Cmd+Z: Bild a ist wieder in der Notiz.
    write_note(
        pool,
        "session-1",
        &note_with_pictures(&["a.png", "b \"quoted\".png"]),
    )
    .await;
    reconcile_note_attachments(pool, "session-1", "2026-09-02T04:02:00.000Z").await;
    assert_eq!(
        tombstones(pool).await,
        vec![
            ("row-a".into(), None),
            ("row-b".into(), None),
            ("session-audio:session-1".into(), None),
        ],
        "ein zurueckgeholtes Bild verliert seinen Tombstone"
    );
}

// Eine Notiz, die kein JSON ist (Markdown-Altbestand), darf den Abgleich
// nicht zum Absturz bringen -- json_tree auf Nicht-JSON ist ein Fehler, und
// der CASE davor faengt ihn. Ohne lebendes Dokument, das das Bild nennt,
// faellt es.
#[tokio::test]
async fn note_attachment_reconciliation_survives_a_non_json_document() {
    let db = test_db().await;
    let pool = db.pool();
    sqlx::query(
        "INSERT INTO sessions (id, workspace_id, title) VALUES ('session-1', 'w', 'Planning');
         INSERT INTO session_attachments (id, workspace_id, session_id, relative_path, source_type)
         VALUES ('row-a', 'w', 'session-1', 'attachments/a.png', 'note_upload');
         INSERT INTO session_documents (id, session_id, kind, body_format, body)
         VALUES ('session-1', 'session-1', 'note', 'markdown', '# not json, mentions a.png')",
    )
    .execute(pool)
    .await
    .unwrap();

    reconcile_note_attachments(pool, "session-1", "2026-09-02T04:00:00.000Z").await;

    assert_eq!(
        tombstones(pool).await,
        vec![("row-a".into(), Some("2026-09-02T04:00:00.000Z".into()))]
    );
}

// G2c (Grok 4, Forge 7; Fix-Runde 2). Eine Datei-Anlage (`fileAttachment`)
// zaehlt wie ein Bild; ein `attachmentId`-Schluessel an einem Knoten, der
// weder Bild noch Datei ist, zaehlt NICHT -- der Abgleich schaut auf den
// Knotentyp, nicht auf irgendeinen Schluessel irgendwo im Dokument.
#[tokio::test]
async fn note_attachment_reconciliation_counts_file_attachments_by_node_type() {
    let db = test_db().await;
    let pool = db.pool();
    seed_session_with_picture(pool).await;
    write_note(
        pool,
        "session-1",
        &note_with_nodes(&[
            r#"{"type":"fileAttachment","attrs":{"attachmentId":"a.png","name":"a.png"}}"#.into(),
        ]),
    )
    .await;

    reconcile_note_attachments(pool, "session-1", "2026-09-02T04:00:00.000Z").await;
    assert_eq!(
        tombstones(pool).await,
        vec![("row-a".into(), None)],
        "eine Datei-Anlage haelt ihre Zeile am Leben"
    );

    write_note(
        pool,
        "session-1",
        &note_with_nodes(&[
            r#"{"type":"paragraph","attrs":{"attachmentId":"a.png"}}"#.into(),
            r#"{"type":"mention-@","attrs":{"attachmentId":"a.png","id":"x"}}"#.into(),
        ]),
    )
    .await;
    reconcile_note_attachments(pool, "session-1", "2026-09-02T04:01:00.000Z").await;
    assert_eq!(
        tombstones(pool).await,
        vec![("row-a".into(), Some("2026-09-02T04:01:00.000Z".into()))],
        "ein attachmentId-Schluessel an einem fremden Knoten zaehlt nicht"
    );
}

// G2c (Grok 4): eine Notiz im Markdown-Format (Altbestand aus dem
// Vault-Import) nennt ihre Anlagen als `attachment://<id>` -- so schreibt
// der Serializer (packages/editor/src/markdown/serializer.ts) und so liest
// der Parser. Bis Fix-Runde 2 lief ein solches Dokument als '{}' durch den
// Abgleich, und jedes Bild darin fiel nach 24 h.
#[tokio::test]
async fn note_attachment_reconciliation_reads_markdown_references() {
    let db = test_db().await;
    let pool = db.pool();
    seed_session_with_picture(pool).await;
    write_document(
        pool,
        "session-1",
        "markdown",
        "# Notes\n\n![diagram](attachment://a.png)\n",
    )
    .await;

    reconcile_note_attachments(pool, "session-1", "2026-09-02T04:00:00.000Z").await;
    assert_eq!(
        tombstones(pool).await,
        vec![("row-a".into(), None)],
        "ein Markdown-Verweis haelt die Zeile am Leben"
    );

    write_document(pool, "session-1", "markdown", "# Notes\n\nno picture\n").await;
    reconcile_note_attachments(pool, "session-1", "2026-09-02T04:01:00.000Z").await;
    assert_eq!(
        tombstones(pool).await,
        vec![("row-a".into(), Some("2026-09-02T04:01:00.000Z".into()))],
        "ohne Verweis faellt sie"
    );
}
