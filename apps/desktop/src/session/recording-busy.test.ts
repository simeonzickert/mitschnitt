import { describe, expect, it } from "vitest";

import { isRecordingBusyState } from "./recording-busy";

type State = Parameters<typeof isRecordingBusyState>[0];

const state = (mode: string, live: Record<string, unknown> = {}) =>
  ({
    getSessionMode: () => mode,
    live: { sessionId: null, loading: false, ...live },
  }) as unknown as State;

describe("isRecordingBusyState", () => {
  it("treats an idle session as deletable", () => {
    expect(isRecordingBusyState(state("inactive"), "s")).toBe(false);
  });

  it("blocks deleting a just-stopped session during post-stop processing (Fix-Runde B8)", () => {
    expect(
      isRecordingBusyState(
        state("inactive", { postStopProcessingBySession: { s: true } }),
        "s",
      ),
    ).toBe(true);
  });

  it("blocks deleting a session with a pending batch (Fix-Runde B8)", () => {
    expect(
      isRecordingBusyState(
        state("inactive", { batchTranscriptionPendingBySession: { s: true } }),
        "s",
      ),
    ).toBe(true);
  });

  it("only blocks the session that is busy", () => {
    expect(
      isRecordingBusyState(
        state("inactive", { postStopProcessingBySession: { other: true } }),
        "s",
      ),
    ).toBe(false);
  });

  it.each(["active", "finalizing", "running_batch"])(
    "keeps blocking while %s",
    (mode) => {
      expect(isRecordingBusyState(state(mode), "s")).toBe(true);
    },
  );
});
