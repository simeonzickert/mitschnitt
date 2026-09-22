import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DateEditor } from "./date";
import { EventDisplay, MetadataButton } from "./index";

import { getSessionEvent } from "~/session/utils";

const mocks = vi.hoisted(() => ({
  createdAt: "2026-07-02T03:53:00.000Z" as unknown,
  setCreatedAt: vi.fn(),
  sessionEvent: null as unknown,
}));

const lingui = vi.hoisted(() => {
  const t = (input: TemplateStringsArray | string, ...values: unknown[]) => {
    if (typeof input === "string") {
      return input;
    }

    return Array.from(input).reduce(
      (text, part, index) => `${text}${part}${values[index] ?? ""}`,
      "",
    );
  };

  return { t };
});

vi.mock("@lingui/react/macro", () => ({
  useLingui: () => ({
    t: lingui.t,
  }),
}));

vi.mock("@anlg/plugin-opener2", () => ({
  commands: {
    openUrl: vi.fn(),
  },
}));

vi.mock("~/shared/config", () => ({
  useConfigValue: () => undefined,
}));

vi.mock("~/session/hooks/useSessionEvent", () => ({
  useSessionEvent: () => mocks.sessionEvent,
}));

vi.mock("~/session/queries", () => ({
  useSession: () => ({ created_at: mocks.createdAt }),
  useUpdateSession: () => mocks.setCreatedAt,
}));

vi.mock("./participants", () => ({
  ParticipantsDisplay: () => null,
}));

describe("Metadata controls", () => {
  beforeEach(() => {
    mocks.createdAt = "2026-07-02T03:53:00.000Z";
    mocks.setCreatedAt.mockClear();
    mocks.sessionEvent = null;
  });

  afterEach(() => {
    cleanup();
  });

  it("renders the metadata calendar trigger as a circle", () => {
    render(<MetadataButton sessionId="session-1" />);

    const metadataButton = screen.getByRole("button", {
      name: "Open note metadata",
    });

    expect(metadataButton.className).toContain("size-7");
    expect(metadataButton.className).toContain("rounded-full");
  });

  // E1 (Fix-Runde 1d): the panel showed an "anarlog.so" line with a Join
  // button for a welcome session from before 02.09.2026, because it renders
  // every non-empty link. The link is emptied where the event is read
  // (getSessionEvent); this pins the panel end of it.
  it("shows no join target for a legacy welcome session", () => {
    const event = getSessionEvent({
      event_json: JSON.stringify({
        tracking_id: "anarlog-onboarding-demo-v1",
        title: "Welcome to Mitschnitt",
        started_at: "2026-06-01T09:00:00.000Z",
        meeting_link: "https://anarlog.so/onboarding-demo/",
        description: "A private, prerecorded introduction to Anarlog.",
      }),
    });

    render(
      <EventDisplay
        event={{
          title: event?.title,
          startedAt: event?.started_at,
          endedAt: event?.ended_at,
          location: event?.location,
          meetingLink: event?.meeting_link,
          description: event?.description,
          calendarId: event?.calendar_id,
        }}
      />,
    );

    expect(screen.queryByRole("button", { name: "Join" })).toBeNull();
    expect(screen.queryByText("anarlog.so")).toBeNull();
    expect(screen.queryByText(/prerecorded/)).toBeNull();
  });

  it("renders date edit action buttons as circles", () => {
    render(<DateEditor sessionId="session-1" />);

    fireEvent.click(screen.getByRole("button", { name: "Edit date" }));

    expect(
      screen.getByRole("button", { name: "Cancel date edit" }).className,
    ).toContain("rounded-full");
    expect(
      screen.getByRole("button", { name: "Save date" }).className,
    ).toContain("rounded-full");
  });
});
