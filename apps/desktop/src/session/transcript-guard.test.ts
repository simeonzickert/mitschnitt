import { afterEach, describe, expect, it, vi } from "vitest";

import { guardUserTranscriptMutation } from "./transcript-editable";

import { listenerStore } from "~/store/zustand/listener/instance";

const toastError = vi.hoisted(() => vi.fn());

vi.mock("@anlg/ui/components/ui/toast", () => ({
  sonnerToast: { error: toastError },
}));

const lock = (live: Record<string, unknown>) =>
  listenerStore.setState((state) => ({
    live: { ...state.live, ...live },
  }));

describe("guardUserTranscriptMutation", () => {
  afterEach(() => {
    lock({
      postStopProcessingBySession: {},
      batchTranscriptionPendingBySession: {},
    });
    toastError.mockClear();
  });

  it("runs the mutation on an idle session", async () => {
    const mutation = vi.fn(async () => 42);
    await expect(guardUserTranscriptMutation("s", mutation)).resolves.toEqual({
      allowed: true,
      value: 42,
    });
    expect(mutation).toHaveBeenCalledTimes(1);
  });

  it.each(["postStopProcessingBySession", "batchTranscriptionPendingBySession"])(
    "blocks the mutation and says why while %s is set",
    async (key) => {
      lock({ [key]: { s: true } });
      const mutation = vi.fn(async () => 42);
      await expect(guardUserTranscriptMutation("s", mutation)).resolves.toEqual(
        { allowed: false },
      );
      expect(mutation).not.toHaveBeenCalled();
      expect(toastError).toHaveBeenCalledTimes(1);
    },
  );

  it("only blocks the locked session", async () => {
    lock({ postStopProcessingBySession: { other: true } });
    const mutation = vi.fn(async () => 1);
    await guardUserTranscriptMutation("s", mutation);
    expect(mutation).toHaveBeenCalledTimes(1);
  });
});
