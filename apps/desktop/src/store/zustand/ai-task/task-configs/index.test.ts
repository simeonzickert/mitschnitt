import type { LanguageModel } from "ai";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { TASK_CONFIGS, type TaskConfig } from ".";

const mocks = vi.hoisted(() => ({
  runEnhanceSuccess: vi.fn(),
}));

vi.mock("./enhance-success", () => ({
  enhanceSuccess: {},
  runEnhanceSuccess: mocks.runEnhanceSuccess,
}));

vi.mock("./enhance-transform", () => ({
  enhanceTransform: {},
}));

vi.mock("./enhance-workflow", () => ({
  enhanceWorkflow: {},
}));

vi.mock("./title-success", () => ({
  titleSuccess: {},
}));

vi.mock("./title-transform", () => ({
  titleTransform: {},
}));

vi.mock("./title-workflow", () => ({
  titleWorkflow: {},
}));

type EnhanceSuccessParams = Parameters<
  NonNullable<TaskConfig<"enhance">["onSuccess"]>
>[0];

function createParams(): EnhanceSuccessParams {
  return {
    taskId: "note-1-enhance",
    text: "# Summary",
    model: {} as LanguageModel,
    args: {
      sessionId: "session-1",
      enhancedNoteId: "note-1",
      templateId: undefined,
    },
    transformedArgs: {} as EnhanceSuccessParams["transformedArgs"],
    signal: new AbortController().signal,
    startTask: vi.fn().mockResolvedValue(undefined),
    getTaskState: vi.fn().mockReturnValue(undefined),
  };
}

describe("enhance task config", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("propagates a failing enhance-success run", async () => {
    const error = new Error("title write failed");
    mocks.runEnhanceSuccess.mockImplementationOnce(async () => {
      throw error;
    });

    await expect(
      TASK_CONFIGS.enhance.onSuccess?.(createParams()),
    ).rejects.toThrow(error);

    expect(mocks.runEnhanceSuccess).toHaveBeenCalledOnce();
  });
});
