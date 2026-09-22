import type { SessionEvent } from "@anlg/store";

import {
  WELCOME_NOTE_DESCRIPTION,
  WELCOME_NOTE_TRACKING_ID,
} from "~/onboarding/welcome-note.constants";

// What the original wrote into the welcome session's event before
// 02.09.2026 (da7f3d41c8 emptied the link for NEW welcome sessions only).
const LEGACY_WELCOME_DESCRIPTION =
  "A private, prerecorded introduction to Anarlog.";

/**
 * The one place a session's embedded event is read (E1, Fix-Runde 1d).
 *
 * Every consumer -- the header's Record/Join button, the metadata panel
 * with its Join button, the remote-meeting detection -- comes through here,
 * so this is where a welcome session created before 02.09.2026 loses the
 * original's demo link: the row still carries
 * `meeting_link: https://anarlog.so/onboarding-demo/` and its description,
 * and the metadata panel showed an "anarlog.so" line with a Join button for
 * it. Corrected on read, not in the database: onboarding metadata, not
 * meeting content, and one bottleneck beats a migration nobody can undo.
 * The tray agenda and the scheduled auto-start read the calendar `events`
 * table, never `event_json`, so they cannot see this link.
 */
export function getSessionEvent(session: {
  event_json?: string | null;
}): SessionEvent | null {
  const eventJson = session.event_json;
  if (!eventJson) return null;
  try {
    const event = JSON.parse(eventJson) as SessionEvent;
    return event?.tracking_id === WELCOME_NOTE_TRACKING_ID
      ? {
          ...event,
          meeting_link: "",
          description:
            event.description === LEGACY_WELCOME_DESCRIPTION
              ? WELCOME_NOTE_DESCRIPTION
              : event.description,
        }
      : event;
  } catch {
    return null;
  }
}
