/**
 * Kanal-tot-Waechter.
 *
 * A recording writes two separate tracks (`audio_mic.wav` / `audio_spk.wav`).
 * When one of them dies mid-meeting, nothing in the UI says so today: the
 * combined amplitude keeps dancing as long as the *other* track carries sound,
 * so the loss only shows up after the meeting, when the conversation is gone.
 *
 * This module is the pure decision layer. It takes per-track levels over time
 * and answers one question: which track has been practically silent long
 * enough that a human should look at it now?
 *
 * It deliberately knows nothing about React, windows or transports so both the
 * overlay and (later) any other surface can share the same verdict.
 */

export type FloatingTrack = "mic" | "speaker";

export type FloatingTrackLevels = {
  /** Microphone track, normalized 0..1 (see `updateLiveAmplitude`). */
  mic: number;
  /** System audio track (the other party), normalized 0..1. */
  speaker: number;
};

/**
 * Anything at or above this counts as "the track is alive".
 *
 * The listener store already treats 0.05 as "audible" for its transcription
 * stall watchdog. We sit deliberately below that: quiet speech, a distant
 * speaker or a compressed conference stream must still count as signal. What
 * we are looking for is a track that is flat, not a track that is quiet.
 */
export const TRACK_SIGNAL_THRESHOLD = 0.02;

/**
 * A track that HAS carried sound may fall silent for this long before we warn.
 * Natural pauses, someone thinking, a screen-share monologue on one side --
 * none of that should raise an alarm.
 */
export const TRACK_SILENCE_TIMEOUT_MS = 30_000;

/**
 * A track that has NEVER carried sound since the recording started is the
 * clearest case there is (wrong input device, permission missing, cable out),
 * so it may warn sooner.
 */
export const TRACK_NEVER_HEARD_TIMEOUT_MS = 12_000;

export type TrackWatch = {
  /** When the current recording started being watched. */
  startedAtMs: number;
  /** Last moment each track was at or above the signal threshold. */
  lastSignalAtMs: { mic: number | null; speaker: number | null };
};

export function createTrackWatch(nowMs: number): TrackWatch {
  return {
    startedAtMs: nowMs,
    lastSignalAtMs: { mic: null, speaker: null },
  };
}

/**
 * Fold one level reading into the watch.
 *
 * Returns the SAME object when nothing changed, so callers can use identity to
 * skip re-renders.
 */
export function observeTrackLevels(
  watch: TrackWatch,
  levels: FloatingTrackLevels,
  nowMs: number,
): TrackWatch {
  const micAlive = levels.mic >= TRACK_SIGNAL_THRESHOLD;
  const speakerAlive = levels.speaker >= TRACK_SIGNAL_THRESHOLD;

  if (!micAlive && !speakerAlive) {
    return watch;
  }

  return {
    startedAtMs: watch.startedAtMs,
    lastSignalAtMs: {
      mic: micAlive ? nowMs : watch.lastSignalAtMs.mic,
      speaker: speakerAlive ? nowMs : watch.lastSignalAtMs.speaker,
    },
  };
}

function isTrackSilent(
  watch: TrackWatch,
  track: FloatingTrack,
  nowMs: number,
): boolean {
  const lastSignalAtMs = watch.lastSignalAtMs[track];

  if (lastSignalAtMs === null) {
    return nowMs - watch.startedAtMs >= TRACK_NEVER_HEARD_TIMEOUT_MS;
  }

  return nowMs - lastSignalAtMs >= TRACK_SILENCE_TIMEOUT_MS;
}

/** Which tracks are currently judged dead. Empty array means all is well. */
export function getSilentTracks(
  watch: TrackWatch,
  nowMs: number,
): FloatingTrack[] {
  const silent: FloatingTrack[] = [];

  if (isTrackSilent(watch, "mic", nowMs)) {
    silent.push("mic");
  }

  if (isTrackSilent(watch, "speaker", nowMs)) {
    silent.push("speaker");
  }

  return silent;
}

/**
 * Plain-language message naming WHICH track is silent, or null when there is
 * nothing to say. Kept in the overlay's own (English) register -- the floating
 * bar does not go through the app's i18n layer.
 */
export function describeSilentTracks(tracks: FloatingTrack[]): string | null {
  const mic = tracks.includes("mic");
  const speaker = tracks.includes("speaker");

  if (mic && speaker) {
    return "No audio on either track";
  }

  if (mic) {
    return "No microphone signal";
  }

  if (speaker) {
    return "No sound from the other side";
  }

  return null;
}
