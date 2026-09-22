import { render, waitFor } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  emitTo: vi.fn(),
  listen: vi.fn(),
  controlHandler: undefined as
    | ((event: { payload: unknown }) => void)
    | undefined,
  startListening: vi.fn(),
  resumeAfterStop: vi.fn(),
  stop: vi.fn(),
  liveState: {
    loading: false,
    status: "inactive" as "inactive" | "active" | "finalizing",
    sessionId: null as string | null,
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  emitTo: mocks.emitTo,
  listen: mocks.listen,
}));

vi.mock("@anlg/plugin-windows", () => ({
  getCurrentWebviewWindowLabel: () => "main",
}));

vi.mock("./useStartListening", () => ({
  useStartListening: () => mocks.startListening,
  useResumeAfterStop: () => mocks.resumeAfterStop,
}));

vi.mock("./contexts", () => ({
  useListener: (selector: (state: { stop: typeof mocks.stop }) => unknown) =>
    selector({ stop: mocks.stop }),
}));

vi.mock("~/store/zustand/listener/instance", () => ({
  listenerStore: {
    getState: () => ({ live: mocks.liveState }),
  },
}));

import {
  MainListenerControlBridge,
  requestMainListenerControl,
} from "./window-control";

beforeEach(() => {
  vi.clearAllMocks();
  mocks.controlHandler = undefined;
  mocks.liveState = { loading: false, status: "inactive", sessionId: null };
  mocks.emitTo.mockResolvedValue(undefined);
  mocks.listen.mockImplementation(
    async (_event: string, handler: (event: { payload: unknown }) => void) => {
      mocks.controlHandler = handler;
      return vi.fn();
    },
  );
  mocks.startListening.mockResolvedValue(undefined);
  mocks.resumeAfterStop.mockResolvedValue({ status: "started" });
});

test("a resume request from a secondary window carries the action verbatim", async () => {
  await requestMainListenerControl("resume", "session-1");

  expect(mocks.emitTo).toHaveBeenCalledWith(
    "main",
    "anlg:listener-control",
    expect.objectContaining({ action: "resume", sessionId: "session-1" }),
  );
});

test("the main window runner resumes after stop instead of plain start", async () => {
  render(<MainListenerControlBridge />);
  await waitFor(() => {
    expect(mocks.controlHandler).toBeTypeOf("function");
  });

  mocks.controlHandler?.({
    payload: {
      action: "resume",
      requestId: "request-1",
      sessionId: "session-1",
    },
  });

  await waitFor(() => {
    expect(mocks.resumeAfterStop).toHaveBeenCalledOnce();
  });
  expect(mocks.startListening).not.toHaveBeenCalled();
});

test("a runner that throws still advances the request queue", async () => {
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
  mocks.resumeAfterStop.mockRejectedValue(new Error("resume exploded"));

  render(<MainListenerControlBridge />);
  await waitFor(() => {
    expect(mocks.controlHandler).toBeTypeOf("function");
  });

  mocks.controlHandler?.({
    payload: {
      action: "resume",
      requestId: "request-fails",
      sessionId: "session-1",
    },
  });
  await waitFor(() => {
    expect(mocks.resumeAfterStop).toHaveBeenCalledOnce();
  });

  mocks.controlHandler?.({
    payload: {
      action: "start",
      requestId: "request-after",
      sessionId: "session-2",
    },
  });

  await waitFor(() => {
    expect(mocks.startListening).toHaveBeenCalledOnce();
  });
  consoleError.mockRestore();
});

test("a resume request is ignored while that session is already recording", async () => {
  mocks.liveState = {
    loading: false,
    status: "active",
    sessionId: "session-1",
  };

  render(<MainListenerControlBridge />);
  await waitFor(() => {
    expect(mocks.controlHandler).toBeTypeOf("function");
  });

  mocks.controlHandler?.({
    payload: {
      action: "resume",
      requestId: "request-2",
      sessionId: "session-1",
    },
  });

  await waitFor(() => {
    expect(mocks.listen).toHaveBeenCalledOnce();
  });
  expect(mocks.resumeAfterStop).not.toHaveBeenCalled();
});
