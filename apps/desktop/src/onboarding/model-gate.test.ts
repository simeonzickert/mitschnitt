import { describe, expect, test } from "vitest";

import {
  canFinishOnboarding,
  canFinishOnboardingWithModels,
  getModelGateState,
  getOnboardingGateState,
  type ModelGateInput,
  shouldStartLlmDownload,
  shouldStartModelDownload,
  shouldStartSttDownload,
} from "./model-gate";

function input(overrides: Partial<ModelGateInput> = {}): ModelGateInput {
  return {
    isDownloaded: false,
    isDownloadedLoading: false,
    isDownloading: false,
    progress: 0,
    errorMessage: null,
    retryCount: 0,
    cloudProviderReady: false,
    ...overrides,
  };
}

describe("getModelGateState / shouldStartModelDownload", () => {
  test("fresh render while the download-check is still loading is checking, and does not start a download", () => {
    const state = getModelGateState(input({ isDownloadedLoading: true }));

    expect(state).toEqual({ kind: "checking" });
    expect(shouldStartModelDownload(input({ isDownloadedLoading: true }))).toBe(false);
  });

  test("check finished, nothing there yet is idle, and a download may start", () => {
    const state = getModelGateState(input());

    expect(state).toEqual({ kind: "idle" });
    expect(shouldStartModelDownload(input())).toBe(true);
  });

  test("download in progress at 42% is downloading, does not restart, and does not finish onboarding", () => {
    const state = getModelGateState(input({ isDownloading: true, progress: 42 }));

    expect(state).toEqual({ kind: "downloading", progress: 42 });
    expect(shouldStartModelDownload(input({ isDownloading: true, progress: 42 }))).toBe(false);
    expect(canFinishOnboarding(state)).toBe(false);
  });

  test("progress above 100 is clamped to 100", () => {
    const state = getModelGateState(input({ isDownloading: true, progress: 130 }));

    expect(state).toEqual({ kind: "downloading", progress: 100 });
  });

  test("progress below 0 is clamped to 0", () => {
    const state = getModelGateState(input({ isDownloading: true, progress: -5 }));

    expect(state).toEqual({ kind: "downloading", progress: 0 });
  });

  test("model already downloaded is ready/model, and onboarding can finish", () => {
    const state = getModelGateState(input({ isDownloaded: true }));

    expect(state).toEqual({ kind: "ready", reason: "model" });
    expect(canFinishOnboarding(state)).toBe(true);
  });

  test("model already downloaded never starts a second download on a fresh start or import", () => {
    expect(shouldStartModelDownload(input({ isDownloaded: true }))).toBe(false);
  });

  test("error with retryCount 0 is error, not exhausted, blocks onboarding, and does not auto-retry", () => {
    const state = getModelGateState(input({ errorMessage: "Download fehlgeschlagen", retryCount: 0 }));

    expect(state).toEqual({
      kind: "error",
      message: "Download fehlgeschlagen",
      retryCount: 0,
      exhausted: false,
    });
    expect(canFinishOnboarding(state)).toBe(false);
    expect(
      shouldStartModelDownload(input({ errorMessage: "Download fehlgeschlagen", retryCount: 0 })),
    ).toBe(false);
  });

  test("error with retryCount at the max is exhausted, but still does not finish onboarding", () => {
    const state = getModelGateState(input({ errorMessage: "Download fehlgeschlagen", retryCount: 3 }));

    expect(state).toEqual({
      kind: "error",
      message: "Download fehlgeschlagen",
      retryCount: 3,
      exhausted: true,
    });
    expect(canFinishOnboarding(state)).toBe(false);
  });

  test("cloud provider ready wins even with an error pending", () => {
    const state = getModelGateState(
      input({ cloudProviderReady: true, errorMessage: "Download fehlgeschlagen", retryCount: 3 }),
    );

    expect(state).toEqual({ kind: "ready", reason: "cloud" });
    expect(canFinishOnboarding(state)).toBe(true);
  });

  test("cloud provider ready wins even while a download is in progress", () => {
    const state = getModelGateState(
      input({ cloudProviderReady: true, isDownloading: true, progress: 50 }),
    );

    expect(state).toEqual({ kind: "ready", reason: "cloud" });
    expect(canFinishOnboarding(state)).toBe(true);
  });

  test("cloud provider ready never starts a model download nobody needs", () => {
    expect(shouldStartModelDownload(input({ cloudProviderReady: true }))).toBe(false);
  });
});

describe("getOnboardingGateState / canFinishOnboardingWithModels / shouldStartLlmDownload", () => {
  test("both empty: stt is active, stt may start, llm must wait", () => {
    const state = getOnboardingGateState(input(), input());

    expect(state.active).toBe("stt");
    expect(shouldStartSttDownload(input())).toBe(true);
    expect(shouldStartLlmDownload(input(), input())).toBe(false);
  });

  test("stt ready, llm empty: llm is active and may start", () => {
    const stt = input({ isDownloaded: true });
    const llm = input();
    const state = getOnboardingGateState(stt, llm);

    expect(state.active).toBe("llm");
    expect(shouldStartLlmDownload(stt, llm)).toBe(true);
  });

  test("stt still downloading, llm empty: llm must not start yet, the order holds", () => {
    const stt = input({ isDownloading: true, progress: 10 });
    const llm = input();

    expect(shouldStartLlmDownload(stt, llm)).toBe(false);
  });

  test("stt ready, llm downloading: onboarding cannot finish yet", () => {
    const state = getOnboardingGateState(
      input({ isDownloaded: true }),
      input({ isDownloading: true, progress: 30 }),
    );

    expect(canFinishOnboardingWithModels(state)).toBe(false);
  });

  test("both ready: nothing is active, onboarding can finish", () => {
    const state = getOnboardingGateState(
      input({ isDownloaded: true }),
      input({ isDownloaded: true }),
    );

    expect(state.active).toBeNull();
    expect(canFinishOnboardingWithModels(state)).toBe(true);
  });

  test("both ready via a cloud provider each: the shortcut resolves on its own", () => {
    const state = getOnboardingGateState(
      input({ cloudProviderReady: true }),
      input({ cloudProviderReady: true }),
    );

    expect(canFinishOnboardingWithModels(state)).toBe(true);
  });

  test("stt ready, llm in error: onboarding cannot finish, llm is still active", () => {
    const state = getOnboardingGateState(
      input({ isDownloaded: true }),
      input({ errorMessage: "Download fehlgeschlagen", retryCount: 1 }),
    );

    expect(canFinishOnboardingWithModels(state)).toBe(false);
    expect(state.active).toBe("llm");
  });
});
