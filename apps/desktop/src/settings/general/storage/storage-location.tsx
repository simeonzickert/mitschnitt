import { Trans, useLingui } from "@lingui/react/macro";
import { CircleNotch, FolderSimple } from "@phosphor-icons/react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { homeDir } from "@tauri-apps/api/path";
import { open as selectFolder } from "@tauri-apps/plugin-dialog";

import { commands as openerCommands } from "@anlg/plugin-opener2";
import { commands as settingsCommands } from "@anlg/plugin-settings";
import { Button } from "@anlg/ui/components/ui/button";

import { displayPath } from "./path-utils";

import { isAnyRecordingBusy } from "~/session/recording-busy";
import {
  flushApplicationState,
  scheduleAutomaticRelaunch,
} from "~/shared/relaunch";
import { commands as appCommands } from "~/types/tauri.gen";

// Mirrors vault_move.rs's DATABASE_POOL_CLOSED_MARKER -- see the comment on
// that constant for why this is a string prefix rather than a typed error
// variant, and the mutation below for how it is used.
const DATABASE_POOL_CLOSED_MARKER = "DATABASE_POOL_CLOSED: ";

/**
 * Where Mitschnitt's own database (app.db) lives, and where the vault items
 * (sessions, humans, ...) live -- not the human-readable meeting mirror (see
 * mirror-folder.tsx), which is a separate, always-shown setting and is not
 * touched by this move (it rebuilds itself from the database at the new
 * location; see vault_move.rs's module doc). This one is upstream's "vault
 * base" (#7340), restored here.
 *
 * The move happens entirely in Rust (appCommands.moveVault, in
 * vault_move.rs -- not the settings plugin's own moveVault, which existed
 * before this and never touched app.db): it copies the vault items first,
 * then checkpoints and closes the database and copies it too, then repoints
 * the app at the new path, and only then removes the old copy best-effort.
 * A failed copy at any step before the database is touched never leaves the
 * running app pointed at a half-copied location or with a closed database:
 * the config still names the old path, the pool is still open, and the
 * error just surfaces here for a retry. A target that already has files in
 * it is rejected before anything is touched, and a move is refused outright
 * while a recording is in progress (checked here, before Rust is even asked
 * -- closing the database pool mid-recording would be a self-inflicted
 * version of exactly the failure this move exists to prevent).
 *
 * The one exception is the database file copy itself: by the time it runs,
 * the pool is already closed (copying it safely requires that -- see
 * `Db::checkpoint_and_close`), so a failure there needs a relaunch to make
 * the app usable again no matter what. That is the one case
 * `DATABASE_POOL_CLOSED_MARKER` exists to tell apart from every other error
 * below.
 */
export function StorageLocationRow() {
  const { t } = useLingui();
  const { data: home } = useQuery({ queryKey: ["home-dir"], queryFn: homeDir });
  const vaultBaseQuery = useQuery({
    queryKey: ["vault-base-path"],
    queryFn: async () => {
      const result = await settingsCommands.vaultBase();
      if (result.status === "error") {
        throw new Error(result.error);
      }
      return result.data;
    },
  });
  const changeMutation = useMutation({
    mutationFn: async (newPath: string) => {
      if (isAnyRecordingBusy()) {
        throw new Error(
          t`Can't change the storage location while a recording is in progress. Stop it first, then try again.`,
        );
      }

      await flushApplicationState();

      const moveResult = await appCommands.moveVault(newPath);

      if (moveResult.status === "error") {
        // vault_move.rs's DATABASE_POOL_CLOSED_MARKER: on every other error
        // the database's pool was never touched and the app is still fully
        // usable, so the person just sees the message and can try again. On
        // this one specific branch the pool is already closed (the database
        // file copy itself failed, after the checkpoint-and-close step that
        // makes copying it safe already succeeded) -- the app is not usable
        // again until it relaunches, whether the move overall counts as a
        // success or not.
        if (moveResult.error.startsWith(DATABASE_POOL_CLOSED_MARKER)) {
          await scheduleAutomaticRelaunch();
          throw new Error(moveResult.error.slice(DATABASE_POOL_CLOSED_MARKER.length));
        }
        throw new Error(moveResult.error);
      }

      await scheduleAutomaticRelaunch();
    },
  });

  const handleChange = async () => {
    const selected = await selectFolder({
      title: t`Choose storage location`,
      directory: true,
      multiple: false,
      defaultPath: vaultBaseQuery.data,
    });

    if (selected && selected !== vaultBaseQuery.data) {
      changeMutation.mutate(selected);
    }
  };

  return (
    <div>
      <div className="grid grid-cols-[minmax(0,1fr)_9rem] items-center gap-3">
        <button
          type="button"
          className="hover:bg-muted/40 flex min-w-0 items-center gap-2 rounded-lg px-2 py-2 text-left transition-colors"
          disabled={!vaultBaseQuery.data}
          onClick={() => {
            if (vaultBaseQuery.data) {
              void openerCommands.openPath(vaultBaseQuery.data, null);
            }
          }}
        >
          <FolderSimple className="text-muted-foreground size-4 shrink-0" />
          <div className="min-w-0">
            <p className="text-sm font-medium">
              <Trans>Where your notes and recordings are stored</Trans>
            </p>
            <p className="text-muted-foreground truncate text-xs">
              {displayPath(vaultBaseQuery.data, home)}
            </p>
          </div>
        </button>
        <Button
          variant="outline"
          className="h-9 w-full justify-center"
          disabled={vaultBaseQuery.isPending || changeMutation.isPending}
          onClick={() => void handleChange()}
        >
          {changeMutation.isPending && (
            <CircleNotch className="size-4 animate-spin" aria-hidden="true" />
          )}
          <Trans>Change</Trans>
        </Button>
      </div>
      {(vaultBaseQuery.error || changeMutation.error) && (
        <p className="mt-1 text-xs text-red-500">
          {(vaultBaseQuery.error ?? changeMutation.error)?.message}
        </p>
      )}
    </div>
  );
}
