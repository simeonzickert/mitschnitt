import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { FloatingBarState } from "@anlg/plugin-windows";

import { FloatingBarOverlay } from "./bar";

vi.mock("@anlg/ui/components/ui/dancing-sticks", () => ({
  DancingSticks: ({
    amplitude,
    color,
  }: {
    amplitude: number;
    color: string;
  }) => (
    <span
      data-testid="waveform"
      data-amplitude={amplitude}
      data-color={color}
    />
  ),
}));

function state(overrides: Partial<FloatingBarState> = {}): FloatingBarState {
  return {
    amplitude: 0.4,
    title: "Weekly sync",
    status: "recording",
    colorScheme: "light",
    opacity: 0.78,
    liveCaptionOpacity: 0.3,
    liveCaptionWidth: 440,
    liveCaptionLineCount: 1,
    liveCaptionPosition: "topCenter",
    liveCaptionMinimized: true,
    liveCaptionToggleVisible: true,
    transcriptBubbles: [
      {
        id: "1",
        speakerLabel: "Ada",
        text: "Let's start.",
        isSelf: false,
        isFinal: true,
        startMs: 0,
        endMs: 1200,
        overlapsPrevious: false,
        overlapsNext: false,
      },
    ],
    ...overrides,
  };
}

describe("FloatingBarOverlay", () => {
  afterEach(() => {
    cleanup();
  });

  it("stops listening from the compact bar", () => {
    const onStop = vi.fn();

    render(
      <FloatingBarOverlay
        state={state()}
        onStop={onStop}
        onToggleExpanded={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Stop listening" }));

    expect(onStop).toHaveBeenCalledOnce();
    // One waveform per track: microphone and system audio.
    expect(screen.getAllByTestId("waveform")).toHaveLength(2);
  });

  it("expands to the live transcript and can collapse again", () => {
    const onToggleExpanded = vi.fn();

    const view = render(
      <FloatingBarOverlay
        state={state()}
        onStop={vi.fn()}
        onToggleExpanded={onToggleExpanded}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Expand live transcript" }),
    );
    expect(onToggleExpanded).toHaveBeenCalledWith(true);

    view.rerender(
      <FloatingBarOverlay
        state={state({ liveCaptionMinimized: false })}
        onStop={vi.fn()}
        onToggleExpanded={onToggleExpanded}
      />,
    );

    expect(screen.getByText("Weekly sync")).toBeTruthy();
    expect(screen.getByText("Let's start.")).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", { name: "Collapse live transcript" }),
    );
    expect(onToggleExpanded).toHaveBeenCalledWith(false);
  });
});

const WARNING_COLOR = "rgb(245, 166, 35)";

describe("dead-track watch in the floating bar", () => {
  afterEach(() => {
    cleanup();
  });

  function trackAmplitude(container: HTMLElement, track: "mic" | "speaker") {
    return container
      .querySelector(`[data-track="${track}"] [data-testid="waveform"]`)
      ?.getAttribute("data-amplitude");
  }

  /**
   * The visible difference, not a flag of our own: a track judged dead is
   * drawn in the warning colour, a live one in the bar's accent colour.
   */
  function trackColor(container: HTMLElement, track: "mic" | "speaker") {
    return container
      .querySelector(`[data-track="${track}"] [data-testid="waveform"]`)
      ?.getAttribute("data-color");
  }

  function isFlagged(container: HTMLElement, track: "mic" | "speaker") {
    const color = trackColor(container, track);
    expect(color).toBeTruthy();
    return color === WARNING_COLOR;
  }

  it("shows the two tracks separately, each with its own level", () => {
    const { container } = render(
      <FloatingBarOverlay
        state={state()}
        levels={{ mic: 0.7, speaker: 0.1 }}
        silentTracks={[]}
        onStop={vi.fn()}
        onToggleExpanded={vi.fn()}
      />,
    );

    expect(trackAmplitude(container, "mic")).toBe("0.7");
    expect(trackAmplitude(container, "speaker")).toBe("0.1");
  });

  it("stays quiet while both tracks are alive", () => {
    const { container } = render(
      <FloatingBarOverlay
        state={state()}
        levels={{ mic: 0.4, speaker: 0.3 }}
        silentTracks={[]}
        onStop={vi.fn()}
        onToggleExpanded={vi.fn()}
      />,
    );

    expect(screen.queryByRole("status")).toBeNull();
    expect(isFlagged(container, "mic")).toBe(false);
    expect(isFlagged(container, "speaker")).toBe(false);
  });

  it("names the silent track in the collapsed pill and flags only that one", () => {
    const { container } = render(
      <FloatingBarOverlay
        state={state()}
        levels={{ mic: 0.4, speaker: 0 }}
        silentTracks={["speaker"]}
        onStop={vi.fn()}
        onToggleExpanded={vi.fn()}
      />,
    );

    expect(screen.getByRole("status").textContent).toBe(
      "No sound from the other side",
    );
    expect(isFlagged(container, "speaker")).toBe(true);
    expect(isFlagged(container, "mic")).toBe(false);
  });

  it("spells the warning out in the expanded panel", () => {
    render(
      <FloatingBarOverlay
        state={state({ liveCaptionMinimized: false })}
        levels={{ mic: 0, speaker: 0.3 }}
        silentTracks={["mic"]}
        onStop={vi.fn()}
        onToggleExpanded={vi.fn()}
      />,
    );

    expect(screen.getByRole("status").textContent).toContain(
      "No microphone signal",
    );
    // The transcript is still reachable next to the warning.
    expect(screen.getByText("Let's start.")).toBeTruthy();
  });

  it("drops the warning again once the track comes back", () => {
    const view = render(
      <FloatingBarOverlay
        state={state()}
        levels={{ mic: 0.4, speaker: 0 }}
        silentTracks={["speaker"]}
        onStop={vi.fn()}
        onToggleExpanded={vi.fn()}
      />,
    );

    expect(screen.getByRole("status")).toBeTruthy();

    view.rerender(
      <FloatingBarOverlay
        state={state()}
        levels={{ mic: 0.4, speaker: 0.2 }}
        silentTracks={[]}
        onStop={vi.fn()}
        onToggleExpanded={vi.fn()}
      />,
    );

    expect(screen.queryByRole("status")).toBeNull();
    expect(isFlagged(view.container, "speaker")).toBe(false);
  });

  it("still lets you stop while a track is dead", () => {
    const onStop = vi.fn();

    render(
      <FloatingBarOverlay
        state={state()}
        levels={{ mic: 0, speaker: 0 }}
        silentTracks={["mic", "speaker"]}
        onStop={onStop}
        onToggleExpanded={vi.fn()}
      />,
    );

    expect(screen.getByRole("status").textContent).toBe(
      "No audio on either track",
    );
    fireEvent.click(screen.getByRole("button", { name: "Stop listening" }));
    expect(onStop).toHaveBeenCalledOnce();
  });
});
