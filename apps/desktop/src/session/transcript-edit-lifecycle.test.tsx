import { cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  useEndEditModeHotkeys,
  useEndEditModeWhenLocked,
} from "./transcript-edit-lifecycle";
import { isTranscriptEditableState } from "./transcript-editable";

const mocks = vi.hoisted(() => ({ editable: true }));

vi.mock("~/session/transcript-editable", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./transcript-editable")>()),
  useTranscriptEditable: () => mocks.editable,
}));

describe("useEndEditModeWhenLocked", () => {
  beforeEach(() => {
    mocks.editable = true;
  });

  it("ends the edit mode when the transcript locks while editing (inactive -> running_batch)", () => {
    const endEditMode = vi.fn();
    const view = renderHook(
      ({ editMode }) =>
        useEndEditModeWhenLocked({ sessionId: "s", editMode, endEditMode }),
      { initialProps: { editMode: true } },
    );
    expect(endEditMode).not.toHaveBeenCalled();

    mocks.editable = false;
    view.rerender({ editMode: true });
    expect(endEditMode).toHaveBeenCalledTimes(1);
  });

  it("leaves a closed edit mode alone", () => {
    const endEditMode = vi.fn();
    mocks.editable = false;
    renderHook(() =>
      useEndEditModeWhenLocked({
        sessionId: "s",
        editMode: false,
        endEditMode,
      }),
    );
    expect(endEditMode).not.toHaveBeenCalled();
  });
});

describe("isTranscriptEditableState", () => {
  const state = (
    mode: string,
    live: Record<string, Record<string, boolean>> = {},
  ) =>
    ({
      getSessionMode: () => mode,
      live,
    }) as unknown as Parameters<typeof isTranscriptEditableState>[0];

  it("is editable only when inactive, past post-stop processing and without a pending batch", () => {
    expect(isTranscriptEditableState(state("inactive"), "s")).toBe(true);
    expect(
      isTranscriptEditableState(
        state("inactive", { postStopProcessingBySession: { s: true } }),
        "s",
      ),
    ).toBe(false);
    expect(
      isTranscriptEditableState(
        state("inactive", { batchTranscriptionPendingBySession: { s: true } }),
        "s",
      ),
    ).toBe(false);
    expect(isTranscriptEditableState(state("running_batch"), "s")).toBe(false);
    expect(isTranscriptEditableState(state("active"), "s")).toBe(false);
  });
});

describe("useEndEditModeHotkeys", () => {
  afterEach(cleanup);

  const press = (key: string, extra: KeyboardEventInit = {}) => {
    const event = new KeyboardEvent("keydown", {
      key,
      code: key === "Escape" ? "Escape" : "KeyS",
      bubbles: true,
      cancelable: true,
      ...extra,
    });
    document.dispatchEvent(event);
    return event;
  };

  it("ends the edit mode on Escape and claims the event, so the global Escape does not navigate back or close the window", () => {
    const endEditMode = vi.fn();
    renderHook(() => useEndEditModeHotkeys({ enabled: true, endEditMode }));
    const event = press("Escape");
    expect(endEditMode).toHaveBeenCalledTimes(1);
    // shared/useMainShortcuts looks at this after a tick: shouldSkipEscapeShortcut
    expect(event.defaultPrevented).toBe(true);
  });

  it("saves and ends with mod+s", () => {
    const endEditMode = vi.fn();
    renderHook(() => useEndEditModeHotkeys({ enabled: true, endEditMode }));
    const event = press("s", { metaKey: true });
    expect(endEditMode).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
  });

  it("does nothing outside the edit mode", () => {
    const endEditMode = vi.fn();
    renderHook(() => useEndEditModeHotkeys({ enabled: false, endEditMode }));
    const event = press("Escape");
    expect(endEditMode).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });
});
