import { t } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { FolderOpen } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open as selectFolder } from "@tauri-apps/plugin-dialog";

import { Button } from "@anlg/ui/components/ui/button";
import { sonnerToast } from "@anlg/ui/components/ui/toast";

import { commands as tauriCommands } from "~/types/tauri.gen";

const QUERY_KEY = ["mirror-root"] as const;

/**
 * Turns the codes the command refuses with into a sentence.
 *
 * Both refusals are folders a person picks by accident and cannot see anything
 * wrong with, so the message has to say why -- "mirror_root_nested_in_current"
 * on screen is a shrug.
 */
function explainMoveFailure(code: string): string {
  if (code === "mirror_root_contains_app_data") {
    return t`That folder contains Mitschnitt's own data folder. Please pick one outside it, so your database never ends up in a synced folder.`;
  }
  if (code === "mirror_root_nested_in_current") {
    return t`That folder is inside your current meeting folder, or contains it. Please pick one next to it instead.`;
  }
  return t`Could not move your meetings: ${code}`;
}

/**
 * Mitschnitt-Fork (F16c). Choosing where the readable folder per meeting lives.
 *
 * Moving the folders and recording the choice is **one command**, in Rust. This
 * used to be two steps with the setting written from here afterwards, and the
 * gap between them was long enough to matter: the background watcher woke up
 * inside it, still resolved the old root, found it empty, read that as "every
 * meeting is missing" and wrote them all back there. Ordering the two steps only
 * made the window small. So this screen asks for a folder, shows what happened,
 * and writes nothing itself.
 */
export function MirrorFolderRow() {
  const { t } = useLingui();
  const queryClient = useQueryClient();
  const rootQuery = useQuery({
    queryKey: QUERY_KEY,
    queryFn: async () => {
      const result = await tauriCommands.mirrorRoot();
      if (result.status === "error") {
        throw new Error(result.error);
      }
      return result.data;
    },
  });

  const chooseMutation = useMutation({
    mutationFn: async () => {
      const selected = await selectFolder({
        title: t`Choose the folder for your meetings`,
        directory: true,
        multiple: false,
        defaultPath: rootQuery.data || undefined,
      });
      if (typeof selected !== "string" || !selected) {
        return null;
      }

      const moved = await tauriCommands.mirrorMoveRoot(selected);
      if (moved.status === "error") {
        throw new Error(moved.error);
      }
      return moved.data;
    },
    onSuccess: async (report) => {
      if (!report) return;
      await queryClient.invalidateQueries({ queryKey: QUERY_KEY });

      // Nothing moved and nothing switched: the move is all or nothing, so the
      // meetings and the app are still together at the old place and trying
      // again after fixing the named folders is safe.
      if (report.failedTotal > 0) {
        sonnerToast.error(
          t`Your meetings were not moved and are all still in the old place. ${report.failedTotal} folder(s) got in the way: ${report.failed
            .map((failure) => failure.folder)
            .join(", ")}`,
          { duration: Number.POSITIVE_INFINITY },
        );
        return;
      }
      sonnerToast.success(t`Moved ${report.moved} meeting folders.`);
    },
    onError: (error: Error) =>
      sonnerToast.error(explainMoveFailure(error.message), {
        duration: Number.POSITIVE_INFINITY,
      }),
  });

  return (
    <div className="flex items-start justify-between gap-4">
      <div className="flex flex-col gap-1">
        <span className="text-sm font-medium">
          <Trans>Meeting folder</Trans>
        </span>
        <span className="text-muted-foreground text-xs">
          <Trans>
            Every meeting gets one folder here, with its recording, transcript
            and summary. A synced folder works; the database itself always stays
            on this machine.
          </Trans>
        </span>
        <span className="text-muted-foreground font-mono text-xs break-all">
          {rootQuery.data ?? "…"}
        </span>
      </div>
      <Button
        type="button"
        size="sm"
        variant="outline"
        onClick={() => chooseMutation.mutate()}
        disabled={chooseMutation.isPending || rootQuery.isPending}
      >
        <FolderOpen size={14} />
        <Trans>Choose folder</Trans>
      </Button>
    </div>
  );
}
