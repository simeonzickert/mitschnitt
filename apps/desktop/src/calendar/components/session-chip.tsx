import { useLingui } from "@lingui/react/macro";
import { platform } from "@tauri-apps/plugin-os";
import { format } from "date-fns";
import { useCallback, useMemo } from "react";

import { Button } from "@anlg/ui/components/ui/button";
import {
  AppFloatingPanel,
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@anlg/ui/components/ui/popover";
import { cn } from "@anlg/utils";

import { toTz, useTimezone } from "~/calendar/hooks";
import { useConfirmedDeleteSession } from "~/session/hooks/useConfirmedDeleteSession";
import { showMeetingFolder } from "~/session/meeting-folder";
import { isRecordingBusyState } from "~/session/recording-busy";
import { getSessionEvent } from "~/session/utils";
import {
  type MenuItemDef,
  useNativeContextMenu,
} from "~/shared/hooks/useNativeContextMenu";
import type { TimelineSessionRow } from "~/sidebar/timeline/utils";
import { useTabs } from "~/store/zustand/tabs";
import { useListener } from "~/stt/contexts";

export function SessionChip({
  sessionId,
  session,
}: {
  sessionId: string;
  session: TimelineSessionRow | undefined;
}) {
  const { t } = useLingui();
  const tz = useTimezone();
  // Mitschnitt-Fork (ISA N8, ZICK-251; G1, Fix-Runde 2 -- Forge): the third
  // single-note delete entry, and until 02.09.2026 the only one left that
  // called deleteSession straight from the context menu, with an entry that
  // never knew about a running recording. Same dialog, same guard as the
  // sidebar row and the note's overflow menu.
  const { requestDelete, dialog: deleteConfirmation } =
    useConfirmedDeleteSession();
  const deleteBlocked = useListener((state) =>
    isRecordingBusyState(state, sessionId),
  );
  const title = session?.title ?? undefined;
  const eventJson = session?.event_json;
  const createdAt = session?.created_at
    ? format(toTz(session.created_at, tz), "h:mm a")
    : null;

  const handleShowInFolder = useCallback(async () => {
    await showMeetingFolder(sessionId);
  }, [sessionId]);

  const handleDelete = useCallback(() => {
    const sessionEvent = getSessionEvent({ event_json: eventJson });
    requestDelete(sessionId, {
      trackingId: sessionEvent?.tracking_id,
      title,
    });
  }, [requestDelete, sessionId, eventJson, title]);

  const contextMenu = useMemo<MenuItemDef[]>(
    () => [
      {
        id: "show",
        text: platform() === "macos" ? t`Show in Finder` : t`Show in folder`,
        action: handleShowInFolder,
      },
      { separator: true },
      {
        id: "delete",
        // A native menu has no tooltip, so the reason rides in the label.
        text: deleteBlocked
          ? t`Delete Note (stop the recording first)`
          : t`Delete Note`,
        action: handleDelete,
        disabled: deleteBlocked,
      },
    ],
    [t, handleShowInFolder, handleDelete, deleteBlocked],
  );
  const showContextMenu = useNativeContextMenu(contextMenu);

  if (!session || !title) {
    return null;
  }

  return (
    <>
      {deleteConfirmation}
      <Popover>
        <PopoverTrigger asChild>
          <button
            className={cn([
              "flex w-full items-center gap-1 rounded pl-0.5 text-left text-xs leading-tight",
              "cursor-pointer select-none hover:opacity-80",
            ])}
            onContextMenu={showContextMenu}
          >
            <div className="border-border w-[4px] shrink-0 self-stretch rounded-full border bg-transparent" />
            <span className="truncate">{title}</span>
            {createdAt && (
              <span className="text-muted-foreground ml-auto shrink-0 font-mono">
                {createdAt}
              </span>
            )}
          </button>
        </PopoverTrigger>
        <PopoverContent
          variant="app"
          align="start"
          className="w-[280px]"
          onClick={(e) => e.stopPropagation()}
        >
          <AppFloatingPanel>
            <SessionPopoverContent sessionId={sessionId} session={session} />
          </AppFloatingPanel>
        </PopoverContent>
      </Popover>
    </>
  );
}

function SessionPopoverContent({
  sessionId,
  session,
}: {
  sessionId: string;
  session: TimelineSessionRow;
}) {
  const openCurrent = useTabs((state) => state.openCurrent);
  const tz = useTimezone();

  const handleOpen = useCallback(() => {
    openCurrent({ type: "sessions", id: sessionId });
  }, [openCurrent, sessionId]);

  const createdAt = session.created_at
    ? format(toTz(session.created_at, tz), "MMM d, yyyy h:mm a")
    : null;

  return (
    <div className="flex flex-col gap-3 p-4">
      <div className="text-foreground text-base font-medium">
        {session.title}
      </div>
      <div className="bg-accent h-px" />
      {createdAt && (
        <div className="text-muted-foreground text-sm">{createdAt}</div>
      )}
      <Button
        size="sm"
        className="bg-primary text-primary-foreground hover:bg-primary/90 min-h-8 w-full"
        onClick={handleOpen}
      >
        Open note
      </Button>
    </div>
  );
}
