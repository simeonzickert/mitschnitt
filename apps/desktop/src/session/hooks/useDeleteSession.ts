import { t } from "@lingui/core/macro";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import { useCallback, useEffect } from "react";

import { getCurrentWebviewWindowLabel } from "@anlg/plugin-windows";
import { sonnerToast } from "@anlg/ui/components/ui/toast";

import { useIgnoredEvents } from "~/calendar/ignored-events";
import { trackPendingSoftDelete } from "~/session/pending-soft-deletes";
import {
  finalizeSessionDeletion,
  restoreDeletedSession,
  SOFT_DELETE_BLOCKED_BY_RECORDING,
  softDeleteSession,
} from "~/session/queries";
import { isSessionBusyRecording } from "~/session/recording-busy";
import { useTabs } from "~/store/zustand/tabs";
import {
  type DeletedSessionData,
  useUndoDelete,
} from "~/store/zustand/undo-delete";

const SESSION_DELETED_FOR_UNDO_EVENT = "anlg://session-deleted-for-undo";

type SessionDeletedForUndoPayload = {
  sessionId: string;
  data: DeletedSessionData;
};

async function closeSessionNoteWindows(sessionId: string) {
  try {
    const noteWindowLabel = `note-${sessionId}`;
    const windows = await getAllWebviewWindows();
    await Promise.all(
      windows
        .filter((window) => window.label === noteWindowLabel)
        .map((window) => window.close().catch(() => undefined)),
    );
  } catch {
    // Closing note windows should not block the deletion path.
  }
}

// The recording guard moved to ~/session/recording-busy.ts (G1, Fix-Runde
// 2) so the queries layer can run it inside the write queue; re-exported
// for the menus that import it from here.
export {
  isBusyRecordingMode,
  isSessionBusyRecording,
} from "~/session/recording-busy";

/**
 * What happens when the undo window closes. G6b (Opus, Fix-Runde 2): the
 * window is five seconds, and a scheduled auto-start can begin a recording
 * on the tombstoned note inside it -- removing the folder then would take
 * the running recording's files. So: ask once more, and if the note is
 * recording, put it back instead of removing anything.
 */
async function finalizeDeletedSession(
  sessionId: string,
  deletedData: DeletedSessionData,
): Promise<void> {
  if (isSessionBusyRecording(sessionId)) {
    await restoreDeletedSession(deletedData);
    return;
  }
  await finalizeSessionDeletion(sessionId);
}

function isSessionDeletedForUndoPayload(
  payload: unknown,
): payload is SessionDeletedForUndoPayload {
  return (
    typeof payload === "object" &&
    payload !== null &&
    "sessionId" in payload &&
    typeof payload.sessionId === "string" &&
    "data" in payload &&
    typeof payload.data === "object" &&
    payload.data !== null
  );
}

export function useDeleteSession() {
  const invalidateResource = useTabs((state) => state.invalidateResource);
  const addDeletion = useUndoDelete((state) => state.addDeletion);
  const clearDeletion = useUndoDelete((state) => state.clearDeletion);
  const { ignoreEvent, unignoreEvent, isIgnored } = useIgnoredEvents();

  return useCallback(
    (
      sessionId: string,
      options?: {
        trackingId?: string | null;
        batchId?: string;
        title?: string;
      },
    ) => {
      const { trackingId, batchId, title } = options ?? {};
      // A repeat delete would replace the pending tombstone and its finalize
      // callback, then no-op in softDeleteSession and clear the undo toast —
      // leaving the note soft-deleted with no undo and no share cleanup.
      if (useUndoDelete.getState().pendingDeletions[sessionId]) {
        return false;
      }
      const windowLabel = getCurrentWebviewWindowLabel();
      const isMainWindow = windowLabel === "main";

      // Mitschnitt-Fork (ISA N8, ZICK-251): a note whose recording is running,
      // finalizing, starting up or being transcribed in a batch is not
      // deletable. Until 02.09.2026 this hook stopped the live recording
      // itself -- not even awaited -- and tombstoned the note right away, so a
      // mis-click during a meeting took the recording with it. The menus
      // disable the entry first (sidebar context menu, note overflow menu,
      // calendar chip); this is the second net, for every caller that reaches
      // the hook without them, and it says why instead of doing anything
      // quietly. The third and binding one runs inside the write queue
      // (softDeleteSession, G1) -- this check only spares the optimistic
      // hide-and-rollback below when the answer is already known.
      if (isSessionBusyRecording(sessionId)) {
        sonnerToast.error(
          t`This note is still recording. Stop the recording before deleting it.`,
        );
        return false;
      }

      // Optimistic path: hide the row, drop tab history, and show the undo
      // toast before the soft-delete commits; rolled back below on failure.
      const tombstone = new Date().toISOString();
      const wasIgnored = trackingId ? isIgnored(trackingId, null) : false;
      const hadOpenTab = useTabs
        .getState()
        .tabs.some((tab) => tab.type === "sessions" && tab.id === sessionId);
      if (trackingId) ignoreEvent(trackingId);
      invalidateResource("sessions", sessionId);

      const commit = softDeleteSession(sessionId, tombstone);
      trackPendingSoftDelete(sessionId, commit);

      const clearOptimisticDeletion = () => {
        const pending = useUndoDelete.getState().pendingDeletions[sessionId];
        if (pending?.data.tombstone === tombstone) {
          clearDeletion(sessionId);
        }
      };
      const rollbackOptimisticDeletion = () => {
        if (isMainWindow) clearOptimisticDeletion();
        // Only undo the optimistic ignore; a pre-existing ignore must
        // survive a delete that did not happen.
        if (trackingId && !wasIgnored) unignoreEvent(trackingId);
        if (hadOpenTab) {
          useTabs.getState().openCurrent({ type: "sessions", id: sessionId });
        }
      };

      if (isMainWindow) {
        // Finalize gates on the commit so a failed, blocked or no-op delete
        // never removes the session folder or revokes the shared link. It
        // returns its promise so app exit can await the share revocation.
        const finalize = () =>
          commit
            .then(async (deletedData) => {
              if (
                !deletedData ||
                deletedData === SOFT_DELETE_BLOCKED_BY_RECORDING
              ) {
                return;
              }
              await finalizeDeletedSession(sessionId, deletedData);
            })
            .catch(() => undefined);
        addDeletion(
          {
            session: { id: sessionId, title: title ?? "" },
            tombstone,
            deletedAt: Date.now(),
          },
          finalize,
          batchId,
        );
      }

      void (async () => {
        let didDelete = false;
        try {
          const deletedData = await commit;
          if (deletedData === SOFT_DELETE_BLOCKED_BY_RECORDING) {
            // The recording started after the check above and before the
            // write's turn in the queue. Nothing was written; take the
            // optimistic deletion back and say why.
            rollbackOptimisticDeletion();
            sonnerToast.error(
              t`This note is still recording. Stop the recording before deleting it.`,
            );
            return;
          }
          if (!deletedData) {
            // The session was already deleted; drop the optimistic toast.
            if (isMainWindow) clearOptimisticDeletion();
            return;
          }
          didDelete = true;

          if (!isMainWindow) {
            await emitTo("main", SESSION_DELETED_FOR_UNDO_EVENT, {
              sessionId,
              data: deletedData,
            } satisfies SessionDeletedForUndoPayload);
          }
        } catch (error) {
          console.error("[delete-session] failed to finish deletion", error);
          if (!didDelete) {
            rollbackOptimisticDeletion();
            sonnerToast.error(t`Could not delete this note. Please try again.`);
          } else {
            // The delete committed but main never learned about it, so its
            // finalize-time cleanup will not run. Finalize here — losing the
            // undo window beats leaving the shared link live forever.
            void finalizeSessionDeletion(sessionId);
          }
        } finally {
          if (didDelete) {
            await closeSessionNoteWindows(sessionId);
          }
        }
      })();
      return true;
    },
    [
      ignoreEvent,
      unignoreEvent,
      isIgnored,
      invalidateResource,
      addDeletion,
      clearDeletion,
    ],
  );
}

export function useRemoteSessionDeletionUndoListener(active: boolean) {
  const invalidateResource = useTabs((state) => state.invalidateResource);
  const addDeletion = useUndoDelete((state) => state.addDeletion);

  useEffect(() => {
    if (!active) {
      return;
    }

    let unlisten: (() => void) | undefined;

    void listen(SESSION_DELETED_FOR_UNDO_EVENT, (event) => {
      const payload = event.payload;
      if (!isSessionDeletedForUndoPayload(payload)) {
        return;
      }

      invalidateResource("sessions", payload.sessionId);
      addDeletion(payload.data, async () => {
        await finalizeDeletedSession(payload.sessionId, payload.data);
      });
      void closeSessionNoteWindows(payload.sessionId);
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [active, invalidateResource, addDeletion]);
}
