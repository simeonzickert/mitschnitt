import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  isModelDownloaded: vi.fn(),
  getStoredSettingValues: vi.fn(),
}));

vi.mock("@anlg/plugin-local-stt", () => ({
  commands: { isModelDownloaded: mocks.isModelDownloaded },
}));

vi.mock("~/settings/queries", () => ({
  getStoredSettingValues: mocks.getStoredSettingValues,
}));

import { checkSelectedLocalModel } from "./model";

function stored(values: Record<string, unknown>) {
  return { values, hasValues: new Set(Object.keys(values)) };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("the model follow-up after an import", () => {
  it("reports a model the machine does not have", async () => {
    // Falsifier 2: the imported setup picks an on-device model that is not
    // on this disk. Silence here is exactly the failure mode F14 forbids.
    mocks.getStoredSettingValues.mockResolvedValue(
      stored({
        current_stt_provider: "whispercpp",
        current_stt_model: "QuantizedLargeV3Turbo",
      }),
    );
    mocks.isModelDownloaded.mockResolvedValue({ status: "ok", data: false });

    expect(await checkSelectedLocalModel()).toEqual({
      state: "missing",
      model: "QuantizedLargeV3Turbo",
    });
  });

  it("stays quiet when the model is there", async () => {
    mocks.getStoredSettingValues.mockResolvedValue(
      stored({
        current_stt_provider: "soniqo",
        current_stt_model: "soniqo-parakeet-v3",
      }),
    );
    mocks.isModelDownloaded.mockResolvedValue({ status: "ok", data: true });

    expect(await checkSelectedLocalModel()).toEqual({
      state: "ready",
      model: "soniqo-parakeet-v3",
    });
  });

  it("keeps a failed check apart from a missing model", async () => {
    // A check that could not run must never come back as "missing" -- that
    // would send people downloading a model they already have -- and never as
    // "ready" either, which is the silent non-functioning.
    mocks.getStoredSettingValues.mockResolvedValue(
      stored({
        current_stt_provider: "whispercpp",
        current_stt_model: "QuantizedLargeV3Turbo",
      }),
    );
    mocks.isModelDownloaded.mockResolvedValue({
      status: "error",
      error: "model directory unreadable",
    });

    expect(await checkSelectedLocalModel()).toEqual({
      state: "unknown",
      reason: "model directory unreadable",
    });
  });

  it("names its own file model instead of falling through as not-applicable", async () => {
    // The imported setup picked its own GGML file on the source machine.
    // `local_stt_model_path` is withheld from every bundle (keys.ts), so
    // there is no path here to check and no file to offer for download --
    // silence would be exactly the non-functioning state F14 forbids.
    mocks.getStoredSettingValues.mockResolvedValue(
      stored({
        current_stt_provider: "local_file",
        current_stt_model: "local-file",
      }),
    );

    expect(await checkSelectedLocalModel()).toEqual({ state: "local-file" });
    expect(mocks.isModelDownloaded).not.toHaveBeenCalled();
  });

  it("says nothing about a cloud provider", async () => {
    mocks.getStoredSettingValues.mockResolvedValue(
      stored({
        current_stt_provider: "openai",
        current_stt_model: "whisper-1",
      }),
    );

    expect(await checkSelectedLocalModel()).toEqual({
      state: "not-applicable",
    });
    expect(mocks.isModelDownloaded).not.toHaveBeenCalled();
  });
});
