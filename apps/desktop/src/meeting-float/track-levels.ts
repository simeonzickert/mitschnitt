/**
 * Per-track level channel between the main window and the floating bar.
 *
 * Why a channel of its own: `FloatingBarState` (generated from the Rust plugin)
 * carries a single scalar `amplitude`, which `getFloatingRouteState` builds by
 * collapsing both tracks with `Math.hypot`. That collapse is exactly what hides
 * a dead track -- one live track keeps the number up. Widening
 * `FloatingBarState` means changing the plugin's Rust struct and its generated
 * bindings; this side-channel keeps the change inside the frontend and stays
 * fully reversible. It follows the same cross-window pattern as
 * `ai/task-window-sync`.
 */

import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { FloatingTrackLevels } from "./track-health";

import { listenerStore } from "~/store/zustand/listener/instance";

export const FLOATING_TRACK_LEVELS_EVENT = "mitschnitt:floating-track-levels";

export type FloatingTrackLevelsPayload = FloatingTrackLevels & {
  /** Null while nothing is being recorded. Lets the overlay reset its watch. */
  sessionId: string | null;
};

export function listenToFloatingTrackLevels(
  onLevels: (payload: FloatingTrackLevelsPayload) => void,
): Promise<UnlistenFn> {
  return listen<FloatingTrackLevelsPayload>(
    FLOATING_TRACK_LEVELS_EVENT,
    (event) => onLevels(event.payload),
  );
}

type ListenerState = ReturnType<typeof listenerStore.getState>;

function getTrackLevelsPayload(
  state: ListenerState,
): FloatingTrackLevelsPayload {
  const active =
    state.live.status === "active" && Boolean(state.live.sessionId);

  return {
    sessionId: active ? (state.live.sessionId ?? null) : null,
    mic: active ? state.live.amplitude.mic : 0,
    speaker: active ? state.live.amplitude.speaker : 0,
  };
}

function isSamePayload(
  left: FloatingTrackLevelsPayload | null,
  right: FloatingTrackLevelsPayload,
) {
  return (
    left !== null &&
    left.sessionId === right.sessionId &&
    left.mic === right.mic &&
    left.speaker === right.speaker
  );
}

/**
 * Broadcast per-track levels whenever they change. Returns an unsubscribe.
 *
 * Emitted at the same cadence the existing single-amplitude update already
 * runs at, and deduplicated so an unchanged reading costs nothing.
 */
export function startFloatingTrackLevelBroadcast(): () => void {
  let lastPayload: FloatingTrackLevelsPayload | null = null;

  const push = (state: ListenerState) => {
    const payload = getTrackLevelsPayload(state);
    if (isSamePayload(lastPayload, payload)) {
      return;
    }

    lastPayload = payload;
    void emit(FLOATING_TRACK_LEVELS_EVENT, payload).catch((error) => {
      console.error("Failed to emit floating track levels:", error);
    });
  };

  push(listenerStore.getState());

  return listenerStore.subscribe((state) => {
    push(state);
  });
}
