import type { SettingValues } from "~/settings/schema";

/**
 * The two ways a person can answer the retention question, as the exact rows
 * each answer writes.
 *
 * Mitschnitt-Fork (F18). Two screens ask about the same deadline -- the picker
 * in the settings and the one-time question the cleanup pass raises -- and both
 * have to write the same three fields or the deadline ends up half-armed:
 * a deadline with no answer beside it never deletes, an answer with no deadline
 * beside it deletes on a span nobody chose, and `save_recordings` left behind
 * reads as "don't save" to `resolveConfigValue`.
 *
 * Keeping them here rather than inline at each call site is the point: a
 * function that two screens share cannot drift the way two object literals do,
 * and it can be tested without rendering either screen.
 */

/**
 * "Yes, this deadline, from now on" -- picked in the settings, or agreed to in
 * the one-time question the cleanup pass raises.
 *
 * Both surfaces write it, and both write the SPAN as well as the answer. In the
 * settings that is obvious; in the dialog it is the fix for a quieter fault.
 * The dialog holds the span it is describing in React state from the pass that
 * raised it, and a person can read it for a while. If an import or another
 * window moves the stored deadline in the meantime, arming "whatever is stored
 * now" would authorise a span nobody was shown -- a dialog that says six months
 * and twenty recordings could arm thirty days. Writing the span it displayed
 * binds the answer to the question.
 */
export function retentionDeadlineAnswered(value: string): SettingValues {
  return {
    audio_retention: value,
    save_recordings: value !== "none",
    audio_retention_confirmed: true,
  };
}

/**
 * "Keep everything": the deadline moves to never, and is armed at that.
 *
 * Moving it matters as much as recording the answer. Leaving the old span in
 * the picker would show six months next to a library that is never going to be
 * cleared, and the next person to touch it would be warned they are shortening
 * something that was never in force.
 */
export function retentionKeepEverything(): SettingValues {
  return {
    audio_retention: "forever",
    save_recordings: true,
    audio_retention_confirmed: true,
  };
}
