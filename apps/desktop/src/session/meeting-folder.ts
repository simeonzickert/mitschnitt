import { t } from "@lingui/core/macro";

import { commands as openerCommands } from "@anlg/plugin-opener2";
import { sonnerToast } from "@anlg/ui/components/ui/toast";

import { commands as tauriCommands } from "~/types/tauri.gen";

/**
 * Mitschnitt-Fork (F16b). The one door to a meeting on disk.
 *
 * Every "Show in Finder" in the app goes through here. Before F16 they resolved
 * `fsSyncCommands.sessionDir`, which opens `sessions/<uuid>`: a folder named
 * after a database id, holding the recording and nothing a person can read.
 * The transcript and the summary were somewhere else entirely -- two places for
 * one meeting.
 *
 * There is deliberately no fallback to that folder. A fallback would fire
 * exactly in the confusing case (a meeting so new the mirror has not caught up)
 * and would hand the person the machine room while telling them nothing.
 */
export type OpenMeetingFolderResult =
  | { status: "opened" }
  /** Mirrored folder does not exist yet -- the watcher runs a moment behind. */
  | { status: "not-yet" }
  | { status: "failed"; error: string };

export async function openMeetingFolder(
  sessionId: string,
): Promise<OpenMeetingFolderResult> {
  const found = await tauriCommands.mirrorSessionFolder(sessionId);
  if (found.status === "error") {
    return { status: "failed", error: found.error };
  }
  if (!found.data) {
    return { status: "not-yet" };
  }

  const opened = await openerCommands.openPath(found.data, null);
  if (opened?.status === "error") {
    return { status: "failed", error: opened.error };
  }
  return { status: "opened" };
}

/**
 * Opens the folder and says what happened. The only thing a caller should use.
 *
 * All three "Show in Finder" entries handled `not-yet` and let `failed` fall
 * through, so a click on a folder that could not be opened did nothing at all,
 * visibly. Three call sites cannot each be relied on to remember a third case,
 * so there is one place that reports and the call sites just ask.
 */
export async function showMeetingFolder(sessionId: string): Promise<void> {
  let result: OpenMeetingFolderResult;
  try {
    result = await openMeetingFolder(sessionId);
  } catch (error) {
    result = {
      status: "failed",
      error: error instanceof Error ? error.message : String(error),
    };
  }

  if (result.status === "not-yet") {
    sonnerToast.info(t`This meeting's folder is still being written.`);
    return;
  }
  if (result.status === "failed") {
    sonnerToast.error(t`Could not open this meeting's folder: ${result.error}`);
  }
}
