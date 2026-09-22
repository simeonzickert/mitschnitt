import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  deleteSession: vi.fn(),
  nativeContextMenus: [] as Array<
    Array<{
      id?: string;
      text?: string;
      action?: () => void;
      disabled?: boolean;
      separator?: boolean;
    }>
  >,
  sessionMode: "inactive",
  liveStartup: { sessionId: null as string | null, loading: false },
}));

vi.mock("@tauri-apps/plugin-os", () => ({
  platform: () => "macos",
}));

vi.mock("~/calendar/hooks", () => ({
  toTz: (value: string) => new Date(value),
  useTimezone: () => "UTC",
}));

vi.mock("~/session/meeting-folder", () => ({
  showMeetingFolder: vi.fn(),
}));

vi.mock("~/session/hooks/useDeleteSession", async (importOriginal) => ({
  ...(await importOriginal<
    typeof import("~/session/hooks/useDeleteSession")
  >()),
  useDeleteSession: () => mocks.deleteSession,
}));

vi.mock("~/shared/hooks/useNativeContextMenu", () => ({
  useNativeContextMenu: (menu: (typeof mocks.nativeContextMenus)[number]) => {
    mocks.nativeContextMenus.push(menu);
    return vi.fn();
  },
}));

vi.mock("~/store/zustand/tabs", () => ({
  useTabs: (selector: (state: { openCurrent: () => void }) => unknown) =>
    selector({ openCurrent: vi.fn() }),
}));

vi.mock("~/stt/contexts", () => ({
  useListener: (
    selector: (state: {
      getSessionMode: (sessionId: string) => string;
      live: { sessionId: string | null; loading: boolean };
    }) => unknown,
  ) =>
    selector({
      getSessionMode: () => mocks.sessionMode,
      live: mocks.liveStartup,
    }),
}));

import { SessionChip } from "./session-chip";

// Mitschnitt-Fork (ISA N8, ZICK-251; G1, Fix-Runde 2). The calendar chip
// was the third single-note delete entry -- and until 02.09.2026 the only
// one that still called deleteSession straight from the context menu, with
// an entry that never knew about a running recording. Same dialog, same
// guard as the sidebar and the note's overflow menu now.
describe("SessionChip deleting a note", () => {
  const session = {
    id: "session-1",
    title: "Planning",
    created_at: "2026-09-01T09:00:00.000Z",
    event_json: JSON.stringify({ tracking_id: "tracking-1" }),
  };

  function renderChip() {
    return render(
      <SessionChip
        sessionId="session-1"
        session={
          session as unknown as Parameters<typeof SessionChip>[0]["session"]
        }
      />,
    );
  }

  function deleteEntry() {
    const menu = mocks.nativeContextMenus.find((items) =>
      items.some((entry) => entry.id === "delete"),
    );
    const entry = menu?.find((entry) => entry.id === "delete");
    if (!entry) throw new Error("no delete entry in the context menu");
    return entry;
  }

  beforeEach(() => {
    cleanup();
    mocks.nativeContextMenus = [];
    mocks.sessionMode = "inactive";
    mocks.liveStartup = { sessionId: null, loading: false };
    mocks.deleteSession.mockReset();
    mocks.deleteSession.mockReturnValue(true);
  });

  it("asks before deleting and deletes exactly once on confirm", async () => {
    renderChip();

    expect(screen.queryByRole("dialog")).toBeNull();

    act(() => {
      deleteEntry().action?.();
    });

    expect(mocks.deleteSession).not.toHaveBeenCalled();
    expect(
      screen.getByRole("heading", { name: 'Delete "Planning"?' }),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    expect(mocks.deleteSession).toHaveBeenCalledExactlyOnceWith("session-1", {
      trackingId: "tracking-1",
      title: "Planning",
    });
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
  });

  it("never deletes when the confirmation is cancelled", async () => {
    renderChip();

    act(() => {
      deleteEntry().action?.();
    });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(mocks.deleteSession).not.toHaveBeenCalled();
  });

  it("offers a plain enabled entry for an idle note", () => {
    renderChip();

    expect(deleteEntry()).toMatchObject({
      text: "Delete Note",
      disabled: false,
    });
  });

  it.each(["active", "finalizing", "running_batch"])(
    "disables the entry with a reason while the recording is %s",
    (mode) => {
      mocks.sessionMode = mode;

      renderChip();

      expect(deleteEntry()).toMatchObject({
        text: "Delete Note (stop the recording first)",
        disabled: true,
      });
    },
  );

  it("disables the entry while the recording of this note is starting up", () => {
    mocks.liveStartup = { sessionId: "session-1", loading: true };

    renderChip();

    expect(deleteEntry().disabled).toBe(true);
  });
});
