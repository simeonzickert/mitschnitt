import { describe, expect, it } from "vitest";

import { getSessionEvent } from "./utils";

// E1 (Fix-Runde 1d). A welcome session created before 02.09.2026 still
// carries the original's demo link and description in its event_json;
// da7f3d41c8 emptied the link for new welcome sessions only. Every consumer
// (header, metadata panel, remote-meeting detection) reads the event through
// getSessionEvent, so the correction lives here and touches no row.
describe("getSessionEvent", () => {
  const legacyWelcome = JSON.stringify({
    tracking_id: "anarlog-onboarding-demo-v1",
    calendar_id: "",
    title: "Welcome to Anarlog",
    started_at: "2026-06-01T09:00:00.000Z",
    ended_at: "",
    is_all_day: false,
    has_recurrence_rules: false,
    meeting_link: "https://anarlog.so/onboarding-demo/",
    description: "A private, prerecorded introduction to Anarlog.",
  });

  it("strips the legacy demo link and description from the welcome session", () => {
    const event = getSessionEvent({ event_json: legacyWelcome });

    expect(event?.meeting_link).toBe("");
    expect(event?.description).toBe("An introduction to Mitschnitt.");
    expect(event?.tracking_id).toBe("anarlog-onboarding-demo-v1");
    expect(event?.title).toBe("Welcome to Anarlog");
    expect(JSON.stringify(event)).not.toContain("anarlog.so");
  });

  it("leaves every other session's link and description alone", () => {
    const event = getSessionEvent({
      event_json: JSON.stringify({
        tracking_id: "calendar-event-1",
        meeting_link: "https://meet.google.com/abc-defg-hij",
        description: "Weekly sync",
      }),
    });

    expect(event?.meeting_link).toBe("https://meet.google.com/abc-defg-hij");
    expect(event?.description).toBe("Weekly sync");
  });

  it("keeps a description someone wrote into the welcome session", () => {
    const event = getSessionEvent({
      event_json: JSON.stringify({
        tracking_id: "anarlog-onboarding-demo-v1",
        meeting_link: "https://anarlog.so/onboarding-demo/",
        description: "My own notes",
      }),
    });

    expect(event?.meeting_link).toBe("");
    expect(event?.description).toBe("My own notes");
  });

  it("returns null for missing or broken event metadata", () => {
    expect(getSessionEvent({ event_json: null })).toBeNull();
    expect(getSessionEvent({ event_json: "" })).toBeNull();
    expect(getSessionEvent({ event_json: "{not json" })).toBeNull();
  });
});
