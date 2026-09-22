import { describe, expect, test, vi } from "vitest";

import type { ParticipantSyncSnapshot } from "../../storage";
import { syncSessionParticipants } from "./sync";

vi.mock("~/shared/utils", () => ({
  id: () => "human-new",
}));

function createSnapshot(
  overrides: Partial<ParticipantSyncSnapshot> = {},
): ParticipantSyncSnapshot {
  return {
    sessions: [],
    humans: [],
    mappings: [],
    ...overrides,
  };
}

const session = {
  id: "session-1",
  ownerUserId: "user-1",
  eventJson: JSON.stringify({ tracking_id: "tracking-1" }),
  trackingId: "tracking-1",
};

describe("syncSessionParticipants", () => {
  test("returns empty output when no events are provided", () => {
    const result = syncSessionParticipants({
      incomingParticipants: new Map(),
      snapshot: createSnapshot(),
    });

    expect(result.toAdd).toEqual([]);
    expect(result.toDelete).toEqual([]);
    expect(result.humansToCreate).toEqual([]);
  });

  test("skips events without an associated session", () => {
    const result = syncSessionParticipants({
      incomingParticipants: new Map([
        ["tracking-1", [{ email: "test@example.com", name: "Test" }]],
      ]),
      snapshot: createSnapshot(),
    });

    expect(result.toAdd).toEqual([]);
    expect(result.humansToCreate).toEqual([]);
  });

  test("creates a human when the participant email is new", () => {
    const result = syncSessionParticipants({
      incomingParticipants: new Map([
        ["tracking-1", [{ email: "new@example.com", name: "New Person" }]],
      ]),
      snapshot: createSnapshot({ sessions: [session] }),
    });

    expect(result.humansToCreate).toEqual([
      {
        id: "human-new",
        ownerUserId: "user-1",
        email: "new@example.com",
        name: "New Person",
      },
    ]);
    expect(result.toAdd).toEqual([
      {
        sessionId: "session-1",
        humanId: "human-new",
        email: "new@example.com",
      },
    ]);
  });

  test("uses an existing human when email matches case-insensitively", () => {
    const result = syncSessionParticipants({
      incomingParticipants: new Map([
        ["tracking-1", [{ email: "Existing@Example.com", name: "Existing" }]],
      ]),
      snapshot: createSnapshot({
        sessions: [session],
        humans: [{ id: "human-1", email: "existing@example.com" }],
      }),
    });

    expect(result.humansToCreate).toEqual([]);
    expect(result.toAdd[0]).toMatchObject({ humanId: "human-1" });
  });

  test("deletes auto mappings when a participant is removed", () => {
    const result = syncSessionParticipants({
      incomingParticipants: new Map([["tracking-1", []]]),
      snapshot: createSnapshot({
        sessions: [session],
        humans: [{ id: "human-1", email: "removed@example.com" }],
        mappings: [
          {
            id: "mapping-1",
            sessionId: "session-1",
            humanId: "human-1",
            source: "auto",
          },
        ],
      }),
    });

    expect(result.toDelete).toEqual(["mapping-1"]);
  });

  test("leaves the owner out so a 1:1 keeps exactly one remote", () => {
    const result = syncSessionParticipants({
      incomingParticipants: new Map([
        [
          "tracking-1",
          [
            {
              email: "owner@example.com",
              name: "Owner",
              is_current_user: true,
            },
            { email: "tom@example.com", name: "Tom" },
          ],
        ],
      ]),
      snapshot: createSnapshot({ sessions: [session] }),
    });

    expect(result.humansToCreate).toEqual([
      {
        id: "human-new",
        ownerUserId: "user-1",
        email: "tom@example.com",
        name: "Tom",
      },
    ]);
    expect(result.toAdd).toEqual([
      {
        sessionId: "session-1",
        humanId: "human-new",
        email: "tom@example.com",
      },
    ]);
  });

  test("drops an owner copy that an earlier sync already stored", () => {
    const result = syncSessionParticipants({
      incomingParticipants: new Map([
        [
          "tracking-1",
          [
            {
              email: "owner@example.com",
              name: "Owner",
              is_current_user: true,
            },
          ],
        ],
      ]),
      snapshot: createSnapshot({
        sessions: [session],
        humans: [{ id: "human-owner-copy", email: "owner@example.com" }],
        mappings: [
          {
            id: "mapping-owner-copy",
            sessionId: "session-1",
            humanId: "human-owner-copy",
            source: "auto",
          },
        ],
      }),
    });

    expect(result.toAdd).toEqual([]);
    expect(result.toDelete).toEqual(["mapping-owner-copy"]);
  });

  test("keeps every attendee when the provider never flags the owner", () => {
    const result = syncSessionParticipants({
      incomingParticipants: new Map([
        [
          "tracking-1",
          [
            { email: "owner@example.com", name: "Owner" },
            { email: "tom@example.com", name: "Tom" },
          ],
        ],
      ]),
      snapshot: createSnapshot({
        sessions: [session],
        humans: [
          { id: "human-owner", email: "owner@example.com" },
          { id: "human-tom", email: "tom@example.com" },
        ],
      }),
    });

    expect(result.toAdd.map((entry) => entry.humanId)).toEqual([
      "human-owner",
      "human-tom",
    ]);
  });

  test("does not remove a manually added owner mapping", () => {
    const result = syncSessionParticipants({
      incomingParticipants: new Map([
        [
          "tracking-1",
          [
            {
              email: "owner@example.com",
              name: "Owner",
              is_current_user: true,
            },
          ],
        ],
      ]),
      snapshot: createSnapshot({
        sessions: [session],
        humans: [{ id: "human-owner-copy", email: "owner@example.com" }],
        mappings: [
          {
            id: "mapping-owner-copy",
            sessionId: "session-1",
            humanId: "human-owner-copy",
            source: "manual",
          },
        ],
      }),
    });

    expect(result.toDelete).toEqual([]);
  });

  test("preserves excluded mappings", () => {
    const result = syncSessionParticipants({
      incomingParticipants: new Map([["tracking-1", []]]),
      snapshot: createSnapshot({
        sessions: [session],
        humans: [{ id: "human-1", email: "excluded@example.com" }],
        mappings: [
          {
            id: "mapping-1",
            sessionId: "session-1",
            humanId: "human-1",
            source: "excluded",
          },
        ],
      }),
    });

    expect(result.toDelete).toEqual([]);
  });
});
