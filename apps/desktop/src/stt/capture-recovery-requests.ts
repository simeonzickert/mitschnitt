import { emit, listen } from "@tauri-apps/api/event";

const CAPTURE_RECOVERY_REQUEST_EVENT = "anlg:capture-recovery-request";

// Sessions whose automatic capture recovery is deliberately suspended, with a
// holder count so overlapping resumes cannot release each other's guard. A
// recovery run finalizes the session (it marks post-stop processing and
// repairs the transcript from the recording); while the user is deliberately
// resuming that very session, that is exactly the wrong thing to do.
const cancelledSessions = new Map<string, number>();
const cancellationListeners = new Set<(sessionId: string) => void>();

export function isCaptureRecoveryCancelled(sessionId: string): boolean {
  return (cancelledSessions.get(sessionId) ?? 0) > 0;
}

// Returns the release for exactly this holder; releasing twice is a no-op.
//
// The registry is module state, so it only covers this window. That is enough
// because both things it guards - LiveCaptureRecovery and the listener control
// bridge - are mounted in the main window only (main/lifecycle.tsx,
// ClassicMainServices). The suppression in listenCaptureRecoveryRequests below
// is what keeps a wakeup emitted by another window out.
export function cancelCaptureRecovery(sessionId: string): () => void {
  cancelledSessions.set(sessionId, (cancelledSessions.get(sessionId) ?? 0) + 1);
  for (const listener of cancellationListeners) {
    try {
      listener(sessionId);
    } catch (error) {
      console.error(
        "[listener] capture recovery cancellation listener failed",
        error,
      );
    }
  }

  let released = false;
  return () => {
    if (released) {
      return;
    }
    released = true;
    const holders = (cancelledSessions.get(sessionId) ?? 1) - 1;
    if (holders > 0) {
      cancelledSessions.set(sessionId, holders);
    } else {
      cancelledSessions.delete(sessionId);
    }
  };
}

export function subscribeCaptureRecoveryCancelled(
  onCancelled: (sessionId: string) => void,
): () => void {
  cancellationListeners.add(onCancelled);
  return () => {
    cancellationListeners.delete(onCancelled);
  };
}

export function requestCaptureRecovery(sessionId: string): Promise<void> {
  if (isCaptureRecoveryCancelled(sessionId)) {
    console.info(
      "[listener] skipped capture recovery wakeup for a deliberately resumed session",
      { sessionId },
    );
    return Promise.resolve();
  }

  return emit(CAPTURE_RECOVERY_REQUEST_EVENT, { sessionId });
}

export function listenCaptureRecoveryRequests(
  onRequest: (sessionId: string) => void,
) {
  return listen<unknown>(CAPTURE_RECOVERY_REQUEST_EVENT, ({ payload }) => {
    if (!payload || typeof payload !== "object") {
      return;
    }

    const sessionId = (payload as { sessionId?: unknown }).sessionId;
    if (typeof sessionId !== "string" || !sessionId.trim()) {
      return;
    }

    // A wakeup can also arrive from another window, where the suppression in
    // requestCaptureRecovery did not apply.
    if (isCaptureRecoveryCancelled(sessionId)) {
      return;
    }

    onRequest(sessionId);
  });
}
