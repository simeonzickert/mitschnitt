const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * How long a recording is kept, by choice.
 *
 * The offered steps were set by the operator (01.09.2026): thirty days, three months, six
 * months, a year, or never delete -- plus "don't save", which is not a length
 * at all but the decision to keep no recording in the first place.
 *
 * **The older, shorter steps are still here and still valid.** They are no
 * longer offered (see `AUDIO_RETENTION_OPTIONS` in the settings screen), but a
 * machine that already stores one must keep meaning what it said: silently
 * reading an unknown value as the default would move somebody from one day to
 * six months, or the other way round, without telling them. A stored value that
 * is not offered is shown alongside the offered ones rather than dropped.
 */
export const AUDIO_RETENTION_DURATION_MS = {
  none: 0,
  oneDay: DAY_MS,
  threeDays: 3 * DAY_MS,
  oneWeek: 7 * DAY_MS,
  oneMonth: 30 * DAY_MS,
  thirtyDays: 30 * DAY_MS,
  threeMonths: 90 * DAY_MS,
  sixMonths: 182 * DAY_MS,
  oneYear: 365 * DAY_MS,
} as const;

/** What the settings screen offers, newest thinking first (Entscheid 01.09.2026). */
export const OFFERED_AUDIO_RETENTION = [
  "none",
  "thirtyDays",
  "threeMonths",
  "sixMonths",
  "oneYear",
  "forever",
] as const;

/**
 * What a machine that has never been asked does.
 *
 * Six months, not "forever" (Entscheid 01.09.2026). Recordings are the heaviest
 * thing this app keeps and the part a person is least likely to want years of.
 */
export const DEFAULT_AUDIO_RETENTION = "sixMonths";

/**
 * What a machine that already holds meetings gets written down as, the first
 * time it is asked (Mitschnitt-Fork, F17).
 *
 * Six months is a rule for recordings that have yet to be made. Applying it to a
 * library that is already there is a different act entirely: on a machine with a
 * year of meetings the first cleanup pass after the update would delete most of
 * them, from the machine room and the meeting folders both, and nobody ever said
 * yes to that. So an installation that has never been asked keeps everything,
 * and the answer is written into the settings rather than left to a default --
 * a default is a thing that changes under you, a written value is a decision.
 */
export const UNASKED_INSTALL_AUDIO_RETENTION = "forever";

export type ExpiringAudioRetentionPolicy =
  keyof typeof AUDIO_RETENTION_DURATION_MS;
export type AudioRetentionPolicy = ExpiringAudioRetentionPolicy | "forever";

const AUDIO_RETENTION_VALUES = new Set([
  ...Object.keys(AUDIO_RETENTION_DURATION_MS),
  "forever",
]);

export function normalizeAudioRetention<
  T extends AudioRetentionPolicy | undefined = "forever",
>(value: unknown, fallback?: T): AudioRetentionPolicy | T {
  if (typeof value === "string" && AUDIO_RETENTION_VALUES.has(value)) {
    return value as AudioRetentionPolicy;
  }

  if (value === false) {
    return "none";
  }

  if (value === true) {
    return "forever";
  }

  return arguments.length >= 2 ? (fallback as T) : ("forever" as T);
}

/**
 * The policy the cleanup pass is allowed to run on.
 *
 * Mitschnitt-Fork (F17). `hasValue` is false on a machine that has never written
 * down how long it keeps recordings, and the answer then is `null` -- not the
 * default. Resolving the default here is the whole bug this guards against: a
 * library that predates the deadline would be cleared by a six-month rule its
 * owner was never asked about, from the meeting folders as well as from the app.
 */
export function retentionForCleanup(
  value: unknown,
  hasValue: boolean,
): AudioRetentionPolicy | null {
  // The fallback is spelled out rather than left to the default so an
  // unreadable stored value keeps recordings rather than picking a deadline.
  return hasValue ? normalizeAudioRetention(value, "forever") : null;
}

/**
 * How long a policy keeps a recording, as a number that can be compared.
 *
 * `forever` is not in the duration table because it has no duration; here it is
 * infinity, so "is the new choice shorter than the old one" is one comparison
 * rather than a special case at both ends.
 */
export function audioRetentionSpanMs(policy: AudioRetentionPolicy): number {
  return policy === "forever"
    ? Number.POSITIVE_INFINITY
    : AUDIO_RETENTION_DURATION_MS[policy];
}

/** Whether moving from `current` to `next` shortens how long recordings live. */
export function shortensAudioRetention(
  current: AudioRetentionPolicy,
  next: AudioRetentionPolicy,
): boolean {
  return audioRetentionSpanMs(next) < audioRetentionSpanMs(current);
}

/**
 * Whether moving to `next` has to be shown as a deletion first.
 *
 * Mitschnitt-Fork (F18). `shortensAudioRetention` compares two spans;
 * this compares what the deadline is DOING with what it would do. On an
 * installation that has never armed its deadline the two differ, and the
 * difference is a hole: the stored span is a value nobody chose and it is
 * deleting nothing, so a pick that reads longer than it -- six months to a year
 * -- is still the moment recordings start being deleted, and it deserves the
 * count just as much as a pick that reads shorter.
 */
export function retentionChangeDeletes(
  current: AudioRetentionPolicy,
  next: AudioRetentionPolicy,
  armed: boolean,
): boolean {
  return shortensAudioRetention(armed ? current : "forever", next);
}
