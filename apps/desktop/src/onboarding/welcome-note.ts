import { md2json } from "@anlg/editor/markdown";
import type { SessionEvent } from "@anlg/store";

import { liveQueryClient } from "~/db";
import {
  WELCOME_NOTE_DESCRIPTION,
  WELCOME_NOTE_TRACKING_ID,
} from "~/onboarding/welcome-note.constants";
import { createSession } from "~/session/queries";
import { DEFAULT_USER_ID } from "~/shared/utils";

// Nur ein localStorage-Schluessel im eigenen Webview, ohne Aussenwirkung. Der
// schlimmste Fall beim Umbenennen ist, dass eine ueber ein Update hinweg
// offene Willkommens-Sitzung einmalig nicht wiedergefunden wird.
const PENDING_WELCOME_SESSION_KEY = "mitschnitt.pending-welcome-session";

// D6 (Fix-Runde 1d): the first sentence of the third paragraph used to
// describe "Join & record" and a prerecorded demo meeting -- a button and a
// video that went with the original's demo link (da7f3d41c8). G6c
// (Fix-Runde 2, Opus): the last paragraph still opened with "When the video
// ends" -- the second sentence about the same removed demo; it now says
// what actually ends the recording. "Text bleibt" (Betreiber-Entscheid, ISA N5) covers
// the note text, not sentences about a button and a video that do not
// exist; everything else is word for word as it was.
const WELCOME_NOTE = `Welcome to Mitschnitt 👋


This note is a quick way to see how Mitschnitt works.


Click **Record** in the top-right corner to capture your microphone and your computer's audio. Mitschnitt will save the audio. To create a transcript and notes, choose a provider in **Settings → Transcription**; if one is not ready, Mitschnitt will show you a setup shortcut.


When you stop the recording, Mitschnitt will stop listening. If transcription and intelligence are configured, it will start creating your summary automatically.`;

let pendingWelcomeSession: Promise<string> | null = null;

export function getOrCreateWelcomeSession(): Promise<string> {
  if (!pendingWelcomeSession) {
    pendingWelcomeSession = findOrCreateWelcomeSession().finally(() => {
      pendingWelcomeSession = null;
    });
  }
  return pendingWelcomeSession;
}

export function setPendingWelcomeSession(sessionId: string | null) {
  if (sessionId) {
    localStorage.setItem(PENDING_WELCOME_SESSION_KEY, sessionId);
  } else {
    localStorage.removeItem(PENDING_WELCOME_SESSION_KEY);
  }
}

export function takePendingWelcomeSession(): string | null {
  const sessionId = localStorage.getItem(PENDING_WELCOME_SESSION_KEY);
  localStorage.removeItem(PENDING_WELCOME_SESSION_KEY);
  return sessionId;
}

async function findOrCreateWelcomeSession(): Promise<string> {
  const rows = await liveQueryClient.execute<{ id: string }>(
    `
      SELECT id
      FROM sessions
      WHERE deleted_at IS NULL
        AND CASE
          WHEN json_valid(event_json)
          THEN json_extract(event_json, '$.tracking_id')
        END = ?
      ORDER BY created_at, id
      LIMIT 1
    `,
    [WELCOME_NOTE_TRACKING_ID],
  );
  if (rows[0]) return rows[0].id;

  const now = new Date().toISOString();
  const event: SessionEvent = {
    tracking_id: WELCOME_NOTE_TRACKING_ID,
    calendar_id: "",
    title: "Welcome to Mitschnitt",
    started_at: now,
    ended_at: "",
    is_all_day: false,
    has_recurrence_rules: false,
    // Kein Link: die Demo-Adresse gehoerte dem Original (Entscheid,
    // ISA N5). Ohne Link bietet der Kopf "Record" an, kein "Join & record".
    meeting_link: "",
    description: WELCOME_NOTE_DESCRIPTION,
  };

  return createSession("Welcome to Mitschnitt", DEFAULT_USER_ID, {
    event_json: JSON.stringify(event),
    raw_md: JSON.stringify(md2json(WELCOME_NOTE)),
  });
}
