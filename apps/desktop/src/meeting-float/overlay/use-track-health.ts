import { useEffect, useRef, useState } from "react";

import {
  createTrackWatch,
  getSilentTracks,
  observeTrackLevels,
  type FloatingTrack,
  type FloatingTrackLevels,
  type TrackWatch,
} from "../track-health";
import {
  listenToFloatingTrackLevels,
  type FloatingTrackLevelsPayload,
} from "../track-levels";

/**
 * How often the verdict is re-evaluated when no new level arrives.
 *
 * Silence is a statement about elapsed TIME, so it cannot be driven by
 * incoming readings alone: a dead track sends nothing new to react to.
 */
export const TRACK_HEALTH_TICK_MS = 1_000;

export type TrackHealth = {
  /** Live per-track levels, for the two separate waveforms. */
  levels: FloatingTrackLevels;
  /** Tracks currently judged dead. Empty means all is well. */
  silentTracks: FloatingTrack[];
};

const NO_LEVELS: FloatingTrackLevels = { mic: 0, speaker: 0 };

export function useTrackHealth(): TrackHealth {
  const [levels, setLevels] = useState<FloatingTrackLevels>(NO_LEVELS);
  const [silentTracks, setSilentTracks] = useState<FloatingTrack[]>([]);
  const watchRef = useRef<TrackWatch | null>(null);
  const sessionIdRef = useRef<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    const applyVerdict = () => {
      const watch = watchRef.current;
      const next = watch ? getSilentTracks(watch, Date.now()) : [];
      setSilentTracks((current) =>
        current.length === next.length &&
        current.every((track, index) => track === next[index])
          ? current
          : next,
      );
    };

    const onLevels = (payload: FloatingTrackLevelsPayload) => {
      if (cancelled) {
        return;
      }

      const now = Date.now();

      if (payload.sessionId !== sessionIdRef.current) {
        sessionIdRef.current = payload.sessionId;
        watchRef.current = payload.sessionId ? createTrackWatch(now) : null;
      }

      const nextLevels = { mic: payload.mic, speaker: payload.speaker };
      setLevels((current) =>
        current.mic === nextLevels.mic && current.speaker === nextLevels.speaker
          ? current
          : nextLevels,
      );

      if (watchRef.current) {
        watchRef.current = observeTrackLevels(
          watchRef.current,
          nextLevels,
          now,
        );
      }

      applyVerdict();
    };

    const interval = setInterval(applyVerdict, TRACK_HEALTH_TICK_MS);
    let unlisten: (() => void) | null = null;

    void listenToFloatingTrackLevels(onLevels)
      .then((nextUnlisten) => {
        if (cancelled) {
          nextUnlisten();
          return;
        }
        unlisten = nextUnlisten;
      })
      .catch((error) => {
        console.error("Failed to listen for floating track levels:", error);
      });

    return () => {
      cancelled = true;
      clearInterval(interval);
      unlisten?.();
    };
  }, []);

  return { levels, silentTracks };
}
