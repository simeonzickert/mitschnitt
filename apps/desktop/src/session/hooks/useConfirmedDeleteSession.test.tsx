import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  deleteSession: vi.fn(),
  sessionMode: "inactive",
  liveStartup: { sessionId: null as string | null, loading: false },
}));

vi.mock("~/session/hooks/useDeleteSession", () => ({
  useDeleteSession: () => mocks.deleteSession,
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

import { useConfirmedDeleteSession } from "./useConfirmedDeleteSession";

function Host() {
  const { requestDelete, dialog } = useConfirmedDeleteSession();
  return (
    <>
      <button
        onClick={() =>
          requestDelete("session-1", {
            title: "Planning",
            trackingId: "tracking-1",
          })
        }
      >
        ask
      </button>
      {dialog}
    </>
  );
}

// Mitschnitt-Fork (ISA N8, ZICK-251; G5, Fix-Runde 2 -- Grok 3). The
// confirmation is the last thing a person sees before the delete; a
// recording that starts while it is open must show up THERE, not as a toast
// after the dialog has already closed.
describe("useConfirmedDeleteSession", () => {
  beforeEach(() => {
    cleanup();
    mocks.sessionMode = "inactive";
    mocks.liveStartup = { sessionId: null, loading: false };
    mocks.deleteSession.mockReset();
    mocks.deleteSession.mockReturnValue(true);
  });

  function open() {
    render(<Host />);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "ask" }));
    });
  }

  it("deletes once and closes on confirm", () => {
    open();

    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    expect(mocks.deleteSession).toHaveBeenCalledExactlyOnceWith("session-1", {
      trackingId: "tracking-1",
      title: "Planning",
    });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it.each(["active", "finalizing", "running_batch"])(
    "disables Delete and says why while the recording is %s",
    (mode) => {
      mocks.sessionMode = mode;

      open();

      const confirm = screen.getByRole("button", { name: "Delete" });
      expect(confirm).toHaveProperty("disabled", true);
      expect(screen.getByRole("dialog").textContent).toContain(
        "This note is still recording",
      );
      expect(screen.getByRole("button", { name: "Cancel" })).toHaveProperty(
        "disabled",
        false,
      );
    },
  );

  it("disables Delete while the recording of this note is starting up", () => {
    mocks.liveStartup = { sessionId: "session-1", loading: true };

    open();

    expect(screen.getByRole("button", { name: "Delete" })).toHaveProperty(
      "disabled",
      true,
    );
  });

  // Falsifier: until this round the hook cleared its pending request BEFORE
  // asking the delete hook -- a refusal closed the dialog and left only the
  // toast behind it.
  it("keeps the dialog open when the delete is refused because the note is recording", () => {
    open();
    mocks.deleteSession.mockImplementation(() => {
      mocks.sessionMode = "active";
      return false;
    });

    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    expect(mocks.deleteSession).toHaveBeenCalledOnce();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("keeps the dialog open on any refusal; Cancel closes it", () => {
    open();
    mocks.deleteSession.mockReturnValue(false);

    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(screen.getByRole("dialog")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
