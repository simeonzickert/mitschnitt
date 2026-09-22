import { json2md } from "@anlg/editor/markdown";
import { commands as fsSyncCommands } from "@anlg/plugin-fs-sync";

import { executeTransaction, liveQueryClient } from "~/db";
import { enqueueDatabaseWrite } from "~/db/write-queue";
import { waitForPendingSoftDelete } from "~/session/pending-soft-deletes";
import { isSessionBusyRecording } from "~/session/recording-busy";
import type { DeletedSessionData } from "~/store/zustand/undo-delete";
import { commands as tauriCommands } from "~/types/tauri.gen";

type SessionIdentitySqlRow = { id: string };
type SessionDeleteSqlRow = { id: string; title: string };
type SessionEmptySqlRow = {
  title: string;
  event_json: string;
  note_body: string;
  note_body_format: string;
  transcript_count: number;
  enhanced_note_count: number;
  meeting_chat_count: number;
  manual_participant_count: number;
  tag_count: number;
};

/** The soft delete found the session recording and wrote nothing. */
export const SOFT_DELETE_BLOCKED_BY_RECORDING = "recording";

export type SoftDeleteSessionResult =
  | DeletedSessionData
  | null
  | typeof SOFT_DELETE_BLOCKED_BY_RECORDING;

/**
 * The one way a session gets its tombstone. Mitschnitt-Fork (ISA N8,
 * ZICK-251; G1, Fix-Runde 2 -- Forge, Kimi, Grok).
 *
 * Until 02.09.2026 the recording guard lived in the delete hook alone and
 * ran once, synchronously, before the write was even queued; a recording
 * that started in between -- confirmation dialog open, Record pressed on the
 * same note -- was tombstoned with its note. So the guard runs HERE, inside
 * the session's write queue, and it asks TWICE. Every delete entry -- the
 * confirmed delete, the silent close of an empty note -- comes through here
 * and gets the same answer.
 *
 * What this queue actually serialises (02.09.2026, Opus-Review 3b, Befund
 * 1+2, measured against the call sites): note and content saves
 * (`session/content-mutations.ts`, `session/queries/sessions.ts`), the
 * enhancer's writes, the recording's own lifecycle markers
 * (`stt/capture-lifecycle-storage.ts`), and the attachment GC's file unlink
 * (`services/note-attachment-retention.ts`) all queue on `session:<id>`.
 * What it does NOT: transcript writes run on `transcript:<id>`
 * (`stt/queries.ts`), and the recording START never touches this queue at
 * all -- `markLiveStartRequested` flips `live.loading`/`live.sessionId`
 * synchronously in the store, and only the DB writes that follow it line up
 * behind us. A single check is therefore check-then-act, not an invariant.
 *
 * Hence two asks in the same queue turn. Before the write: catches a
 * recording that started while this write waited in the queue. After the
 * commit: catches one that started during the two IPC round trips the
 * tombstone needs (the SELECT and the transaction) -- and because we are
 * still inside our own turn, we put the tombstone back right here, before
 * any queued lifecycle marker of that recording can run.
 *
 * `isBusy` is injectable for tests only.
 */
export function softDeleteSession(
  sessionId: string,
  tombstone = new Date().toISOString(),
  isBusy: (sessionId: string) => boolean = isSessionBusyRecording,
): Promise<SoftDeleteSessionResult> {
  return enqueueDatabaseWrite(`session:${sessionId}`, async () => {
    if (isBusy(sessionId)) return SOFT_DELETE_BLOCKED_BY_RECORDING;
    return tombstoneSession(sessionId, tombstone, isBusy);
  });
}

async function tombstoneSession(
  sessionId: string,
  tombstone: string,
  isBusy: (sessionId: string) => boolean,
): Promise<SoftDeleteSessionResult> {
  const [session] = await liveQueryClient.execute<SessionDeleteSqlRow>(
    `SELECT id, title FROM sessions WHERE id = ? AND deleted_at IS NULL LIMIT 1`,
    [sessionId],
  );
  if (!session) return null;

  const rowsAffected = await executeTransaction(
    buildSessionTombstoneStatements(sessionId, tombstone),
  );
  if (rowsAffected[rowsAffected.length - 1] !== 1) return null;

  // The two round trips above are the window: the record button flips the
  // store synchronously and does not queue behind us. Same turn, so the
  // restore lands before anything that recording queued can touch the row.
  if (isBusy(sessionId)) {
    await executeTransaction(
      buildSessionTombstoneStatements(sessionId, tombstone, true),
    );
    return SOFT_DELETE_BLOCKED_BY_RECORDING;
  }

  return {
    session: { id: session.id, title: session.title },
    tombstone,
    deletedAt: Date.now(),
  };
}

export async function isSessionEmpty(sessionId: string): Promise<boolean> {
  const [row] = await liveQueryClient.execute<SessionEmptySqlRow>(
    `
      SELECT
        sessions.title,
        sessions.event_json,
        COALESCE(note.body, '') AS note_body,
        COALESCE(note.body_format, '') AS note_body_format,
        (
          SELECT COUNT(*)
          FROM transcripts
          WHERE session_id = sessions.id AND deleted_at IS NULL
        ) AS transcript_count,
        (
          SELECT COUNT(*)
          FROM session_documents
          WHERE session_id = sessions.id
            AND kind IN ('summary', 'template_output')
            AND deleted_at IS NULL
        ) AS enhanced_note_count,
        (
          SELECT COUNT(*)
          FROM session_documents
          WHERE session_id = sessions.id
            AND kind = 'meeting_chat'
            AND deleted_at IS NULL
        ) AS meeting_chat_count,
        (
          SELECT COUNT(*)
          FROM session_participants
          WHERE session_id = sessions.id
            AND source NOT IN ('auto', 'excluded')
            AND human_id <> sessions.owner_user_id
            AND deleted_at IS NULL
        ) AS manual_participant_count,
        (
          SELECT COUNT(*)
          FROM session_tags
          WHERE session_id = sessions.id AND deleted_at IS NULL
        ) AS tag_count
      FROM sessions
      LEFT JOIN session_documents AS note
        ON note.id = sessions.id
        AND note.kind = 'note'
        AND note.deleted_at IS NULL
      WHERE sessions.id = ? AND sessions.deleted_at IS NULL
      LIMIT 1
    `,
    [sessionId],
  );

  if (!row) return true;
  if (row.title.trim() && !row.event_json) return false;
  if (hasNoteContent(row.note_body, row.note_body_format)) return false;

  return (
    Number(row.transcript_count) === 0 &&
    Number(row.enhanced_note_count) === 0 &&
    Number(row.meeting_chat_count) === 0 &&
    Number(row.manual_participant_count) === 0 &&
    Number(row.tag_count) === 0
  );
}

export async function restoreDeletedSession(
  data: DeletedSessionData,
): Promise<void> {
  // The undo toast shows before the soft-delete write commits. Wait for the
  // in-flight delete to settle first — an "alive" session during that window
  // is not restored, it just isn't tombstoned yet.
  await waitForPendingSoftDelete(data.session.id);
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const rowsAffected = await executeTransaction(
      buildSessionTombstoneStatements(data.session.id, data.tombstone, true),
    );
    if (rowsAffected[rowsAffected.length - 1] === 1) return;

    const [alive] = await liveQueryClient.execute<SessionIdentitySqlRow>(
      `SELECT id FROM sessions WHERE id = ? AND deleted_at IS NULL LIMIT 1`,
      [data.session.id],
    );
    if (alive) return;

    await new Promise((resolve) => setTimeout(resolve, 100));
  }

  throw new Error(`Session ${data.session.id} was never soft-deleted`);
}

export async function finalizeSessionDeletion(
  sessionId: string,
): Promise<void> {
  // Mitschnitt-Fork (F16b). Both places, because since F16 there are two: the
  // machine room under `sessions/<uuid>`, and the folder a person actually
  // looks at. Removing only the first leaves a deleted meeting with its
  // transcript, its summary and its recording still readable -- and on a synced
  // root, still on a server. The audit would report that folder as an orphan
  // and deliberately never remove it, which is the right call for a meeting
  // that merely vanished from the database and the wrong one here.
  //
  // Both are best effort and independent: a machine room that will not let go
  // must not keep the meeting folder alive, and neither failure may take down
  // the deletion the person already saw happen.
  try {
    const result = await fsSyncCommands.deleteSessionFolder(sessionId);
    if (result.status === "error") {
      console.error("[delete-session] failed to delete session folder", {
        sessionId,
        error: result.error,
      });
    }
  } catch (error) {
    console.error("[delete-session] failed to delete session folder", {
      sessionId,
      error,
    });
  }

  try {
    const mirrored = await tauriCommands.mirrorForgetSession(sessionId);
    if (mirrored.status === "error") {
      console.error("[delete-session] failed to delete the meeting folder", {
        sessionId,
        error: mirrored.error,
      });
    }
  } catch (error) {
    console.error("[delete-session] failed to delete the meeting folder", {
      sessionId,
      error,
    });
  }
}

export function buildSessionTombstoneStatements(
  sessionId: string,
  tombstone: string,
  restore = false,
) {
  const value = restore ? null : tombstone;
  const predicate = restore ? "deleted_at = ?" : "deleted_at IS NULL";
  const predicateParams = restore ? [tombstone] : [];
  const directTables = [
    "session_documents",
    "transcripts",
    "session_participants",
    "session_tags",
    "action_items",
    "session_attachments",
  ];

  const statements = directTables.map((table) => ({
    sql: `
      UPDATE ${table}
      SET deleted_at = ?, updated_at = ?
      WHERE session_id = ? AND ${predicate}
    `,
    params: [value, tombstone, sessionId, ...predicateParams],
  }));

  statements.push({
    sql: `
      UPDATE entity_mentions
      SET deleted_at = ?, updated_at = ?
      WHERE (
        (source_type = 'session' AND source_id = ?)
        OR (target_type = 'session' AND target_id = ?)
      ) AND ${predicate}
    `,
    params: [value, tombstone, sessionId, sessionId, ...predicateParams],
  });
  statements.push({
    sql: `
      UPDATE sessions
      SET deleted_at = ?, updated_at = ?
      WHERE id = ? AND ${predicate}
    `,
    params: [value, tombstone, sessionId, ...predicateParams],
  });

  return statements;
}

function hasNoteContent(body: string, format: string): boolean {
  if (!body) return false;

  let markdown = body;
  if (format === "prosemirror_json") {
    try {
      markdown = json2md(JSON.parse(body));
    } catch {
      markdown = body;
    }
  }

  markdown = markdown.trim();
  return Boolean(markdown && markdown !== "&nbsp;");
}
