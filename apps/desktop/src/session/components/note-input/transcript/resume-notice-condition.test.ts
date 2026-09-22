import { describe, expect, it } from "vitest";

import { shouldShowTranscriptResumeNotice } from "./resume-notice-condition";

const base = {
  audioExists: true,
  audioExistsResolved: true,
  postStopProcessing: false,
  sessionMode: "inactive" as const,
  transcriptCount: 2,
};

describe("shouldShowTranscriptResumeNotice", () => {
  it("shows with more than one transcript, resolved audio, inactive, no post-stop lease", () => {
    expect(shouldShowTranscriptResumeNotice(base)).toBe(true);
  });

  it("stays hidden with only one transcript", () => {
    expect(
      shouldShowTranscriptResumeNotice({ ...base, transcriptCount: 1 }),
    ).toBe(false);
  });

  it("stays hidden with no transcripts at all", () => {
    expect(
      shouldShowTranscriptResumeNotice({ ...base, transcriptCount: 0 }),
    ).toBe(false);
  });

  it("stays hidden without audio, even with a transcript already on disk", () => {
    // useRegenerateTranscript needs the audio file; offering the button
    // without one would just produce a "Recording not found" toast.
    expect(
      shouldShowTranscriptResumeNotice({ ...base, audioExists: false }),
    ).toBe(false);
  });

  it("stays hidden while the audio-exists query has not resolved yet", () => {
    expect(
      shouldShowTranscriptResumeNotice({ ...base, audioExistsResolved: false }),
    ).toBe(false);
  });

  it("stays hidden while the post-stop lease is still held", () => {
    expect(
      shouldShowTranscriptResumeNotice({ ...base, postStopProcessing: true }),
    ).toBe(false);
  });

  it.each(["active", "finalizing", "running_batch"] as const)(
    "stays hidden while the session is not inactive (%s)",
    (sessionMode) => {
      expect(
        shouldShowTranscriptResumeNotice({ ...base, sessionMode }),
      ).toBe(false);
    },
  );
});
