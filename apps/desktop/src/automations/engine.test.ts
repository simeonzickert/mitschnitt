import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  exportMeetingMarkdown: vi.fn(),
  getStoredSettingValues: vi.fn(),
  setSettingValue: vi.fn(),
  execute: vi.fn(),
}));

vi.mock("@anlg/plugin-local-api", () => ({
  commands: { exportMeetingMarkdown: mocks.exportMeetingMarkdown },
}));

vi.mock("~/settings/queries", () => ({
  getStoredSettingValues: mocks.getStoredSettingValues,
  setSettingValue: mocks.setSettingValue,
}));

vi.mock("~/db", () => ({
  liveQueryClient: { execute: mocks.execute },
}));

import {
  parseAutomationRunRecord,
  parseAutomationTargetRef,
  runMeetingCompletedAutomations,
  runNoteEnhancedAutomations,
} from "./engine";

function storedSettings(values: Record<string, unknown>) {
  mocks.getStoredSettingValues.mockResolvedValue({
    values,
    hasValues: new Set(Object.keys(values)),
  });
}

function recordedRun(settingKey: string) {
  const calls = mocks.setSettingValue.mock.calls.filter(
    (entry) => entry[0] === settingKey,
  );
  const call = calls[calls.length - 1];
  return call ? parseAutomationRunRecord(call[1] as string) : null;
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.setSettingValue.mockResolvedValue(undefined);
});

describe("runMeetingCompletedAutomations (markdown export)", () => {
  it("does nothing while the automation is disabled", async () => {
    storedSettings({
      automation_markdown_export_enabled: false,
      automation_markdown_export_directory: "/exports",
    });

    await runMeetingCompletedAutomations("session-1");

    expect(mocks.exportMeetingMarkdown).not.toHaveBeenCalled();
    expect(mocks.setSettingValue).not.toHaveBeenCalled();
  });

  it("exports the meeting and records a successful run", async () => {
    storedSettings({
      automation_markdown_export_enabled: true,
      automation_markdown_export_directory: "/exports",
    });
    mocks.exportMeetingMarkdown.mockResolvedValue({
      status: "ok",
      data: "/exports/2026-08-07 Standup [abc123].md",
    });

    await runMeetingCompletedAutomations("session-1");

    expect(mocks.exportMeetingMarkdown).toHaveBeenCalledWith(
      "session-1",
      "/exports",
    );
    expect(recordedRun("automation_markdown_export_last_run")).toMatchObject({
      status: "success",
      detail: "/exports/2026-08-07 Standup [abc123].md",
    });
  });

  it("records a failed run when the export command errors", async () => {
    storedSettings({
      automation_markdown_export_enabled: true,
      automation_markdown_export_directory: "/exports",
    });
    mocks.exportMeetingMarkdown.mockResolvedValue({
      status: "error",
      error: "could not write markdown export: denied",
    });

    await runMeetingCompletedAutomations("session-1");

    expect(recordedRun("automation_markdown_export_last_run")).toMatchObject({
      status: "error",
      detail: "could not write markdown export: denied",
    });
  });
});

describe("parsers", () => {
  it("round-trips run records and rejects malformed values", () => {
    const record = {
      at: "2026-08-07T12:00:00.000Z",
      status: "success",
      detail: "/exports/file.md",
    };
    expect(parseAutomationRunRecord(JSON.stringify(record))).toEqual(record);
    expect(parseAutomationRunRecord(undefined)).toBeNull();
    expect(parseAutomationRunRecord("{broken")).toBeNull();
    expect(parseAutomationRunRecord('{"status":"success"}')).toBeNull();
  });

  it("parses target refs and rejects malformed values", () => {
    expect(parseAutomationTargetRef('{"id":"C1","name":"general"}')).toEqual({
      id: "C1",
      name: "general",
    });
    expect(parseAutomationTargetRef(undefined)).toBeNull();
    expect(parseAutomationTargetRef('{"id":"C1"}')).toBeNull();
    expect(parseAutomationTargetRef("{broken")).toBeNull();
  });
});

describe("custom workflows", () => {
  it("runs an enabled markdown-export workflow after a summary is ready", async () => {
    storedSettings({
      automation_workflows: JSON.stringify([
        {
          id: "wf-1",
          title: "Export to disk",
          enabled: true,
          trigger: "note_enhanced",
          steps: [
            { id: "step-1", type: "markdown_export", directory: "/exports" },
          ],
          lastRun: null,
          processedSessionIds: [],
          chatGroupId: null,
        },
      ]),
    });
    mocks.exportMeetingMarkdown.mockResolvedValue({
      status: "ok",
      data: "/exports/session-1.md",
    });

    await runNoteEnhancedAutomations("session-1");

    expect(mocks.exportMeetingMarkdown).toHaveBeenCalledWith(
      "session-1",
      "/exports",
    );
    const workflowCalls = mocks.setSettingValue.mock.calls.filter(
      (entry) => entry[0] === "automation_workflows",
    );
    const saved = JSON.parse(
      workflowCalls[workflowCalls.length - 1]?.[1] as string,
    );
    expect(saved[0].lastRun.status).toBe("success");
    expect(saved[0].processedSessionIds).toEqual(["session-1"]);
  });

  it("marks a session processed after a successful step so a later failure does not retry", async () => {
    storedSettings({
      automation_workflows: JSON.stringify([
        {
          id: "wf-1",
          title: "Two exports",
          enabled: true,
          trigger: "note_enhanced",
          steps: [
            { id: "step-1", type: "markdown_export", directory: "/exports" },
            { id: "step-2", type: "markdown_export", directory: "" },
          ],
          lastRun: null,
          processedSessionIds: [],
          chatGroupId: null,
        },
      ]),
    });
    mocks.exportMeetingMarkdown.mockResolvedValue({
      status: "ok",
      data: "/exports/session-1.md",
    });

    await runNoteEnhancedAutomations("session-1");

    expect(mocks.exportMeetingMarkdown).toHaveBeenCalledTimes(1);
    const firstSave = mocks.setSettingValue.mock.calls.filter(
      (entry) => entry[0] === "automation_workflows",
    );
    const afterFailure = JSON.parse(
      firstSave[firstSave.length - 1]?.[1] as string,
    );
    expect(afterFailure[0].processedSessionIds).toEqual(["session-1"]);
    expect(afterFailure[0].lastRun.status).toBe("error");

    storedSettings({ automation_workflows: JSON.stringify(afterFailure) });
    mocks.exportMeetingMarkdown.mockClear();

    await runNoteEnhancedAutomations("session-1");

    expect(mocks.exportMeetingMarkdown).not.toHaveBeenCalled();
  });

  it("retries a workflow when the first step fails before any side effect", async () => {
    const workflow = {
      id: "wf-1",
      title: "Export to disk",
      enabled: true,
      trigger: "note_enhanced",
      steps: [{ id: "step-1", type: "markdown_export", directory: "/exports" }],
      lastRun: null,
      processedSessionIds: [],
      chatGroupId: null,
    };
    storedSettings({ automation_workflows: JSON.stringify([workflow]) });
    mocks.exportMeetingMarkdown.mockResolvedValue({
      status: "error",
      error: "denied",
    });

    await runNoteEnhancedAutomations("session-1");

    const firstSave = mocks.setSettingValue.mock.calls.filter(
      (entry) => entry[0] === "automation_workflows",
    );
    const afterFailure = JSON.parse(
      firstSave[firstSave.length - 1]?.[1] as string,
    );
    expect(afterFailure[0].processedSessionIds).toEqual([]);
    expect(afterFailure[0].lastRun.status).toBe("error");

    storedSettings({ automation_workflows: JSON.stringify(afterFailure) });
    mocks.exportMeetingMarkdown.mockClear();
    mocks.exportMeetingMarkdown.mockResolvedValue({
      status: "ok",
      data: "/exports/session-1.md",
    });

    await runNoteEnhancedAutomations("session-1");

    expect(mocks.exportMeetingMarkdown).toHaveBeenCalledTimes(1);
  });

  it("skips disabled or already processed workflows", async () => {
    storedSettings({
      automation_workflows: JSON.stringify([
        {
          id: "wf-1",
          title: "Disabled",
          enabled: false,
          trigger: "note_enhanced",
          steps: [
            { id: "step-1", type: "markdown_export", directory: "/exports" },
          ],
          processedSessionIds: [],
        },
        {
          id: "wf-2",
          title: "Already ran",
          enabled: true,
          trigger: "note_enhanced",
          steps: [
            { id: "step-1", type: "markdown_export", directory: "/exports" },
          ],
          processedSessionIds: ["session-1"],
        },
      ]),
    });

    await runNoteEnhancedAutomations("session-1");

    expect(mocks.exportMeetingMarkdown).not.toHaveBeenCalled();
  });
});
