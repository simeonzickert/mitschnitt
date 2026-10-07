//! Kalender und Termine des Imports gegen den Bestand erkennen.
//!
//! Der Import legte Kalender frueher nur mit `ON CONFLICT(id)` an. Die
//! Import-ID (die alte anarlog-ID) ist aber nie die ID, die der Kalender-Sync
//! vergibt; der Sync findet seine Kalender ueber (provider, connection_id,
//! tracking_id_calendar). Ergebnis: jede Kalender-Kennung stand doppelt in
//! `calendars`, und der Import haengte seine Termine an die zweite Zeile, die
//! der Sync nie wieder las. Hier wird dieselbe Kennung benutzt:
//!
//! - Ein Import-Kalender, zu dem es schon einen nicht geloeschten Kalender
//!   gleicher Kennung gibt, wird NICHT als eigene Zeile angelegt. Seine ID
//!   bleibt als Grabstein (`deleted_at` gesetzt) belegt -- dieselbe Form, die
//!   der Bereinigungs-Step 20260913090000 hinterlaesst -- damit die Termine
//!   des Imports ihren Kalender wiederfinden.
//! - Ein Import-Termin wird an den behaltenen Kalender gehaengt, den die
//!   Kalender-ID des Imports ueber die Kennung aufloest. Gibt es dort schon
//!   einen LEBENDEN Termin mit gleicher tracking_id_event und gleichem Beginn,
//!   wird er nur als Grabstein angelegt. Ist der Zwilling bereits geloescht,
//!   bleibt der lebende Import-Termin stehen (die Kalender-Daten des Imports
//!   sind dann die einzigen lebenden).
//! - Sitzungen des Imports bekommen `event_id` und `event_json.calendar_id`
//!   auf die behaltenen Zeilen umgeschrieben, sonst zeigten sie auf einen
//!   Grabstein.
//!
//! Ein Kalender ohne tracking_id_calendar ist nicht erkennbar und wird wie
//! bisher angelegt.

use sqlx::{Sqlite, Transaction};

use super::{LegacyCalendar, LegacyEvent, LegacySession};

pub(super) enum EventTarget {
    /// Den Termin am (ggf. umgeleiteten) Kalender anlegen.
    Insert(String),
    /// Es gibt schon einen LEBENDEN gleichen Termin am behaltenen Kalender:
    /// der Import-Termin wird nur als Grabstein an diesem Kalender angelegt,
    /// damit Sitzungen des Imports ihn ueber die Kennung auf den lebenden
    /// Zwilling aufloesen koennen (`resolve_session_links`).
    Twin(String),
}

/// `true`, wenn es zum Import-Kalender schon einen nicht geloeschten Kalender
/// gleicher Kennung unter anderer ID gibt. Dann ist der Grabstein angelegt und
/// der Aufrufer legt keine eigene Zeile an.
pub(super) async fn adopt_existing_calendar(
    transaction: &mut Transaction<'_, Sqlite>,
    row: &LegacyCalendar,
) -> Result<bool, sqlx::Error> {
    if row.tracking_id_calendar.is_empty() {
        return Ok(false);
    }
    let twin_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM calendars
           WHERE provider = ? AND connection_id = ? AND tracking_id_calendar = ?
             AND deleted_at IS NULL AND id <> ?
         )",
    )
    .bind(&row.provider)
    .bind(&row.connection_id)
    .bind(&row.tracking_id_calendar)
    .bind(&row.id)
    .fetch_one(&mut **transaction)
    .await?;
    if !twin_exists {
        return Ok(false);
    }

    let angelegt = sqlx::query(
        "INSERT INTO calendars \
         (id, tracking_id_calendar, name, enabled, provider, source, color, connection_id, \
          deleted_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')) \
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(&row.id)
    .bind(&row.tracking_id_calendar)
    .bind(&row.name)
    .bind(row.enabled)
    .bind(&row.provider)
    .bind(&row.source)
    .bind(&row.color)
    .bind(&row.connection_id)
    .execute(&mut **transaction)
    .await?
    .rows_affected()
        == 1;

    // War der Kalender im Import aktiv und die behaltene Zeile noch nicht,
    // wird sie aktiviert: die naechste Inventur (storage.ts) raeumt sonst die
    // Termine jedes nicht aktivierten Kalenders ab, also gerade die importierten.
    // Nur im Import-Pfad; die Migration laesst den Wert des Gewinners stehen.
    // Nur beim ersten Mal (der Grabstein ist neu): ein erneuter Import schaltet
    // nicht wieder ein, was jemand inzwischen ausgeschaltet hat.
    if angelegt && row.enabled {
        sqlx::query(
            "UPDATE calendars SET enabled = 1
             WHERE id = (
               SELECT id FROM calendars
               WHERE provider = ? AND connection_id = ? AND tracking_id_calendar = ?
                 AND deleted_at IS NULL AND id <> ?
               ORDER BY updated_at DESC, created_at ASC, id ASC
               LIMIT 1
             ) AND enabled = 0",
        )
        .bind(&row.provider)
        .bind(&row.connection_id)
        .bind(&row.tracking_id_calendar)
        .bind(&row.id)
        .execute(&mut **transaction)
        .await?;
    }

    Ok(true)
}

pub(super) async fn resolve_event(
    transaction: &mut Transaction<'_, Sqlite>,
    row: &LegacyEvent,
) -> Result<EventTarget, sqlx::Error> {
    let calendar_id = canonical_calendar_id(transaction, &row.calendar_id).await?;

    if !row.tracking_id_event.is_empty() {
        let twin_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM events
               WHERE calendar_id = ? AND tracking_id_event = ? AND id <> ?
                 AND deleted_at IS NULL
                 AND julianday(started_at) = julianday(?)
             )",
        )
        .bind(&calendar_id)
        .bind(&row.tracking_id_event)
        .bind(&row.id)
        .bind(&row.started_at)
        .fetch_one(&mut **transaction)
        .await?;
        if twin_exists {
            return Ok(EventTarget::Twin(calendar_id));
        }
    }

    // Bewusst KEINE Wiederbelebung eines frueheren Import-Grabsteins, auch wenn
    // der Zwilling inzwischen nicht mehr lebt: ein vom Sync oder Nutzer
    // geloeschter oder verschobener Termin kaeme als Phantom zurueck, und der
    // Status 'matched' der Import-Buchung ist dafuer zu breit.
    Ok(EventTarget::Insert(calendar_id))
}

/// Derselbe Rang wie im Bereinigungs-Step: juengstes updated_at, dann die
/// aelteste Zeile, dann die kleinste id. Ist die ID selbst die behaltene
/// Zeile, kommt sie unveraendert zurueck; gibt es die ID oder eine Kennung
/// nicht, bleibt sie stehen.
pub(super) async fn canonical_calendar_id(
    transaction: &mut Transaction<'_, Sqlite>,
    calendar_id: &str,
) -> Result<String, sqlx::Error> {
    Ok(sqlx::query_scalar(
        "SELECT keeper.id
         FROM calendars AS source
         JOIN calendars AS keeper
           ON keeper.provider = source.provider
          AND keeper.connection_id = source.connection_id
          AND keeper.tracking_id_calendar = source.tracking_id_calendar
          AND keeper.deleted_at IS NULL
         WHERE source.id = ? AND source.tracking_id_calendar <> ''
         ORDER BY keeper.updated_at DESC, keeper.created_at ASC, keeper.id ASC
         LIMIT 1",
    )
    .bind(calendar_id)
    .fetch_optional(&mut **transaction)
    .await?
    .unwrap_or_else(|| calendar_id.to_string()))
}

/// Den Import-Termin als Grabstein am behaltenen Kalender anlegen.
pub(super) async fn insert_event_tombstone(
    transaction: &mut Transaction<'_, Sqlite>,
    row: &LegacyEvent,
    calendar_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO events \
         (id, tracking_id_event, calendar_id, title, started_at, ended_at, location, \
          meeting_link, description, note, recurrence_series_id, has_recurrence_rules, \
          is_all_day, provider, participants_json, deleted_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now')) \
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(&row.id)
    .bind(&row.tracking_id_event)
    .bind(calendar_id)
    .bind(&row.title)
    .bind(&row.started_at)
    .bind(&row.ended_at)
    .bind(&row.location)
    .bind(&row.meeting_link)
    .bind(&row.description)
    .bind(&row.note)
    .bind(&row.recurrence_series_id)
    .bind(row.has_recurrence_rules)
    .bind(row.is_all_day)
    .bind(&row.provider)
    .bind(&row.participants_json)
    .execute(&mut **transaction)
    .await?;

    // Luecke fuellen: hat der lebende Zwilling keine Teilnehmer, der Import-Termin
    // aber schon, uebernimmt der Zwilling sie (wie im Bereinigungs-Step).
    let hat_teilnehmer = row
        .participants_json
        .as_deref()
        .is_some_and(|p| !matches!(p, "" | "[]" | "null"));
    if hat_teilnehmer {
        sqlx::query(
            "UPDATE events SET participants_json = ?
             WHERE calendar_id = ? AND tracking_id_event = ? AND id <> ?
               AND deleted_at IS NULL
               AND julianday(started_at) = julianday(?)
               AND (participants_json IS NULL OR participants_json IN ('', '[]', 'null'))",
        )
        .bind(&row.participants_json)
        .bind(calendar_id)
        .bind(&row.tracking_id_event)
        .bind(&row.id)
        .bind(&row.started_at)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

/// Eine importierte Sitzung auf die behaltenen Zeilen zeigen lassen. `None`,
/// wenn nichts umzuschreiben ist. Setzt voraus, dass Kalender und Termine des
/// Imports vor den Sitzungen verarbeitet wurden (Sortierung in
/// plugins/db/src/import/legacy_vault/discovery.rs: calendars.json und
/// events.json stehen alphabetisch vor sessions/).
pub(super) async fn resolve_session_links(
    transaction: &mut Transaction<'_, Sqlite>,
    row: &LegacySession,
) -> Result<Option<LegacySession>, sqlx::Error> {
    let mut changed = false;
    let mut event_id = row.event_id.clone();
    if !event_id.is_empty() {
        let live_twin: Option<String> = sqlx::query_scalar(
            "SELECT twin.id
             FROM events AS dead
             JOIN events AS twin
               ON twin.calendar_id = dead.calendar_id
              AND twin.tracking_id_event = dead.tracking_id_event
              AND julianday(twin.started_at) = julianday(dead.started_at)
              AND twin.deleted_at IS NULL
              AND twin.id <> dead.id
             WHERE dead.id = ? AND dead.deleted_at IS NOT NULL
               AND dead.tracking_id_event <> ''
             ORDER BY twin.updated_at DESC, twin.id ASC
             LIMIT 1",
        )
        .bind(&event_id)
        .fetch_optional(&mut **transaction)
        .await?;
        // Rueckfall fuer einen verschobenen Termin: gleiche Kennung am selben
        // Kalender, anderer Beginn -- nur fuer Einzeltermine und nur, wenn es genau
        // einen lebenden gibt.
        let live_twin = match live_twin {
            Some(twin) => Some(twin),
            None => {
                let candidates: Vec<String> = sqlx::query_scalar(
                    "SELECT twin.id
                     FROM events AS dead
                     JOIN events AS twin
                       ON twin.calendar_id = dead.calendar_id
                      AND twin.tracking_id_event = dead.tracking_id_event
                      AND twin.deleted_at IS NULL
                      AND twin.id <> dead.id
                     WHERE dead.id = ? AND dead.deleted_at IS NOT NULL
                       AND dead.tracking_id_event <> ''
                       AND dead.has_recurrence_rules = 0 AND dead.recurrence_series_id = ''
                       AND twin.has_recurrence_rules = 0 AND twin.recurrence_series_id = ''
                     LIMIT 2",
                )
                .bind(&event_id)
                .fetch_all(&mut **transaction)
                .await?;
                (candidates.len() == 1).then(|| candidates[0].clone())
            }
        };
        if let Some(twin) = live_twin {
            event_id = twin;
            changed = true;
        }
    }

    let mut event_json = row.event_json.clone();
    let json_calendar_id: Option<String> = sqlx::query_scalar(
        "SELECT CASE WHEN json_valid(?1) THEN CAST(json_extract(?1, '$.calendar_id') AS TEXT) END",
    )
    .bind(&row.event_json)
    .fetch_one(&mut **transaction)
    .await?;
    if let Some(old) = json_calendar_id.filter(|id| !id.is_empty()) {
        let new = canonical_calendar_id(transaction, &old).await?;
        if new != old {
            event_json = sqlx::query_scalar("SELECT json_set(?, '$.calendar_id', ?)")
                .bind(&row.event_json)
                .bind(&new)
                .fetch_one(&mut **transaction)
                .await?;
            changed = true;
        }
    }

    Ok(changed.then(|| LegacySession {
        event_id,
        event_json,
        ..row.clone()
    }))
}
