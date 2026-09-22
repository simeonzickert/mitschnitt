import { useLingui } from "@lingui/react/macro";
import { CircleNotch, FolderOpen } from "@phosphor-icons/react";
import { useMutation } from "@tanstack/react-query";
import { platform } from "@tauri-apps/plugin-os";

import { DropdownMenuItem } from "@anlg/ui/components/ui/dropdown-menu";

import { showMeetingFolder } from "~/session/meeting-folder";

export function ShowInFolder({ sessionId }: { sessionId: string }) {
  const { t } = useLingui();
  // Every other entry in this menu is translated; these three were plain
  // English and showed up untranslated in a German app.
  const label = platform() === "macos" ? t`Show in Finder` : t`Show in folder`;
  // `showMeetingFolder` reports every outcome itself, including the failure
  // this mutation used to throw into a mutation with no `onError` -- which
  // meant a click that failed did nothing a person could see.
  const { mutate, isPending } = useMutation({
    mutationFn: () => showMeetingFolder(sessionId),
  });

  return (
    <DropdownMenuItem
      onClick={(e) => {
        e.preventDefault();
        mutate();
      }}
      disabled={isPending}
      className="cursor-pointer"
    >
      {isPending ? (
        <CircleNotch className="animate-spin" />
      ) : (
        <FolderOpen data-testid="show-in-folder-icon" />
      )}
      <span>{isPending ? t`Opening...` : label}</span>
    </DropdownMenuItem>
  );
}
