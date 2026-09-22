//! Folder and file naming for the mirror.
//!
//! A mirror folder has to be readable at a glance in Finder (date + title) and
//! still collision-free for two meetings on the same day with the same title,
//! so every name carries the first eight characters of the session id.

/// The three files a mirrored session consists of.
pub const TRANSCRIPT_FILE: &str = "transcript.md";
pub const SUMMARY_FILE: &str = "summary.md";
pub const META_FILE: &str = "meta.json";

const MAX_SLUG_LEN: usize = 60;
const ID_SUFFIX_LEN: usize = 8;

/// `2026-08-28_welcome-to-mitschnitt_e5f6a7b8`
///
/// `occurred_at` is an ISO-8601 timestamp; only the leading `YYYY-MM-DD` is
/// used, and an unparseable value degrades to `undatiert` rather than failing:
/// a mirror that refuses to write is worse than one with a plain name.
pub fn session_dir_name(session_id: &str, title: &str, occurred_at: &str) -> String {
    let date = iso_date(occurred_at);
    let slug = slugify(title);
    let suffix = short_id(session_id);

    match (slug.is_empty(), suffix.is_empty()) {
        (true, true) => date,
        (true, false) => format!("{date}_{suffix}"),
        (false, true) => format!("{date}_{slug}"),
        (false, false) => format!("{date}_{slug}_{suffix}"),
    }
}

fn iso_date(occurred_at: &str) -> String {
    let candidate: String = occurred_at.chars().take(10).collect();
    let looks_like_a_date = candidate.len() == 10
        && candidate
            .chars()
            .enumerate()
            .all(|(index, character)| match index {
                4 | 7 => character == '-',
                _ => character.is_ascii_digit(),
            });

    if looks_like_a_date {
        candidate
    } else {
        "undatiert".to_string()
    }
}

fn short_id(session_id: &str) -> String {
    session_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(ID_SUFFIX_LEN)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Lowercase ASCII slug. Umlauts are transliterated rather than dropped so a
/// German title stays recognisable; everything else non-alphanumeric collapses
/// into single hyphens.
fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut pending_separator = false;

    for character in title.trim().chars() {
        let replacement = match character {
            'ä' | 'Ä' => Some("ae"),
            'ö' | 'Ö' => Some("oe"),
            'ü' | 'Ü' => Some("ue"),
            'ß' => Some("ss"),
            _ => None,
        };

        if let Some(replacement) = replacement {
            if pending_separator && !slug.is_empty() {
                slug.push('-');
            }
            pending_separator = false;
            slug.push_str(replacement);
            continue;
        }

        if character.is_ascii_alphanumeric() {
            if pending_separator && !slug.is_empty() {
                slug.push('-');
            }
            pending_separator = false;
            slug.push(character.to_ascii_lowercase());
        } else {
            pending_separator = true;
        }

        if slug.chars().count() >= MAX_SLUG_LEN {
            break;
        }
    }

    slug.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_readable_name_from_date_title_and_id() {
        assert_eq!(
            session_dir_name(
                "e5f6a7b8-0000-4000-8000-000000000001",
                "Welcome to Mitschnitt",
                "2026-08-28T19:32:41.644Z"
            ),
            "2026-08-28_welcome-to-mitschnitt_e5f6a7b8"
        );
    }

    #[test]
    fn transliterates_umlauts_instead_of_dropping_them() {
        assert!(
            session_dir_name("id", "Jour Fixe Grün & Söhne", "2026-01-02T00:00:00Z")
                .contains("gruen-soehne")
        );
    }

    // Two meetings on the same day with the same title must not share a folder;
    // the id suffix is the only thing separating them.
    #[test]
    fn same_day_and_title_still_yield_distinct_folders() {
        let first = session_dir_name("11111111-aaaa", "Standup", "2026-05-05T08:00:00Z");
        let second = session_dir_name("22222222-bbbb", "Standup", "2026-05-05T09:00:00Z");
        assert_ne!(first, second);
    }

    #[test]
    fn an_untitled_session_still_gets_a_usable_name() {
        assert_eq!(
            session_dir_name("abcdefgh-1234", "   ", "2026-05-05T08:00:00Z"),
            "2026-05-05_abcdefgh"
        );
    }

    #[test]
    fn an_unparseable_date_degrades_instead_of_failing() {
        assert!(session_dir_name("abcdefgh", "Titel", "").starts_with("undatiert_"));
        assert!(session_dir_name("abcdefgh", "Titel", "kein-datum").starts_with("undatiert_"));
    }

    // A path separator in a meeting title would otherwise escape the mirror root.
    #[test]
    fn a_title_can_never_introduce_a_path_separator() {
        let name = session_dir_name("abcdefgh", "../../etc/passwd", "2026-05-05T08:00:00Z");
        assert!(!name.contains('/'), "{name}");
        assert!(!name.contains(".."), "{name}");
    }

    #[test]
    fn long_titles_are_bounded() {
        let name = session_dir_name("abcdefgh", &"wort ".repeat(80), "2026-05-05T08:00:00Z");
        assert!(name.len() < 100, "{name} is {} chars", name.len());
    }
}
