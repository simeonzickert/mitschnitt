import {
  AUDIO_RETENTION_DURATION_MS,
  type AudioRetentionPolicy,
} from "./audio-retention-policy";

import { liveQueryClient } from "~/db";
import {
  cleanupDeletedSessionAudio,
  deleteLocalSessionAudio,
} from "~/session/attachments";
import { listenerStore } from "~/store/zustand/listener/instance";

export const AUDIO_RETENTION_TASK_ID = "audio-retention-cleanup";
export const AUDIO_RETENTION_INTERVAL = 60 * 1000;

type SessionAudioRetentionEvent = {
  phase: "deleting" | "deleted";
  sessionId: string;
};

const sessionAudioRetentionListeners = new Set<
  (event: SessionAudioRetentionEvent) => void
>();

export function subscribeToSessionAudioRetention(
  listener: (event: SessionAudioRetentionEvent) => void,
) {
  sessionAudioRetentionListeners.add(listener);
  return () => sessionAudioRetentionListeners.delete(listener);
}

function emitSessionAudioRetention(event: SessionAudioRetentionEvent) {
  sessionAudioRetentionListeners.forEach((listener) => listener(event));
}

async function deleteWithRetentionLifecycle(
  sessionId: string,
  deleteAudio: () => Promise<boolean>,
) {
  emitSessionAudioRetention({ phase: "deleting", sessionId });
  const deleted = await deleteAudio();
  if (deleted) {
    emitSessionAudioRetention({ phase: "deleted", sessionId });
  }
  return deleted;
}

export {
  normalizeAudioRetention,
  type AudioRetentionPolicy,
} from "./audio-retention-policy";

/**
 * Ab wann die Frist einer Sitzung laeuft.
 *
 * Fuer eine selbst aufgenommene Sitzung ist das ihr Aufnahmedatum. Fuer eine
 * IMPORTIERTE ist es das Datum des Imports: `created_at` traegt dort das echte
 * alte Datum aus der Vorgaenger-App, teils 2024. Wuerde die Frist ab diesem
 * Datum laufen, waere importiertes Material sofort ueberfaellig -- am gemessenen
 * Bestand (gemessen 10.09.2026) bei einer 30-Tage-Frist 116 der 146 Aufnahmen,
 * bei einem Jahr immer noch zwei. Die Datei ist aber gerade erst angekommen;
 * eine Frist misst, wie lange sie HIER liegt.
 *
 * Dasselbe Muster wie "ein Standardwert, der eine Frist ist, ist ein
 * Loeschbefehl": die Frist selbst ist harmlos, ihr Startpunkt ist es nicht.
 */
const FRISTBEGINN_SQL = `COALESCE(
      json_extract(session.metadata_json, '$.import.imported_at'),
      session.created_at
    )`;

export function sessionAudioExpired(
  createdAt: unknown,
  policy: AudioRetentionPolicy,
  nowMs = Date.now(),
) {
  if (policy === "forever") {
    return false;
  }

  if (policy === "none") {
    return true;
  }

  if (typeof createdAt !== "string") {
    return false;
  }

  const createdAtMs = Date.parse(createdAt);
  if (!Number.isFinite(createdAtMs)) {
    return false;
  }

  return nowMs >= createdAtMs + AUDIO_RETENTION_DURATION_MS[policy];
}

/**
 * How many recordings a policy would delete right now.
 *
 * Mitschnitt-Fork (F17). Shortening the deadline is not a preference change, it
 * is a delete button with a date on it -- and since the deadline reaches the
 * meeting folder too, the recording does not survive anywhere. So the number is
 * put in front of the person before the change is applied.
 *
 * Counted, not estimated: same expiry test the cleanup pass uses, over the
 * sessions that still have a recording on this machine.
 *
 * It can read high, and by more than a rounding error. The cleanup also skips a
 * session busy in the recorder or still being transcribed (a delay, so at most
 * one or two), and under "don't save" it additionally requires a finished
 * transcript -- which this query does not check at all, so on a library full of
 * untranscribed meetings the number can be well over the truth. It cannot read
 * low, which is the direction that matters for a warning: nothing the cleanup
 * deletes is missing from here.
 */
export async function countExpiringSessionAudio(
  policy: AudioRetentionPolicy,
  nowMs = Date.now(),
): Promise<number> {
  if (policy === "forever") {
    return 0;
  }

  // DISTINCT because nothing in the schema stops a session from carrying two
  // rows for its primary audio; without it one meeting could be counted twice.
  const sessions = await liveQueryClient.execute<{ retention_from: string }>(`
    SELECT DISTINCT session.id, ${FRISTBEGINN_SQL} AS retention_from
    FROM sessions AS session
    JOIN session_attachments AS audio
      ON audio.session_id = session.id
     AND audio.source_type = 'session_audio'
     AND audio.source_id = 'primary'
     AND audio.deleted_at IS NULL
    LEFT JOIN attachment_local_state AS local
      ON local.attachment_id = audio.id
    WHERE session.deleted_at IS NULL
      AND COALESCE(local.availability, 'present') != 'absent'
  `);

  return sessions.filter((session) =>
    sessionAudioExpired(session.retention_from, policy, nowMs),
  ).length;
}

export function isSessionAudioIdle(sessionId: string) {
  const state = listenerStore.getState();
  return (
    state.getSessionMode(sessionId) === "inactive" &&
    !(state.live.sessionId === sessionId && state.live.loading)
  );
}

async function sessionAudioIsProcessed(sessionId: string): Promise<boolean> {
  const rows = await liveQueryClient.execute<{
    has_words: number;
    transcript_processing: number;
  }>(
    `
      SELECT
        EXISTS(
          SELECT 1
          FROM transcripts
          WHERE session_id = ?
            AND deleted_at IS NULL
            AND json_valid(words_json)
            AND json_array_length(words_json) > 0
        ) AS has_words,
        EXISTS(
          SELECT 1
          FROM session_attachments
          WHERE session_id = ?
            AND source_type = 'session_audio'
            AND source_id = 'primary'
            AND deleted_at IS NULL
            AND json_valid(metadata_json)
            AND json_extract(metadata_json, '$.transcript_status') = 'processing'
        ) AS transcript_processing
    `,
    [sessionId, sessionId],
  );
  return rows[0]?.has_words === 1 && rows[0]?.transcript_processing !== 1;
}

/**
 * Drops the recording of ONE session the moment its transcript exists, under
 * "don't save recordings".
 *
 * Mitschnitt-Fork (F18): deliberately NOT behind the confirmation marker, and
 * the reason is worth writing down because the next reader will ask. It acts on
 * a single session that this app just recorded and transcribed in this run, so
 * it can never reach a restored library -- those sessions are never handed to
 * it. And "don't save" is a rule about recordings yet to be made, not a
 * deadline applied backwards to a library nobody looked at; holding it for a
 * one-time question would mean keeping a recording somebody said not to keep.
 * The marker guards the sweep over everything that is already here. This is the
 * other thing.
 */
export async function deleteProcessedAudioForRetention(
  policy: AudioRetentionPolicy,
  sessionId: string,
) {
  if (policy !== "none") {
    return false;
  }

  if (!isSessionAudioIdle(sessionId)) {
    return false;
  }

  if (!(await sessionAudioIsProcessed(sessionId))) {
    return false;
  }

  try {
    return await deleteWithRetentionLifecycle(sessionId, () =>
      deleteLocalSessionAudio(sessionId, () => isSessionAudioIdle(sessionId)),
    );
  } catch (error) {
    console.error("[audio-retention] failed to delete audio", {
      sessionId,
      error,
    });
    return false;
  }
}

export type AudioCleanupResult = {
  deletedSessionIds: string[];
  /**
   * Set when the pass found recordings past the deadline and deleted none of
   * them, because nobody has agreed that the deadline may delete on this
   * installation yet. Carries what the answer is about: the deadline, and how
   * many recordings it catches right now.
   */
  awaitingConfirmation: {
    policy: AudioRetentionPolicy;
    expiring: number;
  } | null;
};

/**
 * Deletes the recordings whose time is up.
 *
 * `policy` is `null` when this machine has not written down how long it keeps
 * recordings (see `decideAudioRetention` in `~/settings/queries`). That is not
 * the same as "keep forever" and it is very much not the same as the default:
 * it means nobody has answered yet, and a deadline nobody set may not delete
 * anything. Deletions a person already asked for still finish -- those were
 * answered.
 *
 * Mitschnitt-Fork (F17). Without this the startup task that writes the answer
 * would be racing the cleanup task, which is scheduled at zero delay; whichever
 * won decided whether a year of meetings survived the update.
 *
 * Mitschnitt-Fork (F18). `confirmed` is the second half, and it closes the case
 * F17 could not: a library that arrives AFTER the deadline was written down.
 * The startup answer is honest about the machine it saw -- empty, so six months
 * costs nothing -- and then a Time Machine restore or a copied database drops a
 * year of meetings into it, and one minute later this pass would take them.
 *
 * So an unconfirmed installation does not delete on its first hit: it reports
 * what it found and leaves it alone. That is the safe direction in both
 * directions at once -- it cannot overrule a choice somebody made (a choice
 * sets `confirmed`), and it cannot delete a library nobody was asked about.
 * It costs exactly one question in the life of an installation; after that the
 * deadline runs silently, as expected.
 *
 * The default is `false` on purpose. A caller that forgets the flag keeps
 * recordings, which is the cheap failure; the expensive one is not reachable by
 * forgetting anything.
 */
export async function cleanupExpiredAudio(
  policy: AudioRetentionPolicy | null,
  nowMs = Date.now(),
  confirmed = false,
): Promise<AudioCleanupResult> {
  const deletedSessionIds = await cleanupLogicallyDeletedAudio();
  if (policy === null || policy === "forever") {
    return { deletedSessionIds, awaitingConfirmation: null };
  }

  const deletes: Promise<void>[] = [];
  // `has_local_audio` is Mitschnitt-Fork (F18), and it exists because of what
  // this row set is now used for. The sweep used to ignore whether a recording
  // was still on this machine and let `deleteLocalSessionAudio` return false on
  // the ones with nothing to take -- which cost nothing while the answer was
  // only a delete list. It costs a great deal now that the same set is COUNTED
  // and put in front of a person: a restored database holding a thousand old
  // meetings but only fifty recordings would have asked about a thousand. The
  // conditions are the ones `countExpiringSessionAudio` already uses, so the
  // number in the question and the number in the settings agree.
  const sessions = await liveQueryClient.execute<{
    id: string;
    retention_from: string;
    has_words: number;
    transcript_processing: number;
    has_local_audio: number;
  }>(`
    SELECT
      session.id,
      ${FRISTBEGINN_SQL} AS retention_from,
      EXISTS(
        SELECT 1
        FROM session_attachments AS present
        LEFT JOIN attachment_local_state AS local
          ON local.attachment_id = present.id
        WHERE present.session_id = session.id
          AND present.source_type = 'session_audio'
          AND present.source_id = 'primary'
          AND present.deleted_at IS NULL
          AND COALESCE(local.availability, 'present') != 'absent'
      ) AS has_local_audio,
      EXISTS(
        SELECT 1
        FROM transcripts AS transcript
        WHERE transcript.session_id = session.id
          AND transcript.deleted_at IS NULL
          AND json_valid(transcript.words_json)
          AND json_array_length(transcript.words_json) > 0
      ) AS has_words,
      EXISTS(
        SELECT 1
        FROM session_attachments AS audio
        WHERE audio.session_id = session.id
          AND audio.source_type = 'session_audio'
          AND audio.source_id = 'primary'
          AND audio.deleted_at IS NULL
          AND json_valid(audio.metadata_json)
          AND json_extract(audio.metadata_json, '$.transcript_status') = 'processing'
      ) AS transcript_processing
    FROM sessions AS session
    WHERE session.deleted_at IS NULL
    ORDER BY session.created_at, session.id
  `);

  // Worked out in full before a single delete is issued. The question below is
  // about a number, and a number cannot be reported by a loop that has already
  // started removing the things it counts.
  const due = sessions.filter((session) => {
    if (session.has_local_audio !== 1) {
      return false;
    }

    if (!isSessionAudioIdle(session.id)) {
      return false;
    }

    if (session.transcript_processing === 1) {
      return false;
    }

    if (policy === "none" && session.has_words !== 1) {
      return false;
    }

    return sessionAudioExpired(session.retention_from, policy, nowMs);
  });

  // An empty deadline is not worth a question: nothing would be lost by
  // answering it either way, and asking about nothing trains people to click
  // past the one time it matters. So the pass stays silent until the deadline
  // first has something to take.
  if (!confirmed && due.length > 0) {
    return {
      deletedSessionIds,
      awaitingConfirmation: { policy, expiring: due.length },
    };
  }

  for (const session of due) {
    deletes.push(
      deleteWithRetentionLifecycle(session.id, () =>
        deleteLocalSessionAudio(session.id, () =>
          isSessionAudioIdle(session.id),
        ),
      )
        .then((deleted) => {
          if (deleted) {
            deletedSessionIds.push(session.id);
          }
        })
        .catch((error) => {
          console.error("[audio-retention] failed to delete audio", {
            sessionId: session.id,
            error,
          });
        }),
    );
  }

  await Promise.all(deletes);

  return { deletedSessionIds, awaitingConfirmation: null };
}

async function cleanupLogicallyDeletedAudio() {
  const rows = await liveQueryClient.execute<{ session_id: string }>(`
    SELECT DISTINCT attachment.session_id
    FROM session_attachments AS attachment
    LEFT JOIN attachment_local_state AS local
      ON local.attachment_id = attachment.id
    WHERE attachment.source_type = 'session_audio'
      AND attachment.source_id = 'primary'
      AND attachment.deleted_at IS NOT NULL
      AND COALESCE(local.availability, 'present') != 'absent'
    ORDER BY attachment.session_id
  `);
  const deletedSessionIds: string[] = [];

  await Promise.all(
    rows.map(async ({ session_id: sessionId }) => {
      if (!isSessionAudioIdle(sessionId)) {
        return;
      }

      try {
        if (
          await deleteWithRetentionLifecycle(sessionId, () =>
            cleanupDeletedSessionAudio(sessionId, () =>
              isSessionAudioIdle(sessionId),
            ),
          )
        ) {
          deletedSessionIds.push(sessionId);
        }
      } catch (error) {
        console.error("[audio-retention] failed to finish audio deletion", {
          sessionId,
          error,
        });
      }
    }),
  );

  return deletedSessionIds;
}
