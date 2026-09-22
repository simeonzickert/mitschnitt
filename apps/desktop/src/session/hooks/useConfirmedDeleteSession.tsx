import { useLingui } from "@lingui/react/macro";
import { useCallback, useState } from "react";

import { useDeleteSession } from "~/session/hooks/useDeleteSession";
import { isRecordingBusyState } from "~/session/recording-busy";
import { DestructiveConfirmationDialog } from "~/shared/ui/destructive-confirmation-dialog";
import { useListener } from "~/stt/contexts";

type PendingDeletion = {
  sessionId: string;
  title?: string;
  trackingId?: string | null;
};

/**
 * Mitschnitt-Fork (ISA N8, ZICK-251): deleting ONE note asks first.
 *
 * Until 02.09.2026 the two single-note entries -- the sidebar context menu
 * and the note's overflow menu -- called `deleteSession` straight away, while
 * the multi-select in the sidebar already went through
 * `DestructiveConfirmationDialog`. One mis-click on "Delete Note" took a
 * meeting with it, and the five-second undo toast was the only net.
 *
 * All three single-note entries (sidebar row, note overflow menu, calendar
 * chip) share this hook: `requestDelete` parks the request, `dialog` is the
 * confirmation (null while nothing is pending, so a sidebar with hundreds of
 * rows mounts nothing extra), and only the confirm button reaches
 * `useDeleteSession`. The multi-select keeps its own dialog and its batch
 * id; the silent lifecycle path for EMPTY notes
 * (shared/desktop-tab-lifecycle.ts) stays silent on purpose.
 *
 * G5 (Grok 3, Fix-Runde 2): a recording that starts while the dialog is
 * open shows up IN the dialog -- Delete is disabled, the description says
 * why, Cancel stays available -- and a refusal from the delete hook keeps
 * the dialog open rather than leaving a toast behind a closed one. Same
 * predicate as every other delete entry (session/recording-busy.ts).
 */
export function useConfirmedDeleteSession() {
  const { t } = useLingui();
  const deleteSession = useDeleteSession();
  const [pending, setPending] = useState<PendingDeletion | null>(null);
  const pendingSessionId = pending?.sessionId ?? null;
  const recording = useListener((state) =>
    pendingSessionId ? isRecordingBusyState(state, pendingSessionId) : false,
  );

  const requestDelete = useCallback(
    (sessionId: string, options?: Omit<PendingDeletion, "sessionId">) => {
      setPending({ sessionId, ...options });
    },
    [],
  );

  const confirm = useCallback(() => {
    if (!pending) return;
    const started = deleteSession(pending.sessionId, {
      trackingId: pending.trackingId,
      title: pending.title,
    });
    // A refusal keeps the dialog: if the note is recording, the selector
    // above turns Delete off and says so; the only other reason (a deletion
    // already pending for this note) is one Cancel away.
    if (started !== false) setPending(null);
  }, [deleteSession, pending]);

  const title = pending?.title;
  const dialog = pending ? (
    <DestructiveConfirmationDialog
      open
      onOpenChange={(open) => {
        if (!open) setPending(null);
      }}
      title={title ? t`Delete "${title}"?` : t`Delete this note?`}
      description={
        recording
          ? t`This note is still recording. Stop the recording before deleting it.`
          : t`You can undo this action for a short time.`
      }
      confirmLabel={t`Delete`}
      confirmDisabled={recording}
      onConfirm={confirm}
    />
  ) : null;

  return { requestDelete, dialog };
}
