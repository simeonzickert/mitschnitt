import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Listening } from "./listening";

const {
  isMainWebviewWindowMock,
  requestMainListenerControlMock,
  startListeningMock,
  resumeAfterStopMock,
  sonnerToastErrorMock,
  stopMock,
  useListenerMock,
} = vi.hoisted(() => ({
  isMainWebviewWindowMock: vi.fn(() => true),
  requestMainListenerControlMock: vi.fn(),
  startListeningMock: vi.fn(),
  resumeAfterStopMock: vi.fn(),
  sonnerToastErrorMock: vi.fn(),
  stopMock: vi.fn(),
  useListenerMock: vi.fn(),
}));

vi.mock("@anlg/ui/components/ui/dropdown-menu", () => ({
  DropdownMenuItem: ({
    children,
    ...props
  }: React.ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button type="button" {...props}>
      {children}
    </button>
  ),
}));

vi.mock("@anlg/ui/components/ui/toast", () => ({
  sonnerToast: {
    error: sonnerToastErrorMock,
  },
}));

vi.mock("~/stt/contexts", () => ({
  useListener: useListenerMock,
}));

vi.mock("~/stt/useStartListening", () => ({
  useStartListening: () => startListeningMock,
  useResumeAfterStop: () => resumeAfterStopMock,
}));

vi.mock("~/stt/window-control", () => ({
  isMainWebviewWindow: isMainWebviewWindowMock,
  requestMainListenerControl: requestMainListenerControlMock,
}));

describe("Listening", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    isMainWebviewWindowMock.mockReturnValue(true);
    resumeAfterStopMock.mockResolvedValue({ status: "started" });
    useListenerMock.mockImplementation((selector) =>
      selector({
        getSessionMode: () => "inactive",
        stop: stopMock,
      }),
    );
  });

  afterEach(() => {
    cleanup();
  });

  it("resumes listening (not a plain start) when the session already has content", async () => {
    render(<Listening sessionId="session-1" resume />);

    fireEvent.click(screen.getByRole("button", { name: "Resume listening" }));

    await vi.waitFor(() => {
      expect(resumeAfterStopMock).toHaveBeenCalledTimes(1);
    });
    expect(startListeningMock).not.toHaveBeenCalled();
  });

  it("starts listening directly in the main window before transcript exists", () => {
    render(<Listening sessionId="session-1" resume={false} />);

    fireEvent.click(screen.getByRole("button", { name: "Start listening" }));

    expect(startListeningMock).toHaveBeenCalledTimes(1);
    expect(requestMainListenerControlMock).not.toHaveBeenCalled();
  });

  it("delegates standalone start requests to the main window before transcript exists", () => {
    isMainWebviewWindowMock.mockReturnValue(false);

    render(<Listening sessionId="session-1" resume={false} />);

    fireEvent.click(screen.getByRole("button", { name: "Start listening" }));

    expect(requestMainListenerControlMock).toHaveBeenCalledWith(
      "start",
      "session-1",
    );
    expect(startListeningMock).not.toHaveBeenCalled();
  });

  it("delegates standalone resume requests to the main window when the session has content", async () => {
    isMainWebviewWindowMock.mockReturnValue(false);

    render(<Listening sessionId="session-1" resume />);

    fireEvent.click(screen.getByRole("button", { name: "Resume listening" }));

    await vi.waitFor(() => {
      expect(requestMainListenerControlMock).toHaveBeenCalledWith(
        "resume",
        "session-1",
      );
    });
    expect(resumeAfterStopMock).not.toHaveBeenCalled();
  });

  it("delegates standalone stop requests to the main window", () => {
    isMainWebviewWindowMock.mockReturnValue(false);
    useListenerMock.mockImplementation((selector) =>
      selector({
        getSessionMode: () => "active",
        stop: stopMock,
      }),
    );

    render(<Listening sessionId="session-1" resume />);

    fireEvent.click(screen.getByRole("button", { name: "Stop listening" }));

    expect(requestMainListenerControlMock).toHaveBeenCalledWith(
      "stop",
      "session-1",
    );
    expect(stopMock).not.toHaveBeenCalled();
  });

  it("keeps resume enabled and clickable while the session is finalizing", async () => {
    useListenerMock.mockImplementation((selector) =>
      selector({
        getSessionMode: () => "finalizing",
        stop: stopMock,
      }),
    );

    render(<Listening sessionId="session-1" resume />);

    const button = screen.getByRole("button", { name: "Resume listening" });
    expect(button.hasAttribute("disabled")).toBe(false);

    fireEvent.click(button);

    await vi.waitFor(() => {
      expect(resumeAfterStopMock).toHaveBeenCalledTimes(1);
    });
  });

  it("keeps resume enabled during a running batch when the session already has content", async () => {
    useListenerMock.mockImplementation((selector) =>
      selector({
        getSessionMode: () => "running_batch",
        stop: stopMock,
      }),
    );

    render(<Listening sessionId="session-1" resume />);

    const button = screen.getByRole("button", { name: "Resume listening" });
    expect(button.hasAttribute("disabled")).toBe(false);

    fireEvent.click(button);

    await vi.waitFor(() => {
      expect(resumeAfterStopMock).toHaveBeenCalledTimes(1);
    });
  });

  it("stays a disabled dead end during a running batch with no content to resume", () => {
    useListenerMock.mockImplementation((selector) =>
      selector({
        getSessionMode: () => "running_batch",
        stop: stopMock,
      }),
    );

    render(<Listening sessionId="session-1" resume={false} />);

    const button = screen.getByRole("button", { name: "Batch processing" });
    expect(button.hasAttribute("disabled")).toBe(true);

    fireEvent.click(button);

    expect(resumeAfterStopMock).not.toHaveBeenCalled();
    expect(startListeningMock).not.toHaveBeenCalled();
  });

  it("shows a toast when resuming from the overflow menu is blocked", async () => {
    resumeAfterStopMock.mockResolvedValue({
      status: "blocked",
      reason: "start_failed",
    });

    render(<Listening sessionId="session-1" resume />);

    fireEvent.click(screen.getByRole("button", { name: "Resume listening" }));

    await vi.waitFor(() => {
      expect(sonnerToastErrorMock).toHaveBeenCalledWith(
        "Mitschnitt could not resume recording. Please try again.",
        { id: "resume-after-stop-blocked" },
      );
    });
  });
});
