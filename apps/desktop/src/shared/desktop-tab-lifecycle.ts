import { isTauri } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";

import {
  isSessionEmpty,
  SOFT_DELETE_BLOCKED_BY_RECORDING,
  softDeleteSession,
} from "~/session/queries";
import { isSessionBusyRecording } from "~/session/recording-busy";
import {
  restorePinnedTabsToStore,
  restoreRecentlyOpenedToStore,
  type Tab,
  useTabs,
} from "~/store/zustand/tabs";

type InitializeDesktopTabsOptions = {
  getTabs: () => Tab[];
  setRecentlyOpenedSessionIds: (ids: string[]) => void;
  restorePinnedTabs: () => Promise<void>;
  restoreRecentlyOpenedSessionIds: (
    set: (ids: string[]) => void,
  ) => Promise<void>;
  onInitialized?: (() => void) | null;
  onZeroTabs?: (() => void) | null;
  isTauriEnv?: boolean;
};

type SessionTabCloseHandlerOptions = {
  invalidateSessionResource: (sessionId: string) => void;
  isSessionBusyRecordingFn?: typeof isSessionBusyRecording;
  isSessionEmptyFn?: typeof isSessionEmpty;
  deleteSessionFn?: typeof softDeleteSession;
};

export async function initializeDesktopTabs({
  getTabs,
  setRecentlyOpenedSessionIds,
  restorePinnedTabs,
  restoreRecentlyOpenedSessionIds,
  onInitialized,
  onZeroTabs,
  isTauriEnv = isTauri(),
}: InitializeDesktopTabsOptions) {
  if (!isTauriEnv) {
    onZeroTabs?.();
    return;
  }

  await restorePinnedTabs();
  await restoreRecentlyOpenedSessionIds(setRecentlyOpenedSessionIds);
  onInitialized?.();

  if (getTabs().length > 0) {
    return;
  }

  onZeroTabs?.();
}

/**
 * Closing the tab of an EMPTY note deletes it silently. Mitschnitt-Fork
 * (G1, Fix-Runde 2 -- Forge): the guard here is the same predicate every
 * other delete entry uses (`loading` included, which the old mode check
 * missed), and it is only the cheap early exit -- the emptiness check awaits
 * the database, and a recording can start meanwhile. The check that binds
 * runs inside the write queue (softDeleteSession), which answers
 * "recording" instead of a tombstone; that is a lost race, not a deletion.
 */
export function createSessionTabCloseHandler({
  invalidateSessionResource,
  isSessionBusyRecordingFn = isSessionBusyRecording,
  isSessionEmptyFn = isSessionEmpty,
  deleteSessionFn = softDeleteSession,
}: SessionTabCloseHandlerOptions) {
  return (tab: Tab) => {
    if (tab.type !== "sessions") {
      return;
    }

    const sessionId = tab.id;
    if (isSessionBusyRecordingFn(sessionId)) {
      return;
    }

    void (async () => {
      if (!(await isSessionEmptyFn(sessionId))) return;

      const deleted = await deleteSessionFn(sessionId);
      if (deleted && deleted !== SOFT_DELETE_BLOCKED_BY_RECORDING) {
        invalidateSessionResource(sessionId);
      }
    })().catch((error) => {
      console.error("session close cleanup", error);
    });
  };
}

export function createDesktopTabCloseHandler(
  sessionOptions: SessionTabCloseHandlerOptions,
) {
  return createSessionTabCloseHandler(sessionOptions);
}

export function useDesktopTabLifecycle({
  onEmpty,
  onInitialized,
  onZeroTabs,
}: {
  onEmpty?: (() => void) | null;
  onInitialized?: (() => void) | null;
  onZeroTabs?: (() => void) | null;
}) {
  const { registerOnEmpty, registerCanClose, registerOnClose, openNew, pin } =
    useTabs();
  const hasOpenedInitialTab = useRef(false);

  useEffect(() => {
    if (hasOpenedInitialTab.current) {
      return;
    }

    hasOpenedInitialTab.current = true;

    void initializeDesktopTabs({
      getTabs: () => useTabs.getState().tabs,
      setRecentlyOpenedSessionIds: (ids) => {
        useTabs.setState({ recentlyOpenedSessionIds: ids });
      },
      restorePinnedTabs: () =>
        restorePinnedTabsToStore(openNew, pin, () => useTabs.getState().tabs),
      restoreRecentlyOpenedSessionIds: restoreRecentlyOpenedToStore,
      onInitialized,
      onZeroTabs,
    });
  }, [onInitialized, openNew, pin, onZeroTabs]);

  useEffect(() => {
    registerOnEmpty(onEmpty ?? null);
  }, [onEmpty, registerOnEmpty]);

  useEffect(() => {
    registerCanClose(() => true);
  }, [registerCanClose]);

  useEffect(() => {
    registerOnClose(
      createDesktopTabCloseHandler({
        invalidateSessionResource: (sessionId) => {
          useTabs.getState().invalidateResource("sessions", sessionId);
        },
      }),
    );
  }, [registerOnClose]);
}
