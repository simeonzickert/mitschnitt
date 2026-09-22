import { describe, expect, it } from "vitest";

import type { ImportSourceScan } from "./anarlog-contract";
import {
  assessImportSource,
  formatBytes,
  formatSessionRange,
  hasEnoughSpace,
  IMPORT_SPACE_HEADROOM_BYTES,
  importProblemMessage,
  isBlockingProblem,
} from "./anarlog-source";

const GB = 1024 * 1024 * 1024;

// Reicht die Formen des echten `i18n` fuer diesen Modul: nur `_`. Der echte
// `i18n._` setzt die Werte in die Platzhalter ein; ohne das hier faellt jede
// Meldung mit einem Wert auf ihre Textstuecke zusammen und der Test koennte den
// eingesetzten Schluessel nie sehen.
const i18n = {
  _: (descriptor: {
    message?: string;
    id: string;
    values?: Record<string, unknown>;
  }) =>
    (descriptor.message ?? descriptor.id).replace(
      /\{(\w+)\}/gu,
      (match, name: string) =>
        descriptor.values && name in descriptor.values
          ? String(descriptor.values[name])
          : match,
    ),
};

function scan(overrides: Partial<ImportSourceScan> = {}): ImportSourceScan {
  return {
    kind: "database",
    sourceRoot: "/Users/example/Library/Application Support/other-app",
    sourceApp: "anarlog",
    sessionCount: 42,
    transcriptCount: 40,
    documentCount: 12,
    audioFileCount: 20,
    audioBytes: 2 * GB,
    freeBytesOnTarget: 60 * GB,
    oldestSessionAt: "2024-03-01T09:00:00.000Z",
    newestSessionAt: "2026-08-14T16:30:00.000Z",
    problems: [],
    ...overrides,
  };
}

describe("importProblemMessage", () => {
  it("explains an empty folder instead of naming a failure", () => {
    const message = importProblemMessage(i18n, "no_data_found");

    expect(message).toContain("no conversations");
    expect(message).toContain("Choose the folder");
    expect(message).not.toContain("no_data_found");
  });

  it("names the running app as the thing to quit", () => {
    expect(importProblemMessage(i18n, "source_open")).toContain("Quit it");
  });

  it("carries an unknown key into the sentence so it can be reported", () => {
    const message = importProblemMessage(i18n, "wedged_in_a_new_way");

    expect(message).toContain("wedged_in_a_new_way");
    expect(message).toContain("does not have a name for it yet");
  });

  it("never leaves a key without a sentence", () => {
    for (const key of [
      "no_data_found",
      "source_open",
      "not_enough_space",
      "source_unreadable",
      "same_installation",
    ]) {
      expect(importProblemMessage(i18n, key).length).toBeGreaterThan(20);
    }
  });
});

describe("isBlockingProblem", () => {
  it("lets the space problem go once the audio is left behind", () => {
    expect(isBlockingProblem("not_enough_space", true)).toBe(true);
    expect(isBlockingProblem("not_enough_space", false)).toBe(false);
  });

  it("blocks on a key this version does not know", () => {
    expect(isBlockingProblem("something_new", false)).toBe(true);
  });

  it("keeps blocking a locked source even without audio", () => {
    expect(isBlockingProblem("source_open", false)).toBe(true);
  });
});

describe("hasEnoughSpace", () => {
  it("demands headroom beyond the audio itself", () => {
    const tight = scan({
      audioBytes: 10 * GB,
      freeBytesOnTarget: 10 * GB + IMPORT_SPACE_HEADROOM_BYTES - 1,
    });

    expect(hasEnoughSpace(tight, true)).toBe(false);
    expect(
      hasEnoughSpace(
        { ...tight, freeBytesOnTarget: 10 * GB + IMPORT_SPACE_HEADROOM_BYTES },
        true,
      ),
    ).toBe(true);
  });

  it("stops counting the audio when it is not copied", () => {
    const tight = scan({ audioBytes: 40 * GB, freeBytesOnTarget: 2 * GB });

    expect(hasEnoughSpace(tight, true)).toBe(false);
    expect(hasEnoughSpace(tight, false)).toBe(true);
  });
});

describe("assessImportSource", () => {
  it("clears a healthy source", () => {
    const verdict = assessImportSource(scan(), true);

    expect(verdict.canImport).toBe(true);
    expect(verdict.blockingProblems).toEqual([]);
    expect(verdict.outOfSpace).toBe(false);
  });

  it("refuses the start when the disk is too small for the audio", () => {
    const verdict = assessImportSource(
      scan({ audioBytes: 80 * GB, freeBytesOnTarget: 4 * GB }),
      true,
    );

    expect(verdict.canImport).toBe(false);
    expect(verdict.outOfSpace).toBe(true);
  });

  it("opens the way again once the audio is left behind", () => {
    const tooBig = scan({
      audioBytes: 80 * GB,
      freeBytesOnTarget: 4 * GB,
      problems: ["not_enough_space"],
    });

    expect(assessImportSource(tooBig, true).canImport).toBe(false);
    const withoutAudio = assessImportSource(tooBig, false);
    expect(withoutAudio.canImport).toBe(true);
    expect(withoutAudio.blockingProblems).toEqual([]);
    expect(withoutAudio.advisoryProblems).toEqual(["not_enough_space"]);
  });

  it("refuses a running source no matter what else is fine", () => {
    const verdict = assessImportSource(
      scan({ problems: ["source_open"] }),
      false,
    );

    expect(verdict.canImport).toBe(false);
    expect(verdict.blockingProblems).toEqual(["source_open"]);
  });

  // Der Scan koennte kind: "none" melden und die problems-Liste leer lassen.
  // Dann darf die Oberflaeche trotzdem nicht importieren.
  it("refuses an empty source even when no problem key was set", () => {
    expect(
      assessImportSource(
        scan({ kind: "none", sessionCount: 0, problems: [] }),
        true,
      ).canImport,
    ).toBe(false);
    expect(
      assessImportSource(scan({ sessionCount: 0, problems: [] }), true)
        .canImport,
    ).toBe(false);
  });
});

describe("formatBytes", () => {
  it("reaches gigabytes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatBytes(7 * GB)).toBe("7.0 GB");
    expect(formatBytes(1500 * GB)).toBe("1500.0 GB");
  });
});

describe("formatSessionRange", () => {
  it("collapses a single day into one date", () => {
    expect(
      formatSessionRange(
        {
          oldestSessionAt: "2026-08-14T08:00:00.000Z",
          newestSessionAt: "2026-08-14T20:00:00.000Z",
        },
        "en-US",
      ),
    ).toBe("Aug 14, 2026");
  });

  it("returns nothing when there are no dates at all", () => {
    expect(
      formatSessionRange(
        { oldestSessionAt: null, newestSessionAt: null },
        "en-US",
      ),
    ).toBeNull();
  });

  it("survives a date the source could not produce", () => {
    expect(
      formatSessionRange(
        { oldestSessionAt: "not a date", newestSessionAt: null },
        "en-US",
      ),
    ).toBeNull();
  });

  it("survives a locale that Intl refuses", () => {
    const dates = {
      oldestSessionAt: "2026-08-14T08:00:00.000Z",
      newestSessionAt: "2026-08-14T20:00:00.000Z",
    };

    expect(formatSessionRange(dates, "")).toBeTruthy();
    expect(formatSessionRange(dates, undefined)).toBeTruthy();
    expect(formatSessionRange(dates, "not a locale")).toBeTruthy();
  });
});
