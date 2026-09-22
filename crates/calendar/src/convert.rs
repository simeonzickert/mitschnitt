use anlg_apple_calendar::types::{
    AppleCalendar, AppleEvent, EventStatus as AppleEventStatus, Participant, ParticipantRole,
    ParticipantStatus,
};
use anlg_calendar_interface::{
    AttendeeRole, AttendeeStatus, CalendarEvent, CalendarListItem, CalendarProviderType,
    EventAttendee, EventPerson, EventStatus,
};

pub fn convert_apple_calendars(calendars: Vec<AppleCalendar>) -> Vec<CalendarListItem> {
    calendars
        .into_iter()
        .map(|calendar| {
            let raw = serde_json::to_string(&calendar).unwrap_or_default();

            CalendarListItem {
                provider: CalendarProviderType::Apple,
                id: calendar.id,
                title: calendar.title,
                source: Some(calendar.source.title),
                color: calendar.color.map(apple_color_to_css),
                is_primary: None,
                can_edit: Some(calendar.allows_content_modifications && !calendar.is_immutable),
                raw,
            }
        })
        .collect()
}

fn apple_color_to_css(color: anlg_apple_calendar::types::CalendarColor) -> String {
    format!(
        "rgba({}, {}, {}, {})",
        (color.red * 255.0).round(),
        (color.green * 255.0).round(),
        (color.blue * 255.0).round(),
        color.alpha,
    )
}

pub fn convert_apple_events(events: Vec<AppleEvent>) -> Vec<CalendarEvent> {
    events.into_iter().map(convert_apple_event).collect()
}

fn convert_apple_event(event: AppleEvent) -> CalendarEvent {
    let raw = serde_json::to_string(&event).unwrap_or_default();

    let id = if event.has_recurrence_rules {
        let date = event.occurrence_date.as_ref().unwrap_or(&event.start_date);
        let day = local_date_string(date, event.time_zone.as_deref());
        format!("{}:{}", event.event_identifier, day)
    } else {
        event.event_identifier.clone()
    };

    let organizer = event.organizer.as_ref().map(convert_person);
    let attendees = event.attendees.iter().map(convert_apple_attendee).collect();

    let recurring_event_id = if event.has_recurrence_rules {
        Some(
            event
                .recurrence
                .expect("event with has_recurrence_rules: true must have a recurrence")
                .series_identifier
                .clone(),
        )
    } else {
        None
    };

    let meeting_link =
        resolve_meeting_link(None, event.location.as_deref(), event.notes.as_deref());

    CalendarEvent {
        id,
        calendar_id: event.calendar.id,
        provider: CalendarProviderType::Apple,
        external_id: event.external_identifier,
        title: event.title,
        description: event.notes,
        location: event.location,
        url: event.url,
        meeting_link,
        started_at: event.start_date.to_rfc3339(),
        ended_at: event.end_date.to_rfc3339(),
        timezone: event.time_zone,
        is_all_day: event.is_all_day,
        status: convert_apple_status(event.status),
        organizer,
        attendees,
        has_recurrence_rules: event.has_recurrence_rules,
        recurring_event_id,
        raw,
    }
}

// Graph stores timed start/end as a timezone-naive `{date}T{time}` plus a
// separate timeZone field (Windows IDs or IANA). Persisting that string as-is
// makes JS `Date` treat UTC wall-clock values as local, so CEST notifications
// fire two hours early. All-day values are calendar dates (midnight on that
// date), not instants — converting those through UTC shifts the day west of UTC.

fn convert_apple_status(status: AppleEventStatus) -> EventStatus {
    match status {
        AppleEventStatus::None | AppleEventStatus::Confirmed => EventStatus::Confirmed,
        AppleEventStatus::Tentative => EventStatus::Tentative,
        AppleEventStatus::Canceled => EventStatus::Cancelled,
    }
}

fn convert_person(participant: &Participant) -> EventPerson {
    EventPerson {
        name: participant.name.clone(),
        email: participant.email.clone(),
        is_current_user: participant.is_current_user,
    }
}

fn convert_apple_attendee(participant: &Participant) -> EventAttendee {
    EventAttendee {
        name: participant.name.clone(),
        email: participant.email.clone(),
        is_current_user: participant.is_current_user,
        status: convert_apple_attendee_status(&participant.status),
        role: convert_apple_attendee_role(&participant.role),
    }
}

fn convert_apple_attendee_status(status: &ParticipantStatus) -> AttendeeStatus {
    match status {
        ParticipantStatus::Unknown | ParticipantStatus::Pending => AttendeeStatus::Pending,
        ParticipantStatus::Accepted
        | ParticipantStatus::Delegated
        | ParticipantStatus::Completed
        | ParticipantStatus::InProgress => AttendeeStatus::Accepted,
        ParticipantStatus::Tentative => AttendeeStatus::Tentative,
        ParticipantStatus::Declined => AttendeeStatus::Declined,
    }
}

fn convert_apple_attendee_role(role: &ParticipantRole) -> AttendeeRole {
    match role {
        ParticipantRole::Unknown | ParticipantRole::Required => AttendeeRole::Required,
        ParticipantRole::Optional => AttendeeRole::Optional,
        ParticipantRole::Chair => AttendeeRole::Chair,
        ParticipantRole::NonParticipant => AttendeeRole::NonParticipant,
    }
}

fn local_date_string(date: &chrono::DateTime<chrono::Utc>, event_tz: Option<&str>) -> String {
    if let Some(tz_name) = event_tz
        && let Ok(tz) = tz_name.parse::<chrono_tz::Tz>()
    {
        return date.with_timezone(&tz).format("%Y-%m-%d").to_string();
    }

    date.with_timezone(&chrono::Local)
        .format("%Y-%m-%d")
        .to_string()
}

// Provider-native links win; otherwise fall back to a link parsed from the
// location, then from the description, so every event crosses the Tauri
// bridge with its final meeting link already resolved.
fn resolve_meeting_link(
    provider_link: Option<String>,
    location: Option<&str>,
    description: Option<&str>,
) -> Option<String> {
    provider_link
        .or_else(|| location.and_then(crate::parse_meeting_link))
        .or_else(|| description.and_then(crate::parse_meeting_link))
}

#[cfg(test)]
mod meeting_link_tests {
    use super::*;

    const MEET_LINK: &str = "https://meet.google.com/abc-defg-hij";
    const CAL_LINK: &str = "https://app.cal.com/video/abc123";

    #[test]
    fn provider_link_wins_over_parsed_fields() {
        assert_eq!(
            resolve_meeting_link(
                Some("https://provider.example/join".to_string()),
                Some(MEET_LINK),
                Some(CAL_LINK),
            ),
            Some("https://provider.example/join".to_string())
        );
    }

    #[test]
    fn location_link_wins_over_description_link() {
        assert_eq!(
            resolve_meeting_link(None, Some(MEET_LINK), Some(CAL_LINK)),
            Some(MEET_LINK.to_string())
        );
    }

    #[test]
    fn description_link_is_the_last_fallback() {
        assert_eq!(
            resolve_meeting_link(None, Some("Conference room 4"), Some(CAL_LINK)),
            Some(CAL_LINK.to_string())
        );
    }

    #[test]
    fn no_link_stays_absent() {
        assert_eq!(
            resolve_meeting_link(None, Some("Conference room 4"), Some("Agenda")),
            None
        );
        assert_eq!(resolve_meeting_link(None, None, None), None);
    }
}
