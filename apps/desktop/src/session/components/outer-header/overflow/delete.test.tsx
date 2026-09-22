import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  sessionMode: "inactive" as string,
  live: { sessionId: null as string | null, loading: false },
  title: "Planning" as string | undefined,
}));

vi.mock("@anlg/ui/components/ui/dropdown-menu", () => ({
  DropdownMenuItem: ({ children, ...props }: ComponentProps<"button">) => (
    <button {...props}>{children}</button>
  ),
}));

vi.mock("@anlg/ui/components/ui/tooltip", () => ({
  Tooltip: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  TooltipTrigger: ({ children }: { children: React.ReactNode }) => (
    <>{children}</>
  ),
  TooltipContent: ({ children }: { children: React.ReactNode }) => (
    <div role="tooltip">{children}</div>
  ),
}));

vi.mock("~/audio-player", () => ({
  useAudioPlayer: () => ({
    deleteRecording: vi.fn(),
    isDeletingRecording: false,
  }),
}));

vi.mock("~/session/queries", () => ({
  useSessionSummary: () => ({ title: mocks.title }),
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
      live: mocks.live,
    }),
}));

import { DeleteNote } from "./delete";

describe("DeleteNote", () => {
  afterEach(cleanup);

  beforeEach(() => {
    mocks.sessionMode = "inactive";
    mocks.live = { sessionId: null, loading: false };
    mocks.title = "Planning";
  });

  // ISA N8 (ZICK-251): the entry asks, it does not delete. The confirmation
  // belongs to the overflow button (see index.test.tsx); here the entry only
  // hands over what the dialog needs.
  it("requests a confirmed deletion with the note title instead of deleting", () => {
    const onRequestDelete = vi.fn();

    render(
      <DeleteNote sessionId="session-1" onRequestDelete={onRequestDelete} />,
    );

    const entry = screen.getByRole("button", { name: "Delete" });
    expect(entry.hasAttribute("disabled")).toBe(false);
    expect(screen.queryByRole("tooltip")).toBeNull();

    fireEvent.click(entry);

    expect(onRequestDelete).toHaveBeenCalledExactlyOnceWith("session-1", {
      title: "Planning",
    });
  });

  // The recording is sacred: while it runs, finalizes or is being
  // transcribed, the entry is disabled and says why. Until 02.09.2026 only
  // "Delete recording" knew these states; "Delete" deleted the note anyway.
  it.each(["active", "finalizing", "running_batch"])(
    "is disabled with a reason while the recording is %s",
    (mode) => {
      mocks.sessionMode = mode;
      const onRequestDelete = vi.fn();

      render(
        <DeleteNote sessionId="session-1" onRequestDelete={onRequestDelete} />,
      );

      const entry = screen.getByRole("button", { name: "Delete" });
      expect(entry.hasAttribute("disabled")).toBe(true);
      expect(screen.getByRole("tooltip").textContent).toBe(
        "Recording in progress. Stop it before deleting this note.",
      );

      fireEvent.click(entry);

      expect(onRequestDelete).not.toHaveBeenCalled();
    },
  );

  it("is disabled while the recording of this note is still starting up", () => {
    mocks.live = { sessionId: "session-1", loading: true };

    render(<DeleteNote sessionId="session-1" onRequestDelete={vi.fn()} />);

    expect(
      screen.getByRole("button", { name: "Delete" }).hasAttribute("disabled"),
    ).toBe(true);
  });

  it("stays enabled while a different note is recording", () => {
    mocks.live = { sessionId: "session-2", loading: true };

    render(<DeleteNote sessionId="session-1" onRequestDelete={vi.fn()} />);

    expect(
      screen.getByRole("button", { name: "Delete" }).hasAttribute("disabled"),
    ).toBe(false);
  });
});
