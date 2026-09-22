import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DeletedSessionData } from "~/store/zustand/undo-delete";

const mocks = vi.hoisted(() => {
  const deletedSessionData: DeletedSessionData = {
    session: {
      id: "session-1",
      title: "Deleted note",
    },
    tombstone: "2026-01-01T00:00:00Z",
    deletedAt: 1,
  };

  return {
    addDeletion: vi.fn(),
    clearDeletion: vi.fn(),
    pendingDeletions: {} as Record<string, { data: DeletedSessionData }>,
    emitTo: vi.fn(() => Promise.resolve()),
    finalizeSessionDeletion: vi.fn(),
    restoreDeletedSession: vi.fn(),
    getAllWebviewWindows: vi.fn<
      () => Promise<Array<{ label: string; close: () => Promise<void> }>>
    >(() => Promise.resolve([])),
    getCurrentWebviewWindowLabel: vi.fn(() => "main"),
    ignoreEvent: vi.fn(),
    unignoreEvent: vi.fn(),
    isIgnored: vi.fn(() => false),
    invalidateResource: vi.fn(),
    openCurrent: vi.fn(),
    openTabs: [] as Array<{ type: string; id: string }>,
    listenerGetState: vi.fn(),
    listenerStop: vi.fn(),
    listen: vi.fn(),
    softDeleteSession: vi.fn<
      () => Promise<DeletedSessionData | null | "recording">
    >(() => Promise.resolve(deletedSessionData)),
    toastError: vi.fn(),
    toastWarning: vi.fn(),
    deletedSessionData,
  };
});

vi.mock("@tauri-apps/api/event", () => ({
  emitTo: mocks.emitTo,
  listen: mocks.listen,
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getAllWebviewWindows: mocks.getAllWebviewWindows,
}));

vi.mock("@anlg/plugin-windows", () => ({
  getCurrentWebviewWindowLabel: mocks.getCurrentWebviewWindowLabel,
}));

vi.mock("@anlg/ui/components/ui/toast", () => ({
  sonnerToast: { error: mocks.toastError, warning: mocks.toastWarning },
}));

vi.mock("~/calendar/ignored-events", () => ({
  useIgnoredEvents: () => ({
    ignoreEvent: mocks.ignoreEvent,
    unignoreEvent: mocks.unignoreEvent,
    isIgnored: mocks.isIgnored,
  }),
}));

vi.mock("~/session/queries", () => ({
  SOFT_DELETE_BLOCKED_BY_RECORDING: "recording",
  finalizeSessionDeletion: mocks.finalizeSessionDeletion,
  restoreDeletedSession: mocks.restoreDeletedSession,
  softDeleteSession: mocks.softDeleteSession,
}));

vi.mock("~/store/zustand/listener/instance", () => ({
  listenerStore: {
    getState: mocks.listenerGetState,
  },
}));

vi.mock("~/store/zustand/tabs", () => {
  const getState = () => ({
    tabs: mocks.openTabs,
    invalidateResource: mocks.invalidateResource,
    openCurrent: mocks.openCurrent,
  });
  const useTabs = (selector: (state: ReturnType<typeof getState>) => unknown) =>
    selector(getState());
  useTabs.getState = getState;
  return { useTabs };
});

vi.mock("~/store/zustand/undo-delete", () => {
  const getState = () => ({
    pendingDeletions: mocks.pendingDeletions,
    addDeletion: mocks.addDeletion,
    clearDeletion: mocks.clearDeletion,
  });
  const useUndoDelete = (
    selector: (state: ReturnType<typeof getState>) => unknown,
  ) => selector(getState());
  useUndoDelete.getState = getState;
  return { useUndoDelete };
});

import {
  useDeleteSession,
  useRemoteSessionDeletionUndoListener,
} from "./useDeleteSession";

describe("useDeleteSession", () => {
  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
    for (const key of Object.keys(mocks.pendingDeletions)) {
      delete mocks.pendingDeletions[key];
    }
    mocks.addDeletion.mockImplementation((data: DeletedSessionData) => {
      mocks.pendingDeletions[data.session.id] = { data };
    });
    mocks.clearDeletion.mockImplementation((sessionId: string) => {
      delete mocks.pendingDeletions[sessionId];
    });
    mocks.openTabs.length = 0;
    mocks.softDeleteSession.mockResolvedValue(mocks.deletedSessionData);
    mocks.emitTo.mockResolvedValue(undefined);
    mocks.getAllWebviewWindows.mockResolvedValue([]);
    mocks.getCurrentWebviewWindowLabel.mockReturnValue("main");
    mockListener({ sessionId: null, status: "inactive", loading: false });
    mocks.listen.mockResolvedValue(vi.fn());
  });

  // Der Store leitet den Modus einer Sitzung aus `live` ab (getSessionMode in
  // store/zustand/listener/general.ts); hier dasselbe, plus ein Batch-Lauf.
  function mockListener(
    live: {
      sessionId: string | null;
      status: "inactive" | "active" | "finalizing";
      loading: boolean;
    },
    batchSessionId: string | null = null,
  ) {
    mocks.listenerGetState.mockReturnValue({
      live,
      stop: mocks.listenerStop,
      getSessionMode: (sessionId: string) => {
        if (live.sessionId === sessionId) return live.status;
        if (batchSessionId === sessionId) return "running_batch";
        return "inactive";
      },
    });
  }

  it("adds the undo deletion optimistically in the main window", async () => {
    // Never resolves: the optimistic UI must not wait for the write.
    mocks.softDeleteSession.mockImplementation(() => new Promise(() => {}));
    const { result } = renderHook(() => useDeleteSession());

    act(() => {
      result.current("session-1", { trackingId: "tracking-1", title: "Note" });
    });

    expect(mocks.addDeletion).toHaveBeenCalledWith(
      {
        session: { id: "session-1", title: "Note" },
        tombstone: expect.any(String),
        deletedAt: expect.any(Number),
      },
      expect.any(Function),
      undefined,
    );
    expect(mocks.ignoreEvent).toHaveBeenCalledWith("tracking-1");
    expect(mocks.softDeleteSession).toHaveBeenCalledWith(
      "session-1",
      expect.any(String),
    );
    expect(mocks.invalidateResource).toHaveBeenCalledWith(
      "sessions",
      "session-1",
    );
    expect(mocks.emitTo).not.toHaveBeenCalled();
  });

  it("rolls back the optimistic deletion when the soft delete fails", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    mocks.softDeleteSession.mockRejectedValue(new Error("db locked"));
    const { result } = renderHook(() => useDeleteSession());

    act(() => {
      result.current("session-1", { trackingId: "tracking-1" });
    });

    expect(mocks.addDeletion).toHaveBeenCalledOnce();
    await waitFor(() => {
      expect(mocks.toastError).toHaveBeenCalledOnce();
    });
    expect(mocks.clearDeletion).toHaveBeenCalledWith("session-1");
    expect(mocks.unignoreEvent).toHaveBeenCalledWith("tracking-1");
    expect(mocks.finalizeSessionDeletion).not.toHaveBeenCalled();
    consoleError.mockRestore();
  });

  it("drops the optimistic toast quietly when the session is already deleted", async () => {
    mocks.softDeleteSession.mockResolvedValue(null);
    const { result } = renderHook(() => useDeleteSession());

    act(() => {
      result.current("session-1");
    });

    expect(mocks.addDeletion).toHaveBeenCalledOnce();
    await waitFor(() => {
      expect(mocks.clearDeletion).toHaveBeenCalledWith("session-1");
    });
    expect(mocks.toastError).not.toHaveBeenCalled();
    expect(mocks.unignoreEvent).not.toHaveBeenCalled();
  });

  it("never finalizes a deletion whose write failed", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    mocks.softDeleteSession.mockRejectedValue(new Error("db locked"));
    const { result } = renderHook(() => useDeleteSession());

    act(() => {
      result.current("session-1");
    });

    const finalize = mocks.addDeletion.mock.calls[0]?.[1] as () => void;
    act(() => {
      finalize();
    });

    await waitFor(() => {
      expect(mocks.toastError).toHaveBeenCalledOnce();
    });
    expect(mocks.finalizeSessionDeletion).not.toHaveBeenCalled();
    consoleError.mockRestore();
  });

  // Mitschnitt-Fork (ISA N8, ZICK-251): "Die Aufnahme ist heilig." Bis zum
  // 02.09.2026 hiess dieser Test "stops listening before deleting the active
  // session" und beschrieb das falsche Verhalten: der Hook stoppte die
  // laufende Aufnahme still (nicht einmal awaited) und tombstonierte die
  // Sitzung sofort -- ein Fehlklick nahm die Aufnahme mit. Jetzt weigert er
  // sich, stoppt nichts und sagt, warum. Die Menues davor sind das erste
  // Netz (deaktivierter Eintrag), dieser Test prueft das zweite.
  it.each([
    ["active", { sessionId: "session-1", status: "active", loading: false }],
    [
      "finalizing",
      { sessionId: "session-1", status: "finalizing", loading: false },
    ],
    [
      "starting up",
      { sessionId: "session-1", status: "inactive", loading: true },
    ],
  ] as const)(
    "refuses to delete a session whose recording is %s and does not stop it",
    (_label, live) => {
      mockListener({ ...live });
      const { result } = renderHook(() => useDeleteSession());

      let started: boolean | undefined;
      act(() => {
        started = result.current("session-1", { trackingId: "tracking-1" });
      });

      expect(mocks.listenerStop).not.toHaveBeenCalled();
      expect(mocks.softDeleteSession).not.toHaveBeenCalled();
      expect(started).toBe(false);
      expect(mocks.addDeletion).not.toHaveBeenCalled();
      expect(mocks.ignoreEvent).not.toHaveBeenCalled();
      expect(mocks.invalidateResource).not.toHaveBeenCalled();
      expect(mocks.toastError).toHaveBeenCalledOnce();
    },
  );

  it("refuses to delete a session that is being transcribed in a batch", () => {
    mockListener(
      { sessionId: null, status: "inactive", loading: false },
      "session-1",
    );
    const { result } = renderHook(() => useDeleteSession());

    let started: boolean | undefined;
    act(() => {
      started = result.current("session-1");
    });

    expect(mocks.softDeleteSession).not.toHaveBeenCalled();
    expect(started).toBe(false);
    expect(mocks.toastError).toHaveBeenCalledOnce();
  });

  // G1 (Fix-Runde 2): the guard above is the fast first net; the one that
  // counts runs inside the session's write queue (softDeleteSession) and
  // reports "recording". The hook then takes the optimistic deletion back --
  // undo toast, ignored calendar event, closed tab -- and says why.
  it("rolls back the optimistic deletion and says why when the recording started before the write reached the queue", async () => {
    mocks.softDeleteSession.mockResolvedValue("recording");
    mocks.openTabs.push({ type: "sessions", id: "session-1" });
    const { result } = renderHook(() => useDeleteSession());

    let started: boolean | undefined;
    act(() => {
      started = result.current("session-1", { trackingId: "tracking-1" });
    });

    expect(started).toBe(true);
    expect(mocks.addDeletion).toHaveBeenCalledOnce();
    await waitFor(() => {
      expect(mocks.toastError).toHaveBeenCalledOnce();
    });
    expect(mocks.toastError.mock.calls[0]?.[0]).toMatch(/still recording/);
    expect(mocks.clearDeletion).toHaveBeenCalledWith("session-1");
    expect(mocks.unignoreEvent).toHaveBeenCalledWith("tracking-1");
    expect(mocks.openCurrent).toHaveBeenCalledWith({
      type: "sessions",
      id: "session-1",
    });
    expect(mocks.finalizeSessionDeletion).not.toHaveBeenCalled();
    expect(mocks.listenerStop).not.toHaveBeenCalled();

    // The finalize callback the undo toast holds must be a no-op too.
    const finalize = mocks.addDeletion.mock
      .calls[0]?.[1] as () => Promise<void>;
    await act(async () => {
      await finalize();
    });
    expect(mocks.finalizeSessionDeletion).not.toHaveBeenCalled();
  });

  it("removes the folders when the undo window closes on an idle note", async () => {
    const { result } = renderHook(() => useDeleteSession());

    act(() => {
      result.current("session-1");
    });
    const finalize = mocks.addDeletion.mock
      .calls[0]?.[1] as () => Promise<void>;
    await act(async () => {
      await finalize();
    });

    expect(mocks.finalizeSessionDeletion).toHaveBeenCalledWith("session-1");
    expect(mocks.restoreDeletedSession).not.toHaveBeenCalled();
  });

  // G6b (Opus): the undo window is five seconds; a recording that starts on
  // the tombstoned note inside it (scheduled auto-start) must not lose its
  // folder. Finalize checks again and restores instead of removing.
  it("restores the note instead of removing its folder when a recording started inside the undo window", async () => {
    const { result } = renderHook(() => useDeleteSession());

    act(() => {
      result.current("session-1");
    });
    const finalize = mocks.addDeletion.mock
      .calls[0]?.[1] as () => Promise<void>;
    mockListener({ sessionId: "session-1", status: "active", loading: false });
    await act(async () => {
      await finalize();
    });

    expect(mocks.finalizeSessionDeletion).not.toHaveBeenCalled();
    expect(mocks.restoreDeletedSession).toHaveBeenCalledWith(
      mocks.deletedSessionData,
    );
  });

  it("deletes an inactive session while another one is recording, without touching the recorder", async () => {
    mockListener({ sessionId: "session-2", status: "active", loading: false });
    const { result } = renderHook(() => useDeleteSession());

    let started: boolean | undefined;
    act(() => {
      started = result.current("session-1");
    });

    expect(started).toBe(true);
    expect(mocks.listenerStop).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(mocks.softDeleteSession).toHaveBeenCalledWith(
        "session-1",
        expect.any(String),
      );
    });
    expect(mocks.toastError).not.toHaveBeenCalled();
  });

  it("forwards undo data to main and closes the matching note window", async () => {
    const close = vi.fn(() => Promise.resolve());
    mocks.getCurrentWebviewWindowLabel.mockReturnValue("note-session-1");
    mocks.getAllWebviewWindows.mockResolvedValue([
      { label: "note-session-1", close },
      { label: "note-session-2", close: vi.fn() },
    ]);
    const { result } = renderHook(() => useDeleteSession());

    act(() => {
      result.current("session-1");
    });

    await waitFor(() => {
      expect(mocks.emitTo).toHaveBeenCalledWith(
        "main",
        "anlg://session-deleted-for-undo",
        {
          sessionId: "session-1",
          data: mocks.deletedSessionData,
        },
      );
      expect(close).toHaveBeenCalled();
    });

    expect(mocks.addDeletion).not.toHaveBeenCalled();
  });

  it("closes the matching note window when deleting from the main window", async () => {
    const close = vi.fn(() => Promise.resolve());
    mocks.getAllWebviewWindows.mockResolvedValue([
      { label: "note-session-1", close },
    ]);
    const { result } = renderHook(() => useDeleteSession());

    act(() => {
      result.current("session-1");
    });

    await waitFor(() => {
      expect(close).toHaveBeenCalled();
    });
  });

  it("still closes the standalone note window when forwarding undo data fails", async () => {
    const close = vi.fn(() => Promise.resolve());
    mocks.getCurrentWebviewWindowLabel.mockReturnValue("note-session-1");
    mocks.emitTo.mockRejectedValue(new Error("main window unavailable"));
    mocks.getAllWebviewWindows.mockResolvedValue([
      { label: "note-session-1", close },
    ]);
    const { result } = renderHook(() => useDeleteSession());

    act(() => {
      result.current("session-1");
    });

    await waitFor(() => {
      expect(close).toHaveBeenCalled();
    });
  });

  it("listens for forwarded standalone note deletions in the main window", async () => {
    let handler:
      | ((event: {
          payload: { sessionId: string; data: DeletedSessionData };
        }) => void)
      | null = null;
    mocks.listen.mockImplementation((_, callback) => {
      handler = callback;
      return Promise.resolve(vi.fn());
    });

    renderHook(() => useRemoteSessionDeletionUndoListener(true));

    await waitFor(() => {
      expect(mocks.listen).toHaveBeenCalledWith(
        "anlg://session-deleted-for-undo",
        expect.any(Function),
      );
    });

    act(() => {
      handler?.({
        payload: {
          sessionId: "session-1",
          data: mocks.deletedSessionData,
        },
      });
    });

    expect(mocks.addDeletion).toHaveBeenCalledWith(
      mocks.deletedSessionData,
      expect.any(Function),
    );
    expect(mocks.invalidateResource).toHaveBeenCalledWith(
      "sessions",
      "session-1",
    );
    expect(mocks.softDeleteSession).not.toHaveBeenCalled();
  });
});
