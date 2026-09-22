import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  deleteSessionFolder: vi.fn(),
  mirrorForgetSession: vi.fn(),
  execute: vi.fn(),
  executeTransaction: vi.fn(),
}));

vi.mock("@anlg/editor/markdown", () => ({ json2md: () => "" }));

vi.mock("@anlg/plugin-fs-sync", () => ({
  commands: { deleteSessionFolder: mocks.deleteSessionFolder },
}));

vi.mock("~/db", () => ({
  executeTransaction: mocks.executeTransaction,
  liveQueryClient: { execute: mocks.execute },
}));

vi.mock("~/session/pending-soft-deletes", () => ({
  waitForPendingSoftDelete: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("~/types/tauri.gen", () => ({
  commands: { mirrorForgetSession: mocks.mirrorForgetSession },
}));

import { finalizeSessionDeletion, softDeleteSession } from "./deletion";

import { enqueueDatabaseWrite } from "~/db/write-queue";

describe("finalizing a deleted meeting", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.execute.mockResolvedValue([]);
    mocks.executeTransaction.mockResolvedValue([]);
    mocks.deleteSessionFolder.mockResolvedValue({ status: "ok", data: true });
    mocks.mirrorForgetSession.mockResolvedValue({ status: "ok", data: true });
  });

  // Mitschnitt-Fork (F16b). Since F16 a meeting lives in two folders: the
  // machine room under `sessions/<uuid>` and the one a person actually opens.
  // Clearing only the first leaves a deleted meeting with its transcript, its
  // summary and its recording still readable -- and on a synced root, still on
  // a server. The audit reports such a folder as an orphan and deliberately
  // never removes it, which is right for a meeting that merely vanished from
  // the database and wrong for one the person deleted.
  it("clears the meeting folder as well as the machine room", async () => {
    await finalizeSessionDeletion("session-1");

    expect(mocks.deleteSessionFolder).toHaveBeenCalledWith("session-1");
    expect(mocks.mirrorForgetSession).toHaveBeenCalledWith("session-1");
  });

  // The two are independent on purpose: a machine room that will not let go
  // must not keep the readable copy of a deleted meeting alive.
  it("still clears the meeting folder when the machine room refuses", async () => {
    mocks.deleteSessionFolder.mockResolvedValue({
      status: "error",
      error: "permission denied",
    });

    await expect(finalizeSessionDeletion("session-1")).resolves.toBeUndefined();
    expect(mocks.mirrorForgetSession).toHaveBeenCalledWith("session-1");
  });

  // The person has already seen the meeting disappear; neither failure may
  // surface as a rejected promise now.
  it("does not throw when the meeting folder cannot be removed", async () => {
    mocks.mirrorForgetSession.mockRejectedValue(new Error("disk gone"));

    await expect(finalizeSessionDeletion("session-1")).resolves.toBeUndefined();
  });
});

// Mitschnitt-Fork (ISA N8, ZICK-251; G1, Fix-Runde 2). Deleting is an
// invariant, not a check-then-act: the recording guard runs again inside the
// session's write queue, right before the tombstone, so a recording that
// started between the confirmation and the write is still seen. Every entry
// (confirmed delete, silent close of an empty note) comes through here.
describe("soft-deleting a session", () => {
  const liveRow = [{ id: "session-1", title: "Planning" }];

  beforeEach(() => {
    vi.clearAllMocks();
    mocks.execute.mockResolvedValue(liveRow);
    mocks.executeTransaction.mockResolvedValue([0, 0, 0, 0, 0, 0, 0, 1]);
  });

  it("tombstones an idle session", async () => {
    await expect(
      softDeleteSession("session-1", "2026-09-02T04:00:00.000Z", () => false),
    ).resolves.toMatchObject({
      session: { id: "session-1", title: "Planning" },
      tombstone: "2026-09-02T04:00:00.000Z",
    });
    expect(mocks.executeTransaction).toHaveBeenCalledOnce();
  });

  // Falsifier: a guard that runs before the queue -- or none, as until
  // 02.09.2026 -- lets this write through, and the note is tombstoned while
  // its recording runs.
  it("does not tombstone a session whose recording started while the write waited in the queue", async () => {
    let busy = false;
    void enqueueDatabaseWrite("session:session-1", async () => {
      busy = true;
    });

    await expect(
      softDeleteSession(
        "session-1",
        "2026-09-02T04:00:00.000Z",
        (sessionId) => sessionId === "session-1" && busy,
      ),
    ).resolves.toBe("recording");

    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  it("runs in the session's write queue, behind every earlier write", async () => {
    const order: string[] = [];
    void enqueueDatabaseWrite("session:session-1", async () => {
      await new Promise((resolve) => setTimeout(resolve, 5));
      order.push("note save");
    });
    mocks.executeTransaction.mockImplementation(async () => {
      order.push("tombstone");
      return [0, 0, 0, 0, 0, 0, 0, 1];
    });

    await softDeleteSession(
      "session-1",
      "2026-09-02T04:00:00.000Z",
      () => false,
    );

    expect(order).toEqual(["note save", "tombstone"]);
  });

  // Falsifier: a single check before the write is check-then-act. The record
  // button flips the store synchronously and does not queue behind us, so a
  // recording can begin during the two IPC round trips the tombstone needs.
  // Asking again after the commit -- and undoing it in the same turn -- is
  // what makes this an invariant. Opus-Review 3b, Befund 1.
  it("restores the tombstone in the same turn when the recording started during the write", async () => {
    let asked = 0;

    await expect(
      softDeleteSession("session-1", "2026-09-02T04:00:00.000Z", () => {
        asked += 1;
        return asked > 1;
      }),
    ).resolves.toBe("recording");

    expect(mocks.executeTransaction).toHaveBeenCalledTimes(2);

    const restoreStatements = mocks.executeTransaction.mock.calls[1]?.[0] as {
      sql: string;
      params: unknown[];
    }[];
    // The restore is the tombstone read backwards: it writes NULL into
    // `deleted_at` and finds its rows by the tombstone it just wrote, where
    // the tombstone wrote the timestamp and matched on `deleted_at IS NULL`.
    // Checking the written value and the predicate, not a literal string
    // anyone is free to reformat.
    expect(restoreStatements).not.toHaveLength(0);
    for (const statement of restoreStatements) {
      expect(statement.params[0]).toBeNull();
      expect(statement.params[statement.params.length - 1]).toBe(
        "2026-09-02T04:00:00.000Z",
      );
      expect(statement.sql).not.toContain("deleted_at IS NULL");
    }
  });
});
