// Drop excluded people and any contact that is the current user (or a
// calendar copy of them) so a 1:1 meeting still has one remote. The self
// addresses come from two places: the own contact record, and every
// address the calendar itself flagged with is_current_user. The second
// source is the load-bearing one -- a self contact without an email
// (measured on 2026-09-02: the owner human carried an empty email) makes
// the first source empty, and one address would only cover one calendar.
// Only a real JSON boolean counts as the flag: a loose match there would
// drop a real remote, and a missing name beats a wrong one.
//
// The calendar account's own address is the third source, and it is the one
// that survives a provider putting the flag on the WRONG attendee. That case
// is the destructive one: the flagged person is dropped from the list, their
// own row is cleaned up as stale, and the user's leftover copy would then be
// stamped onto the other person's audio. With the account address in here the
// leftover copy drops out too and the channel keeps its number, which is the
// honest answer. Measured on 2026-09-02: over 36 calendar events the flag and
// the account address agreed 27 times and never disagreed.
export const SESSION_PARTICIPANT_HUMAN_IDS_SQL = `
      SELECT DISTINCT participant.human_id
      FROM session_participants AS participant
      LEFT JOIN humans AS human
        ON human.id = participant.human_id
        AND human.deleted_at IS NULL
      LEFT JOIN sessions AS session
        ON session.id = participant.session_id
      WHERE participant.session_id = ?
        AND participant.human_id <> ''
        AND participant.source <> 'excluded'
        AND participant.deleted_at IS NULL
        AND participant.human_id <> COALESCE(session.owner_user_id, '')
        AND (
          NULLIF(lower(COALESCE(NULLIF(human.email, ''), participant.email)), '') IS NULL
          OR lower(COALESCE(NULLIF(human.email, ''), participant.email)) NOT IN (
            SELECT lower(self_human.email)
            FROM humans AS self_human
            WHERE self_human.id = session.owner_user_id
              AND self_human.deleted_at IS NULL
              AND NULLIF(lower(self_human.email), '') IS NOT NULL
            UNION ALL
            SELECT lower(json_extract(self_invite.value, '$.email'))
            FROM events AS event
            JOIN json_each(
              CASE WHEN json_valid(event.participants_json)
                THEN event.participants_json
                ELSE '[]' END
            ) AS self_invite
            WHERE event.id = session.event_id
              AND event.deleted_at IS NULL
              AND json_extract(self_invite.value, '$.is_current_user') = 1
              AND NULLIF(lower(json_extract(self_invite.value, '$.email')), '') IS NOT NULL
            UNION ALL
            SELECT lower(calendar.source)
            FROM events AS event
            JOIN calendars AS calendar
              ON calendar.id = event.calendar_id
              AND calendar.deleted_at IS NULL
            WHERE event.id = session.event_id
              AND event.deleted_at IS NULL
              AND instr(calendar.source, '@') > 1
          )
        )
      ORDER BY participant.human_id
    `;
