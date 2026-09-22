import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Transcript } from "./index";

const {
  useListenerMock,
  useAudioPlayerMock,
  useSessionTranscriptMetadataMock,
  regenerateTranscriptMock,
} = vi.hoisted(() => ({
  useListenerMock: vi.fn(),
  useAudioPlayerMock: vi.fn(),
  useSessionTranscriptMetadataMock: vi.fn(),
  regenerateTranscriptMock: vi.fn(),
}));

vi.mock("./actions", () => ({
  useRegenerateTranscript: () => regenerateTranscriptMock,
}));

vi.mock("~/stt/queries", () => ({
  useSessionTranscriptMetadata: useSessionTranscriptMetadataMock,
}));

vi.mock("~/stt/contexts", () => ({
  useListener: useListenerMock,
}));

vi.mock("~/audio-player", () => ({
  useAudioPlayer: useAudioPlayerMock,
}));

vi.mock("./screens/batch", () => ({
  BatchState: () => <div data-testid="batch-state" />,
}));

vi.mock("./screens/empty", () => ({
  TranscriptEmptyState: () => <div data-testid="empty-state" />,
}));

vi.mock("./screens/listening", () => ({
  TranscriptListeningState: ({ status }: { status: string }) => (
    <div data-testid="listening-state">{status}</div>
  ),
}));

vi.mock("./renderer", () => ({
  TranscriptViewer: ({
    captureGeneration,
    editMode,
  }: {
    captureGeneration: number;
    editMode?: boolean;
  }) => (
    <div
      data-testid="transcript-viewer"
      data-capture-generation={captureGeneration}
      data-edit-mode={String(editMode ?? false)}
    />
  ),
}));

vi.mock("~/stt/useUploadFile", () => ({
  useUploadFile: vi.fn(() => ({
    uploadAudio: vi.fn(),
    uploadTranscript: vi.fn(),
    processFile: vi.fn(),
  })),
}));

vi.mock("~/stt/pending-upload", () => ({
  consumePendingUpload: vi.fn(() => null),
}));

describe("Transcript", () => {
  const sessionId = "session-1";
  const transcriptId = "transcript-1";

  let listenerState: {
    getSessionMode: (id: string) => "inactive" | "active" | "finalizing";
    batch: Record<string, { error?: string | null }>;
    live: {
      captureGenerationCounter: number;
      captureGenerationBySession: Record<string, number>;
      degraded: null;
      requestedLiveTranscription: boolean;
      liveTranscriptionActive: boolean;
      postStopProcessingBySession: Record<string, boolean>;
    };
    liveSegments: unknown[];
    partialWordsByChannel: Record<number, unknown[]>;
    partialHintsByChannel: Record<number, unknown[]>;
  };
  let transcripts: Array<{ id: string; hasWords: boolean }>;

  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    regenerateTranscriptMock.mockClear();
    transcripts = [{ id: transcriptId, hasWords: false }];

    listenerState = {
      getSessionMode: () => "active",
      batch: {},
      live: {
        captureGenerationCounter: 2,
        captureGenerationBySession: {
          [sessionId]: 1,
          "session-2": 2,
        },
        degraded: null,
        requestedLiveTranscription: true,
        liveTranscriptionActive: true,
        postStopProcessingBySession: {},
      },
      liveSegments: [],
      partialWordsByChannel: {},
      partialHintsByChannel: {},
    };

    useSessionTranscriptMetadataMock.mockImplementation(() => transcripts);
    useListenerMock.mockImplementation((selector) => selector(listenerState));
    useAudioPlayerMock.mockReturnValue({ audioExists: false });
  });

  it("switches to transcript viewer after transcript words persist", () => {
    const scrollRef = createRef<HTMLDivElement>();
    const view = render(
      <Transcript sessionId={sessionId} scrollRef={scrollRef} />,
    );

    expect(screen.getByTestId("listening-state").textContent).toBe("listening");

    transcripts = [{ id: transcriptId, hasWords: true }];

    view.rerender(<Transcript sessionId={sessionId} scrollRef={scrollRef} />);

    expect(
      screen
        .getByTestId("transcript-viewer")
        .getAttribute("data-capture-generation"),
    ).toBe("1");
  });

  it("keeps existing transcript content unobstructed while finalizing", () => {
    listenerState = {
      ...listenerState,
      getSessionMode: () => "finalizing",
    };
    transcripts = [{ id: transcriptId, hasWords: true }];

    render(<Transcript sessionId={sessionId} scrollRef={createRef()} />);

    expect(screen.queryByText("Finalizing transcript...")).toBeNull();
    expect(screen.getByTestId("transcript-viewer")).not.toBeNull();
  });

  it("shows recording state for record-only capture sessions", () => {
    listenerState = {
      ...listenerState,
      live: {
        ...listenerState.live,
        requestedLiveTranscription: false,
        liveTranscriptionActive: false,
      },
    };

    render(<Transcript sessionId={sessionId} scrollRef={createRef()} />);

    expect(screen.queryByTestId("listening-state")).toBeNull();
    expect(screen.getByTestId("batch-state")).not.toBeNull();
  });

  it("renders finalized transcripts in the requested edit mode", () => {
    listenerState = {
      ...listenerState,
      getSessionMode: () => "inactive",
    };
    transcripts = [{ id: transcriptId, hasWords: true }];

    const view = render(
      <Transcript
        sessionId={sessionId}
        scrollRef={createRef()}
        editMode={false}
      />,
    );

    expect(
      screen.getByTestId("transcript-viewer").getAttribute("data-edit-mode"),
    ).toBe("false");
    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();

    view.rerender(
      <Transcript sessionId={sessionId} scrollRef={createRef()} editMode />,
    );

    expect(
      screen.getByTestId("transcript-viewer").getAttribute("data-edit-mode"),
    ).toBe("true");
  });

  it("shows a Re-transcribe button in the transcript tab and calls the regenerate action on click (ZICK-318)", () => {
    listenerState = {
      ...listenerState,
      getSessionMode: () => "inactive",
    };
    transcripts = [{ id: transcriptId, hasWords: true }];
    useAudioPlayerMock.mockReturnValue({
      audioExists: true,
      audioExistsResolved: true,
    });

    render(<Transcript sessionId={sessionId} scrollRef={createRef()} />);

    const button = screen.getByRole("button", { name: "Re-transcribe" });
    fireEvent.click(button);

    expect(regenerateTranscriptMock).toHaveBeenCalledTimes(1);
  });

  it("hides the Re-transcribe button while no audio recording exists", () => {
    listenerState = {
      ...listenerState,
      getSessionMode: () => "inactive",
    };
    transcripts = [{ id: transcriptId, hasWords: true }];
    useAudioPlayerMock.mockReturnValue({
      audioExists: false,
      audioExistsResolved: true,
    });

    render(<Transcript sessionId={sessionId} scrollRef={createRef()} />);

    expect(screen.queryByRole("button", { name: "Re-transcribe" })).toBeNull();
  });

  it("hides the Re-transcribe button while a meeting is actively recording", () => {
    listenerState = {
      ...listenerState,
      getSessionMode: () => "active",
    };
    transcripts = [{ id: transcriptId, hasWords: true }];
    useAudioPlayerMock.mockReturnValue({
      audioExists: true,
      audioExistsResolved: true,
    });

    render(<Transcript sessionId={sessionId} scrollRef={createRef()} />);

    expect(screen.queryByRole("button", { name: "Re-transcribe" })).toBeNull();
  });
});
