import { describe, expect, it } from "vitest";

import {
  createTrackWatch,
  describeSilentTracks,
  getSilentTracks,
  observeTrackLevels,
  TRACK_NEVER_HEARD_TIMEOUT_MS,
  TRACK_SILENCE_TIMEOUT_MS,
  type FloatingTrackLevels,
  type TrackWatch,
} from "./track-health";

const START = 1_000_000;

/** Feed a stretch of constant levels, one reading per second. */
function feed(
  watch: TrackWatch,
  levels: FloatingTrackLevels,
  fromMs: number,
  durationMs: number,
): TrackWatch {
  let next = watch;
  for (let offset = 0; offset <= durationMs; offset += 1_000) {
    next = observeTrackLevels(next, levels, fromMs + offset);
  }
  return next;
}

describe("track health watch", () => {
  it("never warns while both tracks carry sound", () => {
    let watch = createTrackWatch(START);
    watch = feed(watch, { mic: 0.4, speaker: 0.3 }, START, 5 * 60_000);

    expect(getSilentTracks(watch, START + 5 * 60_000)).toEqual([]);
  });

  it("warns for the microphone once it stays flat, and names only that track", () => {
    let watch = createTrackWatch(START);
    watch = feed(watch, { mic: 0.4, speaker: 0.3 }, START, 5_000);

    const wentQuietAt = START + 5_000;
    watch = feed(
      watch,
      { mic: 0, speaker: 0.3 },
      wentQuietAt,
      TRACK_SILENCE_TIMEOUT_MS + 2_000,
    );

    expect(
      getSilentTracks(watch, wentQuietAt + TRACK_SILENCE_TIMEOUT_MS),
    ).toEqual(["mic"]);
    expect(
      describeSilentTracks(getSilentTracks(watch, wentQuietAt + 31_000)),
    ).toBe("No microphone signal");
  });

  it("warns for the system track when the other side goes dead", () => {
    let watch = createTrackWatch(START);
    watch = feed(watch, { mic: 0.4, speaker: 0.3 }, START, 5_000);

    const wentQuietAt = START + 5_000;
    watch = feed(
      watch,
      { mic: 0.4, speaker: 0 },
      wentQuietAt,
      TRACK_SILENCE_TIMEOUT_MS + 2_000,
    );

    expect(
      describeSilentTracks(
        getSilentTracks(watch, wentQuietAt + TRACK_SILENCE_TIMEOUT_MS),
      ),
    ).toBe("No sound from the other side");
  });

  it("treats a natural pause as a pause, not a dead track", () => {
    let watch = createTrackWatch(START);
    watch = feed(watch, { mic: 0.4, speaker: 0.3 }, START, 5_000);

    const pauseStart = START + 5_000;
    watch = feed(watch, { mic: 0, speaker: 0 }, pauseStart, 8_000);

    // Well inside the silence window: nobody is speaking, nothing is broken.
    expect(getSilentTracks(watch, pauseStart + 8_000)).toEqual([]);
    expect(
      getSilentTracks(watch, pauseStart + TRACK_SILENCE_TIMEOUT_MS - 1_000),
    ).toEqual([]);
  });

  it("does not warn about quiet speech that stays above the signal floor", () => {
    let watch = createTrackWatch(START);
    // 0.03 is quiet but real -- a distant or compressed speaker.
    watch = feed(watch, { mic: 0.5, speaker: 0.03 }, START, 5 * 60_000);

    expect(getSilentTracks(watch, START + 5 * 60_000)).toEqual([]);
  });

  it("clears the warning as soon as the track comes back", () => {
    let watch = createTrackWatch(START);
    watch = feed(watch, { mic: 0.4, speaker: 0.3 }, START, 5_000);

    const wentQuietAt = START + 5_000;
    watch = feed(
      watch,
      { mic: 0, speaker: 0.3 },
      wentQuietAt,
      TRACK_SILENCE_TIMEOUT_MS + 2_000,
    );
    const warnedAt = wentQuietAt + TRACK_SILENCE_TIMEOUT_MS + 2_000;
    expect(getSilentTracks(watch, warnedAt)).toEqual(["mic"]);

    watch = observeTrackLevels(watch, { mic: 0.35, speaker: 0.3 }, warnedAt);

    expect(getSilentTracks(watch, warnedAt)).toEqual([]);
  });

  it("warns sooner for a track that never carried sound at all", () => {
    let watch = createTrackWatch(START);
    watch = feed(
      watch,
      { mic: 0.4, speaker: 0 },
      START,
      TRACK_NEVER_HEARD_TIMEOUT_MS + 2_000,
    );

    // Earlier than the ordinary silence window: this is the clearest case.
    expect(TRACK_NEVER_HEARD_TIMEOUT_MS).toBeLessThan(TRACK_SILENCE_TIMEOUT_MS);
    expect(
      getSilentTracks(watch, START + TRACK_NEVER_HEARD_TIMEOUT_MS - 1_000),
    ).toEqual([]);
    expect(
      getSilentTracks(watch, START + TRACK_NEVER_HEARD_TIMEOUT_MS),
    ).toEqual(["speaker"]);
  });

  it("names both tracks when the whole recording is flat", () => {
    const watch = createTrackWatch(START);
    const silent = getSilentTracks(watch, START + TRACK_NEVER_HEARD_TIMEOUT_MS);

    expect(silent).toEqual(["mic", "speaker"]);
    expect(describeSilentTracks(silent)).toBe("No audio on either track");
  });

  it("says nothing when nothing is wrong", () => {
    expect(describeSilentTracks([])).toBeNull();
  });
});
