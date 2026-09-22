import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  flushApplicationState: vi.fn(),
  homeDir: vi.fn(),
  isAnyRecordingBusy: vi.fn(),
  moveVault: vi.fn(),
  openPath: vi.fn(),
  scheduleAutomaticRelaunch: vi.fn(),
  selectFolder: vi.fn(),
  vaultBase: vi.fn(),
}));

vi.mock("@lingui/react/macro", () => ({
  Trans: ({ children }: { children?: ReactNode }) => <>{children}</>,
  useLingui: () => ({
    t: (strings: TemplateStringsArray, ...values: unknown[]) =>
      strings.reduce(
        (message, part, index) =>
          `${message}${part}${index < values.length ? String(values[index]) : ""}`,
        "",
      ),
  }),
}));

vi.mock("@tauri-apps/api/path", () => ({ homeDir: mocks.homeDir }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: mocks.selectFolder,
}));
vi.mock("@anlg/plugin-opener2", () => ({
  commands: { openPath: mocks.openPath },
}));
vi.mock("@anlg/plugin-settings", () => ({
  commands: {
    vaultBase: mocks.vaultBase,
  },
}));
vi.mock("~/types/tauri.gen", () => ({
  commands: {
    moveVault: mocks.moveVault,
  },
}));
vi.mock("~/session/recording-busy", () => ({
  isAnyRecordingBusy: mocks.isAnyRecordingBusy,
}));
vi.mock("~/shared/relaunch", () => ({
  flushApplicationState: mocks.flushApplicationState,
  scheduleAutomaticRelaunch: mocks.scheduleAutomaticRelaunch,
}));

import { StorageLocationRow } from "./storage-location";

function renderRow() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <StorageLocationRow />
    </QueryClientProvider>,
  );
}

describe("StorageLocationRow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.homeDir.mockResolvedValue("/Users/test");
    mocks.vaultBase.mockResolvedValue({
      status: "ok",
      data: "/Users/test/Google Drive/Anarlog",
    });
    mocks.isAnyRecordingBusy.mockReturnValue(false);
    mocks.flushApplicationState.mockResolvedValue(undefined);
    mocks.moveVault.mockResolvedValue({ status: "ok", data: null });
    mocks.scheduleAutomaticRelaunch.mockResolvedValue("scheduled");
    mocks.selectFolder.mockResolvedValue(null);
  });

  afterEach(cleanup);

  it("shows the current storage location", async () => {
    renderRow();

    expect(
      screen.getByText("Where your notes and recordings are stored"),
    ).toBeTruthy();
    expect(await screen.findByText("~/Google Drive/Anarlog")).toBeTruthy();
  });

  it("flushes pending writes, moves the vault, and relaunches", async () => {
    mocks.selectFolder.mockResolvedValue("/Users/test/Anarlog");
    renderRow();

    await screen.findByText("~/Google Drive/Anarlog");
    fireEvent.click(await screen.findByRole("button", { name: "Change" }));

    await waitFor(() => {
      expect(mocks.flushApplicationState).toHaveBeenCalledTimes(1);
      expect(mocks.moveVault).toHaveBeenCalledWith("/Users/test/Anarlog");
      expect(mocks.scheduleAutomaticRelaunch).toHaveBeenCalledTimes(1);
    });

    expect(
      mocks.flushApplicationState.mock.invocationCallOrder[0],
    ).toBeLessThan(mocks.moveVault.mock.invocationCallOrder[0]);
  });

  it("picks no target and leaves the vault untouched when the picker is cancelled", async () => {
    mocks.selectFolder.mockResolvedValue(null);
    renderRow();

    await screen.findByText("~/Google Drive/Anarlog");
    fireEvent.click(await screen.findByRole("button", { name: "Change" }));

    await waitFor(() => expect(mocks.selectFolder).toHaveBeenCalledTimes(1));
    expect(mocks.flushApplicationState).not.toHaveBeenCalled();
    expect(mocks.moveVault).not.toHaveBeenCalled();
  });

  it("does not relaunch when moving the vault fails, and the old location stays current", async () => {
    mocks.selectFolder.mockResolvedValue("/Users/test/Anarlog");
    mocks.moveVault.mockResolvedValue({
      status: "error",
      error: "Could not move storage",
    });
    renderRow();

    await screen.findByText("~/Google Drive/Anarlog");
    fireEvent.click(await screen.findByRole("button", { name: "Change" }));

    expect(await screen.findByText("Could not move storage")).toBeTruthy();
    expect(mocks.scheduleAutomaticRelaunch).not.toHaveBeenCalled();
    // The read model still names the old path -- nothing in the UI ever
    // claimed the move succeeded.
    expect(screen.getByText("~/Google Drive/Anarlog")).toBeTruthy();
  });

  it("does not move the vault when pending writes fail to flush", async () => {
    mocks.selectFolder.mockResolvedValue("/Users/test/Anarlog");
    mocks.flushApplicationState.mockRejectedValue(
      new Error("Could not save pending changes"),
    );
    renderRow();

    await screen.findByText("~/Google Drive/Anarlog");
    fireEvent.click(await screen.findByRole("button", { name: "Change" }));

    expect(
      await screen.findByText("Could not save pending changes"),
    ).toBeTruthy();
    expect(mocks.moveVault).not.toHaveBeenCalled();
    expect(mocks.scheduleAutomaticRelaunch).not.toHaveBeenCalled();
  });

  it("surfaces a non-writable target the same way as any other move failure", async () => {
    mocks.selectFolder.mockResolvedValue("/Users/test/ReadOnlyDisk/Anarlog");
    mocks.moveVault.mockResolvedValue({
      status: "error",
      error: "Permission denied",
    });
    renderRow();

    await screen.findByText("~/Google Drive/Anarlog");
    fireEvent.click(await screen.findByRole("button", { name: "Change" }));

    expect(await screen.findByText("Permission denied")).toBeTruthy();
    expect(mocks.scheduleAutomaticRelaunch).not.toHaveBeenCalled();
  });

  it("refuses to move while a recording is in progress, without asking Rust", async () => {
    mocks.selectFolder.mockResolvedValue("/Users/test/Anarlog");
    mocks.isAnyRecordingBusy.mockReturnValue(true);
    renderRow();

    await screen.findByText("~/Google Drive/Anarlog");
    fireEvent.click(await screen.findByRole("button", { name: "Change" }));

    expect(
      await screen.findByText(
        "Can't change the storage location while a recording is in progress. Stop it first, then try again.",
      ),
    ).toBeTruthy();
    expect(mocks.flushApplicationState).not.toHaveBeenCalled();
    expect(mocks.moveVault).not.toHaveBeenCalled();
    expect(mocks.scheduleAutomaticRelaunch).not.toHaveBeenCalled();
  });

  it("relaunches even on failure when the database copy already closed the pool", async () => {
    mocks.selectFolder.mockResolvedValue("/Users/test/Anarlog");
    mocks.moveVault.mockResolvedValue({
      status: "error",
      error:
        "DATABASE_POOL_CLOSED: failed to copy the database to the new location: disk full. " +
        "Your data is unchanged at the old location, but Mitschnitt must be restarted before it can be used again.",
    });
    renderRow();

    await screen.findByText("~/Google Drive/Anarlog");
    fireEvent.click(await screen.findByRole("button", { name: "Change" }));

    await waitFor(() => {
      expect(mocks.scheduleAutomaticRelaunch).toHaveBeenCalledTimes(1);
    });
    // The internal marker is Rust-and-frontend-only bookkeeping -- it must
    // never reach the person reading the error.
    expect(
      await screen.findByText(
        "failed to copy the database to the new location: disk full. " +
          "Your data is unchanged at the old location, but Mitschnitt must be restarted before it can be used again.",
      ),
    ).toBeTruthy();
    expect(screen.queryByText(/DATABASE_POOL_CLOSED/)).toBeNull();
  });

  it("shows an error when the current location cannot be read", async () => {
    mocks.vaultBase.mockResolvedValue({
      status: "error",
      error: "Could not read storage location",
    });
    renderRow();

    expect(
      await screen.findByText("Could not read storage location"),
    ).toBeTruthy();
  });
});
