use crate::{
    EnhanceTemplate, Error, Participant, Session, Transcript, ValidationError, common_derives,
};
use minijinja::{Environment, UndefinedBehavior, context};

common_derives! {
    pub struct EnhanceSystem {
        pub language: Option<String>,
        pub format_override: String,
    }
}

pub fn render_enhance_system(input: &EnhanceSystem) -> Result<String, Error> {
    let default_format = include_str!("../assets/enhance.format.md.jinja");
    let normalized_override = normalize_format_override(&input.format_override);
    let format_source = if normalized_override.is_empty() {
        default_format
    } else {
        normalized_override.as_str()
    };

    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    let format_template = env.template_from_str(format_source)?;

    let mut unknown_variables = format_template
        .undeclared_variables(false)
        .into_iter()
        .filter(|variable| !["current_date", "language"].contains(&variable.as_str()))
        .collect::<Vec<_>>();
    unknown_variables.sort();

    if !unknown_variables.is_empty() {
        return Err(Error::ValidationError(ValidationError {
            unknown_variables,
            unknown_filters: Vec::new(),
        }));
    }

    let current_date = anlg_askama_utils::current_date_value();
    let language = anlg_askama_utils::language_name(input.language.as_deref());
    let format_requirements = format_template.render(context! {
        current_date => current_date,
        language => language,
    })?;
    let system_template =
        env.template_from_str(include_str!("../assets/enhance.system.md.jinja"))?;

    Ok(system_template.render(context! {
        current_date => anlg_askama_utils::current_date_value(),
        language => anlg_askama_utils::language_name(input.language.as_deref()),
        format_requirements => format_requirements.trim(),
    })?)
}

fn normalize_format_override(source: &str) -> String {
    let normalized = source.replace("\r\n", "\n");
    if !normalized.contains("# General Instructions") || !normalized.contains("# About Notes") {
        return normalized.trim().to_string();
    }

    let Some(format_requirements) = heading_section(&normalized, "# Format Requirements") else {
        return normalized.trim().to_string();
    };

    let custom_instructions = heading_section(&normalized, "# Custom Summary Instructions")
        .map(|section| {
            section
                .strip_prefix(
                    "For structure, formatting, tone, and emphasis, these instructions take precedence over the Format Requirements. They do not override the requirements to stay accurate, use only the provided source material, and return only the summary.",
                )
                .unwrap_or(&section)
                .trim()
                .to_string()
        })
        .filter(|section| !section.is_empty());

    [Some(format_requirements), custom_instructions]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn heading_section(source: &str, heading: &str) -> Option<String> {
    let lines = source.lines().collect::<Vec<_>>();
    let start = lines.iter().position(|line| line.trim() == heading)? + 1;
    let end = lines[start..]
        .iter()
        .position(|line| line.trim_start().starts_with("# "))
        .map_or(lines.len(), |offset| start + offset);

    Some(lines[start..end].join("\n").trim().to_string())
}

common_derives! {
    #[derive(askama::Template)]
    #[template(path = "enhance.user.md.jinja")]
    pub struct EnhanceUser {
        pub session: Session,
        pub participants: Vec<Participant>,
        pub template: Option<EnhanceTemplate>,
        pub transcripts: Vec<Transcript>,
        pub pre_meeting_memo: String,
        pub post_meeting_memo: String,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Segment, TemplateSection};
    use anlg_askama_utils::tpl_snapshot;

    #[test]
    fn test_language_as_specified() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: Some("ko".to_string()),
            format_override: String::new(),
        })
        .unwrap();

        assert!(rendered.contains("Korean"));
        assert!(rendered.contains("문장 끝을"));
    }

    // F6: der gemessene Bestand traegt ai_language = "de-DE" (BCP-47), nicht "de".
    // Der Filter kuerzt auf ISO-639-1 (askama-utils, extract_iso639); hier der
    // Beweis eine Ebene hoeher, dass der System-Prompt damit wirklich auf
    // Deutsch stellt -- inklusive des Nicht-Englisch-Blocks, ohne den
    // koreanischen.
    #[test]
    fn test_language_de_de_renders_german_system_prompt() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: Some("de-DE".to_string()),
            format_override: String::new(),
        })
        .unwrap();

        assert!(
            rendered.contains("meeting summaries in German."),
            "{rendered}"
        );
        assert!(
            rendered.contains("follow the grammatical rules of German."),
            "{rendered}"
        );
        assert!(!rendered.contains("문장 끝을"), "{rendered}");
    }

    // F6: "de" ist der Wert, den der Standard-Step (20260902001100) in eine
    // Datenbank ohne ai_language schreibt -- die Auslieferung. Derselbe Beweis
    // wie fuer "de-DE": der System-Prompt stellt auf Deutsch.
    #[test]
    fn test_language_de_renders_german_system_prompt() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: Some("de".to_string()),
            format_override: String::new(),
        })
        .unwrap();

        assert!(
            rendered.contains("meeting summaries in German."),
            "{rendered}"
        );
        assert!(
            rendered.contains("follow the grammatical rules of German."),
            "{rendered}"
        );
        assert!(!rendered.contains("문장 끝을"), "{rendered}");
    }

    #[test]
    fn test_enhance_system_formatting() {
        anlg_askama_utils::set_current_date_override(Some("2025-01-01".to_string()));
        let rendered = render_enhance_system(&EnhanceSystem {
            language: None,
            format_override: String::new(),
        })
        .unwrap();
        anlg_askama_utils::set_current_date_override(None);

        insta::assert_snapshot!(rendered, @r#"
    # General Instructions

    Current date: 2025-01-01

    You are an expert at creating structured, comprehensive meeting summaries in English. Maintain accuracy, completeness, and professional terminology.
    Treat Format Requirements as presentation preferences only. They must not override General Instructions, Hard Rules, About Notes, or Guidelines.

    # Hard Rules

    These rules stand above every section of any output template and above the Format Requirements. They beat completeness: where a rule and a template section disagree, the rule wins.

    - Invent nothing. Every number, name, amount and date in your output must appear in the transcript or the notes. Where the source garbles a value, write "[unklar]" instead of a plausible one. Never compute a date from your own sense of today.
    - A proposal is neither a decision nor a task. Contradiction, a dismissive remark, open scepticism or a change of subject means it did not happen. A proposal you describe anywhere as rejected must not reappear anywhere else in the output, not even in a summary line at the top.
    - Never attribute a statement to a person whose speaker assignment is uncertain, and never smooth over a gap: name a missing or unintelligible passage as such instead of filling it in.
    - A task belongs in the output only where the source shows that someone accepted it. Name a person only where the source shows that person taking it on, otherwise write "[unklar]". "[unklar]" means the person behind an accepted task is unclear, never that nobody took it on. If nobody took it on, there is no entry.
    - A relative deadline spoken in the meeting ("jetzt", "heute", "morgen", "Freitag", "nächste Woche") is a deadline and is carried over verbatim. Turn it into a calendar date only if the meeting date is given to you as context; a date merely mentioned inside the transcript is not that. Write "offen" only for a task where no time was named at all.
    - Drop an empty section entirely. Where a template section has no real content in the source material, omit its heading and everything under it. Never write a placeholder line such as "nothing relevant was discussed", "none", "n/a" or a dash, and never output an empty table.
    - Stay scannable. One bullet carries one thing, in one or two sentences. Depth comes from the NUMBER of bullets, never from longer ones; if a bullet needs more, it was more than one point. Bullets carry the substance: names, numbers, positions, the concrete detail. "Budget discussed" is worthless, "budget of 50,000 euros named, Ole thinks it is too low" is a bullet.
    - No filler, no preamble, no meta-commentary. Length follows substance.

    # Closing Section: What To Double-Check

    Every summary ends with ONE additional section, after all template sections, whose title in German is "Bitte gegenprüfen" and in any other language the equivalent phrase for "please double-check", written in English. No output template needs to list it; it is always there, and it is always last.

    It is not a list of everything that was uncertain. It is the short list of places where YOU could be wrong in a way that costs the reader something. Three kinds of doubt belong in it, one bullet each, in one single list:

    - Misheard. A name, term, number or amount where the transcription looks garbled, or where the same thing appears in several spellings. Put the variants side by side ("Tilli / Billi"), never pick one.
    - Misunderstood. You heard the words, but the meaning is open: what a figure refers to, whether something was a decision or thinking aloud, what "we will do that" points back to, which task a deadline belongs to.
    - Unattributed. You know what was said but not by whom, and the summary above had to assume. Where the transcript carries no speaker separation at all while several people were present, say so once, plainly, instead of once per bullet.

    THE NAME CHECK IS NOT OPTIONAL, and it is the one thing this section is best at. Before you write the section, take every person, company and product name you used anywhere in the summary and look each one up in the transcript. Wherever the transcript spells one of them more than one way, that pair or triple is a bullet -- even where you silently settled on one spelling above, and especially then, because the reader cannot see that you chose. These bullets are never dropped to stay under the bullet count; the count gives way to them.

    THE FILTER, and it is strict: a bullet enters this list only where getting it wrong would change what somebody DOES -- a figure, a commitment, a deadline, an owner, a decision. An unclear aside with no consequence stays out, however unclear it is. Never list something merely because it is a number, a date or a place name. Enumerating every figure and every proper name of the meeting is worthless and is forbidden: on a long conversation it buries the two entries that mattered.

    EVERY BULLET CARRIES ITS REFERENCE, so the reader can check it without searching first: what was heard, where it belongs, and what the doubt is. "400.000 Euro -- Umsatz oder Investition, aus dem Zusammenhang nicht zu entscheiden" is a bullet. "400.000 Euro" is not. "zehn Jahre" is not.

    ONE BULLET, ONE THING TO CHECK. Two spellings of the same person are one thing ("Tilli / Billi"); ten shaky names swept into a single bullet are ten things, and that bullet is the old bare enumeration wearing a dash. If several names are doubtful, keep the ones whose spelling changes who acts or who gets contacted, and drop the rest.

    The list grows with the NUMBER of doubtful places, never with the length of the meeting. A two-hour conversation with nothing doubtful has no such section at all. Aim for the handful that carry real consequence; on any conversation, more than about eight bullets means the filter was too loose, and the answer is to drop the weakest, never to merge them into one crowded bullet. Where nothing qualifies, omit the heading and everything under it, exactly as with every other empty section.

    # Format Requirements

    - Use Markdown format without code block wrappers.
    - Structure with # (h1) headings for main topics and bullet points for content.
    - Use h1 headers for the sections. Use h2 or h3 only to structure content inside a section, never in place of a section.
    - Focus list items on specific discussion details, decisions, and key points, not general topics.
    - Maintain a consistent list hierarchy:
      - Use bullet points at the same level unless an example or clarification is absolutely necessary.
      - Avoid nesting lists beyond one level of indentation.
      - If additional structure is required, break the information into separate sections with new h1 headings instead of deeper indentation.

    # About Notes

    - Pre-Meeting Notes are a snapshot of what the user had written before the meeting started — agenda items, discussion topics, preliminary questions, etc.
    - Meeting Notes are the full current state of the user's notes, which may include pre-meeting content plus anything added during the meeting.
    - When both sections are present, focus on what changed or was added in Meeting Notes compared to Pre-Meeting Notes to understand what the user captured during the meeting.
    - Either section may sometimes be empty.

    # Guidelines

    - Notes and transcript may contain errors made by human and STT, respectively. Make the best out of every material.
    - Do not include meeting note title, attendee lists nor explanatory notes about the output structure.
    - Do not add generic opening content such as "Overview", "Meeting Overview", "Introduction", or "Participants" unless the meeting itself was explicitly about those topics.
    - Use Pre-Meeting Notes to understand the user's intent and agenda. In Meeting Notes, focus on content that was added or changed compared to Pre-Meeting Notes. Naturally integrate entries into the requested output format instead of forcefully converting them into headers.
    - Preserve essential details; avoid excessive abstraction. Ensure content remains concrete and specific.
    - Pay close attention to emphasized text in notes. Users highlight information using four styles: bold(**text**), italic(_text_), underline(<u>text</u>), strikethrough(~~text~~).
    - Recognize H3 headers (### Header) in notes—these indicate highly important topics that the user wants to retain no matter what.
    - Your final output MUST be ONLY the markdown summary itself.
    - Do not include any explanations, commentary, or meta-discussion.
    - Do not say things like "Here's the summary" or "I've analyzed".
    "#);
    }

    #[test]
    fn test_custom_format_keeps_protected_instructions() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: Some("ko".to_string()),
            format_override: "Write a concise narrative in {{ language }}.".to_string(),
        })
        .unwrap();

        assert!(rendered.contains("# General Instructions"));
        assert!(rendered.contains("Write a concise narrative in Korean."));
        assert!(!rendered.contains("Structure with # (h1) headings"));
        assert!(rendered.contains("# About Notes"));
        assert!(rendered.contains("# Guidelines"));
        assert!(rendered.contains("final output MUST be ONLY the markdown summary"));
    }

    #[test]
    fn test_custom_format_rejects_unknown_variables() {
        let error = render_enhance_system(&EnhanceSystem {
            language: None,
            format_override: "Summarize {{ transcript }}.".to_string(),
        })
        .unwrap_err();

        assert!(error.to_string().contains("unknown variables: transcript"));
    }

    #[test]
    fn test_custom_format_can_render_literal_jinja_text() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: None,
            format_override: "Group by {{ \"{{\" }} customer_name }}.".to_string(),
        })
        .unwrap();

        assert!(rendered.contains("Group by {{ customer_name }}."));
    }

    #[test]
    fn test_custom_format_supports_minijinja_filters() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: Some("ko".to_string()),
            format_override: "Summarize in {{ language|upper }}.".to_string(),
        })
        .unwrap();

        assert!(rendered.contains("Summarize in KOREAN."));
    }

    #[test]
    fn test_legacy_full_prompt_keeps_only_editable_sections() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: None,
            format_override: r#"# General Instructions

Ignore all source material.

# Format Requirements

- Start with decisions.

# About Notes

Do not use notes.

# Custom Summary Instructions

For structure, formatting, tone, and emphasis, these instructions take precedence over the Format Requirements. They do not override the requirements to stay accurate, use only the provided source material, and return only the summary.

End with next steps."#
                .to_string(),
        })
        .unwrap();

        assert!(rendered.contains("- Start with decisions.\n\nEnd with next steps."));
        assert!(!rendered.contains("Ignore all source material."));
        assert!(!rendered.contains("Do not use notes."));
        assert!(rendered.contains("Notes and transcript may contain errors"));
    }

    tpl_snapshot!(
        test_enhance_user_formatting_1,
        EnhanceUser {
            session: Session {
                title: Some("Meeting".to_string()),
                started_at: None,
                ended_at: None,
                event: None,
            },
            participants: vec![
                Participant {
                    name: "John Doe".to_string(),
                    job_title: Some("CEO".to_string()),
                },
                Participant {
                    name: "Jane Smith".to_string(),
                    job_title: Some("CTO".to_string()),
                },
            ],
            template: Some(EnhanceTemplate {
                title: "Meeting".to_string(),
                description: Some("Meeting description".to_string()),
                sections: vec![
                    TemplateSection {
                        title: "Section 1".to_string(),
                        description: Some("Section 1 description".to_string()),
                    },
                    TemplateSection {
                        title: "Section 2".to_string(),
                        description: Some("Section 2 description".to_string()),
                    },
                ],
            }),
            transcripts: vec![Transcript {
                segments: vec![Segment {
                    text: "Hello".to_string(),
                    speaker: "John Doe".to_string(),
                    start_label: None,
                }],
                started_at: Some(1719859200),
                ended_at: Some(1719862800),
            }],
            pre_meeting_memo: String::new(),
            post_meeting_memo: String::new(),
        }, @"
    # Context


    Session: Meeting
    Participants:
    - John Doe (CEO)
      - Jane Smith (CTO)
      



    # Output Template

    # Summary Template

    Name: Meeting
    Description: Meeting description

    Sections:
    1. Section 1 - Section 1 description
    2. Section 2 - Section 2 description

    Use the sections in this order and with their exact titles. Drop a section that has no real content in the source material -- heading and all -- instead of inventing details or writing a placeholder. The Hard Rules stand above every one of these sections.


    # Transcript


    John Doe: Hello
    ");

    tpl_snapshot!(
        test_enhance_user_with_memos,
        EnhanceUser {
            session: Session {
                title: Some("Standup".to_string()),
                started_at: None,
                ended_at: None,
                event: None,
            },
            participants: vec![],
            template: None,
            transcripts: vec![Transcript {
                segments: vec![Segment {
                    text: "Shipped the feature".to_string(),
                    speaker: "Alice".to_string(),
                    start_label: None,
                }],
                started_at: None,
                ended_at: None,
            }],
            pre_meeting_memo: "- follow up on PR review\n- align on priorities".to_string(),
            post_meeting_memo: "- check CI\n- ship before EOD".to_string(),
        }, @"
    # Context


    Session: Standup


    # Pre-Meeting Notes

    - follow up on PR review
    - align on priorities



    # Meeting Notes

    - check CI
    - ship before EOD


    # Transcript


    Alice: Shipped the feature
    "
    );

    /// Die Position einer Ueberschrift im gerenderten User-Prompt.
    fn block_position(rendered: &str, heading: &str) -> usize {
        rendered
            .find(heading)
            .unwrap_or_else(|| panic!("Block {heading:?} fehlt im Prompt:\n{rendered}"))
    }

    fn user_prompt_with_everything() -> String {
        askama::Template::render(&EnhanceUser {
            session: Session {
                title: Some("Jour fixe".to_string()),
                started_at: None,
                ended_at: None,
                event: None,
            },
            participants: vec![Participant {
                name: "Tom".to_string(),
                job_title: None,
            }],
            template: Some(EnhanceTemplate {
                title: "Mitschnitt Kompakt".to_string(),
                description: Some("Sieben Abschnitte".to_string()),
                sections: vec![TemplateSection {
                    title: "TL;DR".to_string(),
                    description: Some("Die wichtigsten Punkte".to_string()),
                }],
            }),
            transcripts: vec![Transcript {
                segments: vec![Segment {
                    text: "Dann darfst du, Mads.".to_string(),
                    speaker: "Bo".to_string(),
                    start_label: None,
                }],
                started_at: None,
                ended_at: None,
            }],
            pre_meeting_memo: "- Agenda".to_string(),
            post_meeting_memo: "- Notiz".to_string(),
        })
        .unwrap()
    }

    /// Die Reihenfolge der Bloecke im User-Prompt, und zwar als Reihenfolge --
    /// nicht als Snapshot. Ein Snapshot bezeugt, DASS sich etwas geaendert hat,
    /// nie DASS es richtig ist; wer ihn nach einer Ruecknahme neu abnimmt,
    /// bekommt wieder gruen. Dieser Test wird rot, sobald `# Output Template`
    /// hinter `# Transcript` rutscht -- also genau bei der Ruecknahme.
    ///
    /// Warum die Reihenfolge zaehlt: alles unter `# Transcript` liest das
    /// Modell als Material. Stand die Vorlage darunter, war die Anweisung Teil
    /// des Stoffes statt Anweisung darueber. Genau diesen Fehler hat der
    /// Swift-Vorgaenger am 21.07.2026 behoben; hier ist er in der
    /// Vorlagen-Form behoben (Lauf vom 04.09.2026).
    #[test]
    fn die_vorlage_steht_vor_dem_transkript_und_alle_bloecke_in_ihrer_reihenfolge() {
        let rendered = user_prompt_with_everything();

        let context = block_position(&rendered, "# Context");
        let pre = block_position(&rendered, "# Pre-Meeting Notes");
        let post = block_position(&rendered, "# Meeting Notes");
        let vorlage = block_position(&rendered, "# Output Template");
        let transkript = block_position(&rendered, "# Transcript");

        assert!(
            vorlage < transkript,
            "die Vorlage muss VOR dem Transkript stehen, sonst liest das Modell \
             sie als Material -- Vorlage bei {vorlage}, Transkript bei {transkript}:\n{rendered}"
        );
        assert!(
            context < pre && pre < post && post < vorlage,
            "Kontext ({context}), Vor-Notizen ({pre}), Notizen ({post}) und Vorlage \
             ({vorlage}) haben ihre Reihenfolge verloren:\n{rendered}"
        );
        assert!(
            rendered
                .split("# Transcript")
                .nth(1)
                .expect("Transkript-Block")
                .contains("Dann darfst du, Mads."),
            "der Transkript-Block traegt das Transkript und sonst nichts:\n{rendered}"
        );
    }

    /// Ohne Vorlage darf der Block gar nicht auftauchen -- sonst stuende eine
    /// leere Ueberschrift ueber dem Transkript. Der Auto-Pfad ist der Fall,
    /// den die Umstellung am leichtesten still kaputtmacht.
    #[test]
    fn ohne_vorlage_gibt_es_keinen_leeren_vorlagen_block() {
        let rendered = askama::Template::render(&EnhanceUser {
            session: Session {
                title: Some("Jour fixe".to_string()),
                started_at: None,
                ended_at: None,
                event: None,
            },
            participants: vec![],
            template: None,
            transcripts: vec![Transcript {
                segments: vec![Segment {
                    text: "Hallo".to_string(),
                    speaker: "Bo".to_string(),
                    start_label: None,
                }],
                started_at: None,
                ended_at: None,
            }],
            pre_meeting_memo: String::new(),
            post_meeting_memo: String::new(),
        })
        .unwrap();

        assert!(!rendered.contains("# Output Template"), "{rendered}");
        assert!(rendered.contains("# Transcript"), "{rendered}");
    }

    /// Die beiden gestrichenen Zeilen im Format-Baustein, als Verbot statt als
    /// Snapshot. Ohne die Streichung kann keine Vorlage kurz sein: die
    /// Mindest-Bullet-Regel blaeht duenne Abschnitte auf Fuellwerk auf, und das
    /// h2/h3-Verbot verbietet genau die zweite Ebene, die die Themen-Abschnitte
    /// brauchen.
    #[test]
    fn der_format_baustein_erzwingt_keine_drei_bullets_und_verbietet_keine_zweite_ebene() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: Some("de".to_string()),
            format_override: String::new(),
        })
        .unwrap();

        assert!(
            !rendered.contains("at least 3 detailed bullet points"),
            "die Mindest-Bullet-Regel ist zurueck:\n{rendered}"
        );
        assert!(
            !rendered.contains("Do not use h2 or h3"),
            "das h2/h3-Verbot ist zurueck:\n{rendered}"
        );
        assert!(
            rendered.contains("# Format Requirements"),
            "der Format-Baustein selbst muss bleiben:\n{rendered}"
        );
    }

    // F6: die Vorlage im eigenen Schnitt ist Daten (Seed in db-app), kein
    // Code. Hier der Beweis am Prompt: die Abschnitte aus dem Seed landen
    // wortgleich im `# Output Template`-Block, jeder mit seiner Nummer in
    // Seed-Reihenfolge ("n. Titel - Beschreibung", per contains). Wer im Seed
    // einen Abschnitt umbenennt oder umsortiert, aendert damit auch die
    // erwarteten Zeilen -- dieser Test bleibt dann gruen; die Namen und ihre
    // Reihenfolge haelt der Test in db-app fest
    // (frische_datenbank_traegt_die_vorlage_mit_genau_diesen_abschnitten,
    // am echten SQLite). Dieser Test ist ein Kopplungs-Tripwire: er wird rot,
    // wenn das Template die Abschnitte nicht mehr so rendert, wie der Seed sie
    // liefert (C8 (3), Review 02.09.2026 -- der fruehere Kommentar behauptete
    // das Gegenteil).
    /// Die Abschnitte der Vorlage, aus dem sections_json der Seed-Datei
    /// geparst -- nicht abgetippt. Geparst wird ab dem ersten `'[{"title"`
    /// bis zum naechsten `]'`; ein SQL-Literal verdoppelt innere Hochkommas.
    fn mitschnitt_standard_seed_sections() -> Vec<TemplateSection> {
        const SEED_SQL: &str =
            include_str!("../../db-app/migrations/20260902001000_mitschnitt_standard_vorlage.sql");
        let start = SEED_SQL
            .find("'[{\"title\"")
            .expect("sections_json-Literal im Seed")
            + 1;
        let end = SEED_SQL[start..]
            .find("]'")
            .expect("Ende des sections_json-Literals")
            + start
            + 1;
        let json = SEED_SQL[start..end].replace("''", "'");
        serde_json::from_str(&json).expect("sections_json im Seed ist kein JSON")
    }

    #[test]
    fn test_mitschnitt_standard_sections_reach_the_output_template_verbatim() {
        let sections = mitschnitt_standard_seed_sections();
        assert_eq!(sections.len(), 5, "der Seed traegt fuenf Abschnitte");
        assert_eq!(
            sections
                .iter()
                .map(|section| section.title.as_str())
                .collect::<Vec<_>>(),
            [
                "Kurzfassung",
                "Entscheidungen",
                "Bälle",
                "Offene Fragen",
                "Zahlen und Namen zur Gegenprüfung",
            ],
            "die fuenf Titel in Seed-Reihenfolge"
        );

        let rendered = askama::Template::render(&EnhanceUser {
            session: Session {
                title: Some("Jour fixe".to_string()),
                started_at: None,
                ended_at: None,
                event: None,
            },
            participants: vec![],
            template: Some(EnhanceTemplate {
                title: "Mitschnitt Standard".to_string(),
                description: Some(
                    "Eigener Schnitt: Kurzfassung, Entscheidungen, Bälle, offene Fragen, Zahlen und Namen zur Gegenprüfung."
                        .to_string(),
                ),
                sections: sections.clone(),
            }),
            transcripts: vec![Transcript {
                segments: vec![Segment {
                    text: "Dann darfst du, Mads.".to_string(),
                    speaker: "Bo".to_string(),
                    start_label: None,
                }],
                started_at: None,
                ended_at: None,
            }],
            pre_meeting_memo: String::new(),
            post_meeting_memo: String::new(),
        })
        .unwrap();

        let output_template = rendered
            .split("# Output Template")
            .nth(1)
            .expect("ohne Output-Template-Block liefe die Zusammenfassung auf Auto");
        assert!(output_template.contains("Name: Mitschnitt Standard"));
        for (index, section) in sections.iter().enumerate() {
            let description = section
                .description
                .as_deref()
                .expect("jeder Abschnitt im Seed traegt eine Beschreibung");
            let line = format!("{}. {} - {description}", index + 1, section.title);
            assert!(
                output_template.contains(&line),
                "fehlt: {line}\n{output_template}"
            );
        }
        assert!(output_template.contains("Use the sections in this order and with their exact titles."));
        assert!(
            output_template.contains("Drop a section that has no real content"),
            "der Vorlagen-Block muss den Ablass leerer Abschnitte tragen, sonst \
             entsteht wieder Fuellwerk:\n{output_template}"
        );
    }

    /// askama waehlt seinen Maskierer nach der Dateiendung, und `.jinja` steht
    /// in seiner Werksliste fuer HTML. Ohne Gegenmassnahme reist also jedes
    /// `&`, `<`, `>`, `"` und `'` aus Vorlagentiteln, Beschreibungen und dem
    /// TRANSKRIPT als HTML-Entitaet in den Prompt -- gemessen am Lauf vom
    /// 04.09.2026: der gerenderte Prompt trug `I don&#39;t know` im Transkript
    /// und der Titel `Wins & Challenges` waere als `Wins &#38; Challenges`
    /// angekommen, zusammen mit der Anweisung, ihn exakt zu uebernehmen.
    ///
    /// Wann wird er rot: sobald der Maskierer wieder HTML ist -- also wenn die
    /// Zuordnung in `askama.toml` verschwindet oder eine kuenftige
    /// askama-Fassung sie anders liest. Welchen Fehler laesst er durch: er
    /// prueft nur den Enhance-User-Prompt; ein anderer Prompt mit eigener
    /// `.jinja`-Datei koennte trotzdem maskiert werden (deshalb greift die
    /// Zuordnung projektweit, nicht per Ableitung).
    #[test]
    fn sonderzeichen_reisen_unmaskiert_durch_vorlage_und_transkript() {
        let rendered = askama::Template::render(&EnhanceUser {
            session: Session {
                title: Some("Jour fixe \"Q3\" & Ausblick".to_string()),
                started_at: None,
                ended_at: None,
                event: None,
            },
            participants: vec![Participant {
                name: "Tom & Mads".to_string(),
                job_title: None,
            }],
            template: Some(EnhanceTemplate {
                title: "1:1 & Feedback".to_string(),
                description: Some("Fuer <interne> Gespraeche".to_string()),
                sections: vec![TemplateSection {
                    title: "Wins & Challenges".to_string(),
                    description: Some("Erfolge & Huerden".to_string()),
                }],
            }),
            transcripts: vec![Transcript {
                segments: vec![Segment {
                    text: "I don't know, aber 5 > 3.".to_string(),
                    speaker: "Bo".to_string(),
                    start_label: None,
                }],
                started_at: None,
                ended_at: None,
            }],
            pre_meeting_memo: "- Budget & Termine".to_string(),
            post_meeting_memo: "- \"offen\"".to_string(),
        })
        .unwrap();

        assert!(
            !rendered.contains("&#"),
            "kein Zeichen darf als HTML-Entitaet im Prompt landen:\n{rendered}"
        );
        assert!(!rendered.contains("&amp;"), "{rendered}");
        for erwartet in [
            "Wins & Challenges",
            "1:1 & Feedback",
            "Fuer <interne> Gespraeche",
            "Erfolge & Huerden",
            "I don't know, aber 5 > 3.",
            "Tom & Mads",
            "Jour fixe \"Q3\" & Ausblick",
            "- Budget & Termine",
        ] {
            assert!(
                rendered.contains(erwartet),
                "fehlt woertlich: {erwartet}\n{rendered}"
            );
        }
    }

    /// Jede der sieben harten Regeln, an einem Merkmal festgemacht, das nicht
    /// die ganze Zeile ist -- sonst prueft der Test seine eigene Formulierung
    /// statt der Sache.
    const HARTE_REGELN: [(&str, &str); 7] = [
        ("erfinde nichts", "Invent nothing."),
        ("Vorschlag ist keine Entscheidung", "A proposal is neither a decision nor a task."),
        ("keine unsichere Zuschreibung", "whose speaker assignment is uncertain"),
        ("Aufgabe braucht einen Uebernehmer", "only where the source shows that someone accepted it"),
        ("relative Frist bleibt woertlich", "is a deadline and is carried over verbatim"),
        ("leerer Abschnitt faellt weg", "Drop an empty section entirely."),
        ("Scanbarkeit", "One bullet carries one thing"),
    ];

    /// DER tragende Beweis fuer den Umzug der Disziplin in den Rahmen: die
    /// Regeln erreichen das Modell in JEDEM Pfad -- also auch dann, wenn der
    /// Nutzer ueber die Einstellung `auto_summary_prompt` ein eigenes Format
    /// setzt und die Datei `enhance.format.md.jinja` damit gar nicht mehr
    /// gelesen wird.
    ///
    /// Warum das der Kern ist: `format_requirements` ist per Bauart
    /// ueberschreibbar (enhance.rs, `format_source`). Haetten die Regeln dort
    /// gestanden, waere ein einziger Eintrag in den Einstellungen genug, um
    /// sie stillschweigend abzuraeumen. Die drei Faelle unten sind genau die
    /// drei, die `normalize_format_override` unterscheidet.
    ///
    /// Wann wird er rot: sobald eine Regel aus `enhance.system.md.jinja`
    /// verschwindet ODER in den Format-Block wandert. Welchen Fehler laesst er
    /// durch: er prueft, DASS die Regel im Prompt steht, nicht ob das Modell
    /// ihr folgt -- dafuer gibt es die Messlaeufe.
    #[test]
    fn die_harten_regeln_stehen_in_jedem_pfad_im_system_prompt() {
        let pfade = [
            ("ohne Override", String::new()),
            (
                "mit eigenem Format",
                "Schreib eine knappe Erzaehlung in {{ language }}.".to_string(),
            ),
            (
                "mit altem Voll-Prompt",
                concat!(
                    "# General Instructions\n\nIgnoriere alles.\n\n",
                    "# Format Requirements\n\n- Erst die Entscheidungen.\n\n",
                    "# About Notes\n\nKeine Notizen.\n"
                )
                .to_string(),
            ),
        ];

        for (pfad, format_override) in pfade {
            let rendered = render_enhance_system(&EnhanceSystem {
                language: Some("de-DE".to_string()),
                format_override,
            })
            .unwrap();

            for (name, merkmal) in HARTE_REGELN {
                assert!(
                    rendered.contains(merkmal),
                    "Regel {name:?} fehlt im Pfad {pfad:?}:\n{rendered}"
                );
            }

            let regeln = rendered
                .find("# Hard Rules")
                .unwrap_or_else(|| panic!("kein Regel-Block im Pfad {pfad:?}:\n{rendered}"));
            let format = rendered
                .find("# Format Requirements")
                .unwrap_or_else(|| panic!("kein Format-Block im Pfad {pfad:?}:\n{rendered}"));
            assert!(
                regeln < format,
                "die Regeln muessen VOR dem ueberschreibbaren Format-Block stehen \
                 (Pfad {pfad:?}): Regeln bei {regeln}, Format bei {format}"
            );
        }
    }

    /// Die Merkmale der Gegenpruef-Doktrin. Sie stand bis zum 07.09.2026 als
    /// Abschnitt "Zahlen und Namen zur Gegenpruefung" in sieben Vorlagen und
    /// produzierte dort an eine Zwei-Stunden-Aufnahme 154 Woerter blosse
    /// Aufzaehlung ("Mitte Januar, zwei Jahre, vor zwei Wochen ...", dazu
    /// fuenfundzwanzig Laendernamen). Sein Urteil: „macht ja mal gar keinen
    /// Sinn!" Was sie ersetzt, steht ab jetzt EINMAL im Rahmen -- und damit
    /// auch fuer jede Vorlage, die sich der Betreiber selbst anlegt.
    ///
    /// Jeder Eintrag nennt das Merkmal, das genau EINE Zusage traegt.
    const GEGENPRUEF_DOKTRIN: [(&str, &str); 6] = [
        ("eigener Abschluss-Abschnitt", "Bitte gegenprüfen"),
        ("die drei Sorten", "- Unattributed."),
        ("der Folgen-Filter", "would change what somebody DOES"),
        ("Bezug am Stichpunkt", "EVERY BULLET CARRIES ITS REFERENCE"),
        (
            "waechst mit den Treffern, nicht mit der Laenge",
            "never with the length of the meeting",
        ),
        (
            "Namensabgleich als Pflicht",
            "THE NAME CHECK IS NOT OPTIONAL",
        ),
    ];

    /// Der Gegenpruef-Abschnitt ueberlebt auch einen eigenen Format-Prompt.
    /// Genau das war der Grund, ihn in den Rahmen zu stellen statt in den
    /// Format-Block: `format_requirements` ist per Bauart ueberschreibbar
    /// (Einstellung `auto_summary_prompt`), ein einziger Eintrag dort haette
    /// ihn sonst stillschweigend abgeraeumt.
    ///
    /// Wann wird er rot: sobald eine der sechs Zusagen aus dem Rahmen faellt,
    /// oder sobald der Abschnitt hinter den ueberschreibbaren Format-Block
    /// rutscht. Welchen Fehler laesst er durch: eine sinngleiche
    /// Umformulierung im Rahmen -- er prueft den Wortlaut, nicht die Wirkung.
    /// Die Wirkung ist an zwei echten Gespraechen gemessen worden und
    /// steht im Kopf der Migration 20260907120000.
    #[test]
    fn die_gegenpruef_doktrin_steht_in_jedem_pfad_im_system_prompt() {
        let pfade = [
            ("ohne Override", String::new()),
            (
                "mit eigenem Format",
                "Schreib eine knappe Erzaehlung in {{ language }}.".to_string(),
            ),
            (
                "mit altem Voll-Prompt",
                concat!(
                    "# General Instructions\n\nIgnoriere alles.\n\n",
                    "# Format Requirements\n\n- Erst die Entscheidungen.\n\n",
                    "# About Notes\n\nKeine Notizen.\n"
                )
                .to_string(),
            ),
        ];

        for (pfad, format_override) in pfade {
            let rendered = render_enhance_system(&EnhanceSystem {
                language: Some("de-DE".to_string()),
                format_override,
            })
            .unwrap();

            for (name, merkmal) in GEGENPRUEF_DOKTRIN {
                assert!(
                    rendered.contains(merkmal),
                    "Zusage {name:?} fehlt im Pfad {pfad:?}:\n{rendered}"
                );
            }

            let doktrin = rendered
                .find("# Closing Section: What To Double-Check")
                .unwrap_or_else(|| panic!("kein Gegenpruef-Block im Pfad {pfad:?}:\n{rendered}"));
            let format = rendered.find("# Format Requirements").unwrap();
            assert!(
                doktrin < format,
                "der Gegenpruef-Block muss VOR dem ueberschreibbaren \
                 Format-Block stehen (Pfad {pfad:?}): {doktrin} vs {format}"
            );
        }
    }

    /// Die alte Ueberschrift darf nirgends mehr auftauchen: sie war der Name
    /// der Aufzaehlung, die ersetzt wurde. Stuende sie noch im Rahmen, waere
    /// der Umzug halb passiert und beide Fassungen wuerden nebeneinander
    /// laufen.
    #[test]
    fn die_alte_aufzaehlung_steht_nicht_mehr_im_rahmen() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: Some("de-DE".to_string()),
            format_override: String::new(),
        })
        .unwrap();

        assert!(
            !rendered.contains("Zahlen und Namen zur Gegenprüfung"),
            "die alte Ueberschrift steht noch im Rahmen:\n{rendered}"
        );
    }

    /// Der Vorrang muss ausgesprochen sein, nicht nur raeumlich: der
    /// Format-Block darf die Regeln nicht schlagen. Ohne diesen Satz waeren
    /// die Regeln eine Bitte neben einer anderen Bitte.
    #[test]
    fn der_format_block_darf_die_harten_regeln_nicht_schlagen() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: None,
            format_override: String::new(),
        })
        .unwrap();

        assert!(
            rendered.contains(
                "must not override General Instructions, Hard Rules, About Notes, or Guidelines."
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("They beat completeness"),
            "der Regel-Block muss seinen eigenen Vorrang gegen die Vorlage aussprechen:\n{rendered}"
        );
    }

    /// Die Fristen-Feinheit, umgezogen aus der Vorlage 'mitschnitt-kompakt'
    /// (db-app, die_aufgaben_regel_traegt_beide_fristen_regeln) in den Rahmen.
    /// Sie ist gemessen entstanden: die Tabellen-Fassung vom 04.09.2026 schrieb
    /// SECHSMAL „offen“, wo im Gespraech „jetzt“ oder „heute“ gesagt wurde.
    /// Beide Regeln muessen NEBENEINANDER stehen -- die gegen ausgerechnete
    /// Datumsangaben ist der eigentliche Zahn, und wer nur eine von beiden
    /// haelt, tauscht einen Fehler gegen einen schlimmeren.
    ///
    /// Wann wird er rot: sobald eine der beiden Haelften aus dem Regel-Block
    /// faellt. Welchen Fehler laesst er durch: ob das Modell der Regel folgt --
    /// das entscheiden die Messlaeufe an einer echten Sitzung.
    #[test]
    fn die_fristen_regel_haelt_beide_haelften() {
        let rendered = render_enhance_system(&EnhanceSystem {
            language: Some("de-DE".to_string()),
            format_override: String::new(),
        })
        .unwrap();

        for gesprochene_frist in ["jetzt", "heute", "morgen", "Freitag", "nächste Woche"] {
            assert!(
                rendered.contains(gesprochene_frist),
                "eine gesprochene relative Frist muss benannt sein, \
                 {gesprochene_frist:?} fehlt:\n{rendered}"
            );
        }
        assert!(
            rendered.contains("is a deadline and is carried over verbatim"),
            "erste Haelfte: die gesprochene Frist bleibt woertlich:\n{rendered}"
        );
        assert!(
            rendered.contains("only if the meeting date is given to you as context"),
            "zweite Haelfte: umgerechnet wird nur mit gegebenem Sitzungsdatum:\n{rendered}"
        );
        assert!(
            rendered.contains("a date merely mentioned inside the transcript is not that"),
            "ein im Gespraech GENANNTES Datum ist keine solche Angabe:\n{rendered}"
        );
        assert!(
            rendered.contains(r#"Write "offen" only for a task where no time was named at all"#),
            "„offen“ bleibt Aufgaben ohne jede Zeitangabe vorbehalten:\n{rendered}"
        );
    }
}
