import { describe, expect, it, vi } from "vitest";

import type { ImportRunReport } from "./anarlog-contract";
import { runImportWithProgress } from "./anarlog-run";

function report(overrides: Partial<ImportRunReport> = {}): ImportRunReport {
  return {
    runId: "run-1",
    status: "running",
    dryRun: false,
    discovered: 10,
    imported: 0,
    skipped: 0,
    conflicts: 0,
    errors: 0,
    conversationsDiscovered: 0,
    conversationsImported: 0,
    audioCopied: 0,
    audioBytesCopied: 0,
    startedAt: "2026-09-10T08:00:00.000Z",
    completedAt: null,
    error: null,
    ...overrides,
  };
}

function deps(reports: ImportRunReport[]) {
  const queue = [...reports];
  return {
    start: vi.fn(async () => "run-1"),
    read: vi.fn(async () => queue.shift() ?? reports[reports.length - 1]!),
    wait: vi.fn(async () => {}),
  };
}

describe("runImportWithProgress", () => {
  it("passes the dry-run flag and the audio choice straight through", async () => {
    const stubs = deps([report({ status: "completed" })]);

    await runImportWithProgress(
      { sourcePath: "/somewhere/else", dryRun: true, copyAudio: false },
      { deps: stubs },
    );

    expect(stubs.start).toHaveBeenCalledWith("/somewhere/else", true, false);
  });

  it("keeps asking while the run is still going and stops the moment it is not", async () => {
    const stubs = deps([
      report({ imported: 1 }),
      report({ imported: 4 }),
      report({
        status: "completed",
        imported: 9,
        completedAt: "2026-09-10T08:01:00.000Z",
      }),
    ]);
    const seen: number[] = [];

    const final = await runImportWithProgress(
      { sourcePath: "/somewhere", dryRun: false, copyAudio: true },
      { deps: stubs, onProgress: (update) => seen.push(update.imported) },
    );

    expect(stubs.read).toHaveBeenCalledTimes(3);
    expect(stubs.wait).toHaveBeenCalledTimes(2);
    expect(final.status).toBe("completed");
    // Der letzte Bericht muss ebenfalls durch onProgress gehen, sonst haengt die
    // Anzeige bei 4 von 9, waehrend daneben "fertig" steht.
    expect(seen).toEqual([1, 4, 9]);
  });

  it("stops on a failed run instead of waiting forever", async () => {
    const stubs = deps([
      report({ status: "failed", error: "the source moved away" }),
    ]);

    const final = await runImportWithProgress(
      { sourcePath: "/gone", dryRun: false, copyAudio: true },
      { deps: stubs },
    );

    expect(final.status).toBe("failed");
    expect(stubs.wait).not.toHaveBeenCalled();
  });

  it("stops on a run that finished with issues", async () => {
    const stubs = deps([
      report({ status: "completed_with_issues", errors: 2 }),
    ]);

    const final = await runImportWithProgress(
      { sourcePath: "/somewhere", dryRun: false, copyAudio: true },
      { deps: stubs },
    );

    expect(final.status).toBe("completed_with_issues");
    expect(stubs.read).toHaveBeenCalledTimes(1);
  });
});
