import { beforeEach, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  createSession: vi.fn(),
  execute: vi.fn(),
}));

vi.mock("~/db", () => ({
  liveQueryClient: { execute: mocks.execute },
}));

vi.mock("~/session/queries", () => ({
  createSession: mocks.createSession,
}));

import {
  getOrCreateWelcomeSession,
  setPendingWelcomeSession,
  takePendingWelcomeSession,
} from "./welcome-note";

beforeEach(() => {
  vi.clearAllMocks();
  const values = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    removeItem: (key: string) => values.delete(key),
    setItem: (key: string, value: string) => values.set(key, value),
  });
});

it("reuses an existing onboarding welcome note", async () => {
  mocks.execute.mockResolvedValueOnce([{ id: "welcome-session" }]);

  await expect(getOrCreateWelcomeSession()).resolves.toBe("welcome-session");
  expect(mocks.createSession).not.toHaveBeenCalled();
  expect(mocks.execute).toHaveBeenCalledWith(expect.any(String), [
    "anarlog-onboarding-demo-v1",
  ]);
});

// Entscheid (ISA N5): Link raus, Text bleibt. Die Willkommens-Sitzung
// traegt keinen meeting_link mehr -- die Adresse gehoerte dem Original.
it("creates the welcome note without any meeting link", async () => {
  mocks.execute.mockResolvedValueOnce([]);
  mocks.createSession.mockResolvedValueOnce("welcome-session");

  await expect(getOrCreateWelcomeSession()).resolves.toBe("welcome-session");

  const [title, , initial] = mocks.createSession.mock.calls[0];
  const event = JSON.parse(initial.event_json);
  expect(title).toBe("Welcome to Mitschnitt");
  expect(event.meeting_link).toBe("");
  expect(initial.event_json).not.toContain("anarlog.so");
  expect(event.tracking_id).toBe("anarlog-onboarding-demo-v1");
  // D6 (Fix-Runde 1d): the button is "Record"; there is no demo meeting.
  expect(initial.raw_md).not.toContain("prerecorded demo meeting");
  expect(initial.raw_md).not.toContain("Join & record");
  // md2json turns **Record** into a bold text node, so the words are checked
  // where the JSON keeps them.
  expect(initial.raw_md).toContain('"text":"Click "');
  expect(initial.raw_md).toContain('"text":"Record"');
  expect(initial.raw_md).toContain(
    "in the top-right corner to capture your microphone",
  );
  expect(initial.raw_md).toContain("Settings → Transcription");
  expect(initial.raw_md).toContain(
    "If transcription and intelligence are configured",
  );
  expect(initial.raw_md).not.toContain("Mitschnitt will listen, transcribe");
  // G6c (Opus, Fix-Runde 2): there is no video either -- the second
  // sentence about the removed demo.
  expect(initial.raw_md).not.toContain("When the video ends");
  expect(initial.raw_md).toContain("When you stop the recording");

  const note = JSON.parse(initial.raw_md);
  expect(note.content).toHaveLength(7);
  expect(note.content[1]).toEqual({ type: "paragraph" });
  expect(note.content[3]).toEqual({ type: "paragraph" });
  expect(note.content[5]).toEqual({ type: "paragraph" });
});

it("guards empty event metadata before reading its tracking ID", async () => {
  mocks.execute.mockResolvedValueOnce([]);
  mocks.createSession.mockResolvedValueOnce("welcome-session");

  await getOrCreateWelcomeSession();

  const [query] = mocks.execute.mock.calls[0];
  expect(query).toMatch(
    /CASE\s+WHEN json_valid\(event_json\)\s+THEN json_extract\(event_json, '\$\.tracking_id'\)\s+END = \?/,
  );
});

it("carries the welcome note across a one-time onboarding relaunch", () => {
  setPendingWelcomeSession("welcome-session");

  expect(takePendingWelcomeSession()).toBe("welcome-session");
  expect(takePendingWelcomeSession()).toBeNull();
});
