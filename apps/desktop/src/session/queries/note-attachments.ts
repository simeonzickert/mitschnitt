/**
 * Mitschnitt-Fork (ISA N8, ZICK-251): pictures a person takes out of a note
 * do not live forever on disk.
 *
 * Measured on 01.09.2026 (Vermessung, Teil 1): an image removed from the
 * note left its file under `sessions/<id>/attachments/<attachmentId>` AND its
 * row in `session_attachments` untouched -- there was no `DELETE FROM
 * session_attachments` anywhere, and the only caller of `attachment_remove`
 * was the rollback of a failed upload. Audio had a garbage collector
 * (services/audio-retention.ts); note attachments had nothing.
 *
 * Two statements, run inside the same transaction as the note write, right
 * after the document upsert:
 *
 * 1. Every `note_upload` attachment of the session that no living document of
 *    the session still references gets a tombstone (`deleted_at`). Not
 *    deleted: the editor keeps an undo history (prosemirror-history in
 *    packages/editor/src/note), and a note save between an upload and the
 *    editor inserting the node would otherwise take the fresh picture with
 *    it. The tombstone is a delay, not a deletion.
 * 2. Every tombstoned `note_upload` attachment that IS referenced again comes
 *    back (`deleted_at = NULL`) -- Cmd+Z after removing a picture, or the
 *    race above resolving. Only while the session itself is alive.
 *
 * What counts as a reference depends on the document's format (G2c,
 * Fix-Runde 2 -- Grok 4, Forge 7):
 *
 * - `prosemirror_json` (every note the editor saves): the `attachmentId`
 *   attribute of an `image` or `fileAttachment` node
 *   (packages/editor/src/note/portable-attachments.ts). Walked with
 *   `json_tree` and matched on the NODE -- object nodes whose `type` is one of
 *   the two and whose `attrs.attachmentId` is the id -- not on any
 *   `attachmentId` key anywhere in the document, and not with a substring
 *   search, so a quote or a backslash in an id (it is a sanitised file name)
 *   cannot fake or hide a match. `json_tree` on a body that is not JSON is an
 *   error, not an empty set, hence the inner `CASE`.
 * - `markdown` (legacy vault imports): the portable link
 *   `attachment://<id>` the serializer writes and the parser reads
 *   (packages/editor/src/markdown/serializer.ts, parser.ts). Until this round
 *   a markdown body went through the JSON branch as `'{}'`, so every picture
 *   of such a note was tombstoned and gone after the grace window. Substring
 *   match, on purpose: it can only err towards keeping a file.
 *
 * The file and the row go away later, once the tombstone is older than the
 * grace window (services/note-attachment-retention.ts). The same SQL text is
 * proven against a real SQLite in crates/db-app
 * (schema_tests/attachments.rs, note_attachment_reconciliation_*); keep the
 * two in step.
 */

const REFERENCED_BY_A_LIVING_DOCUMENT = `
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
`;

export const NOTE_ATTACHMENT_TOMBSTONE_SQL = `
  UPDATE session_attachments
  SET deleted_at = ?, updated_at = ?
  WHERE session_id = ?
    AND source_type = 'note_upload'
    AND deleted_at IS NULL
    AND NOT EXISTS (${REFERENCED_BY_A_LIVING_DOCUMENT})
`;

export const NOTE_ATTACHMENT_RESTORE_SQL = `
  UPDATE session_attachments
  SET deleted_at = NULL, updated_at = ?
  WHERE session_id = ?
    AND source_type = 'note_upload'
    AND deleted_at IS NOT NULL
    AND EXISTS (
      SELECT 1 FROM sessions
      WHERE sessions.id = session_attachments.session_id
        AND sessions.deleted_at IS NULL
    )
    AND EXISTS (${REFERENCED_BY_A_LIVING_DOCUMENT})
`;

export function buildNoteAttachmentReconcileStatements(
  sessionId: string,
  now: string,
): Array<{ sql: string; params: unknown[] }> {
  return [
    { sql: NOTE_ATTACHMENT_TOMBSTONE_SQL, params: [now, now, sessionId] },
    { sql: NOTE_ATTACHMENT_RESTORE_SQL, params: [now, sessionId] },
  ];
}
