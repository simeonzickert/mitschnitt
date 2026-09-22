import { describe, expect, it } from "vitest";

import { canResumeSession } from "./resume-condition";

describe("canResumeSession", () => {
  it("refuses while already actively recording", () => {
    expect(
      canResumeSession({ sessionMode: "active", hasContent: true }),
    ).toBe(false);
  });

  it("refuses a running batch with nothing to resume into (a pure import)", () => {
    expect(
      canResumeSession({ sessionMode: "running_batch", hasContent: false }),
    ).toBe(false);
  });

  it("allows a running batch that is repairing existing content", () => {
    expect(
      canResumeSession({ sessionMode: "running_batch", hasContent: true }),
    ).toBe(true);
  });

  it("allows finalizing regardless of content", () => {
    expect(
      canResumeSession({ sessionMode: "finalizing", hasContent: false }),
    ).toBe(true);
  });

  it("allows an inactive session regardless of content", () => {
    expect(
      canResumeSession({ sessionMode: "inactive", hasContent: false }),
    ).toBe(true);
  });
});
