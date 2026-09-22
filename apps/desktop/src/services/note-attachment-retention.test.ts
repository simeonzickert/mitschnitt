import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  attachmentRemove: vi.fn(),
  execute: vi.fn(),
  executeTransaction: vi.fn(),
  enqueueDatabaseWrite: vi.fn((_key: string, write: () => Promise<unknown>) =>
    write(),
  ),
}));

vi.mock("@anlg/plugin-fs-sync", () => ({
  commands: { attachmentRemove: mocks.attachmentRemove },
}));

vi.mock("~/db", () => ({
  executeTransaction: mocks.executeTransaction,
  liveQueryClient: { execute: mocks.execute },
}));

vi.mock("~/db/write-queue", () => ({
  enqueueDatabaseWrite: mocks.enqueueDatabaseWrite,
}));

import {
  cleanupOrphanedNoteAttachments,
  NOTE_ATTACHMENT_UNDO_GRACE_MS,
  noteAttachmentTombstoneExpired,
} from "./note-attachment-retention";

const NOW = Date.parse("2026-09-02T12:00:00.000Z");

function tombstoned(agoMs: number) {
  return new Date(NOW - agoMs).toISOString();
}

const row = (deletedAt: string) => ({
  id: "attachment-row-1",
  session_id: "session-1",
  relative_path: "attachments/diagram.png",
  deleted_at: deletedAt,
});

// Mitschnitt-Fork (ISA N8, ZICK-251). The note write only tombstones a
// picture taken out of the note; this pass removes file and row once the
// tombstone is older than the grace window. Measured on 01.09.2026: before
// it, both lived for ever.
describe("cleanupOrphanedNoteAttachments", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.attachmentRemove.mockResolvedValue({ status: "ok", data: null });
    mocks.executeTransaction.mockResolvedValue([1, 1]);
  });

  it("leaves a picture alone while its tombstone is inside the grace window", async () => {
    const fresh = row(tombstoned(NOTE_ATTACHMENT_UNDO_GRACE_MS - 60_000));
    mocks.execute.mockResolvedValueOnce([fresh]);

    await expect(cleanupOrphanedNoteAttachments(NOW)).resolves.toEqual([]);

    expect(mocks.attachmentRemove).not.toHaveBeenCalled();
    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  // G2b (Forge 4, Kimi 1, Grok 5; Fix-Runde 2): row first, file second.
  // The row goes only if its tombstone is still the one the sweep saw --
  // a restore (deleted_at NULL) or a fresh tombstone leaves it, and then the
  // file stays too. No writer, queued or not, can slip a restore between the
  // re-read and the unlink any more.
  it("removes row and file once the tombstone is older than the grace window", async () => {
    const expired = row(tombstoned(NOTE_ATTACHMENT_UNDO_GRACE_MS + 60_000));
    mocks.execute
      .mockResolvedValueOnce([expired])
      .mockResolvedValueOnce([{ deleted_at: expired.deleted_at }]);
    mocks.executeTransaction.mockResolvedValue([1, 1]);

    await expect(cleanupOrphanedNoteAttachments(NOW)).resolves.toEqual([
      "attachment-row-1",
    ]);

    // In the session's write queue, behind every note save.
    expect(mocks.enqueueDatabaseWrite).toHaveBeenCalledWith(
      "session:session-1",
      expect.any(Function),
    );
    const statements = mocks.executeTransaction.mock.calls[0]![0] as Array<{
      sql: string;
      params: unknown[];
    }>;
    expect(statements.map((statement) => statement.sql.trim())).toEqual([
      "DELETE FROM session_attachments WHERE id = ? AND session_id = ? AND deleted_at = ?",
      "DELETE FROM attachment_local_state WHERE attachment_id = ? AND NOT EXISTS (SELECT 1 FROM session_attachments WHERE id = ?)",
    ]);
    expect(statements[0]!.params).toEqual([
      "attachment-row-1",
      "session-1",
      expired.deleted_at,
    ]);
    expect(mocks.attachmentRemove).toHaveBeenCalledExactlyOnceWith(
      "session-1",
      "diagram.png",
    );
    expect(mocks.executeTransaction.mock.invocationCallOrder[0]).toBeLessThan(
      mocks.attachmentRemove.mock.invocationCallOrder[0]!,
    );
  });

  // Cmd+Z between the sweep and the removal: the note save restored the row
  // (deleted_at back to NULL). The re-read inside the queue sees that and
  // touches nothing -- the picture is back in the note.
  it("keeps a picture that was put back into the note before its turn came", async () => {
    const expired = row(tombstoned(NOTE_ATTACHMENT_UNDO_GRACE_MS + 60_000));
    mocks.execute
      .mockResolvedValueOnce([expired])
      .mockResolvedValueOnce([{ deleted_at: null }]);

    await expect(cleanupOrphanedNoteAttachments(NOW)).resolves.toEqual([]);

    expect(mocks.attachmentRemove).not.toHaveBeenCalled();
    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  // The restore landed after the re-read (a writer outside the queue, or
  // the moment between the two). The conditional DELETE finds a different
  // deleted_at, removes nothing -- and the file is never touched.
  it("keeps the file when the row was restored between the re-read and the delete", async () => {
    const expired = row(tombstoned(NOTE_ATTACHMENT_UNDO_GRACE_MS + 60_000));
    mocks.execute
      .mockResolvedValueOnce([expired])
      .mockResolvedValueOnce([{ deleted_at: expired.deleted_at }]);
    mocks.executeTransaction.mockResolvedValue([0, 0]);

    await expect(cleanupOrphanedNoteAttachments(NOW)).resolves.toEqual([]);

    expect(mocks.attachmentRemove).not.toHaveBeenCalled();
  });

  // G2a: a session soft-deleted after the sweep is inside its own undo
  // window; its pictures are not this pass's business (finalizeSessionDeletion
  // removes the whole folder later, or the undo brings everything back).
  it("leaves a picture alone when its session was soft-deleted after the sweep", async () => {
    const expired = row(tombstoned(NOTE_ATTACHMENT_UNDO_GRACE_MS + 60_000));
    mocks.execute
      .mockResolvedValueOnce([expired])
      // The re-read joins the living session; a tombstoned one yields no row.
      .mockResolvedValueOnce([]);

    await expect(cleanupOrphanedNoteAttachments(NOW)).resolves.toEqual([]);

    const [sql] = mocks.execute.mock.calls[1]!;
    expect(sql).toContain("session.deleted_at IS NULL");
    expect(mocks.attachmentRemove).not.toHaveBeenCalled();
    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  // Row first means a file that will not go is an orphan on disk, not a
  // row for the next pass. Logged; the session folder takes it along when
  // the session itself is deleted. The other order cost a restored picture
  // its file, which is the worse trade.
  it("reports the row as removed and logs when the file will not go", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    const expired = row(tombstoned(NOTE_ATTACHMENT_UNDO_GRACE_MS + 60_000));
    mocks.execute
      .mockResolvedValueOnce([expired])
      .mockResolvedValueOnce([{ deleted_at: expired.deleted_at }]);
    mocks.executeTransaction.mockResolvedValue([1, 1]);
    mocks.attachmentRemove.mockResolvedValue({
      status: "error",
      error: "permission denied",
    });

    await expect(cleanupOrphanedNoteAttachments(NOW)).resolves.toEqual([
      "attachment-row-1",
    ]);

    expect(consoleError).toHaveBeenCalledOnce();
    consoleError.mockRestore();
  });

  it("only ever asks for note uploads of living sessions", async () => {
    mocks.execute.mockResolvedValueOnce([]);

    await cleanupOrphanedNoteAttachments(NOW);

    const [sql] = mocks.execute.mock.calls[0]!;
    expect(sql).toContain("source_type = 'note_upload'");
    expect(sql).toContain("attachment.deleted_at IS NOT NULL");
    expect(sql).toContain("session.deleted_at IS NULL");
  });

  it("treats an unreadable tombstone as not expired", () => {
    expect(noteAttachmentTombstoneExpired(null, NOW)).toBe(false);
    expect(noteAttachmentTombstoneExpired("yesterday", NOW)).toBe(false);
    expect(
      noteAttachmentTombstoneExpired(
        tombstoned(NOTE_ATTACHMENT_UNDO_GRACE_MS),
        NOW,
      ),
    ).toBe(true);
  });
});
