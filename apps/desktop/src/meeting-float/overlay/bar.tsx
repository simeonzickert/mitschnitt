import {
  ArrowsInSimple,
  ArrowsOutSimple,
  CaretDown,
  Square,
  Warning,
} from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";

import type {
  FloatingBarState,
  FloatingTranscriptBubble,
} from "@anlg/plugin-windows";
import { DancingSticks } from "@anlg/ui/components/ui/dancing-sticks";
import { cn } from "@anlg/utils";

import {
  describeSilentTracks,
  type FloatingTrack,
  type FloatingTrackLevels,
} from "../track-health";
import {
  FLOATING_BAR_COMPACT_GAP,
  FLOATING_BAR_COMPACT_HEIGHT,
  FLOATING_BAR_COMPACT_HORIZONTAL_PADDING,
  FLOATING_BAR_COMPACT_ICON_SIZE,
  FLOATING_BAR_COMPACT_SOLO_STOP_WIDTH,
  FLOATING_BAR_COMPACT_STOP_WIDTH,
  FLOATING_BAR_COMPACT_RADIUS,
  FLOATING_BAR_CONTROL_RADIUS,
  FLOATING_BAR_EXPANDED_HEIGHT,
  FLOATING_BAR_EXPANDED_RADIUS,
  FLOATING_BAR_EXPANDED_WIDTH,
  FLOATING_BAR_HOVER_HANDLE_HEIGHT,
  FLOATING_BAR_HOVER_HANDLE_RESERVED_HEIGHT,
  FLOATING_BAR_HOVER_HANDLE_TOP_PADDING,
  FLOATING_BAR_INSET,
  FLOATING_BAR_TRACK_WARNING_HEIGHT,
  compactControlsWidth,
  compactWidth,
} from "./layout";

const NO_LEVELS: FloatingTrackLevels = { mic: 0, speaker: 0 };
const NO_SILENT_TRACKS: FloatingTrack[] = [];

export function FloatingBarOverlay({
  state,
  levels = NO_LEVELS,
  silentTracks = NO_SILENT_TRACKS,
  onStop,
  onToggleExpanded,
}: {
  state: FloatingBarState;
  /** Per-track levels; see ../track-levels for why these travel separately. */
  levels?: FloatingTrackLevels;
  /** Tracks the dead-track watch currently judges silent. */
  silentTracks?: FloatingTrack[];
  onStop: () => void;
  onToggleExpanded: (expanded: boolean) => void;
}) {
  const [hovered, setHovered] = useState(false);
  const isExpanded =
    state.liveCaptionToggleVisible && !state.liveCaptionMinimized;

  return (
    <div
      className="flex h-full w-full items-end justify-end"
      style={{ padding: FLOATING_BAR_INSET }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {isExpanded ? (
        <ExpandedPanel
          state={state}
          levels={levels}
          silentTracks={silentTracks}
          hovered={hovered}
          onStop={onStop}
          onToggleExpanded={onToggleExpanded}
        />
      ) : (
        <CompactPill
          state={state}
          levels={levels}
          silentTracks={silentTracks}
          hovered={hovered}
          onStop={onStop}
          onToggleExpanded={onToggleExpanded}
        />
      )}
    </div>
  );
}

function CompactPill({
  state,
  levels,
  silentTracks,
  hovered,
  onStop,
  onToggleExpanded,
}: {
  state: FloatingBarState;
  levels: FloatingTrackLevels;
  silentTracks: FloatingTrack[];
  hovered: boolean;
  onStop: () => void;
  onToggleExpanded: (expanded: boolean) => void;
}) {
  const width = compactWidth(state.liveCaptionToggleVisible);
  const height =
    FLOATING_BAR_COMPACT_HEIGHT +
    (hovered ? FLOATING_BAR_HOVER_HANDLE_RESERVED_HEIGHT : 0);
  const colors = barColors(state);

  return (
    <div
      className="relative overflow-hidden"
      style={{
        width,
        height,
        borderRadius: FLOATING_BAR_COMPACT_RADIUS,
        background: hovered ? colors.envelopeSurface : colors.surface,
        boxShadow: `inset 0 0 0 0.5px ${colors.outerStroke}`,
      }}
    >
      {hovered ? <HoverHandle color={colors.handle} width={width} /> : null}
      <div
        className="absolute right-0 bottom-0 flex items-center justify-center"
        style={{
          width,
          height: FLOATING_BAR_COMPACT_HEIGHT,
        }}
      >
        <FloatingControls
          state={state}
          levels={levels}
          silentTracks={silentTracks}
          isExpanded={false}
          colors={colors}
          onStop={onStop}
          onToggleExpanded={onToggleExpanded}
        />
      </div>
      <TrackWarningAnnouncement silentTracks={silentTracks} />
    </div>
  );
}

/**
 * The collapsed pill has no room for a sentence, so the visible signal there is
 * the amber flat line plus the warning glyph. This keeps the same information
 * available to assistive tech (and to tests) without adding visual chrome.
 */
function TrackWarningAnnouncement({
  silentTracks,
}: {
  silentTracks: FloatingTrack[];
}) {
  const message = describeSilentTracks(silentTracks);
  if (!message) {
    return null;
  }

  return (
    <span className="sr-only" role="status">
      {message}
    </span>
  );
}

function ExpandedPanel({
  state,
  levels,
  silentTracks,
  hovered,
  onStop,
  onToggleExpanded,
}: {
  state: FloatingBarState;
  levels: FloatingTrackLevels;
  silentTracks: FloatingTrack[];
  hovered: boolean;
  onStop: () => void;
  onToggleExpanded: (expanded: boolean) => void;
}) {
  const colors = barColors(state);
  const warning = describeSilentTracks(silentTracks);

  return (
    <div
      className="relative overflow-hidden"
      style={{
        width: FLOATING_BAR_EXPANDED_WIDTH,
        height:
          FLOATING_BAR_EXPANDED_HEIGHT +
          (hovered ? FLOATING_BAR_HOVER_HANDLE_RESERVED_HEIGHT : 0),
        borderRadius: FLOATING_BAR_EXPANDED_RADIUS,
        background: colors.surface,
        boxShadow: `inset 0 0 0 0.5px ${colors.outerStroke}`,
      }}
    >
      <div
        className="absolute inset-x-0 top-0"
        style={{
          height: FLOATING_BAR_HOVER_HANDLE_RESERVED_HEIGHT,
          paddingTop: FLOATING_BAR_HOVER_HANDLE_TOP_PADDING,
          opacity: hovered ? 1 : 0,
        }}
      >
        <HoverHandle
          color={colors.handle}
          width={FLOATING_BAR_EXPANDED_WIDTH}
        />
      </div>
      <div
        className="absolute inset-x-0 bottom-0"
        style={{ height: FLOATING_BAR_EXPANDED_HEIGHT }}
      >
        <div
          className="flex items-center"
          style={{
            height: FLOATING_BAR_COMPACT_HEIGHT,
            paddingLeft: 16,
            paddingRight:
              compactControlsWidth(state.liveCaptionToggleVisible) + 12,
          }}
        >
          <p
            className="min-w-0 truncate text-[13px] font-semibold"
            style={{ color: colors.content }}
          >
            {state.title}
          </p>
        </div>
        {warning ? (
          <div
            role="status"
            className="flex items-center gap-1.5 px-4 text-[11px] font-semibold"
            style={{
              height: FLOATING_BAR_TRACK_WARNING_HEIGHT,
              color: colors.warning,
            }}
          >
            <Warning size={12} weight="fill" aria-hidden />
            {warning}
          </div>
        ) : null}
        <TranscriptList
          bubbles={state.transcriptBubbles ?? []}
          colorScheme={state.colorScheme}
          reservedHeight={
            FLOATING_BAR_COMPACT_HEIGHT +
            (warning ? FLOATING_BAR_TRACK_WARNING_HEIGHT : 0)
          }
        />
        <div
          className="absolute top-0 right-0 flex items-center justify-center"
          style={{
            width: compactControlsWidth(state.liveCaptionToggleVisible),
            height: FLOATING_BAR_COMPACT_HEIGHT,
            marginRight: FLOATING_BAR_COMPACT_HORIZONTAL_PADDING,
          }}
        >
          <FloatingControls
            state={state}
            levels={levels}
            silentTracks={silentTracks}
            isExpanded
            colors={colors}
            onStop={onStop}
            onToggleExpanded={onToggleExpanded}
          />
        </div>
      </div>
    </div>
  );
}

function FloatingControls({
  state,
  levels,
  silentTracks,
  isExpanded,
  colors,
  onStop,
  onToggleExpanded,
}: {
  state: FloatingBarState;
  levels: FloatingTrackLevels;
  silentTracks: FloatingTrack[];
  isExpanded: boolean;
  colors: BarColors;
  onStop: () => void;
  onToggleExpanded: (expanded: boolean) => void;
}) {
  return (
    <div
      className="flex items-center"
      style={{ gap: FLOATING_BAR_COMPACT_GAP }}
    >
      <StopControl
        state={state}
        levels={levels}
        silentTracks={silentTracks}
        colors={colors}
        onStop={onStop}
      />
      {state.liveCaptionToggleVisible ? (
        <button
          type="button"
          data-tauri-drag-region="false"
          aria-label={
            isExpanded ? "Collapse live transcript" : "Expand live transcript"
          }
          onClick={() => onToggleExpanded(!isExpanded)}
          className="flex items-center justify-center"
          style={{
            width: FLOATING_BAR_COMPACT_ICON_SIZE,
            height: FLOATING_BAR_COMPACT_ICON_SIZE,
            borderRadius: FLOATING_BAR_CONTROL_RADIUS,
            color: colors.content,
          }}
        >
          {isExpanded ? (
            <ArrowsInSimple size={14} weight="bold" />
          ) : (
            <ArrowsOutSimple size={14} weight="bold" />
          )}
        </button>
      ) : null}
    </div>
  );
}

function StopControl({
  state,
  levels,
  silentTracks,
  colors,
  onStop,
}: {
  state: FloatingBarState;
  levels: FloatingTrackLevels;
  silentTracks: FloatingTrack[];
  colors: BarColors;
  onStop: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  const width = state.liveCaptionToggleVisible
    ? FLOATING_BAR_COMPACT_STOP_WIDTH
    : FLOATING_BAR_COMPACT_SOLO_STOP_WIDTH;

  return (
    <button
      type="button"
      data-tauri-drag-region="false"
      aria-label="Stop listening"
      onClick={onStop}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      className="flex items-center justify-center"
      style={{
        width,
        height: FLOATING_BAR_COMPACT_ICON_SIZE,
        borderRadius: FLOATING_BAR_CONTROL_RADIUS,
        background: hovered ? "rgba(255, 51, 77, 0.18)" : colors.controlFill,
        color: colors.accent,
      }}
    >
      {hovered ? (
        <span className="flex items-center gap-1.5 text-xs font-semibold">
          <Square size={9} weight="fill" />
          Stop
        </span>
      ) : state.status === "error" ? (
        <Warning size={16} weight="fill" />
      ) : (
        <TrackWaveforms
          levels={levels}
          silentTracks={silentTracks}
          colors={colors}
        />
      )}
    </button>
  );
}

/**
 * Two waveforms where there used to be one: microphone (you) on the left,
 * system audio (the other side) on the right, split by a hairline.
 *
 * Together they occupy exactly the 26px the single waveform had, so the pill
 * keeps its size and its formal language. A track at zero already renders as a
 * flat line -- which is also what a natural pause looks like, so a track the
 * watch has actually judged dead is additionally tinted amber, and the glyph
 * next to it says that this flat line is not just a pause.
 */
function TrackWaveforms({
  levels,
  silentTracks,
  colors,
}: {
  levels: FloatingTrackLevels;
  silentTracks: FloatingTrack[];
  colors: BarColors;
}) {
  const micSilent = silentTracks.includes("mic");
  const speakerSilent = silentTracks.includes("speaker");
  const anySilent = micSilent || speakerSilent;

  return (
    <span
      className="flex items-center"
      style={{ gap: anySilent ? 3 : FLOATING_BAR_COMPACT_GAP }}
    >
      {anySilent ? (
        <Warning size={11} weight="fill" color={colors.warning} aria-hidden />
      ) : null}
      <span className="flex items-center" style={{ gap: 2 }}>
        <TrackWaveform
          track="mic"
          amplitude={levels.mic}
          silent={micSilent}
          colors={colors}
        />
        <span
          aria-hidden
          style={{
            width: 1,
            height: 12,
            background: colors.trackDivider,
            borderRadius: 1,
          }}
        />
        <TrackWaveform
          track="speaker"
          amplitude={levels.speaker}
          silent={speakerSilent}
          colors={colors}
        />
      </span>
    </span>
  );
}

function TrackWaveform({
  track,
  amplitude,
  silent,
  colors,
}: {
  track: FloatingTrack;
  amplitude: number;
  silent: boolean;
  colors: BarColors;
}) {
  return (
    <span data-track={track}>
      <DancingSticks
        color={silent ? colors.warning : colors.accent}
        amplitude={amplitude}
        width={11}
        height={20}
        stickWidth={3}
        gap={2}
      />
    </span>
  );
}

function TranscriptList({
  bubbles,
  colorScheme,
  reservedHeight = FLOATING_BAR_COMPACT_HEIGHT,
}: {
  bubbles: FloatingTranscriptBubble[];
  colorScheme: FloatingBarState["colorScheme"];
  /** Vertical space above the list (title row, plus warning strip if shown). */
  reservedHeight?: number;
}) {
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const [pinned, setPinned] = useState(true);

  useEffect(() => {
    if (pinned) {
      bottomRef.current?.scrollIntoView?.({ block: "end" });
    }
  }, [bubbles, pinned]);

  return (
    <div
      className="relative px-3 pb-3"
      style={{ height: `calc(100% - ${reservedHeight}px)` }}
    >
      <div
        className="h-full overflow-y-auto"
        onScroll={(event) => {
          const target = event.currentTarget;
          const distance =
            target.scrollHeight - target.scrollTop - target.clientHeight;
          setPinned(distance < 20);
        }}
      >
        <div className="flex min-h-full flex-col justify-end gap-2">
          {bubbles.map((bubble, index) => (
            <TranscriptBubble
              key={bubble.id}
              bubble={bubble}
              colorScheme={colorScheme}
              showsSpeakerLabel={
                index === 0 ||
                bubbles[index - 1]?.speakerLabel !== bubble.speakerLabel ||
                bubbles[index - 1]?.isSelf !== bubble.isSelf
              }
            />
          ))}
          <div ref={bottomRef} />
        </div>
      </div>
      {!pinned && bubbles.length > 0 ? (
        <button
          type="button"
          data-tauri-drag-region="false"
          onClick={() => {
            setPinned(true);
            bottomRef.current?.scrollIntoView?.({
              block: "end",
              behavior: "smooth",
            });
          }}
          className="absolute bottom-3 left-1/2 flex -translate-x-1/2 items-center gap-1.5 rounded-[10px] px-3 py-1.5 text-[11px] font-medium"
          style={{
            background:
              colorScheme === "dark" ? "rgb(46, 46, 43)" : "rgb(242, 242, 237)",
            color: colorScheme === "dark" ? "white" : "rgb(31, 28, 26)",
          }}
        >
          <CaretDown size={10} weight="bold" />
          Go to bottom
        </button>
      ) : null}
    </div>
  );
}

function TranscriptBubble({
  bubble,
  showsSpeakerLabel,
  colorScheme,
}: {
  bubble: FloatingTranscriptBubble;
  showsSpeakerLabel: boolean;
  colorScheme: FloatingBarState["colorScheme"];
}) {
  const overlapping = bubble.overlapsPrevious || bubble.overlapsNext;

  return (
    <div
      className={cn(["flex", bubble.isSelf ? "justify-end" : "justify-start"])}
    >
      <div
        className={cn([
          "max-w-[calc(100%-40px)]",
          bubble.isSelf ? "items-end" : "items-start",
        ])}
      >
        {(showsSpeakerLabel || overlapping) && (
          <p className="mb-1 px-1 text-[10px] font-semibold text-white">
            {showsSpeakerLabel ? bubble.speakerLabel : ""}
          </p>
        )}
        <p
          className="rounded-[11px] px-2.5 py-2 text-[13px] leading-5 text-white"
          style={{
            background: bubble.isSelf
              ? `rgba(0, 0, 0, ${colorScheme === "dark" ? 0.34 : 0.24})`
              : `rgba(0, 0, 0, ${colorScheme === "dark" ? 0.28 : 0.2})`,
            boxShadow: overlapping
              ? `inset 0 0 0 1px rgba(255, 255, 255, ${
                  colorScheme === "dark" ? 0.26 : 0.34
                })`
              : undefined,
          }}
        >
          {bubble.text}
        </p>
      </div>
    </div>
  );
}

function HoverHandle({ color, width }: { color: string; width: number }) {
  return (
    <div
      data-tauri-drag-region
      className="flex items-center justify-center"
      style={{
        height: FLOATING_BAR_HOVER_HANDLE_HEIGHT,
        width,
      }}
    >
      <div
        data-tauri-drag-region
        className="h-full"
        style={{
          width: Math.max(0, width - 16),
          backgroundImage: `radial-gradient(circle, ${color} 0.8px, transparent 0.9px)`,
          backgroundSize: "5px 7px",
        }}
      />
    </div>
  );
}

type BarColors = {
  surface: string;
  envelopeSurface: string;
  content: string;
  handle: string;
  outerStroke: string;
  controlFill: string;
  accent: string;
  warning: string;
  trackDivider: string;
};

function barColors(state: FloatingBarState): BarColors {
  const opacity = Math.min(Math.max(state.opacity, 0.35), 0.95);
  const dark = state.colorScheme === "dark";
  const content = dark ? "rgb(255, 255, 255)" : "rgb(31, 28, 26)";
  const surfaceRgb = dark ? "110, 112, 102" : "219, 217, 209";

  return {
    surface: `rgba(${surfaceRgb}, ${opacity * 0.82})`,
    envelopeSurface: `rgba(${surfaceRgb}, ${Math.min(opacity * 1.08, 0.95)})`,
    content,
    handle: dark ? "rgba(255, 255, 255, 0.48)" : "rgba(31, 28, 26, 0.36)",
    outerStroke: dark ? "rgba(255, 255, 255, 0.14)" : "rgba(31, 28, 26, 0.12)",
    controlFill: dark ? "rgba(255, 255, 255, 0.08)" : "rgba(31, 28, 26, 0.07)",
    accent: state.status === "error" ? "rgb(255, 64, 61)" : "rgb(255, 51, 77)",
    warning: "rgb(245, 166, 35)",
    trackDivider: dark ? "rgba(255, 255, 255, 0.28)" : "rgba(31, 28, 26, 0.24)",
  };
}
