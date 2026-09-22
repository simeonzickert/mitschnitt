import { listenerStore } from "~/store/zustand/listener/instance";

type ListenerState = ReturnType<typeof listenerStore.getState>;
type SessionMode = ReturnType<ListenerState["getSessionMode"]>;

/**
 * Whether a session's recording is in a state that must not be deleted from
 * under it: live, finalizing, still starting up, or being transcribed in a
 * batch. Mitschnitt-Fork (ISA N8, ZICK-251).
 *
 * One predicate for every delete entry -- the sidebar context menu, the
 * note's overflow menu, the calendar chip, the silent close of an empty
 * note, and the write queue itself (session/queries/deletion.ts), which asks
 * once more right before the tombstone. `loading` covers the seconds between
 * pressing Record and the first audio frame, where the mode still reads
 * "inactive". Lives outside the hook so the queries layer can import it
 * without pulling React in.
 */
export function isBusyRecordingMode(mode: SessionMode): boolean {
  return mode === "active" || mode === "finalizing" || mode === "running_batch";
}

export function isRecordingBusyState(
  state: Pick<ListenerState, "getSessionMode" | "live">,
  sessionId: string,
): boolean {
  return (
    isBusyRecordingMode(state.getSessionMode(sessionId)) ||
    (state.live.sessionId === sessionId && state.live.loading)
  );
}

export function isSessionBusyRecording(sessionId: string): boolean {
  return isRecordingBusyState(listenerStore.getState(), sessionId);
}

/**
 * Whether ANY session is currently busy recording -- not scoped to one id.
 * For guards that are not "may I delete this note" but "may I disrupt the
 * app as a whole right now" (e.g. moving the storage location, which closes
 * the database pool mid-move; see vault_move.rs). Checks every session the
 * store tracks as busy: the one live recording, any session still
 * finalizing, and any session still running a batch transcription --
 * `getSessionMode` already knows how to read all three, this just asks it
 * about every candidate instead of one.
 */
export function isAnyRecordingBusy(): boolean {
  const state = listenerStore.getState();
  const candidateIds = new Set<string>();
  if (state.live.sessionId) {
    candidateIds.add(state.live.sessionId);
  }
  Object.keys(state.live.finalizingBySession).forEach((id) => candidateIds.add(id));
  Object.keys(state.batch).forEach((id) => candidateIds.add(id));

  for (const id of candidateIds) {
    if (isRecordingBusyState(state, id)) {
      return true;
    }
  }
  return false;
}
