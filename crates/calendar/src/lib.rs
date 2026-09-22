mod convert;
mod error;
pub mod runtime;

pub use anlg_calendar_interface::{
    CalendarEvent, CalendarListItem, CalendarProviderType, CreateEventInput, EventFilter,
};
pub use error::Error;

pub fn start(runtime: impl runtime::CalendarRuntime) {
    #[cfg(target_os = "macos")]
    {
        use std::sync::Arc;
        let runtime = Arc::new(runtime);
        anlg_apple_calendar::setup_change_notification(move || {
            runtime.emit_changed();
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = runtime;
}

#[cfg(target_os = "macos")]
use chrono::{DateTime, Utc};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ProviderConnectionIds {
    pub provider: CalendarProviderType,
    pub connection_ids: Vec<String>,
}

pub fn available_providers() -> Vec<CalendarProviderType> {
    // Der Fork spricht mit keinem Server. Google und Outlook liefen ueber die
    // Gegenstelle des Originals; Apple kommt ohne Konto aus (EventKit, lokal
    // per tauri-plugin-permissions freigegeben) und ist deshalb der einzige
    // Anbieter, der uebrig bleibt.
    #[cfg(target_os = "macos")]
    let providers = vec![CalendarProviderType::Apple];

    #[cfg(not(target_os = "macos"))]
    let providers = Vec::new();

    providers
}

pub async fn list_connection_ids(
    apple_authorized: bool,
) -> Result<Vec<ProviderConnectionIds>, Error> {
    #[cfg(target_os = "macos")]
    {
        // leerer Vec = Anbieter da, aber ohne Verbindung (im Gegensatz zu: fehlt ganz)
        let connection_ids = if apple_authorized {
            vec!["apple".to_string()]
        } else {
            Vec::new()
        };

        Ok(vec![ProviderConnectionIds {
            provider: CalendarProviderType::Apple,
            connection_ids,
        }])
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = apple_authorized;
        Ok(Vec::new())
    }
}

pub async fn is_provider_enabled(
    apple_authorized: bool,
    provider: CalendarProviderType,
) -> Result<bool, Error> {
    let all = list_connection_ids(apple_authorized).await?;
    Ok(all
        .iter()
        .any(|p| p.provider == provider && !p.connection_ids.is_empty()))
}

pub async fn list_calendars(
    provider: CalendarProviderType,
) -> Result<Vec<CalendarListItem>, Error> {
    match provider {
        CalendarProviderType::Apple => {
            let calendars = list_apple_calendars()?;
            Ok(convert::convert_apple_calendars(calendars))
        }
        _ => Err(Error::ProviderUnavailable { provider }),
    }
}

pub async fn list_events(
    provider: CalendarProviderType,
    filter: EventFilter,
) -> Result<Vec<CalendarEvent>, Error> {
    match provider {
        CalendarProviderType::Apple => {
            let events = list_apple_events(filter)?;
            Ok(convert::convert_apple_events(events))
        }
        _ => Err(Error::ProviderUnavailable { provider }),
    }
}

pub fn open_calendar(provider: CalendarProviderType) -> Result<(), Error> {
    match provider {
        CalendarProviderType::Apple => open_apple_calendar(),
        _ => Err(Error::UnsupportedOperation {
            operation: "open_calendar",
            provider,
        }),
    }
}

pub fn create_event(
    provider: CalendarProviderType,
    input: CreateEventInput,
) -> Result<String, Error> {
    match provider {
        CalendarProviderType::Apple => create_apple_event(input),
        _ => Err(Error::UnsupportedOperation {
            operation: "create_event",
            provider,
        }),
    }
}

pub fn parse_meeting_link(text: &str) -> Option<String> {
    use std::sync::LazyLock;

    use regex::Regex;

    static MEETING_REGEXES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![
            Regex::new(r"https://meet\.google\.com/[a-z0-9]{3,4}-[a-z0-9]{3,4}-[a-z0-9]{3,4}")
                .unwrap(),
            Regex::new(r"https://[a-z0-9.-]+\.zoom\.us/j/\d+(\?pwd=[a-zA-Z0-9.]+)?").unwrap(),
            Regex::new(r"https://app\.cal\.com/video/[a-zA-Z0-9]+").unwrap(),
        ]
    });
    for regex in MEETING_REGEXES.iter() {
        if let Some(m) = regex.find(text) {
            return Some(m.as_str().to_string());
        }
    }
    static URL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://[^\s]+").unwrap());
    URL_RE.find(text).map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_meeting_link_real_world() {
        let cases = vec![
            (
                "cal.com",
                "Where:\nhttps://app.cal.com/video/d713v9w1d2krBptPtwUAnJ\nNeed to reschedule?",
                "https://app.cal.com/video/d713v9w1d2krBptPtwUAnJ",
            ),
            (
                "zoom with pwd",
                "Where:\nhttps://us05web.zoom.us/j/87636383039?pwd=NOWbxkY9GNblR0yaLKaIzcy76IWRoj.1\nDescription",
                "https://us05web.zoom.us/j/87636383039?pwd=NOWbxkY9GNblR0yaLKaIzcy76IWRoj.1",
            ),
            (
                "google meet",
                "https://meet.google.com/xhv-ubut-zph\ntel:+1%20650-817-8427",
                "https://meet.google.com/xhv-ubut-zph",
            ),
            (
                "zoom in html",
                "<p>Join Zoom Meeting<br/>https://anarlog.zoom.us/j/86746313244?pwd=zFIICnVHzPim44QcYGbLCAAqtBrGzx.1<br/></p>",
                "https://anarlog.zoom.us/j/86746313244?pwd=zFIICnVHzPim44QcYGbLCAAqtBrGzx.1",
            ),
            (
                "korean google meet",
                "Google Meet으로 참석: https://meet.google.com/xkf-xcmo-rwh\n또는 다음 전화번호로",
                "https://meet.google.com/xkf-xcmo-rwh",
            ),
        ];

        for (name, input, expected) in cases {
            assert_eq!(
                parse_meeting_link(input),
                Some(expected.to_string()),
                "failed: {name}"
            );
        }
    }
}

// --- Apple helpers ---

#[cfg(target_os = "macos")]
fn open_apple_calendar() -> Result<(), Error> {
    let script = String::from(
        "
            tell application \"Calendar\"
                activate
                switch view to month view
                view calendar at current date
            end tell
        ",
    );

    std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .spawn()
        .map_err(|e| Error::Apple(e.to_string()))?
        .wait()
        .map_err(|e| Error::Apple(e.to_string()))?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn list_apple_calendars() -> Result<Vec<anlg_apple_calendar::types::AppleCalendar>, Error> {
    let handle = anlg_apple_calendar::Handle::new();
    handle
        .list_calendars()
        .map_err(|e| Error::Apple(e.to_string()))
}

#[cfg(target_os = "macos")]
fn list_apple_events(
    filter: EventFilter,
) -> Result<Vec<anlg_apple_calendar::types::AppleEvent>, Error> {
    let handle = anlg_apple_calendar::Handle::new();
    let filter = anlg_apple_calendar::types::EventFilter {
        from: filter.from,
        to: filter.to,
        calendar_tracking_id: filter.calendar_tracking_id,
    };

    handle
        .list_events(filter)
        .map_err(|e| Error::Apple(e.to_string()))
}

#[cfg(target_os = "macos")]
fn create_apple_event(input: CreateEventInput) -> Result<String, Error> {
    let handle = anlg_apple_calendar::Handle::new();

    let start_date = parse_datetime(&input.started_at, "started_at")?;
    let end_date = parse_datetime(&input.ended_at, "ended_at")?;

    let input = anlg_apple_calendar::types::CreateEventInput {
        title: input.title,
        start_date,
        end_date,
        calendar_id: input.calendar_tracking_id,
        is_all_day: input.is_all_day,
        location: input.location,
        notes: input.notes,
        url: input.url,
    };

    handle
        .create_event(input)
        .map_err(|e| Error::Apple(e.to_string()))
}

#[cfg(target_os = "macos")]
fn parse_datetime(value: &str, field: &'static str) -> Result<DateTime<Utc>, Error> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| Error::InvalidDateTime {
            field,
            value: value.to_string(),
        })
}

#[cfg(not(target_os = "macos"))]
fn open_apple_calendar() -> Result<(), Error> {
    Err(Error::ProviderUnavailable {
        provider: CalendarProviderType::Apple,
    })
}

#[cfg(not(target_os = "macos"))]
fn list_apple_calendars() -> Result<Vec<anlg_apple_calendar::types::AppleCalendar>, Error> {
    Err(Error::ProviderUnavailable {
        provider: CalendarProviderType::Apple,
    })
}

#[cfg(not(target_os = "macos"))]
fn list_apple_events(
    _filter: EventFilter,
) -> Result<Vec<anlg_apple_calendar::types::AppleEvent>, Error> {
    Err(Error::ProviderUnavailable {
        provider: CalendarProviderType::Apple,
    })
}

#[cfg(not(target_os = "macos"))]
fn create_apple_event(_input: CreateEventInput) -> Result<String, Error> {
    Err(Error::ProviderUnavailable {
        provider: CalendarProviderType::Apple,
    })
}
