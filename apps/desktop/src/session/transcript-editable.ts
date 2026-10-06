import { t } from "@lingui/core/macro";

import { sonnerToast } from "@anlg/ui/components/ui/toast";

import { listenerStore } from "~/store/zustand/listener/instance";
import { useListener } from "~/stt/contexts";

type ListenerState = ReturnType<typeof listenerStore.getState>;

/**
 * Whether the transcript of a session may be edited right now. One predicate
 * for the header's Edit button, the transcript renderer and the selection
 * menu (Upstream-Kette #7600ff, Fix-Runde B).
 *
 * Between the stop and the start of the batch run the mode already reads
 * "inactive", but the post-stop refinement is about to REPLACE the transcript
 * with new word ids -- text fixes and deletions made in that window are lost.
 * So the post-stop window and a pending/running batch lock editing. Resume
 * is deliberately NOT tied to this (ZICK-319).
 */
export function isTranscriptEditableState(
  state: Pick<ListenerState, "getSessionMode" | "live">,
  sessionId: string,
): boolean {
  return (
    state.getSessionMode(sessionId) === "inactive" &&
    !state.live.postStopProcessingBySession?.[sessionId] &&
    !state.live.batchTranscriptionPendingBySession?.[sessionId]
  );
}

export function useTranscriptEditable(sessionId: string): boolean {
  return useListener((state) => isTranscriptEditableState(state, sessionId));
}

export type GuardedMutation<T> =
  | { allowed: true; value: T }
  | { allowed: false };

/** Thrown by the `assertEditable` the guard hands to a mutation. */
export class TranscriptLockedError extends Error {
  constructor() {
    super("The transcript is locked for post-processing");
    this.name = "TranscriptLockedError";
  }
}

/**
 * The one gate for every USER-triggered transcript mutation (text save,
 * speaker assignment, merge, split, delete, undo). It reads the store right
 * before the mutation, so a button that was still enabled when the user
 * clicked cannot write into a transcript the refinement is about to replace.
 * The buttons are disabled as well, but the safety lives here.
 *
 * The mutation receives `assertEditable`: a write that waits (a query, the
 * write queue) calls it again right before it writes, and a lock that arrived
 * meanwhile stops it with the same toast and `{ allowed: false }`.
 *
 * Internal paths (batch, refinement, live capture) and the rescue-save of
 * half-typed text on unmount do NOT go through this gate. An unknown session
 * id (a standalone segment without a session) cannot be checked and passes.
 */
export async function guardUserTranscriptMutation<T>(
  sessionId: string | undefined,
  mutation: (assertEditable: () => void) => Promise<T> | T,
): Promise<GuardedMutation<T>> {
  const assertEditable = () => {
    if (
      sessionId &&
      !isTranscriptEditableState(listenerStore.getState(), sessionId)
    ) {
      throw new TranscriptLockedError();
    }
  };
  try {
    assertEditable();
    return { allowed: true, value: await mutation(assertEditable) };
  } catch (error) {
    if (error instanceof TranscriptLockedError) {
      sonnerToast.error(
        t`The transcript is being refined right now. Editing is possible again in a moment.`,
        { id: "transcript-locked" },
      );
      return { allowed: false };
    }
    throw error;
  }
}
