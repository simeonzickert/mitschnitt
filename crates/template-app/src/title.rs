use crate::{Participant, Session, common_derives};
use anlg_askama_utils::filters;

common_derives! {
    #[derive(askama::Template)]
    #[template(path = "title.system.md.jinja")]
    pub struct TitleSystem {
        pub language: Option<String>,
    }
}

common_derives! {
    /// Der Titel bekommt DENSELBEN Kontext wie die Zusammenfassung.
    ///
    /// Bis zum 03.09.2026 bestand dieser Prompt aus der fertigen Notiz und dem
    /// Satz "give me SUPER CONCISE title" -- kein Kalendertitel, kein Zeitraum,
    /// keine Teilnehmer. Aus einer verhoerten Fassung von "Sarec" wurde so der
    /// erfundene Name "Serredi": das Modell hatte nichts, woran es ihn haette
    /// pruefen koennen. Die Zusammenfassung nebenan hatte all das die ganze
    /// Zeit (`enhance.user.md.jinja`).
    #[derive(askama::Template)]
    #[template(path = "title.user.md.jinja")]
    pub struct TitleUser {
        pub enhanced_note: String,
        pub session: Option<Session>,
        pub participants: Vec<Participant>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anlg_askama_utils::{tpl_assert, tpl_snapshot};

    tpl_assert!(
        test_language_as_specified,
        TitleSystem {
            language: Some("ko".to_string()),
        },
        |v| v.contains("Korean")
    );

    tpl_snapshot!(
        test_title_system,
        TitleSystem { language: None },
        fixed_date = "2025-01-01",
        @r#"
    # General Instructions

    Current date: 2025-01-01

    - You are a professional assistant that generates a perfect title for a meeting note, in English language.

    # Format Requirements

    - Only output the title as plaintext, nothing else. No characters like *"'([{}]):.
    - Never ask questions or request more information.
    - If the note is empty or has no meaningful content, output exactly: <EMPTY>
    "#);

    tpl_snapshot!(
        test_title_user,
        TitleUser {
            enhanced_note: "".to_string(),
            session: None,
            participants: vec![],
        },
        @"
    <note>

    </note>

    Now, give me SUPER CONCISE title for above note. Only about the topic of the meeting.
    "
    );

    // Der Fall, um den es geht: ein Termin mit Teilnehmern.
    tpl_snapshot!(
        test_title_user_mit_kontext,
        TitleUser {
            enhanced_note: "Serredi berichtet vom Stand.".to_string(),
            session: Some(crate::Session {
                title: Some("Sedacz Jourfixe".to_string()),
                started_at: Some("2026-09-02 10:00".to_string()),
                ended_at: Some("2026-09-02 10:45".to_string()),
                event: Some(crate::Event {
                    name: "Sedacz Jourfixe".to_string(),
                }),
            }),
            participants: vec![crate::Participant {
                name: "Sarec".to_string(),
                job_title: Some("Projektleitung".to_string()),
            }],
        },
        @"
    # Context


    Meeting: Sedacz Jourfixe
    Time: 2026-09-02 10:00 - 2026-09-02 10:45
    Participants:

    - Sarec (Projektleitung)

    <note>
    Serredi berichtet vom Stand.
    </note>

    Now, give me SUPER CONCISE title for above note. Only about the topic of the meeting.
    "
    );
}
