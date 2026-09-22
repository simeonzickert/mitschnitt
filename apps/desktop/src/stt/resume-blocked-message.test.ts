import { describe, expect, it } from "vitest";

import { resumeBlockedMessage } from "./resume-blocked-message";

const REASONS = [
  "session_active",
  "session_finalizing",
  "post_stop_processing",
  "running_batch",
  "another_session_active",
  "start_in_progress",
  "start_failed",
] as const;

describe("resumeBlockedMessage", () => {
  it("gives start_failed its own exact sentence", () => {
    expect(resumeBlockedMessage("start_failed")).toBe(
      "Mitschnitt could not resume recording. Please try again.",
    );
  });

  it("falls back to a generic sentence for a missing reason", () => {
    expect(resumeBlockedMessage(null)).toBe(
      "Mitschnitt could not resume recording right now.",
    );
  });

  // A Set-size check would only say "something collided" without saying
  // which -- pairwise "differs from" per reason names the exact pair a
  // failure points at (Fix-Runde B6).
  it.each(REASONS)(
    "gives %s a sentence that differs from every other reason",
    (reason) => {
      const message = resumeBlockedMessage(reason);
      for (const other of REASONS) {
        if (other === reason) {
          continue;
        }
        expect(message).not.toBe(resumeBlockedMessage(other));
      }
    },
  );
});
