import { commands as fsSyncCommands } from "@anlg/plugin-fs-sync";

import { executeTransaction, liveQueryClient } from "~/db";
import { enqueueDatabaseWrite } from "~/db/write-queue";

/**
 * How long a picture taken out of a note stays on disk before it is gone for
 * good. Mitschnitt-Fork (ISA N8, ZICK-251).
 *
 * The note write only tombstones the attachment (session/queries/
 * note-attachments.ts); this pass removes file and row once the tombstone is
 * older than this. The window is the editor's undo history, and that history
 * lives as long as the note stays open -- a person can take a picture out in
 * the morning and press Cmd+Z after lunch. Five seconds, the undo window of
 * a deleted NOTE, would be far too short for that; a day costs at most a few
 * megabytes per picture (uploads are capped at 4 MB) and errs on the side
 * that can be undone.
 */
export const NOTE_ATTACHMENT_UNDO_GRACE_MS = 24 * 60 * 60 * 1000;

const NOTE_ATTACHMENT_PATH_PREFIX = "attachments/";

type TombstonedNoteAttachmentRow = {
  id: string;
  session_id: string;
  relative_path: string;
  deleted_at: string;
};

export function noteAttachmentTombstoneExpired(
  deletedAt: unknown,
  nowMs: number,
): boolean {
  if (typeof deletedAt !== "string") return false;
  const deletedAtMs = Date.parse(deletedAt);
  if (!Number.isFinite(deletedAtMs)) return false;
  return nowMs >= deletedAtMs + NOTE_ATTACHMENT_UNDO_GRACE_MS;
}

/**
 * Removes file and row of every note attachment whose tombstone has expired.
 * Returns the ids it removed.
 *
 * Only attachments of LIVING sessions: a deleted session takes its whole
 * folder with it when its own undo window closes (finalizeSessionDeletion ->
 * delete_session_folder, remove_dir_all), and its rows stay tombstoned like
 * every other child row of a deleted session. The sweep and the re-read
 * both join the session for that (G2a, Fix-Runde 2): a session soft-deleted
 * after the sweep is inside its own undo window, and an undo must find its
 * pictures where it left them.
 *
 * Each removal runs in the session's write queue -- the queue every editor
 * save goes through (updateSession) -- and re-reads the row there. Then the
 * ROW goes first, and only if its tombstone is still the one the re-read
 * saw (`deleted_at = ?`); the file goes second, and only if the row went
 * (G2b -- Forge 4, Kimi 1, Grok 5). Not every writer sits in this queue
 * (restoreDeletedSession, the Rust importers), and the old order -- file
 * first, then an unconditional row delete -- would have cost a picture
 * restored between the re-read and the unlink its file. Now such a restore
 * makes the conditional delete a no-op and the file is never touched. The
 * price is the other failure: a file that will not go after its row is gone
 * stays on disk as an orphan (logged; bounded by the upload cap; the session
 * folder takes it along when the session is deleted). A lost file is data
 * loss, a stray file is disk -- that is the trade.
 */
export async function cleanupOrphanedNoteAttachments(
  nowMs = Date.now(),
): Promise<string[]> {
  const rows = await liveQueryClient.execute<TombstonedNoteAttachmentRow>(`
    SELECT
      attachment.id,
      attachment.session_id,
      attachment.relative_path,
      attachment.deleted_at
    FROM session_attachments AS attachment
    JOIN sessions AS session
      ON session.id = attachment.session_id
     AND session.deleted_at IS NULL
    WHERE attachment.source_type = 'note_upload'
      AND attachment.deleted_at IS NOT NULL
      AND attachment.relative_path LIKE 'attachments/%'
    ORDER BY attachment.deleted_at, attachment.id
  `);

  const removed: string[] = [];
  for (const row of rows) {
    if (!noteAttachmentTombstoneExpired(row.deleted_at, nowMs)) continue;

    try {
      const done = await enqueueDatabaseWrite(`session:${row.session_id}`, () =>
        removeExpiredNoteAttachment(row, nowMs),
      );
      if (done) removed.push(row.id);
    } catch (error) {
      console.error("[note-attachments] failed to remove an orphaned file", {
        sessionId: row.session_id,
        attachmentId: row.id,
        error,
      });
    }
  }
  return removed;
}

async function removeExpiredNoteAttachment(
  row: TombstonedNoteAttachmentRow,
  nowMs: number,
): Promise<boolean> {
  const [current] = await liveQueryClient.execute<{
    deleted_at: string | null;
  }>(
    `
      SELECT attachment.deleted_at
      FROM session_attachments AS attachment
      JOIN sessions AS session
        ON session.id = attachment.session_id
       AND session.deleted_at IS NULL
      WHERE attachment.id = ? AND attachment.session_id = ?
    `,
    [row.id, row.session_id],
  );
  if (!current || !noteAttachmentTombstoneExpired(current.deleted_at, nowMs)) {
    return false;
  }

  // Compare-and-delete: the row goes only with the tombstone the re-read
  // saw. Restored (NULL) or tombstoned afresh -> nothing goes, file stays.
  const [rowsRemoved] = await executeTransaction([
    {
      sql: `DELETE FROM session_attachments WHERE id = ? AND session_id = ? AND deleted_at = ?`,
      params: [row.id, row.session_id, current.deleted_at],
    },
    {
      sql: `DELETE FROM attachment_local_state WHERE attachment_id = ? AND NOT EXISTS (SELECT 1 FROM session_attachments WHERE id = ?)`,
      params: [row.id, row.id],
    },
  ]);
  if (rowsRemoved !== 1) return false;

  const attachmentId = row.relative_path.slice(
    NOTE_ATTACHMENT_PATH_PREFIX.length,
  );
  try {
    const result = await fsSyncCommands.attachmentRemove(
      row.session_id,
      attachmentId,
    );
    if (result.status === "error") {
      throw new Error(result.error);
    }
  } catch (error) {
    console.error(
      "[note-attachments] the row is gone but the file stays on disk",
      { sessionId: row.session_id, attachmentId: row.id, error },
    );
  }
  return true;
}
