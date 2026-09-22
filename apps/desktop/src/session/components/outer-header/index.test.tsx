import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { EditorView } from "~/store/zustand/tabs/schema";

const mocks = vi.hoisted(() => ({
  leftsidebar: {
    expanded: true,
    toggleExpanded: vi.fn(),
  },
  canGoBack: false,
  canGoNext: false,
  goBack: vi.fn(),
  goNext: vi.fn(),
  sessionModes: {} as Record<string, string>,
  sessionEvents: {} as Record<string, any>,
  postStopProcessingBySession: {} as Record<string, boolean>,
  nowMs: new Date("2026-06-05T09:50:00.000Z").getTime(),
  openUrl: vi.fn(),
  startListening: vi.fn(),
  resumeAfterStop: vi.fn(),
  sonnerToastError: vi.fn(),
  stopListening: vi.fn(),
  stopTranscription: vi.fn(),
  requestMainListenerControl: vi.fn(),
  isMainWebviewWindow: true,
  audioExists: false,
  audioExistsResolved: true,
  hasTranscriptBySession: {} as Record<string, boolean>,
  configValues: {
    auto_join_scheduled_meetings: false,
    auto_start_scheduled_meetings: false,
  } as Record<string, boolean>,
  overflowProps: [] as Array<{
    allowListening?: boolean;
    standaloneWindow?: boolean;
  }>,
  windowControlsGutter: true,
  meetingAccessibilityActive: false,
}));

vi.mock("../folder-picker", () => ({
  FolderPicker: () => (
    <button type="button" role="combobox" aria-label="Select folder">
      Folder
    </button>
  ),
}));

vi.mock("./metadata", () => ({
  MetadataButton: () => (
    <button
      type="button"
      data-tauri-drag-region="false"
      aria-label="Open event metadata"
    >
      <svg aria-hidden="true" data-testid="metadata-calendar-icon" />
    </button>
  ),
}));

vi.mock("../title-input", () => ({
  TitleInput: () => <input aria-label="Session title" placeholder="Untitled" />,
}));

vi.mock("./overflow", () => ({
  OverflowButton: (props: {
    allowListening?: boolean;
    standaloneWindow?: boolean;
  }) => {
    mocks.overflowProps.push(props);
    return <button type="button">More</button>;
  },
}));

vi.mock("../shared", () => ({
  RecordingIcon: () => <div data-testid="recording-icon" />,
  useHasTranscript: (sessionId: string) =>
    mocks.hasTranscriptBySession[sessionId] ?? false,
}));

vi.mock("@anlg/ui/components/ui/spinner", () => ({
  Spinner: () => <div data-testid="spinner" />,
}));

vi.mock("@anlg/plugin-opener2", () => ({
  commands: {
    openUrl: mocks.openUrl,
  },
}));

vi.mock("~/calendar/hooks", () => ({
  useNow: () => new Date(mocks.nowMs),
}));

vi.mock("~/audio-player", () => ({
  useAudioPlayer: () => ({
    audioExists: mocks.audioExists,
    audioExistsResolved: mocks.audioExistsResolved,
  }),
}));

vi.mock("~/contexts/shell", () => ({
  useShell: () => ({
    leftsidebar: mocks.leftsidebar,
  }),
}));

vi.mock("~/session/hooks/useSessionEvent", () => ({
  useSessionEvent: (sessionId: string) =>
    mocks.sessionEvents[sessionId] ?? null,
}));

vi.mock("~/session/hooks/useMeetingAccessibilityActive", () => ({
  useMeetingAccessibilityActive: () => mocks.meetingAccessibilityActive,
}));

vi.mock("~/shared/config", () => ({
  useConfigValue: (key: string) => mocks.configValues[key],
}));

vi.mock("~/shared/hooks/useWindowControlsGutter", () => ({
  useWindowControlsGutter: () => mocks.windowControlsGutter,
}));

vi.mock("~/store/zustand/tabs", () => ({
  useTabs: vi.fn((selector: (state: unknown) => unknown) =>
    selector({
      canGoBack: mocks.canGoBack,
      canGoNext: mocks.canGoNext,
      goBack: mocks.goBack,
      goNext: mocks.goNext,
    }),
  ),
}));

vi.mock("~/stt/contexts", () => ({
  useListener: vi.fn((selector: (state: unknown) => unknown) =>
    selector({
      getSessionMode: (sessionId: string) =>
        mocks.sessionModes[sessionId] ?? "inactive",
      canStartLiveSession: (sessionId: string) =>
        (mocks.sessionModes[sessionId] ?? "inactive") === "inactive",
      stop: mocks.stopListening,
      stopTranscription: mocks.stopTranscription,
      live: {
        postStopProcessingBySession: mocks.postStopProcessingBySession,
      },
    }),
  ),
}));

vi.mock("~/stt/useStartListening", () => ({
  useStartListening: () => mocks.startListening,
  useResumeAfterStop: () => mocks.resumeAfterStop,
}));

vi.mock("~/stt/window-control", () => ({
  isMainWebviewWindow: () => mocks.isMainWebviewWindow,
  requestMainListenerControl: mocks.requestMainListenerControl,
}));

vi.mock("@anlg/ui/components/ui/toast", () => ({
  sonnerToast: {
    error: mocks.sonnerToastError,
  },
}));

import { OuterHeader } from "./index";

describe("OuterHeader", () => {
  beforeEach(() => {
    mocks.leftsidebar.expanded = true;
    mocks.leftsidebar.toggleExpanded.mockClear();
    mocks.canGoBack = false;
    mocks.canGoNext = false;
    mocks.goBack.mockClear();
    mocks.goNext.mockClear();
    mocks.sessionModes = {};
    mocks.sessionEvents = {};
    mocks.postStopProcessingBySession = {};
    mocks.nowMs = new Date("2026-06-05T09:50:00.000Z").getTime();
    mocks.openUrl.mockClear();
    mocks.startListening.mockClear();
    mocks.resumeAfterStop.mockReset();
    mocks.resumeAfterStop.mockResolvedValue({ status: "started" });
    mocks.sonnerToastError.mockClear();
    mocks.stopListening.mockClear();
    mocks.stopTranscription.mockClear();
    mocks.requestMainListenerControl.mockClear();
    mocks.isMainWebviewWindow = true;
    mocks.audioExists = false;
    mocks.audioExistsResolved = true;
    mocks.hasTranscriptBySession = {};
    mocks.configValues = {
      auto_join_scheduled_meetings: false,
      auto_start_scheduled_meetings: false,
    };
    mocks.overflowProps = [];
    mocks.windowControlsGutter = true;
    mocks.meetingAccessibilityActive = false;
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("does not show a separate stop listening button for active sessions while the sidebar is collapsed", () => {
    mocks.leftsidebar.expanded = false;
    mocks.sessionModes = { "session-1": "active" };

    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const spacer = container.firstElementChild?.firstElementChild;

    expect(screen.queryByRole("button", { name: "Stop listening" })).toBeNull();
    expect(spacer?.className).toContain("flex-1");
    expect(spacer?.className).not.toContain("right-[140px]");
  });

  it("shows a resumable pill (not join/record/stop) while a scheduled meeting finalizes", () => {
    mocks.leftsidebar.expanded = false;
    mocks.sessionModes = { "session-1": "finalizing" };
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T09:45:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };

    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const spacer = container.firstElementChild?.firstElementChild;

    // Finalizing no longer hides the pill outright (ZICK-319 Strecke B): it
    // shows Resume instead of Join & record/Record/Stop.
    expect(screen.getByRole("button", { name: "Resume" })).not.toBeNull();
    expect(screen.queryByRole("button", { name: "Join & record" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Record" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
    expect(screen.getByTestId("metadata-calendar-icon")).not.toBeNull();
    expect(spacer?.className).toContain("flex-1");
    expect(spacer?.className).not.toContain("right-[140px]");
  });

  it("uses the collapsed sidebar gutter without a title field", () => {
    mocks.leftsidebar.expanded = false;

    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const header = container.firstElementChild;
    const spacer = header?.firstElementChild;

    expect(header?.className).toContain("pl-[108px]");
    expect(header?.className).toContain("h-12");
    expect(header?.className).not.toContain("pb-1");
    expect(spacer?.className).toContain("flex-1");
    expect(spacer?.className).not.toContain("-translate-y-1");
    expect(spacer?.className).not.toContain("right-[140px]");
    expect(screen.queryByRole("button", { name: "Show sidebar" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Go back" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Go forward" })).toBeNull();
  });

  it("uses the collapsed sidebar gutter without native window controls", () => {
    mocks.leftsidebar.expanded = false;
    mocks.windowControlsGutter = false;

    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(container.firstElementChild?.className).toContain("pl-[32px]");
    expect(container.firstElementChild?.className).not.toContain("pl-[108px]");
    expect(container.firstElementChild?.className).not.toContain("pl-2");
  });

  it("does not add a title offset while the sidebar is expanded", () => {
    mocks.leftsidebar.expanded = true;

    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const spacer = container.firstElementChild?.firstElementChild;

    expect(spacer?.className).toContain("flex-1");
    expect(spacer?.className).not.toContain("right-[140px]");
    expect(spacer?.className).not.toContain("justify-center");
    expect(container.firstElementChild?.className).toContain("pl-2");
    expect(container.firstElementChild?.className).not.toContain("pl-[108px]");
    expect(container.firstElementChild?.className).not.toContain("pl-[116px]");
  });

  it("keeps sidebar header controls hidden while the sidebar is expanded", () => {
    mocks.sessionModes = { "session-1": "active" };

    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(screen.queryByRole("button", { name: "Hide sidebar" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Go back" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Go forward" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Stop listening" })).toBeNull();
    expect(container.firstElementChild?.className).toContain("pl-2");
    expect(container.firstElementChild?.className).not.toContain("pl-[108px]");
  });

  it("keeps the session header at 48px tall", () => {
    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(container.firstElementChild?.className).toContain("h-12");
  });

  it("marks the spacer and action strip as draggable", () => {
    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const header = container.firstElementChild;
    const spacer = header?.firstElementChild;
    const actionStrip = header?.lastElementChild;

    expect(header?.hasAttribute("data-tauri-drag-region")).toBe(true);
    expect(spacer?.hasAttribute("data-tauri-drag-region")).toBe(true);
    expect(actionStrip?.hasAttribute("data-tauri-drag-region")).toBe(true);
  });

  it("places the folder and calendar controls in order", () => {
    mocks.hasTranscriptBySession = { "session-1": true };

    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
        viewSwitcher={
          <div role="group" aria-label="Session note views">
            Tabs
          </div>
        }
      />,
    );

    const header = container.firstElementChild;
    const views = screen.getByRole("group", { name: "Session note views" });
    const folder = screen.getByRole("combobox", { name: "Select folder" });
    const calendar = screen.getByRole("button", {
      name: "Open event metadata",
    });
    const actionStrip = header?.lastElementChild;
    const actionChildren = [...(actionStrip?.children ?? [])];

    expect(header?.firstElementChild).toBe(views);
    expect(actionStrip?.contains(folder)).toBe(true);
    expect(actionStrip?.contains(calendar)).toBe(true);
    expect(
      actionChildren.findIndex((child) => child.contains(folder)),
    ).toBeLessThan(
      actionChildren.findIndex((child) => child.contains(calendar)),
    );
  });

  it("shows an editable title in the header on the summary tab", () => {
    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "enhanced", id: "note-1" } as EditorView}
        tab={{
          active: true,
          id: "session-1",
          pinned: false,
          slotId: "slot-1",
          state: { autoStart: null, view: { type: "enhanced", id: "note-1" } },
          type: "sessions",
        }}
      />,
    );

    const title = screen.getByRole("textbox", { name: "Session title" });

    expect(title.getAttribute("placeholder")).toBe("Untitled");
    expect(screen.queryByRole("button", { name: "Create brief" })).toBeNull();
  });

  it("hides the title input after the meeting is over", () => {
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
      },
    };
    mocks.nowMs = new Date("2026-06-05T10:31:00.000Z").getTime();

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "enhanced", id: "note-1" } as EditorView}
        tab={{
          active: true,
          id: "session-1",
          pinned: false,
          slotId: "slot-1",
          state: { autoStart: null, view: { type: "enhanced", id: "note-1" } },
          type: "sessions",
        }}
        viewSwitcher={
          <div role="group" aria-label="Session note views">
            Tabs
          </div>
        }
      />,
    );

    expect(screen.queryByRole("textbox", { name: "Session title" })).toBeNull();
    expect(screen.getByRole("group", { name: "Session note views" })).not.toBe(
      null,
    );
  });

  it("hides the title input after an ad hoc recording", () => {
    mocks.hasTranscriptBySession = { "session-1": true };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "enhanced", id: "note-1" } as EditorView}
        tab={{
          active: true,
          id: "session-1",
          pinned: false,
          slotId: "slot-1",
          state: { autoStart: null, view: { type: "enhanced", id: "note-1" } },
          type: "sessions",
        }}
      />,
    );

    expect(screen.queryByRole("textbox", { name: "Session title" })).toBeNull();
  });

  it("shows an editable title on the memo tab before recording", () => {
    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
        tab={{
          active: true,
          id: "session-1",
          pinned: false,
          slotId: "slot-1",
          state: { autoStart: null, view: { type: "raw" } },
          type: "sessions",
        }}
      />,
    );

    const title = screen.getByRole("textbox", { name: "Session title" });

    expect(title.getAttribute("placeholder")).toBe("Untitled");
  });

  it("hides the title input on the memo tab after recording", () => {
    mocks.hasTranscriptBySession = { "session-1": true };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
        tab={{
          active: true,
          id: "session-1",
          pinned: false,
          slotId: "slot-1",
          state: { autoStart: null, view: { type: "raw" } },
          type: "sessions",
        }}
      />,
    );

    expect(screen.queryByRole("textbox", { name: "Session title" })).toBeNull();
  });

  it("hides the title input on the transcript tab", () => {
    mocks.hasTranscriptBySession = { "session-1": true };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "transcript" } as EditorView}
        tab={{
          active: true,
          id: "session-1",
          pinned: false,
          slotId: "slot-1",
          state: { autoStart: null, view: { type: "transcript" } },
          type: "sessions",
        }}
      />,
    );

    expect(screen.queryByRole("textbox", { name: "Session title" })).toBeNull();
  });

  it.each(["active", "running_batch", "finalizing"] as const)(
    "hides the title input during a live meeting (%s)",
    (sessionMode) => {
      mocks.sessionModes = { "session-1": sessionMode };
      mocks.hasTranscriptBySession = { "session-1": true };

      render(
        <OuterHeader
          sessionId="session-1"
          currentView={{ type: "raw" } as EditorView}
          tab={{
            active: true,
            id: "session-1",
            pinned: false,
            slotId: "slot-1",
            state: { autoStart: null, view: { type: "raw" } },
            type: "sessions",
          }}
          viewSwitcher={
            <div role="group" aria-label="Session note views">
              Tabs
            </div>
          }
        />,
      );

      expect(
        screen.queryByRole("textbox", { name: "Session title" }),
      ).toBeNull();
      expect(
        screen.getByRole("group", { name: "Session note views" }),
      ).not.toBe(null);
    },
  );

  it("places stop immediately before the folder while listening", () => {
    mocks.sessionModes = { "session-1": "active" };

    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
        viewSwitcher={
          <div role="group" aria-label="Session note views">
            Tabs
          </div>
        }
      />,
    );

    const stop = screen.getByRole("button", { name: "Stop" });
    const folder = screen.getByRole("combobox", { name: "Select folder" });
    const actionStrip = container.firstElementChild?.lastElementChild;
    const actionChildren = [...(actionStrip?.children ?? [])];
    const stopIndex = actionChildren.findIndex((child) => child.contains(stop));
    const folderIndex = actionChildren.findIndex((child) =>
      child.contains(folder),
    );

    expect(actionStrip?.contains(stop)).toBe(true);
    expect(stopIndex).toBe(folderIndex - 1);
    expect(screen.getByRole("group", { name: "Session note views" })).not.toBe(
      stop.closest("[role='group']"),
    );
  });

  it("keeps the dedicated stop button visible while the sidebar is expanded", () => {
    mocks.sessionModes = { "session-1": "active" };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(screen.getByRole("button", { name: "Stop" })).not.toBeNull();
  });

  it("shows stop next to folder in standalone windows", () => {
    mocks.leftsidebar.expanded = true;
    mocks.sessionModes = { "session-1": "active" };

    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
        standaloneWindow
      />,
    );

    const header = container.firstElementChild;
    const stop = screen.getByRole("button", { name: "Stop" });
    const folder = screen.getByRole("combobox", { name: "Select folder" });
    const actionStrip = header?.lastElementChild;
    const actionChildren = [...(actionStrip?.children ?? [])];

    expect(header?.className).toContain("pl-[76px]");
    expect(header?.className).not.toContain("right-[153px]");
    expect(
      actionChildren.findIndex((child) => child.contains(stop)),
    ).toBeLessThan(actionChildren.findIndex((child) => child.contains(folder)));

    const overflowProps = mocks.overflowProps[mocks.overflowProps.length - 1];
    expect(overflowProps?.standaloneWindow).toBe(true);
    expect(overflowProps?.allowListening).toBeUndefined();
  });

  it("delegates live meeting stop from the header pill in standalone windows", () => {
    mocks.sessionModes = { "session-1": "active" };
    mocks.isMainWebviewWindow = false;

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
        standaloneWindow
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Stop" }));

    expect(mocks.requestMainListenerControl).toHaveBeenCalledWith(
      "stop",
      "session-1",
    );
    expect(mocks.stopListening).not.toHaveBeenCalled();
  });

  it("does not reserve collapsed sidebar gutter in standalone windows", () => {
    mocks.leftsidebar.expanded = false;

    const { container } = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
        standaloneWindow
      />,
    );

    const header = container.firstElementChild;

    expect(header?.className).not.toContain("pl-[108px]");
    expect(header?.className).toContain("pl-[76px]");
  });

  it.each([
    ["expanded", true],
    ["collapsed", false],
  ])(
    "drops the window controls inset in standalone windows without native chrome with the sidebar %s",
    (_state, expanded) => {
      mocks.leftsidebar.expanded = expanded;
      mocks.windowControlsGutter = false;

      const { container } = render(
        <OuterHeader
          sessionId="session-1"
          currentView={{ type: "raw" } as EditorView}
          standaloneWindow
        />,
      );

      const header = container.firstElementChild;

      expect(header?.className).toContain("pl-2");
      expect(header?.className).not.toContain("pl-[76px]");
    },
  );

  it("shows a join-and-record pill before a remote meeting with a video link", () => {
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };
    mocks.nowMs = new Date("2026-06-05T09:55:00.000Z").getTime();

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const joinButton = screen.getByRole("button", { name: "Join & record" });
    const metadataButton = screen.getByRole("button", {
      name: "Open event metadata",
    });

    expect(joinButton.className).toContain("bg-primary");
    expect(joinButton.className).toContain("dark:bg-white");
    expect(joinButton.className).toContain("dark:text-black");
    expect(joinButton.className).toContain("hover:bg-primary/90");
    expect(joinButton.className).toContain("dark:hover:bg-white/90");
    expect(joinButton.querySelector("img")?.className).toContain("size-3.5");
    expect(joinButton.getAttribute("aria-label")).toBe("Join & record");
    expect(joinButton.textContent).toContain("Join & record");
    expect(joinButton.getAttribute("data-tauri-drag-region")).toBe("false");
    expect(metadataButton.getAttribute("data-tauri-drag-region")).toBe("false");
    expect(joinButton.parentElement?.contains(metadataButton)).toBe(false);

    fireEvent.click(joinButton);

    expect(mocks.openUrl).toHaveBeenCalledWith(
      "https://meet.google.com/abc-defg-hij",
      null,
    );
    expect(mocks.startListening).toHaveBeenCalledTimes(1);
  });

  it("switches from join-and-record to record when accessibility sees an active meeting", () => {
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };
    mocks.meetingAccessibilityActive = true;

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(screen.queryByRole("button", { name: "Join & record" })).toBeNull();
    const recordButton = screen.getByRole("button", { name: "Record" });

    fireEvent.click(recordButton);

    expect(mocks.openUrl).not.toHaveBeenCalled();
    expect(mocks.startListening).toHaveBeenCalledOnce();
  });

  // Entscheid (ISA N5): Link raus, Text bleibt. Die Willkommens-Notiz
  // hat keinen Link mehr, also auch kein "Join & record" -- nur "Record".
  it("offers plain recording for the welcome note", () => {
    mocks.sessionEvents = {
      "session-1": {
        tracking_id: "anarlog-onboarding-demo-v1",
        meeting_link: "",
      },
    };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(screen.queryByRole("button", { name: "Join & record" })).toBeNull();
    expect(screen.queryByText("Try the demo")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Record" }));

    expect(mocks.openUrl).not.toHaveBeenCalled();
    expect(mocks.startListening).toHaveBeenCalledOnce();
  });

  // Eine Bestands-Notiz aus der Zeit vor dem 02.09.2026 traegt den alten
  // Demo-Link des Originals noch in der Datenbank. Er ist kein erkannter
  // Meeting-Anbieter und darf kein Klickziel mehr sein.
  it("does not turn a legacy welcome demo link into a join target", () => {
    mocks.sessionEvents = {
      "session-1": {
        tracking_id: "anarlog-onboarding-demo-v1",
        meeting_link: "https://anarlog.so/onboarding-demo/",
      },
    };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(screen.queryByRole("button", { name: "Join & record" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Record" }));

    expect(mocks.openUrl).not.toHaveBeenCalled();
    expect(mocks.startListening).toHaveBeenCalledOnce();
  });

  // E4 b (Opus): der geloeschte Demo-Test war der einzige, der den
  // Doppelklick-Schutz (joiningMeetingRef / disabled) prueft -- hier wieder,
  // mit einem echten Meeting-Link.
  it("ignores repeated joins while the start is in progress", async () => {
    let resolveStart: () => void = () => {};
    mocks.startListening.mockReturnValue(
      new Promise<void>((resolve) => {
        resolveStart = resolve;
      }),
    );
    mocks.sessionEvents = {
      "session-1": {
        tracking_id: "calendar-event-1",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const joinButton = screen.getByRole("button", { name: "Join & record" });

    fireEvent.click(joinButton);
    fireEvent.click(joinButton);

    expect(joinButton.hasAttribute("disabled")).toBe(true);
    expect(mocks.startListening).toHaveBeenCalledOnce();
    expect(mocks.openUrl).toHaveBeenCalledExactlyOnceWith(
      "https://meet.google.com/abc-defg-hij",
      null,
    );

    await act(async () => {
      resolveStart();
    });

    await vi.waitFor(() => {
      expect(joinButton.hasAttribute("disabled")).toBe(false);
    });
  });

  it("shows the meeting countdown in a callout below the header action", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-05T09:55:30.000Z"));
    mocks.nowMs = Date.now();
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const countdown = screen
      .getByText("starts in 4m 30s")
      .closest("[data-header-meeting-countdown]");
    const joinButton = screen.getByRole("button", { name: "Join & record" });

    expect(countdown).not.toBeNull();
    if (!countdown) throw new Error("meeting countdown is missing");
    expect(countdown.getAttribute("data-header-meeting-countdown")).toBe(
      "true",
    );
    expect(countdown.className).toContain("font-mono");
    expect(countdown.className).toContain("rounded-md");
    expect(countdown.className).toContain("border");
    expect(countdown.className).toContain("shadow-sm");
    expect(countdown.className).toContain("tabular-nums");
    expect(countdown.className).toContain("absolute");
    expect(countdown.className).toContain("top-full");
    expect(countdown.className).toContain("left-1/2");
    expect(
      countdown.querySelector("[data-header-meeting-countdown-tail]"),
    ).not.toBeNull();
    expect(countdown.parentElement?.className).toContain("relative");
    expect(joinButton.textContent).not.toContain("starts in");
  });

  // Scheduled auto-start is owned by ScheduledMeetingAutoStart so it fires
  // regardless of which tab is open; the header must not start a second one.
  it("does not auto-start when the countdown reaches the meeting start time", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-05T09:59:58.000Z"));
    mocks.nowMs = Date.now();
    mocks.configValues.auto_start_scheduled_meetings = true;
    mocks.configValues.auto_join_scheduled_meetings = true;
    mocks.sessionEvents = {
      "session-1": {
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(mocks.startListening).not.toHaveBeenCalled();
    expect(mocks.openUrl).not.toHaveBeenCalled();
  });

  it("hides the meeting countdown while listening is active", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-05T09:55:30.000Z"));
    mocks.nowMs = Date.now();
    mocks.sessionModes = { "session-1": "active" };
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(screen.getByRole("button", { name: "Stop" })).not.toBeNull();
    expect(
      document.querySelector("[data-header-meeting-countdown]"),
    ).toBeNull();
  });

  it("shows record before a meeting without a video link", () => {
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
      },
    };
    mocks.nowMs = new Date("2026-06-05T09:55:00.000Z").getTime();

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Record" }));

    expect(mocks.startListening).toHaveBeenCalledTimes(1);
    expect(
      screen.getByRole("button", { name: "Open event metadata" }),
    ).not.toBeNull();
  });

  it("shows record before a meeting with an unrecognized video link", () => {
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
        meeting_link: "https://naver.me/example",
      },
    };
    mocks.nowMs = new Date("2026-06-05T09:55:00.000Z").getTime();

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Record" }));

    expect(screen.queryByRole("button", { name: "Join & record" })).toBeNull();
    expect(mocks.startListening).toHaveBeenCalledTimes(1);
    expect(mocks.openUrl).not.toHaveBeenCalled();
  });

  it("shows record for a new ad hoc meeting note", () => {
    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const recordButton = screen.getByRole("button", { name: "Record" });

    expect(recordButton.className).toContain("bg-primary");
    expect(recordButton.className).toContain("dark:bg-white");
    expect(recordButton.className).toContain("dark:text-black");
    expect(recordButton.className).toContain("hover:bg-primary/90");
    expect(recordButton.className).toContain("dark:hover:bg-white/90");
    expect(recordButton.querySelector("span")?.className).not.toContain(
      "@max-[480px]:sr-only",
    );
    fireEvent.click(recordButton);

    expect(mocks.startListening).toHaveBeenCalledTimes(1);
    expect(
      screen.getByRole("button", { name: "Open event metadata" }),
    ).not.toBeNull();
  });

  it("shows resume for an inactive ad hoc session with a transcript", async () => {
    mocks.hasTranscriptBySession = { "session-1": true };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "transcript" } as EditorView}
      />,
    );

    const resumeButton = screen.getByRole("button", { name: "Resume" });
    expect(screen.queryByRole("button", { name: "Record" })).toBeNull();
    expect(
      screen.getByRole("button", { name: "Open event metadata" }),
    ).not.toBeNull();
    expect(mocks.startListening).not.toHaveBeenCalled();

    fireEvent.click(resumeButton);

    await vi.waitFor(() => {
      expect(mocks.resumeAfterStop).toHaveBeenCalledTimes(1);
    });
    expect(mocks.startListening).not.toHaveBeenCalled();
  });

  it("shows resume for an inactive ad hoc session with audio", () => {
    mocks.audioExists = true;

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "transcript" } as EditorView}
      />,
    );

    expect(screen.getByRole("button", { name: "Resume" })).not.toBeNull();
    expect(screen.queryByRole("button", { name: "Record" })).toBeNull();
    expect(
      screen.getByRole("button", { name: "Open event metadata" }),
    ).not.toBeNull();
    expect(mocks.startListening).not.toHaveBeenCalled();
  });

  it("shows a loading resume button during a running batch when the session already has content", async () => {
    mocks.sessionModes = { "session-1": "running_batch" };
    mocks.hasTranscriptBySession = { "session-1": true };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const resumeButton = screen.getByRole("button", { name: "Resume" });
    expect(resumeButton.hasAttribute("disabled")).toBe(false);
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();

    fireEvent.click(resumeButton);

    await vi.waitFor(() => {
      expect(mocks.resumeAfterStop).toHaveBeenCalledTimes(1);
    });
    expect(mocks.stopTranscription).not.toHaveBeenCalled();
  });

  it("keeps stop transcription for a running batch with nothing to resume into", () => {
    mocks.sessionModes = { "session-1": "running_batch" };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const stopButton = screen.getByRole("button", { name: "Stop" });
    expect(stopButton.hasAttribute("disabled")).toBe(false);

    fireEvent.click(stopButton);

    expect(mocks.stopTranscription).toHaveBeenCalledWith("session-1");
    expect(mocks.resumeAfterStop).not.toHaveBeenCalled();
  });

  it("keeps the resume pill up and clickable while finalizing", async () => {
    mocks.sessionModes = { "session-1": "finalizing" };
    mocks.hasTranscriptBySession = { "session-1": true };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const resumeButton = screen.getByRole("button", { name: "Resume" });
    expect(resumeButton.hasAttribute("disabled")).toBe(false);

    fireEvent.click(resumeButton);

    await vi.waitFor(() => {
      expect(mocks.resumeAfterStop).toHaveBeenCalledTimes(1);
    });
  });

  it("keeps a spinning resume pill up while the post-stop lease is held, even with no transcript yet and the event over", async () => {
    // No transcript, no resolved audio, and a meeting whose event already
    // ended -- on the old gate this combination fell through to "ended -> "
    // null. postStopProcessing alone must still hold the pill open with a
    // spinner (Fix-Runde B1).
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T09:00:00.000Z",
        ended_at: "2026-06-05T09:30:00.000Z",
      },
    };
    mocks.postStopProcessingBySession = { "session-1": true };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const resumeButton = screen.getByRole("button", { name: "Resume" });
    expect(resumeButton.hasAttribute("disabled")).toBe(false);
    expect(
      resumeButton.querySelector("[data-testid='spinner']"),
    ).not.toBeNull();

    fireEvent.click(resumeButton);

    await vi.waitFor(() => {
      expect(mocks.resumeAfterStop).toHaveBeenCalledTimes(1);
    });
  });

  it("keeps the pill up while the audio-exists query has not resolved yet, past the event", () => {
    // A fresh mount (or one right after a stop) can see audioExistsResolved
    // still false; without a transcript either, the old gate treated this
    // exactly like "confirmed no content" and hid the pill entirely once
    // the event was over. Not-yet-resolved must NOT be read as "no
    // content" -- the label is still Record here (nothing else says
    // otherwise yet), but the pill must exist rather than disappear
    // (Fix-Runde B1).
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T09:00:00.000Z",
        ended_at: "2026-06-05T09:30:00.000Z",
      },
    };
    mocks.audioExistsResolved = false;

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(screen.getByRole("button", { name: "Record" })).not.toBeNull();
  });

  it("shows resume for an inactive session with resolved audio, even past the event's ended_at (kills the dropped-canResume mutant)", () => {
    // Everything here is fully settled -- audioExistsResolved is true,
    // postStopProcessing is not set -- so this scenario only passes if the
    // gate really checks `canResume`, not just "content unknown".
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T09:00:00.000Z",
        ended_at: "2026-06-05T09:30:00.000Z",
      },
    };
    mocks.audioExists = true;
    mocks.audioExistsResolved = true;

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(screen.getByRole("button", { name: "Resume" })).not.toBeNull();
  });

  it("shows a distinct message when resuming fails to start after the block clears", async () => {
    mocks.hasTranscriptBySession = { "session-1": true };
    mocks.resumeAfterStop.mockResolvedValue({
      status: "blocked",
      reason: "start_failed",
    });

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Resume" }));

    await vi.waitFor(() => {
      expect(mocks.sonnerToastError).toHaveBeenCalledWith(
        "Mitschnitt could not resume recording. Please try again.",
        { id: "resume-after-stop-blocked" },
      );
    });
  });

  it("shows the another-session message, distinct from start_failed, when a different meeting is recording", async () => {
    mocks.hasTranscriptBySession = { "session-1": true };
    mocks.resumeAfterStop.mockResolvedValue({
      status: "blocked",
      reason: "another_session_active",
    });

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Resume" }));

    await vi.waitFor(() => {
      expect(mocks.sonnerToastError).toHaveBeenCalledWith(
        "Another meeting is currently recording. Stop it before resuming this one.",
        { id: "resume-after-stop-blocked" },
      );
    });
  });

  it("keeps the separate stop pill next to folder when the view switcher is present", () => {
    mocks.sessionModes = { "session-1": "active" };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
        viewSwitcher={<div>tabs</div>}
      />,
    );

    const stop = screen.getByRole("button", { name: "Stop" });
    const folder = screen.getByRole("combobox", { name: "Select folder" });

    expect(stop).not.toBeNull();
    expect(screen.getByText("tabs")).not.toBeNull();
    expect(screen.getByTestId("metadata-calendar-icon")).not.toBeNull();
    expect(
      stop.compareDocumentPosition(folder) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("keeps stop available for an active ad hoc session", () => {
    mocks.sessionModes = { "session-1": "active" };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Stop" }));

    expect(
      screen.getByRole("button", { name: "Open event metadata" }),
    ).not.toBeNull();
    expect(mocks.stopListening).toHaveBeenCalledTimes(1);
  });

  it("shows stop while the meeting is in progress", () => {
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };
    mocks.sessionModes = { "session-1": "active" };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const stopButton = screen.getByRole("button", { name: "Stop" });

    fireEvent.click(stopButton);

    expect(stopButton.querySelector("svg")?.getAttribute("class")).toContain(
      "text-red-500",
    );
    expect(screen.queryByRole("button", { name: "Join & record" })).toBeNull();
    expect(
      screen.getByRole("button", { name: "Open event metadata" }),
    ).not.toBeNull();
    expect(mocks.stopListening).toHaveBeenCalledTimes(1);
  });

  it("keeps stop available when recording runs past the scheduled end", () => {
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };
    mocks.sessionModes = { "session-1": "active" };
    mocks.nowMs = new Date("2026-06-05T10:31:00.000Z").getTime();

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Stop" }));

    expect(screen.getByTestId("metadata-calendar-icon")).not.toBeNull();
    expect(mocks.stopListening).toHaveBeenCalledTimes(1);
  });

  it("shows only the calendar metadata button after the meeting is over", () => {
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };
    mocks.nowMs = new Date("2026-06-05T10:31:00.000Z").getTime();

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    const metadataButton = screen.getByRole("button", {
      name: "Open event metadata",
    });

    expect(screen.getByTestId("metadata-calendar-icon")).not.toBeNull();
    expect(screen.queryByRole("button", { name: "Join & record" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Resume" })).toBeNull();
    expect(metadataButton.getAttribute("data-tauri-drag-region")).toBe("false");
    expect(metadataButton.parentElement?.className).not.toContain("mr-1");
    expect(mocks.startListening).not.toHaveBeenCalled();
  });

  it("shows resume instead of join when a scheduled meeting already has a transcript", () => {
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        meeting_link: "https://meet.google.com/abc-defg-hij",
      },
    };
    mocks.hasTranscriptBySession = { "session-1": true };
    mocks.nowMs = new Date("2026-06-05T10:31:00.000Z").getTime();

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
      />,
    );

    expect(screen.getByTestId("metadata-calendar-icon")).not.toBeNull();
    expect(screen.queryByRole("button", { name: "Join & record" })).toBeNull();
    expect(screen.getByRole("button", { name: "Resume" })).not.toBeNull();
    expect(mocks.startListening).not.toHaveBeenCalled();
  });

  it("shows transcript editing in the meeting-action slot after an ad hoc meeting", () => {
    mocks.hasTranscriptBySession = { "session-1": true };
    const onTranscriptEditModeChange = vi.fn();
    const view = render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "transcript" } as EditorView}
        transcriptEditMode={false}
        onTranscriptEditModeChange={onTranscriptEditModeChange}
      />,
    );

    const editButton = screen.getByRole("button", { name: "Edit" });
    expect(editButton.className).toContain("@max-[480px]:w-7");
    expect(editButton.querySelector("span")?.className).toContain(
      "@max-[480px]:sr-only",
    );
    fireEvent.click(editButton);
    expect(onTranscriptEditModeChange).toHaveBeenCalledWith(true);
    expect(screen.queryByRole("button", { name: "Record" })).toBeNull();
    expect(
      screen.getByRole("button", { name: "Open event metadata" }),
    ).not.toBeNull();

    view.rerender(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "transcript" } as EditorView}
        transcriptEditMode
        onTranscriptEditModeChange={onTranscriptEditModeChange}
      />,
    );

    const doneButton = screen.getByRole("button", { name: "Done" });
    expect(doneButton.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(doneButton);
    expect(onTranscriptEditModeChange).toHaveBeenLastCalledWith(false);
  });

  it("does not show transcript editing outside the transcript tab", () => {
    mocks.hasTranscriptBySession = { "session-1": true };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "raw" } as EditorView}
        onTranscriptEditModeChange={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
  });

  it("does not show transcript editing while the meeting is active", () => {
    mocks.hasTranscriptBySession = { "session-1": true };
    mocks.sessionModes = { "session-1": "active" };

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "transcript" } as EditorView}
        onTranscriptEditModeChange={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
    expect(screen.getByRole("button", { name: "Stop" })).not.toBeNull();
  });

  it("shows transcript editing alongside metadata after a scheduled meeting", () => {
    mocks.hasTranscriptBySession = { "session-1": true };
    mocks.sessionEvents = {
      "session-1": {
        title: "Design Review",
        started_at: "2026-06-05T10:00:00.000Z",
        ended_at: "2026-06-05T10:30:00.000Z",
      },
    };
    mocks.nowMs = new Date("2026-06-05T10:31:00.000Z").getTime();

    render(
      <OuterHeader
        sessionId="session-1"
        currentView={{ type: "transcript" } as EditorView}
        onTranscriptEditModeChange={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "Edit" })).not.toBeNull();
    expect(
      screen.getByRole("button", { name: "Open event metadata" }),
    ).not.toBeNull();
  });
});
