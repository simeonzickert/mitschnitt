import { Trans, useLingui } from "@lingui/react/macro";
import { CircleNotch, Trash } from "@phosphor-icons/react";
import { useCallback } from "react";

import { DropdownMenuItem } from "@anlg/ui/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@anlg/ui/components/ui/tooltip";
import { cn } from "@anlg/utils";

import { useAudioPlayer } from "~/audio-player";
import { isBusyRecordingMode } from "~/session/hooks/useDeleteSession";
import { useSessionSummary } from "~/session/queries";
import { useListener } from "~/stt/contexts";

export function DeleteRecording({ sessionId }: { sessionId: string }) {
  const { deleteRecording, isDeletingRecording } = useAudioPlayer();
  const mode = useListener((state) => state.getSessionMode(sessionId));
  const isDisabled = isDeletingRecording || isBusyRecordingMode(mode);

  const handleDeleteRecording = useCallback(() => {
    void deleteRecording();
  }, [deleteRecording]);

  return (
    <DropdownMenuItem
      onClick={handleDeleteRecording}
      disabled={isDisabled}
      className={cn([
        "cursor-pointer text-red-600 dark:text-red-400",
        "hover:bg-red-50 hover:text-red-700 dark:hover:bg-red-950/50 dark:hover:text-red-300",
      ])}
    >
      {isDeletingRecording ? (
        <CircleNotch className="animate-spin" />
      ) : (
        <Trash />
      )}
      <span>
        {isDeletingRecording ? (
          <Trans>Deleting...</Trans>
        ) : (
          <Trans>Delete recording</Trans>
        )}
      </span>
    </DropdownMenuItem>
  );
}

/**
 * Mitschnitt-Fork (ISA N8, ZICK-251). Two things changed on 02.09.2026:
 *
 * - The entry no longer deletes; it asks. The confirmation lives in
 *   `OverflowButton` (useConfirmedDeleteSession), outside the dropdown --
 *   the menu unmounts on close and would take a dialog rendered here with it.
 * - While the note's recording is live, finalizing or being transcribed, the
 *   entry is disabled and says why. Until then only "Delete recording" above
 *   knew about these states; "Delete" right below it deleted the note, and
 *   with it the recording that was still running.
 */
export function DeleteNote({
  sessionId,
  onRequestDelete,
}: {
  sessionId: string;
  onRequestDelete: (sessionId: string, options: { title?: string }) => void;
}) {
  const { t } = useLingui();
  const title = useSessionSummary(sessionId)?.title;
  const busy = useListener(
    (state) =>
      isBusyRecordingMode(state.getSessionMode(sessionId)) ||
      (state.live.sessionId === sessionId && state.live.loading),
  );

  const handleDeleteNote = useCallback(() => {
    onRequestDelete(sessionId, { title });
  }, [sessionId, onRequestDelete, title]);

  const item = (
    <DropdownMenuItem
      onClick={handleDeleteNote}
      disabled={busy}
      className={cn([
        "cursor-pointer text-red-600 dark:text-red-400",
        "hover:bg-red-50 hover:text-red-700 dark:hover:bg-red-950/50 dark:hover:text-red-300",
      ])}
    >
      <Trash />
      <span>
        <Trans>Delete</Trans>
      </span>
    </DropdownMenuItem>
  );

  if (!busy) {
    return item;
  }

  // A disabled menu item swallows pointer events, so the tooltip hangs on a
  // wrapper that still receives them.
  return (
    <Tooltip delayDuration={0}>
      <TooltipTrigger asChild>
        <span className="block" data-delete-note-blocked>
          {item}
        </span>
      </TooltipTrigger>
      <TooltipContent side="left">
        {t`Recording in progress. Stop it before deleting this note.`}
      </TooltipContent>
    </Tooltip>
  );
}
