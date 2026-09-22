import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  mirrorSessionFolder: vi.fn(),
  openPath: vi.fn(),
  sessionDir: vi.fn(),
}));

vi.mock("~/types/tauri.gen", () => ({
  commands: { mirrorSessionFolder: mocks.mirrorSessionFolder },
}));

vi.mock("@anlg/plugin-opener2", () => ({
  commands: { openPath: mocks.openPath },
}));

// Present only so a regression that reaches for the machine room again is
// caught here rather than in Finder.
vi.mock("@anlg/plugin-fs-sync", () => ({
  commands: { sessionDir: mocks.sessionDir },
}));

import { openMeetingFolder } from "./meeting-folder";

beforeEach(() => {
  vi.clearAllMocks();
  mocks.openPath.mockResolvedValue({ status: "ok", data: null });
});

describe("opening a meeting's folder", () => {
  it("opens the readable folder, not the session id folder", async () => {
    mocks.mirrorSessionFolder.mockResolvedValue({
      status: "ok",
      data: "/Users/mads/Mitschnitt/2026-05-05_standup_e5f6a7b8",
    });

    await expect(openMeetingFolder("e5f6a7b8")).resolves.toEqual({
      status: "opened",
    });
    expect(mocks.openPath).toHaveBeenCalledWith(
      "/Users/mads/Mitschnitt/2026-05-05_standup_e5f6a7b8",
      null,
    );
    expect(mocks.sessionDir).not.toHaveBeenCalled();
  });

  // Falsifier 2, at the level where it is decidable in the frontend: a
  // fallback would fire exactly in the confusing case and hand the person a
  // folder named after a uuid with no transcript in it.
  it("opens nothing at all when the folder has not been written yet", async () => {
    mocks.mirrorSessionFolder.mockResolvedValue({ status: "ok", data: null });

    await expect(openMeetingFolder("e5f6a7b8")).resolves.toEqual({
      status: "not-yet",
    });
    expect(mocks.openPath).not.toHaveBeenCalled();
    expect(mocks.sessionDir).not.toHaveBeenCalled();
  });

  it("reports a lookup failure instead of opening something else", async () => {
    mocks.mirrorSessionFolder.mockResolvedValue({
      status: "error",
      error: "the database is not open",
    });

    await expect(openMeetingFolder("e5f6a7b8")).resolves.toEqual({
      status: "failed",
      error: "the database is not open",
    });
    expect(mocks.openPath).not.toHaveBeenCalled();
  });

  it("reports a failure to open the folder", async () => {
    mocks.mirrorSessionFolder.mockResolvedValue({ status: "ok", data: "/weg" });
    mocks.openPath.mockResolvedValue({
      status: "error",
      error: "no such path",
    });

    await expect(openMeetingFolder("e5f6a7b8")).resolves.toEqual({
      status: "failed",
      error: "no such path",
    });
  });
});
