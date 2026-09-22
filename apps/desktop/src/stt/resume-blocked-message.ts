import type { ResumeAfterStopResult } from "./useStartListening";

export type ResumeBlockedReason = Extract<
  ResumeAfterStopResult,
  { status: "blocked" }
>["reason"];

// One line of copy per reason a resume attempt can report back. Kept as a
// plain, independently testable mapping so every "Resume" entry point
// (header pill, floating button, context menu, overflow menu) shows the same
// wording instead of each re-deriving its own -- and so start_failed (a
// failure AFTER the block was cleared) never gets confused with a block
// reason that is still standing in the way.
export function resumeBlockedMessage(reason: ResumeBlockedReason): string {
  switch (reason) {
    case "session_active":
      return "This session is already recording.";
    case "session_finalizing":
      return "This meeting is still wrapping up. Try resuming again in a moment.";
    case "post_stop_processing":
      return "Mitschnitt is still finishing up the last recording. Try resuming again in a moment.";
    case "running_batch":
      return "A transcription is still running for this session. Try resuming again in a moment.";
    case "another_session_active":
      return "Another meeting is currently recording. Stop it before resuming this one.";
    case "start_in_progress":
      return "A recording is already starting. Try resuming again in a moment.";
    case "start_failed":
      return "Mitschnitt could not resume recording. Please try again.";
    case null:
    default:
      return "Mitschnitt could not resume recording right now.";
  }
}
