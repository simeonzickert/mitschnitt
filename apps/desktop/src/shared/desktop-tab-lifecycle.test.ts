import { describe, expect, it, vi } from "vitest";

import {
  createSessionTabCloseHandler,
  initializeDesktopTabs,
} from "./desktop-tab-lifecycle";

import {
  createContactsTab,
  createSessionTab,
} from "~/store/zustand/tabs/test-utils";

const flushAsyncCleanup = () =>
  new Promise((resolve) => {
    setTimeout(resolve, 0);
  });

describe("desktop tab lifecycle", () => {
  describe("initializeDesktopTabs", () => {
    it("restores pinned tabs and recent notes without opening a startup tab", async () => {
      const tabs = [createSessionTab({ id: "restored-session" })];
      const openNew = vi.fn();
      const setRecentlyOpenedSessionIds = vi.fn();
      const restorePinnedTabs = vi.fn().mockResolvedValue(undefined);
      const restoreRecentlyOpenedSessionIds = vi
        .fn()
        .mockImplementation(async (set: (ids: string[]) => void) => {
          set(["restored-session"]);
        });

      await initializeDesktopTabs({
        getTabs: () => tabs,
        setRecentlyOpenedSessionIds,
        restorePinnedTabs,
        restoreRecentlyOpenedSessionIds,
        onZeroTabs: null,
        isTauriEnv: true,
      });

      expect(restorePinnedTabs).toHaveBeenCalledTimes(1);
      expect(setRecentlyOpenedSessionIds).toHaveBeenCalledWith([
        "restored-session",
      ]);
      expect(openNew).not.toHaveBeenCalled();
    });

    it("stays on home when startup has no restored tabs", async () => {
      const openNew = vi.fn();

      await initializeDesktopTabs({
        getTabs: () => [],
        setRecentlyOpenedSessionIds: vi.fn(),
        restorePinnedTabs: vi.fn().mockResolvedValue(undefined),
        restoreRecentlyOpenedSessionIds: vi.fn().mockResolvedValue(undefined),
        onZeroTabs: null,
        isTauriEnv: true,
      });

      expect(openNew).not.toHaveBeenCalled();
    });

    it("calls onZeroTabs when startup has no restored tabs", async () => {
      const openNew = vi.fn();
      const onZeroTabs = vi.fn();

      await initializeDesktopTabs({
        getTabs: () => [],
        setRecentlyOpenedSessionIds: vi.fn(),
        restorePinnedTabs: vi.fn().mockResolvedValue(undefined),
        restoreRecentlyOpenedSessionIds: vi.fn().mockResolvedValue(undefined),
        onZeroTabs,
        isTauriEnv: true,
      });

      expect(openNew).not.toHaveBeenCalled();
      expect(onZeroTabs).toHaveBeenCalledTimes(1);
    });

    it("runs startup work even when pinned tabs were restored", async () => {
      const tabs = [createSessionTab({ id: "restored-session" })];
      const onInitialized = vi.fn();
      const onZeroTabs = vi.fn();

      await initializeDesktopTabs({
        getTabs: () => tabs,
        setRecentlyOpenedSessionIds: vi.fn(),
        restorePinnedTabs: vi.fn().mockResolvedValue(undefined),
        restoreRecentlyOpenedSessionIds: vi.fn().mockResolvedValue(undefined),
        onInitialized,
        onZeroTabs,
        isTauriEnv: true,
      });

      expect(onInitialized).toHaveBeenCalledTimes(1);
      expect(onZeroTabs).not.toHaveBeenCalled();
    });
  });

  describe("createSessionTabCloseHandler", () => {
    it("cleans up empty sessions on close", async () => {
      const invalidateSessionResource = vi.fn();
      const deleteSessionFn = vi.fn().mockResolvedValue({ session: {} });
      const handler = createSessionTabCloseHandler({
        invalidateSessionResource,
        isSessionBusyRecordingFn: () => false,
        isSessionEmptyFn: vi.fn().mockResolvedValue(true),
        deleteSessionFn,
      });

      handler(createSessionTab({ id: "session-1" }));

      await flushAsyncCleanup();

      expect(invalidateSessionResource).toHaveBeenCalledWith("session-1");
      expect(deleteSessionFn).toHaveBeenCalledWith("session-1");
    });

    it("keeps sessions that contain SQLite data", async () => {
      const invalidateSessionResource = vi.fn();
      const deleteSessionFn = vi.fn();
      const isSessionEmptyFn = vi.fn().mockResolvedValue(false);
      const handler = createSessionTabCloseHandler({
        invalidateSessionResource,
        isSessionBusyRecordingFn: () => false,
        isSessionEmptyFn,
        deleteSessionFn,
      });

      handler(createSessionTab({ id: "session-1" }));

      await flushAsyncCleanup();

      expect(deleteSessionFn).not.toHaveBeenCalled();
      expect(invalidateSessionResource).not.toHaveBeenCalled();
    });

    it("does not invalidate when the SQLite delete loses a race", async () => {
      const invalidateSessionResource = vi.fn();
      const deleteSessionFn = vi.fn().mockResolvedValue(null);
      const handler = createSessionTabCloseHandler({
        invalidateSessionResource,
        isSessionBusyRecordingFn: () => false,
        isSessionEmptyFn: vi.fn().mockResolvedValue(true),
        deleteSessionFn,
      });

      handler(createSessionTab({ id: "session-1" }));

      await flushAsyncCleanup();

      expect(deleteSessionFn).toHaveBeenCalledWith("session-1");
      expect(invalidateSessionResource).not.toHaveBeenCalled();
    });

    // G1 (Fix-Runde 2): one predicate for every delete entry -- the same
    // one the confirmed delete and the write queue use, `loading` included.
    it("skips cleanup for busy sessions and non-session tabs", async () => {
      const invalidateSessionResource = vi.fn();
      const deleteSessionFn = vi.fn();
      const isSessionEmptyFn = vi.fn().mockResolvedValue(true);
      const handler = createSessionTabCloseHandler({
        invalidateSessionResource,
        isSessionBusyRecordingFn: (sessionId) => sessionId === "session-busy",
        isSessionEmptyFn,
        deleteSessionFn,
      });

      handler(createSessionTab({ id: "session-busy" }));
      handler(createContactsTab());

      await flushAsyncCleanup();

      expect(isSessionEmptyFn).not.toHaveBeenCalled();
      expect(invalidateSessionResource).not.toHaveBeenCalled();
      expect(deleteSessionFn).not.toHaveBeenCalled();
    });

    // The emptiness check awaits the database; the primitive re-checks the
    // recording inside the write queue and answers "recording" -- the
    // handler treats that like a lost race, not like a deletion.
    it("does not invalidate when the recording started while the emptiness check ran", async () => {
      const invalidateSessionResource = vi.fn();
      const deleteSessionFn = vi.fn().mockResolvedValue("recording");
      const handler = createSessionTabCloseHandler({
        invalidateSessionResource,
        isSessionBusyRecordingFn: () => false,
        isSessionEmptyFn: vi.fn().mockResolvedValue(true),
        deleteSessionFn,
      });

      handler(createSessionTab({ id: "session-1" }));

      await flushAsyncCleanup();

      expect(deleteSessionFn).toHaveBeenCalledWith("session-1");
      expect(invalidateSessionResource).not.toHaveBeenCalled();
    });
  });
});
