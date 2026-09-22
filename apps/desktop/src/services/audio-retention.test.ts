import { beforeEach, describe, expect, test, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  cleanupDeletedSessionAudio: vi.fn(),
  deleteLocalSessionAudio: vi.fn(),
  execute: vi.fn(),
  getSessionMode: vi.fn(),
  live: { loading: false, sessionId: null as string | null },
}));

vi.mock("~/session/attachments", () => ({
  cleanupDeletedSessionAudio: mocks.cleanupDeletedSessionAudio,
  deleteLocalSessionAudio: mocks.deleteLocalSessionAudio,
}));

vi.mock("~/db", () => ({
  liveQueryClient: { execute: mocks.execute },
}));

vi.mock("~/store/zustand/listener/instance", () => ({
  listenerStore: {
    getState: () => ({
      getSessionMode: mocks.getSessionMode,
      live: mocks.live,
    }),
  },
}));

import {
  cleanupExpiredAudio,
  deleteProcessedAudioForRetention,
  normalizeAudioRetention,
  sessionAudioExpired,
  subscribeToSessionAudioRetention,
} from "./audio-retention";
import {
  AUDIO_RETENTION_DURATION_MS,
  DEFAULT_AUDIO_RETENTION,
  OFFERED_AUDIO_RETENTION,
  retentionChangeDeletes,
  retentionForCleanup,
  shortensAudioRetention,
} from "./audio-retention-policy";

// `has_local_audio` defaults to 1 because that is the ordinary row: a meeting
// whose recording is still on this machine. A test that wants the other case --
// an old meeting whose recording is already gone or lives only on another
// device -- says `has_local_audio: 0` and means it (Mitschnitt-Fork F18).
function mockCleanupRows(
  sessions: Array<{
    id: string;
    retention_from: string;
    has_words: number;
    transcript_processing?: number;
    has_local_audio?: number;
  }>,
  logicallyDeleted: Array<{ session_id: string }> = [],
) {
  const rows = sessions.map((session) => ({
    has_local_audio: 1,
    ...session,
  }));
  mocks.execute.mockImplementation((sql: string) =>
    Promise.resolve(
      sql.includes("SELECT DISTINCT attachment.session_id")
        ? logicallyDeleted
        : rows,
    ),
  );
}

/**
 * The cleanup pass on an installation whose deadline is armed.
 *
 * Mitschnitt-Fork (F18). Every test below this line is about WHICH recordings
 * the deadline takes, not about whether anybody agreed to it taking any -- so
 * they say "armed" out loud rather than relying on a default. The default is
 * the other way round on purpose (see `cleanupExpiredAudio`), which is why the
 * flag has to be spelled out here.
 */
async function cleanupArmed(
  policy: Parameters<typeof cleanupExpiredAudio>[0],
  nowMs = Date.now(),
) {
  const { deletedSessionIds } = await cleanupExpiredAudio(policy, nowMs, true);
  return deletedSessionIds;
}

describe("audio retention", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.cleanupDeletedSessionAudio.mockResolvedValue(true);
    mocks.deleteLocalSessionAudio.mockResolvedValue(true);
    mocks.getSessionMode.mockReturnValue("inactive");
    mocks.live.loading = false;
    mocks.live.sessionId = null;
    mockCleanupRows([]);
  });

  test("normalizes current and legacy values", () => {
    expect(normalizeAudioRetention("none")).toBe("none");
    expect(normalizeAudioRetention("oneWeek")).toBe("oneWeek");
    expect(normalizeAudioRetention("forever")).toBe("forever");
    expect(normalizeAudioRetention(false)).toBe("none");
    expect(normalizeAudioRetention(true)).toBe("forever");
    expect(normalizeAudioRetention("invalid")).toBe("forever");
    expect(normalizeAudioRetention("invalid", undefined)).toBeUndefined();
  });

  test("applies each retention window", () => {
    const now = Date.parse("2026-05-13T00:00:00.000Z");

    expect(sessionAudioExpired("not-a-date", "none", now)).toBe(true);
    expect(
      sessionAudioExpired("2026-01-01T00:00:00.000Z", "forever", now),
    ).toBe(false);
    expect(sessionAudioExpired("2026-05-11T23:59:59.999Z", "oneDay", now)).toBe(
      true,
    );
    expect(sessionAudioExpired("2026-05-12T00:00:00.001Z", "oneDay", now)).toBe(
      false,
    );
    expect(sessionAudioExpired("not-a-date", "oneDay", now)).toBe(false);
  });

  test("deletes only expired inactive SQLite sessions", async () => {
    mockCleanupRows([
      {
        id: "expired",
        retention_from: "2026-05-11T23:59:59.999Z",
        has_words: 1,
      },
      {
        id: "fresh",
        retention_from: "2026-05-12T00:00:00.001Z",
        has_words: 1,
      },
      {
        id: "active",
        retention_from: "2026-05-11T23:59:59.999Z",
        has_words: 1,
      },
    ]);
    mocks.getSessionMode.mockImplementation((sessionId) =>
      sessionId === "active" ? "active" : "inactive",
    );

    const deleted = await cleanupArmed(
      "oneDay",
      Date.parse("2026-05-13T00:00:00.000Z"),
    );

    expect(mocks.deleteLocalSessionAudio).toHaveBeenCalledTimes(1);
    expect(mocks.deleteLocalSessionAudio).toHaveBeenCalledWith(
      "expired",
      expect.any(Function),
    );
    expect(deleted).toEqual(["expired"]);
  });

  test("retention none keeps audio until transcript words exist", async () => {
    mockCleanupRows([
      {
        id: "unprocessed",
        retention_from: "2026-05-13T00:00:00.000Z",
        has_words: 0,
      },
      {
        id: "processed",
        retention_from: "2026-05-13T00:00:00.000Z",
        has_words: 1,
      },
    ]);

    await expect(
      cleanupArmed("none", Date.parse("2026-05-13T00:00:00.000Z")),
    ).resolves.toEqual(["processed"]);
    expect(mocks.deleteLocalSessionAudio).toHaveBeenCalledWith(
      "processed",
      expect.any(Function),
    );
  });

  test("keeps source audio while transcription is still processing", async () => {
    mockCleanupRows([
      {
        id: "partial",
        retention_from: "2026-05-01T00:00:00.000Z",
        has_words: 1,
        transcript_processing: 1,
      },
    ]);

    await expect(
      cleanupArmed("none", Date.parse("2026-05-13T00:00:00.000Z")),
    ).resolves.toEqual([]);
    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
  });

  test("deletes processed audio immediately when retention is none", async () => {
    mocks.execute.mockResolvedValueOnce([{ has_words: 1 }]);
    const listener = vi.fn();
    const unsubscribe = subscribeToSessionAudioRetention(listener);

    await expect(
      deleteProcessedAudioForRetention("none", "processed"),
    ).resolves.toBe(true);
    expect(mocks.deleteLocalSessionAudio).toHaveBeenCalledWith(
      "processed",
      expect.any(Function),
    );
    expect(listener).toHaveBeenNthCalledWith(1, {
      phase: "deleting",
      sessionId: "processed",
    });
    expect(listener).toHaveBeenNthCalledWith(2, {
      phase: "deleted",
      sessionId: "processed",
    });
    unsubscribe();
  });

  test("keeps unprocessed audio when retention is none", async () => {
    mocks.execute.mockResolvedValueOnce([{ has_words: 0 }]);

    await expect(
      deleteProcessedAudioForRetention("none", "unprocessed"),
    ).resolves.toBe(false);
    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
  });

  test("keeps partially persisted audio when retention is none", async () => {
    mocks.execute.mockResolvedValueOnce([
      { has_words: 1, transcript_processing: 1 },
    ]);

    await expect(
      deleteProcessedAudioForRetention("none", "partial"),
    ).resolves.toBe(false);
    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
  });

  test("skips immediate deletion for retained audio", async () => {
    await expect(
      deleteProcessedAudioForRetention("oneDay", "processed"),
    ).resolves.toBe(false);
    expect(mocks.execute).not.toHaveBeenCalled();
    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
  });

  test("only scans logical deletions when retention is forever", async () => {
    await expect(cleanupArmed("forever")).resolves.toEqual([]);
    expect(mocks.execute).toHaveBeenCalledTimes(1);
    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
  });

  test("does not report failed audio deletions as deleted", async () => {
    mockCleanupRows([
      {
        id: "expired",
        retention_from: "2026-05-01T00:00:00.000Z",
        has_words: 1,
      },
    ]);
    mocks.deleteLocalSessionAudio.mockRejectedValueOnce(
      new Error("disk failure"),
    );
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => {});

    await expect(
      cleanupArmed("oneDay", Date.parse("2026-05-13T00:00:00.000Z")),
    ).resolves.toEqual([]);
    expect(consoleError).toHaveBeenCalled();
    consoleError.mockRestore();
  });

  test("does not report a device without local audio as deleted", async () => {
    mockCleanupRows([
      {
        id: "remote-only",
        retention_from: "2026-05-01T00:00:00.000Z",
        has_words: 1,
      },
    ]);
    mocks.deleteLocalSessionAudio.mockResolvedValueOnce(false);

    await expect(
      cleanupArmed("oneDay", Date.parse("2026-05-13T00:00:00.000Z")),
    ).resolves.toEqual([]);
  });

  test("retries local cleanup for logically deleted audio", async () => {
    mockCleanupRows([], [{ session_id: "deleted-session" }]);

    await expect(cleanupArmed("forever")).resolves.toEqual(["deleted-session"]);
    expect(mocks.cleanupDeletedSessionAudio).toHaveBeenCalledWith(
      "deleted-session",
      expect.any(Function),
    );
    expect(mocks.execute.mock.calls[0]![0]).toContain(
      "COALESCE(local.availability, 'present') != 'absent'",
    );
  });

  test("does not clean a remote tombstone while the session is recording", async () => {
    mockCleanupRows([], [{ session_id: "active-session" }]);
    mocks.getSessionMode.mockReturnValue("active");

    await expect(cleanupArmed("forever")).resolves.toEqual([]);
    expect(mocks.cleanupDeletedSessionAudio).not.toHaveBeenCalled();
  });

  test("does not clean audio while capture startup is loading", async () => {
    mockCleanupRows([], [{ session_id: "starting-session" }]);
    mocks.live.sessionId = "starting-session";
    mocks.live.loading = true;

    await expect(cleanupArmed("forever")).resolves.toEqual([]);
    expect(mocks.cleanupDeletedSessionAudio).not.toHaveBeenCalled();
  });
});

// Mitschnitt-Fork. The steps a person can choose (Entscheid 01.09.2026) and what
// they mean. Six months is what a machine that has never been asked does; the
// older, shorter steps stay valid so a machine already set to one keeps meaning
// what it said.
describe("how long recordings are kept", () => {
  test("offers thirty days, three months, six months, a year, or never", () => {
    expect([...OFFERED_AUDIO_RETENTION]).toEqual([
      "none",
      "thirtyDays",
      "threeMonths",
      "sixMonths",
      "oneYear",
      "forever",
    ]);
    expect(DEFAULT_AUDIO_RETENTION).toBe("sixMonths");
    expect(AUDIO_RETENTION_DURATION_MS.sixMonths).toBe(
      182 * 24 * 60 * 60 * 1000,
    );
  });

  // Shortening is a deletion, so the picker has to know which direction a
  // change goes before it applies one silently.
  test("knows which way a change goes", () => {
    expect(shortensAudioRetention("forever", "sixMonths")).toBe(true);
    expect(shortensAudioRetention("sixMonths", "thirtyDays")).toBe(true);
    expect(shortensAudioRetention("thirtyDays", "none")).toBe(true);
    expect(shortensAudioRetention("thirtyDays", "sixMonths")).toBe(false);
    expect(shortensAudioRetention("sixMonths", "forever")).toBe(false);
    expect(shortensAudioRetention("sixMonths", "sixMonths")).toBe(false);
  });

  // Found by the second-look review, and it is the door straight through F18.
  // A fresh laptop writes six months over an empty library; a restore drops
  // three years of meetings in; the person opens the settings, sees "6 months",
  // picks "1 year" because it feels safer. Measured against the box that is not
  // a shortening, so no count and no warning -- and picking arms the deadline,
  // so the F18 question never appears either. Measured against what the
  // deadline is DOING, which on an unarmed installation is nothing, it is the
  // moment deleting starts.
  test("on an unarmed deadline even a longer span starts deleting", () => {
    expect(retentionChangeDeletes("sixMonths", "oneYear", false)).toBe(true);
    expect(retentionChangeDeletes("sixMonths", "forever", false)).toBe(false);
    expect(retentionChangeDeletes("sixMonths", "thirtyDays", false)).toBe(true);
  });

  // The control: once armed, the box IS what the deadline does, and a longer
  // span takes nothing. Without this the rule above would pass on a warning
  // that fires at every single change.
  test("once armed, a longer span still takes nothing", () => {
    expect(retentionChangeDeletes("sixMonths", "oneYear", true)).toBe(false);
    expect(retentionChangeDeletes("sixMonths", "thirtyDays", true)).toBe(true);
    expect(retentionChangeDeletes("forever", "sixMonths", true)).toBe(true);
  });

  test("keeps the steps it no longer offers readable", () => {
    // Dropping them would read as "unknown" and fall back to the default,
    // silently moving somebody from one day to six months.
    for (const legacy of ["oneDay", "threeDays", "oneWeek", "oneMonth"]) {
      expect(normalizeAudioRetention(legacy)).toBe(legacy);
    }
  });

  test("a recording older than the chosen span has expired, a younger one has not", () => {
    const now = Date.parse("2026-09-01T00:00:00Z");
    const sevenMonthsAgo = "2026-02-01T00:00:00Z";
    const oneMonthAgo = "2026-08-01T00:00:00Z";

    expect(sessionAudioExpired(sevenMonthsAgo, "sixMonths", now)).toBe(true);
    expect(sessionAudioExpired(oneMonthAgo, "sixMonths", now)).toBe(false);
  });

  test("never delete means nothing expires, however old", () => {
    expect(
      sessionAudioExpired("2016-01-01T00:00:00Z", "forever", Date.now()),
    ).toBe(false);
  });
});

// Mitschnitt-Fork (F17). The deadline reaches the meeting folder as well as the
// machine room, so the value it runs on is a licence to delete. A machine that
// has not written one down has not given that licence, and `null` says so --
// which is not the same as "forever" and very much not the same as the default.
describe("an installation that has not written down its retention", () => {
  // Its own reset. This block sits outside the suite above, so without it the
  // tests inherit that suite's last state -- a `deleteLocalSessionAudio` still
  // resolving true, an un-cleared call log -- and the control test below would
  // pass on somebody else's setup rather than on its own.
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.cleanupDeletedSessionAudio.mockResolvedValue(true);
    mocks.deleteLocalSessionAudio.mockResolvedValue(true);
    mocks.getSessionMode.mockReturnValue("inactive");
    mocks.live.loading = false;
    mocks.live.sessionId = null;
    mockCleanupRows([]);
  });

  // The stored value decides, never the resolved one. `useConfigValue` would
  // hand back the six-month default here, and that default is exactly what must
  // not reach a library nobody was asked about.
  test("is what an unanswered machine reports to the cleanup", () => {
    expect(retentionForCleanup(undefined, false)).toBeNull();
    expect(retentionForCleanup("thirtyDays", true)).toBe("thirtyDays");
    expect(retentionForCleanup(false, true)).toBe("none");
  });

  test("deletes nothing at all, and does not even go looking", async () => {
    mockCleanupRows([
      {
        id: "s1",
        retention_from: "2024-01-01T00:00:00Z",
        has_words: 1,
        transcript_processing: 0,
      },
    ]);

    const deleted = await cleanupArmed(null);

    expect(deleted).toEqual([]);
    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
    // The expiry sweep must not run at all. Asserting only "nothing was
    // deleted" would pass on a sweep whose arithmetic happens to come out
    // empty, which is an accident and not a guard.
    for (const [sql] of mocks.execute.mock.calls) {
      expect(String(sql)).toContain("SELECT DISTINCT attachment.session_id");
    }
  });

  // The other half: a deletion a person already asked for was answered, so it
  // still finishes. Otherwise an un-migrated machine would leave half-deleted
  // recordings on disk for ever.
  test("still finishes the deletions a person asked for", async () => {
    mockCleanupRows([], [{ session_id: "s9" }]);

    await expect(cleanupArmed(null)).resolves.toEqual(["s9"]);
    expect(mocks.cleanupDeletedSessionAudio).toHaveBeenCalledWith(
      "s9",
      expect.any(Function),
    );
  });

  // The control: the same library under a written six-month deadline does get
  // cleared. Without this the test above would pass on a cleanup that never
  // deletes anything.
  test("but the same library under a written deadline is cleared", async () => {
    mockCleanupRows([
      {
        id: "s1",
        retention_from: "2024-01-01T00:00:00Z",
        has_words: 1,
        transcript_processing: 0,
      },
    ]);

    await expect(cleanupArmed("sixMonths")).resolves.toEqual(["s1"]);
  });
});

// Mitschnitt-Fork (F18). The hole F17 left open on purpose: a library that
// arrives AFTER the deadline was written down. The startup answer was honest
// about the machine it saw -- empty, so six months costs nothing -- and then a
// Time Machine restore, a copied database or a new device in a cloud sync drops
// a year of meetings into it. A minute later the pass would take them.
//
// So the first pass that would actually delete does not delete: it reports what
// it found and waits for one answer. Which is the safe direction in both
// directions at once -- it cannot overrule a choice (a choice arms the
// deadline) and it cannot clear a library nobody was asked about.
describe("an installation whose deadline has never been armed", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.cleanupDeletedSessionAudio.mockResolvedValue(true);
    mocks.deleteLocalSessionAudio.mockResolvedValue(true);
    mocks.getSessionMode.mockReturnValue("inactive");
    mocks.live.loading = false;
    mocks.live.sessionId = null;
    mockCleanupRows([]);
  });

  const restoredLibrary = [
    { id: "alt-1", retention_from: "2024-01-01T00:00:00Z", has_words: 1 },
    { id: "alt-2", retention_from: "2024-06-01T00:00:00Z", has_words: 1 },
    { id: "neu", retention_from: "2026-08-25T00:00:00Z", has_words: 1 },
  ];
  const now = Date.parse("2026-09-01T00:00:00Z");

  // (a) The whole point. Nothing is deleted, and the question carries the two
  // things an answer needs: which deadline, and how many recordings it takes.
  test("deletes nothing and asks once, with the deadline and the count", async () => {
    mockCleanupRows(restoredLibrary);

    const result = await cleanupExpiredAudio("sixMonths", now, false);

    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
    expect(result.deletedSessionIds).toEqual([]);
    expect(result.awaitingConfirmation).toEqual({
      policy: "sixMonths",
      expiring: 2,
    });
  });

  // (b) The control, and the other half of (a): the same library, the same
  // deadline, once somebody has said yes. Without this, (a) would pass on a
  // cleanup that never deletes anything at all.
  test("clears exactly those recordings once the deadline is armed", async () => {
    mockCleanupRows(restoredLibrary);

    const result = await cleanupExpiredAudio("sixMonths", now, true);

    expect(result.deletedSessionIds).toEqual(["alt-1", "alt-2"]);
    expect(result.awaitingConfirmation).toBeNull();
    // The list of what came back is not the same claim as what was asked for:
    // a mocked delete that resolves true would fill the list either way.
    expect(mocks.deleteLocalSessionAudio.mock.calls.map(([id]) => id)).toEqual([
      "alt-1",
      "alt-2",
    ]);
  });

  // (e) A question about nothing is worse than no question: it teaches people
  // to click past the one time it matters. A fresh install runs for months
  // before its first recording reaches the deadline, and stays silent.
  test("says nothing at all while the deadline still catches nothing", async () => {
    mockCleanupRows([
      { id: "neu", retention_from: "2026-08-25T00:00:00Z", has_words: 1 },
    ]);

    const result = await cleanupExpiredAudio("sixMonths", now, false);

    expect(result.awaitingConfirmation).toBeNull();
    expect(result.deletedSessionIds).toEqual([]);
  });

  // Waiting for an answer must not strand a deletion somebody already asked
  // for by hand. Those were answered; only the deadline is in question.
  test("still finishes the deletions a person asked for while it waits", async () => {
    mockCleanupRows(restoredLibrary, [{ session_id: "von-hand-geloescht" }]);

    const result = await cleanupExpiredAudio("sixMonths", now, false);

    expect(result.deletedSessionIds).toEqual(["von-hand-geloescht"]);
    expect(result.awaitingConfirmation?.expiring).toBe(2);
    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
  });

  // The number in the question is the number the pass was about to delete, not
  // the number of old rows. A recording the cleanup would skip anyway must not
  // inflate it -- a count that reads high is a count nobody can act on.
  test("counts what it would take, not what merely looks old", async () => {
    mockCleanupRows(restoredLibrary);
    mocks.getSessionMode.mockImplementation((sessionId) =>
      sessionId === "alt-2" ? "active" : "inactive",
    );

    const result = await cleanupExpiredAudio("sixMonths", now, false);

    expect(result.awaitingConfirmation).toEqual({
      policy: "sixMonths",
      expiring: 1,
    });
  });

  // Found by the cross-vendor audit. The sweep used to ignore whether a
  // recording was still here, because the answer only ever fed a delete list
  // and `deleteLocalSessionAudio` quietly returned false on the empty ones.
  // The moment the same set became a NUMBER shown to a person, that shortcut
  // turned into a lie: a restored database with a thousand old meetings and
  // fifty recordings on disk would have asked about a thousand.
  test("does not count a meeting whose recording is no longer here", async () => {
    mockCleanupRows([
      {
        id: "nur-transkript",
        retention_from: "2024-01-01T00:00:00Z",
        has_words: 1,
        has_local_audio: 0,
      },
      { id: "echt", retention_from: "2024-01-01T00:00:00Z", has_words: 1 },
    ]);

    const result = await cleanupExpiredAudio("sixMonths", now, false);

    expect(result.awaitingConfirmation).toEqual({
      policy: "sixMonths",
      expiring: 1,
    });
  });

  // The same row, once armed: it is not merely uncounted, it is not touched.
  test("does not even try to delete a recording that is no longer here", async () => {
    mockCleanupRows([
      {
        id: "nur-transkript",
        retention_from: "2024-01-01T00:00:00Z",
        has_words: 1,
        has_local_audio: 0,
      },
    ]);

    const result = await cleanupExpiredAudio("sixMonths", now, true);

    expect(result.deletedSessionIds).toEqual([]);
    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
  });

  // An unarmed deadline is a weaker statement than an unwritten one, and the
  // two must not collapse into each other: `null` skips the expiry sweep
  // entirely (F17), unarmed runs it and reports.
  test("an unwritten deadline still never even looks", async () => {
    mockCleanupRows(restoredLibrary);

    const result = await cleanupExpiredAudio(null, now, false);

    expect(result.awaitingConfirmation).toBeNull();
    // Iterating an empty call log would pass without proving anything, so the
    // log is required to be non-empty first.
    expect(mocks.execute).toHaveBeenCalled();
    for (const [sql] of mocks.execute.mock.calls) {
      expect(String(sql)).toContain("SELECT DISTINCT attachment.session_id");
    }
  });

  // Forgetting the flag must land on the cheap failure. This is the one place
  // the default is load-bearing, so it is asserted rather than assumed.
  test("keeps recordings when a caller forgets to say whether it is armed", async () => {
    mockCleanupRows(restoredLibrary);

    const result = await cleanupExpiredAudio("sixMonths", now);

    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
    expect(result.awaitingConfirmation?.expiring).toBe(2);
  });
});

describe("imported recordings and the retention deadline", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.cleanupDeletedSessionAudio.mockResolvedValue(true);
    mocks.deleteLocalSessionAudio.mockResolvedValue(true);
    mocks.getSessionMode.mockReturnValue("inactive");
    mocks.live.loading = false;
    mocks.live.sessionId = null;
    mockCleanupRows([]);
  });

  // Beide Tests zusammen, sonst beweist keiner etwas: der erste prueft, dass
  // der Filter die Ankunftszeit benutzt, der zweite dass die Abfrage sie
  // ueberhaupt liefert. Ohne den zweiten liefe der erste gegen eine Spalte,
  // die es in der echten Abfrage nicht gibt.
  test("die Frist laeuft ab dem Import, nicht ab der alten Aufnahme", async () => {
    // Ein Gespraech von 2024, heute importiert. Genau der Fall aus dem
    // Bestand: created_at traegt 2024, die Datei kam gerade erst an.
    mockCleanupRows([
      {
        id: "importiert",
        retention_from: "2026-09-01T00:00:00Z",
        has_words: 1,
      },
    ]);

    const result = await cleanupExpiredAudio(
      "sixMonths",
      Date.parse("2026-09-02T00:00:00Z"),
      true,
    );

    expect(mocks.deleteLocalSessionAudio).not.toHaveBeenCalled();
    expect(result.deletedSessionIds).toEqual([]);
  });

  test("die Abfrage holt die Ankunftszeit aus der Herkunft der Sitzung", async () => {
    mockCleanupRows([]);
    await cleanupExpiredAudio("sixMonths", Date.now(), true);

    const calls = mocks.execute.mock.calls;
    const sql = calls[calls.length - 1]![0] as string;
    expect(sql).toContain("$.import.imported_at");
    expect(sql).toContain("AS retention_from");
  });
});
