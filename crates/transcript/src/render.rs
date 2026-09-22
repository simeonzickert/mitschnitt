use std::collections::{HashMap, HashSet};

use crate::{
    FinalizedWord, IdentityAssignment, Segment, SegmentBuilderOptions, SegmentKey, SegmentWord,
    SpeakerLabelContext, SpeakerLabeler, WordState, build_segments,
    channel_assignments_for_participants, render_speaker_label, segment_options_for_participants,
    segments::{MAX_BRUECKE_MS, verschmelze_anzeige_nachbarn},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct RenderTranscriptWordInput {
    pub id: String,
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub channel: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_index: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct RenderTranscriptHuman {
    pub human_id: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct RenderTranscriptInput {
    pub started_at: Option<i64>,
    pub words: Vec<RenderTranscriptWordInput>,
    pub assignments: Vec<IdentityAssignment>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct RenderTranscriptRequest {
    pub transcripts: Vec<RenderTranscriptInput>,
    pub participant_human_ids: Vec<String>,
    pub self_human_id: Option<String>,
    pub humans: Vec<RenderTranscriptHuman>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct RenderedTranscriptSegment {
    pub id: String,
    pub key: SegmentKey,
    pub speaker_label: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub words: Vec<SegmentWord>,
}

pub fn render_transcript_segments(
    request: RenderTranscriptRequest,
) -> Vec<RenderedTranscriptSegment> {
    let RenderTranscriptRequest {
        transcripts,
        participant_human_ids,
        self_human_id,
        humans,
    } = request;

    let base_started_at = earliest_started_at(&transcripts);

    // EINE Quelle fuer die Mehrsprecher-Frage, ueber ALLE Transkript-Zeilen der
    // Sitzung -- berechnet bevor die Schleife die Zeilen einzeln verarbeitet.
    //
    // Fork (08.09.2026), zweiter Anlauf: die erste Fassung rechnete zweimal,
    // einmal je Zeile in `create_speaker_state` (fuer die Zuweisung) und einmal
    // ueber die fertigen Bloecke (fuer das Etikett). Beides war falsch. Die
    // Zeilen-Sicht ist zu eng: eine Sitzung aus zwei Zeilen mit je einem
    // Sprecher haette pro Zeile "ein Sprecher" gesehen und beide Zeilen dem
    // Selbst-Menschen zugeschlagen. Und die Block-Sicht ist zu spaet: die
    // Absatz-Phase verschmilzt Bloecke verschiedener Sprecher-Nummern, sobald
    // sie denselben Menschen tragen (`segments/collect.rs`,
    // `should_merge_adjacent_keys`), ein Sprecher kann dort also verschwinden.
    //
    // Am Wort gerechnet und einmal, gibt es die Frage nicht mehr zweimal.
    let multi_speaker_channels = multi_speaker_channels(&transcripts);
    let segment_options = SegmentBuilderOptions {
        multi_speaker_channels: Some(multi_speaker_channels.iter().copied().collect()),
        ..segment_options_for_participants(&participant_human_ids, self_human_id.as_deref())
    };

    let mut all_segments = Vec::new();

    for transcript in transcripts {
        let offset = transcript
            .started_at
            .map(|started_at| started_at - base_started_at)
            .unwrap_or(0);

        let (words, mut assignments) =
            offset_transcript_data(transcript.words, transcript.assignments, offset);
        let channel_assignments =
            channel_assignments_for_participants(&participant_human_ids, self_human_id.as_deref());
        assignments.extend(channel_assignments);

        let segments = build_segments(&words, &[], &assignments, Some(&segment_options));
        all_segments.extend(segments);
    }

    all_segments.sort_by_key(|seg| seg.words.first().map(|w| w.start_ms).unwrap_or(i64::MAX));

    // Was ohnehin nie auf dem Schirm landet, wird VOR der Absatz-Phase
    // aussortiert -- sonst trennt ein unsichtbarer Block zwei sichtbare
    // Nachbarn desselben Menschen, und genau der Zustand soll weg. Frueher
    // stand diese Pruefung erst unten im `filter_map`, also nach dem
    // Verschmelzen; hier ist ihre einzige Heimat.
    //
    // Gemessen am 03.09.2026 ueber den gemessenen Bestand (9 Transkripte, 28 458
    // Woerter): KEIN einziges Wort ist leer oder reiner Leerraum, und
    // `build_segments` verwirft Woerter nie nach Text. Der Pfad ist heute
    // also tot. Er ist trotzdem abgesichert, weil ein Erkenner oder der
    // Woerterbuch-Nachlauf jederzeit ein leeres Wort liefern kann und der
    // Schaden dann still im Transkript stuende.
    all_segments.retain(ist_sichtbar);

    // HIER entsteht die Reihenfolge, die der Nutzer sieht: die Oberflaeche
    // rendert `segments[index]` der Reihe nach und sortiert nicht nach
    // (`transcript.tsx`). Erst ab dieser Zeile ist "Nachbar in der Anzeige"
    // ueberhaupt definiert -- deshalb sitzt die Absatz-Phase hier und nicht
    // in `build_segments`. Sie muss VOR dem Labeler laufen, sonst zaehlt der
    // Bloecke, die es nicht mehr gibt.
    verschmelze_anzeige_nachbarn(&mut all_segments, MAX_BRUECKE_MS, Some(&segment_options));

    let ctx = SpeakerLabelContext {
        self_human_id: self_human_id.clone(),
        human_name_by_id: humans
            .into_iter()
            .map(|human| (human.human_id, human.name))
            .collect::<HashMap<_, _>>(),
        multi_speaker_channels: multi_speaker_channels.clone(),
    };
    // Fork (21.09.2026): hier stand ein Deckel auf die Teilnehmerzahl
    // (`max_speaker_number_for_participants`). Er legte ueberzaehlige
    // Stimmgruppen auf dasselbe Etikett und verschmolz damit in einer
    // 122,8-Minuten-Raumaufnahme mit vier Personen die beiden Hauptredner.
    // Begruendung des Ausbaus im Kopfkommentar von `SpeakerLabeler`.
    let mut labeler = SpeakerLabeler::from_segments(&all_segments, Some(&ctx));

    all_segments
        .into_iter()
        .filter_map(|segment| {
            let words = normalize_rendered_segment_words(segment.words);
            let first = words.first()?;
            let last = words.last()?;
            // Kein Leer-Text-Guard mehr: `retain(ist_sichtbar)` oben hat
            // ihn bereits erledigt. Zwei Stellen fuer dieselbe Sache waren
            // genau der Grund, warum die Absatz-Phase unsichtbare Nachbarn
            // sah.
            let text = words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<String>()
                .trim()
                .to_string();

            Some(RenderedTranscriptSegment {
                id: stable_segment_id(&segment.key, &words),
                speaker_label: render_speaker_label(&segment.key, Some(&ctx), Some(&mut labeler)),
                start_ms: first.start_ms,
                end_ms: last.end_ms,
                text,
                words,
                key: segment.key,
            })
        })
        .collect()
}

/// Landet dieser Block sichtbar auf dem Schirm?
///
/// Deckungsgleich mit der Bedingung, die frueher unten im `filter_map`
/// stand: der gerenderte Text ist die getrimmte Verkettung der Worttexte,
/// und `normalized_rendered_word_text` trimmt oder stellt ein Leerzeichen
/// voran -- aus Leerraum wird dort nie etwas anderes. Der Block ist also
/// genau dann sichtbar, wenn mindestens ein Wort etwas anderes als
/// Leerraum traegt.
fn ist_sichtbar(segment: &Segment) -> bool {
    segment
        .words
        .iter()
        .any(|word| !word.text.trim().is_empty())
}

/// Auf welchen Kanaelen hat die Trennung mehr als EINEN Sprecher gefunden?
///
/// Fork (08.09.2026). Ueber die rohen Eingabewoerter ALLER Transkript-Zeilen
/// gerechnet -- das ist die frueheste Stelle, an der die Sitzung vollstaendig
/// vorliegt, und die einzige, an der die Antwort weder zu eng (je Zeile) noch
/// zu spaet (nach dem Verschmelzen) ist.
///
/// Das Ergebnis speist beide Verbraucher: die Zuweisung ueber
/// `SegmentBuilderOptions::multi_speaker_channels` und das Etikett ueber
/// `SpeakerLabelContext::multi_speaker_channels`. Sie koennen nicht
/// auseinanderlaufen, weil es nur eine Zaehlung gibt.
fn multi_speaker_channels(transcripts: &[RenderTranscriptInput]) -> HashSet<crate::ChannelProfile> {
    let mut speakers_by_channel: HashMap<crate::ChannelProfile, HashSet<i32>> = HashMap::new();
    for transcript in transcripts {
        for word in &transcript.words {
            if let Some(speaker_index) = word.speaker_index {
                speakers_by_channel
                    .entry(crate::ChannelProfile::from(word.channel))
                    .or_default()
                    .insert(speaker_index);
            }
        }
    }

    speakers_by_channel
        .into_iter()
        .filter(|(_, speakers)| speakers.len() > 1)
        .map(|(channel, _)| channel)
        .collect()
}

fn offset_transcript_data(
    raw_words: Vec<RenderTranscriptWordInput>,
    assignments: Vec<IdentityAssignment>,
    time_offset: i64,
) -> (Vec<FinalizedWord>, Vec<IdentityAssignment>) {
    let words: Vec<FinalizedWord> = raw_words
        .into_iter()
        .map(|w| FinalizedWord {
            id: w.id,
            text: w.text,
            start_ms: w.start_ms + time_offset,
            end_ms: w.end_ms + time_offset,
            channel: w.channel,
            state: WordState::Final,
            speaker_index: w.speaker_index,
        })
        .collect();

    (words, assignments)
}

fn earliest_started_at(transcripts: &[RenderTranscriptInput]) -> i64 {
    transcripts
        .iter()
        .filter_map(|transcript| transcript.started_at)
        .min()
        .unwrap_or(0)
}

pub fn normalize_rendered_segment_words(words: Vec<SegmentWord>) -> Vec<SegmentWord> {
    words
        .into_iter()
        .enumerate()
        .map(|(index, mut word)| {
            word.text = normalized_rendered_word_text(&word.text, index == 0);
            word
        })
        .collect()
}

pub fn stable_segment_id(key: &SegmentKey, words: &[SegmentWord]) -> String {
    let first_anchor = words
        .first()
        .map(|word| {
            word.id
                .clone()
                .unwrap_or_else(|| format!("start:{}", word.start_ms))
        })
        .unwrap_or_else(|| "none".to_string());
    let last_anchor = words
        .last()
        .map(|word| {
            word.id
                .clone()
                .unwrap_or_else(|| format!("end:{}", word.end_ms))
        })
        .unwrap_or_else(|| "none".to_string());

    format!(
        "{}:{}:{}:{}:{}",
        key.channel as i32,
        key.speaker_index
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        key.speaker_human_id.as_deref().unwrap_or("none"),
        first_anchor,
        last_anchor
    )
}

fn normalized_rendered_word_text(text: &str, is_first_word: bool) -> String {
    let trimmed_start = text.trim_start();
    if trimmed_start.is_empty() {
        return text.to_string();
    }

    if is_first_word {
        return trimmed_start.to_string();
    }

    if text.starts_with(' ') {
        return text.to_string();
    }

    if trimmed_start.starts_with(|c: char| ",.;:!?)}]'".contains(c)) {
        return trimmed_start.to_string();
    }

    format!(" {trimmed_start}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChannelProfile, IdentityScope};

    fn word(
        id: &str,
        text: &str,
        start_ms: i64,
        end_ms: i64,
        channel: i32,
    ) -> RenderTranscriptWordInput {
        RenderTranscriptWordInput {
            id: id.to_string(),
            text: text.to_string(),
            start_ms,
            end_ms,
            channel,
            speaker_index: None,
        }
    }

    fn word_si(
        id: &str,
        text: &str,
        start_ms: i64,
        end_ms: i64,
        channel: i32,
        speaker_index: i32,
    ) -> RenderTranscriptWordInput {
        RenderTranscriptWordInput {
            id: id.to_string(),
            text: text.to_string(),
            start_ms,
            end_ms,
            channel,
            speaker_index: Some(speaker_index),
        }
    }

    fn channel_assignment(human_id: &str, channel: ChannelProfile) -> IdentityAssignment {
        IdentityAssignment {
            human_id: human_id.to_string(),
            scope: IdentityScope::Channel { channel },
        }
    }

    fn speaker_assignment(
        human_id: &str,
        channel: ChannelProfile,
        speaker_index: i32,
    ) -> IdentityAssignment {
        IdentityAssignment {
            human_id: human_id.to_string(),
            scope: IdentityScope::ChannelSpeaker {
                channel,
                speaker_index,
            },
        }
    }

    #[test]
    fn renders_segments_with_labels() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words: vec![
                    word("w1", " hello", 0, 100, 0),
                    word("w2", " world", 120, 240, 1),
                ],
                assignments: vec![],
            }],
            participant_human_ids: vec!["human-1".to_string(), "human-2".to_string()],
            self_human_id: Some("human-1".to_string()),
            humans: vec![
                RenderTranscriptHuman {
                    human_id: "human-1".to_string(),
                    name: "Alice".to_string(),
                },
                RenderTranscriptHuman {
                    human_id: "human-2".to_string(),
                    name: "Bob".to_string(),
                },
            ],
        });

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].speaker_label, "Alice");
        assert_eq!(segments[0].text, "hello");
        assert_eq!(segments[1].speaker_label, "Bob");
        assert_eq!(segments[1].text, "world");
    }

    /// Hiess vorher `caps_unknown_speaker_labels_to_participant_count` und
    /// schrieb ueber den ganzen Renderpfad fest, dass der dritte Sprecher bei
    /// zwei Teilnehmern dasselbe Etikett wie der zweite bekommt. Umgeschrieben
    /// am 21.09.2026 auf die Zusage, die jetzt gilt: die Anzeige legt nie zwei
    /// Sprecher-Schluessel auf ein Etikett. Ueberzaehlige Stimmgruppen sind ein
    /// Fall fuer die Trennung, nicht fuer den Renderer.
    #[test]
    fn never_merges_two_speaker_keys_onto_one_label() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words: vec![
                    word_si("w1", " one", 0, 100, 2, 0),
                    word_si("w2", " two", 200, 300, 2, 1),
                    word_si("w3", " three", 400, 500, 2, 2),
                ],
                assignments: vec![],
            }],
            participant_human_ids: vec!["self".to_string(), "remote".to_string()],
            self_human_id: Some("self".to_string()),
            humans: vec![],
        });

        assert_eq!(segments.len(), 3);
        let etiketten: Vec<&str> = segments
            .iter()
            .map(|segment| segment.speaker_label.as_str())
            .collect();
        assert_eq!(
            etiketten,
            vec!["Speaker 1", "Speaker 2", "Speaker 3"],
            "drei Schluessel bei zwei Teilnehmern: drei Etiketten, kein doppelt belegtes"
        );
        let verschiedene: HashSet<&str> = etiketten.iter().copied().collect();
        assert_eq!(verschiedene.len(), 3);
    }

    #[test]
    fn labels_diarized_direct_mic_as_self_without_remote_participant() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words: vec![word_si("w1", " hello", 0, 100, 0, 2)],
                assignments: vec![],
            }],
            participant_human_ids: vec![],
            self_human_id: Some("self".to_string()),
            humans: vec![RenderTranscriptHuman {
                human_id: "self".to_string(),
                name: "Me".to_string(),
            }],
        });

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].speaker_label, "Me");
        assert_eq!(segments[0].key.speaker_index, Some(2));
        assert_eq!(segments[0].key.speaker_human_id.as_deref(), Some("self"));
    }

    /// Der Kettentest zu diesem Fehler: EIN Kanal, VIER getrennte Sprecher.
    ///
    /// Nachgebaut nach eine Zwei-Stunden-Raumaufnahme vom 08.09.2026
    /// (Sitzung `d4e5f6a7`, gemessen in seiner Datenbank: 17 024 Woerter, alle
    /// auf `channel: 0`, Sprecher-Index 0..3 mit 746/5727/6888/3663 Woertern,
    /// vier Teilnehmer). Vor dem Fix trug JEDER Block seinen Namen, weil
    /// `channel_assignments_for_participants` den Selbst-Menschen
    /// bedingungslos auf DirectMic legte.
    ///
    /// Der Test greift ueber die ganze Strecke: Zuweisung
    /// (`create_speaker_state`/`apply_identity_rules`), Blockbildung
    /// (`build_segments`), Nummernvergabe (`SpeakerLabeler::from_segments`)
    /// und Etikett (`render_speaker_label`). Ein No-op an einer dieser vier
    /// Stellen bringt ihn zu Fall -- deshalb steht er hier und nicht als vier
    /// Einzeltests bei den jeweiligen Funktionen.
    #[test]
    fn vier_sprecher_auf_einem_kanal_bekommen_vier_verschiedene_etiketten() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words: vec![
                    word_si("w1", " eins", 0, 400, 0, 0),
                    word_si("w2", " zwei", 60_000, 60_400, 0, 1),
                    word_si("w3", " drei", 120_000, 120_400, 0, 2),
                    word_si("w4", " vier", 180_000, 180_400, 0, 3),
                ],
                assignments: vec![],
            }],
            participant_human_ids: vec![
                "self".to_string(),
                "gast-1".to_string(),
                "gast-2".to_string(),
                "gast-3".to_string(),
            ],
            self_human_id: Some("self".to_string()),
            humans: vec![RenderTranscriptHuman {
                human_id: "self".to_string(),
                name: "Mads Verlin".to_string(),
            }],
        });

        assert_eq!(segments.len(), 4, "jeder Sprecherwechsel trennt einen Block");

        let etiketten: Vec<&str> = segments
            .iter()
            .map(|segment| segment.speaker_label.as_str())
            .collect();

        // Der eigentliche Befund: kein Block darf den Namen des Selbst-Menschen
        // tragen, denn keiner dieser vier Sprecher ist ihm zugeordnet worden.
        assert!(
            !etiketten.contains(&"Mads Verlin"),
            "der Selbst-Mensch darf nicht auf vier getrennte Sprecher gelegt werden, war: {etiketten:?}"
        );

        // Und sie muessen unterscheidbar bleiben -- vier gleiche Nummern waeren
        // derselbe Schaden in anderer Schreibweise (genau das tat der
        // Teilnehmer-Deckel, solange die Teilnehmerliste kleiner war als die
        // Zahl der gefundenen Sprecher; am 21.09.2026 ausgebaut).
        let verschiedene: HashSet<&str> = etiketten.iter().copied().collect();
        assert_eq!(
            verschiedene.len(),
            4,
            "vier Sprecher, vier Etiketten: {etiketten:?}"
        );

        // Und kein Block darf still einem Menschen zugewiesen worden sein.
        for segment in &segments {
            assert_eq!(
                segment.key.speaker_human_id, None,
                "Block {:?} bekam eine Zuordnung, die niemand geprueft hat",
                segment.speaker_label
            );
        }
    }

    /// Die Mehrsprecher-Frage wird ueber die GANZE Sitzung beantwortet, nicht
    /// je Transkript-Zeile.
    ///
    /// Gefunden im Zweitblick zum ersten Fix (08.09.2026): `build_segments`
    /// laeuft je Zeile. Wer die Frage dort beantwortet, sieht bei einer
    /// zweizeiligen Sitzung (fortgesetzte Aufnahme) zweimal "ein Sprecher" und
    /// drueckt beiden Zeilen den Selbst-Menschen auf -- zwei verschiedene
    /// Menschen unter einem Namen, derselbe Fehler eine Ebene hoeher.
    ///
    /// Grenze, ausdruecklich: Sprecher-Nummern gelten nur INNERHALB eines
    /// Trennungslaufs. Traegt jede der beiden Zeilen die Nummer 0, sieht auch
    /// diese Zaehlung nur einen Sprecher, und der Fall bleibt offen. Das ist
    /// eine bekannte Luecke, kein Versehen -- sie zu schliessen hiesse, die
    /// Nummern je Zeile zu qualifizieren, und das aendert den `SegmentKey`.
    #[test]
    fn die_mehrsprecher_frage_gilt_ueber_alle_transkript_zeilen() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![
                RenderTranscriptInput {
                    started_at: Some(0),
                    words: vec![word_si("a1", " erste", 0, 400, 0, 0)],
                    assignments: vec![],
                },
                RenderTranscriptInput {
                    started_at: Some(600_000),
                    words: vec![word_si("b1", " zweite", 0, 400, 0, 1)],
                    assignments: vec![],
                },
            ],
            participant_human_ids: vec!["self".to_string(), "gast".to_string()],
            self_human_id: Some("self".to_string()),
            humans: vec![RenderTranscriptHuman {
                human_id: "self".to_string(),
                name: "Mads Verlin".to_string(),
            }],
        });

        assert_eq!(segments.len(), 2);
        for segment in &segments {
            assert_ne!(
                segment.speaker_label, "Mads Verlin",
                "eine zeilenweise Zaehlung haette hier zweimal den Selbst-Menschen gesetzt"
            );
            assert_eq!(segment.key.speaker_human_id, None);
        }
        assert_ne!(segments[0].speaker_label, segments[1].speaker_label);
    }

    /// Nicht-Regression, die andere Haelfte: EIN Sprecher auf dem Mikrofonkanal
    /// bleibt der Selbst-Mensch. Das ist der Normalfall (Kopfhoerer, ein
    /// Mensch je Kanal) und darf sich durch den Fix nicht bewegen.
    #[test]
    fn ein_einzelner_sprecher_auf_dem_mikrofonkanal_bleibt_der_selbst_mensch() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words: vec![
                    word_si("w1", " hallo", 0, 400, 0, 0),
                    word_si("w2", " welt", 60_000, 60_400, 0, 0),
                ],
                assignments: vec![],
            }],
            participant_human_ids: vec!["self".to_string(), "gast".to_string()],
            self_human_id: Some("self".to_string()),
            humans: vec![RenderTranscriptHuman {
                human_id: "self".to_string(),
                name: "Mads Verlin".to_string(),
            }],
        });

        assert!(!segments.is_empty());
        for segment in &segments {
            assert_eq!(segment.speaker_label, "Mads Verlin");
        }
    }

    /// Nicht-Regression fuer das Zwei-Kanal-Gespraech MIT Trennung auf der
    /// Gegenseite: Kanal 0 traegt keinen Sprecher-Index (die Trennung
    /// ueberspringt ihn baulich, siehe `soniqo_diarization_speaker_count`),
    /// Kanal 1 traegt zwei. Der Selbst-Mensch behaelt seinen Namen, die Gegenseite wird
    /// aufgetrennt statt zusammengefasst.
    #[test]
    fn getrennte_gegenseite_laesst_den_mikrofonkanal_unberuehrt() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words: vec![
                    word("w1", " hallo", 0, 400, 0),
                    word_si("w2", " ja", 60_000, 60_400, 1, 0),
                    word_si("w3", " nein", 120_000, 120_400, 1, 1),
                ],
                assignments: vec![],
            }],
            participant_human_ids: vec![
                "self".to_string(),
                "gast-1".to_string(),
                "gast-2".to_string(),
            ],
            self_human_id: Some("self".to_string()),
            humans: vec![RenderTranscriptHuman {
                human_id: "self".to_string(),
                name: "Mads Verlin".to_string(),
            }],
        });

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].speaker_label, "Mads Verlin");
        assert_ne!(segments[1].speaker_label, "Mads Verlin");
        assert_ne!(segments[2].speaker_label, "Mads Verlin");
        assert_ne!(
            segments[1].speaker_label, segments[2].speaker_label,
            "zwei getrennte Sprecher der Gegenseite bleiben unterscheidbar"
        );
    }

    #[test]
    fn normalizes_word_spacing_for_rendered_segments() {
        let words = normalize_rendered_segment_words(vec![
            SegmentWord {
                text: "What".to_string(),
                start_ms: 0,
                end_ms: 100,
                channel: crate::ChannelProfile::DirectMic,
                is_final: true,
                id: Some("w1".to_string()),
            },
            SegmentWord {
                text: "do".to_string(),
                start_ms: 100,
                end_ms: 200,
                channel: crate::ChannelProfile::DirectMic,
                is_final: true,
                id: Some("w2".to_string()),
            },
            SegmentWord {
                text: "'s".to_string(),
                start_ms: 200,
                end_ms: 250,
                channel: crate::ChannelProfile::DirectMic,
                is_final: true,
                id: Some("w3".to_string()),
            },
        ]);

        assert_eq!(words[0].text, "What");
        assert_eq!(words[1].text, " do");
        assert_eq!(words[2].text, "'s");
    }

    #[test]
    fn propagates_remote_labels_when_complete_channel_is_requested() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words: vec![
                    word("w1", " remote", 0, 100, 1),
                    word("w2", " reply", 120, 220, 1),
                ],
                assignments: vec![channel_assignment("remote", ChannelProfile::RemoteParty)],
            }],
            participant_human_ids: vec!["self".to_string(), "remote".to_string()],
            self_human_id: None,
            humans: vec![RenderTranscriptHuman {
                human_id: "remote".to_string(),
                name: "Remote".to_string(),
            }],
        });

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].speaker_label, "Remote");
        assert_eq!(segments[0].text, "remote reply");
    }

    #[test]
    fn keeps_same_provider_speaker_index_isolated_per_channel() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words: vec![
                    word_si("w1", " john", 0, 100, 0, 0),
                    word_si("w1b", " says", 120, 220, 0, 0),
                    word_si("w1c", " hi", 240, 340, 0, 0),
                    word_si("w2", " janet", 500, 600, 1, 0),
                    word_si("w2b", " replies", 620, 720, 1, 0),
                    word_si("w2c", " back", 740, 840, 1, 0),
                    word_si("w3", " again", 1_000, 1_100, 0, 0),
                ],
                assignments: vec![
                    speaker_assignment("john", ChannelProfile::DirectMic, 0),
                    speaker_assignment("janet", ChannelProfile::RemoteParty, 0),
                ],
            }],
            participant_human_ids: vec![],
            self_human_id: None,
            humans: vec![
                RenderTranscriptHuman {
                    human_id: "john".to_string(),
                    name: "John".to_string(),
                },
                RenderTranscriptHuman {
                    human_id: "janet".to_string(),
                    name: "Janet".to_string(),
                },
            ],
        });

        // Umgeschrieben: der Punkt bleibt, dass Sprecher-Nummer 0 auf beiden
        // Kanaelen NICHT derselbe Mensch ist. Johns vier Woerter liegen seit
        // der kanalbewussten Block-Bildung in einem Block statt in zweien.
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].speaker_label, "John");
        assert_eq!(segments[0].text, "john says hi again");
        assert_eq!(segments[1].speaker_label, "Janet");
        assert_eq!(segments[1].text, "janet replies back");
    }

    #[test]
    fn normalizes_multi_row_offsets_from_earliest_transcript() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![
                RenderTranscriptInput {
                    started_at: Some(5_000),
                    words: vec![word("late", " later", 100, 200, 1)],
                    assignments: vec![],
                },
                RenderTranscriptInput {
                    started_at: Some(1_000),
                    words: vec![word("early", " hello", 0, 100, 0)],
                    assignments: vec![],
                },
            ],
            participant_human_ids: vec!["self".to_string(), "remote".to_string()],
            self_human_id: Some("self".to_string()),
            humans: vec![RenderTranscriptHuman {
                human_id: "self".to_string(),
                name: "Me".to_string(),
            }],
        });

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "hello");
        assert_eq!(segments[0].start_ms, 0);
        assert_eq!(segments[1].text, "later");
        assert_eq!(segments[1].start_ms, 4_100);
    }

    #[test]
    fn propagates_remote_labels_when_self_not_in_participant_list() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words: vec![
                    word("w1", " hello", 0, 100, 0),
                    word("w2", " remote", 120, 220, 1),
                    word("w3", " more", 240, 340, 1),
                ],
                assignments: vec![channel_assignment("remote", ChannelProfile::RemoteParty)],
            }],
            participant_human_ids: vec!["remote".to_string()],
            self_human_id: Some("self".to_string()),
            humans: vec![
                RenderTranscriptHuman {
                    human_id: "self".to_string(),
                    name: "Me".to_string(),
                },
                RenderTranscriptHuman {
                    human_id: "remote".to_string(),
                    name: "Remote".to_string(),
                },
            ],
        });

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].speaker_label, "Me");
        assert_eq!(segments[1].speaker_label, "Remote");
        assert_eq!(segments[1].text, "remote more");
    }

    /// `render_transcript_segments` arbeitet mit `MAX_BRUECKE_MS`, nicht mit
    /// einer daneben stehenden Zahl.
    ///
    /// `die_bruecken_grenze_steht_fest` nagelt die Konstante fest, sagt aber
    /// nichts darueber, ob sie hier auch ankommt: wuerde der Aufruf
    /// versehentlich 30 000 direkt uebergeben, bliebe jener Test gruen.
    /// Dieser Test faehrt deshalb eine Luecke, die GENAU zwischen dem alten
    /// und dem heutigen Wert liegt -- 35 s werden verschmolzen, solange die
    /// Konstante durchgereicht wird, und getrennt, sobald irgendwo ein
    /// kleinerer Wert steht.
    #[test]
    fn der_render_pfad_reicht_die_bruecken_grenze_durch() {
        let strecke = |luecke_ms: i64| {
            let mut words = Vec::new();
            for i in 0..4 {
                let start = i * 600;
                words.push(word(&format!("a{i}"), " wort", start, start + 500, 0));
            }
            let basis = 1_800 + luecke_ms;
            for i in 0..4 {
                let start = basis + i * 600;
                words.push(word(&format!("b{i}"), " wort", start, start + 500, 0));
            }

            render_transcript_segments(RenderTranscriptRequest {
                transcripts: vec![RenderTranscriptInput {
                    started_at: Some(0),
                    words,
                    assignments: vec![],
                }],
                participant_human_ids: vec!["self".to_string(), "remote".to_string()],
                self_human_id: Some("self".to_string()),
                humans: vec![RenderTranscriptHuman {
                    human_id: "self".to_string(),
                    name: "Me".to_string(),
                }],
            })
            .len()
        };

        assert_eq!(
            strecke(35_000),
            1,
            "35 s liegen unter MAX_BRUECKE_MS (40 s) -- hier muss verschmolzen werden; \
             wird getrennt, reicht der Render-Pfad einen kleineren Wert durch"
        );
        assert_eq!(
            strecke(45_000),
            2,
            "45 s liegen darueber -- hier muss getrennt werden"
        );
    }

    /// Ein Block, der nie auf dem Schirm landet, darf zwei sichtbare
    /// Nachbarn desselben Menschen nicht trennen.
    ///
    /// Gefunden von zwei unabhaengigen Pruefern: die Absatz-Phase lief vor
    /// dem Aussortieren leerer Bloecke, also verhinderte ein unsichtbarer
    /// Zwischenblock das Verschmelzen und verschwand danach -- uebrig
    /// blieben zwei Absaetze desselben Menschen unmittelbar untereinander,
    /// genau der Zustand, den F57 ausschliesst.
    ///
    /// Auf echtem Material ist der Pfad heute tot (28 458 Woerter aus
    /// im gemessenen Bestand, kein einziges leer). Der Test faehrt ihn deshalb mit
    /// gesetzten Leerraum-Woertern an. Ohne den Sichtbarkeits-Filter liefert
    /// er drei Bloecke statt einem -- am 03.09.2026 nachgemessen.
    #[test]
    fn ein_unsichtbarer_block_trennt_keine_sichtbaren_nachbarn() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words: vec![
                    word("a1", " hallo", 0, 500, 0),
                    word("a2", " welt", 600, 1_100, 0),
                    word("a3", " noch", 1_200, 1_700, 0),
                    // Unsichtbar: reiner Leerraum, auf dem Gegenkanal.
                    word("leer1", "   ", 5_000, 5_400, 1),
                    word("leer2", " ", 5_500, 6_900, 1),
                    word("b1", " weiter", 8_000, 8_500, 0),
                    word("b2", " gehts", 8_600, 9_100, 0),
                    word("b3", " hier", 9_200, 9_700, 0),
                ],
                assignments: vec![],
            }],
            participant_human_ids: vec!["self".to_string(), "remote".to_string()],
            self_human_id: Some("self".to_string()),
            humans: vec![
                RenderTranscriptHuman {
                    human_id: "self".to_string(),
                    name: "Me".to_string(),
                },
                RenderTranscriptHuman {
                    human_id: "remote".to_string(),
                    name: "Remote".to_string(),
                },
            ],
        });

        assert_eq!(
            segments.len(),
            1,
            "der unsichtbare Block darf die beiden Absaetze nicht trennen"
        );
        assert_eq!(segments[0].text, "hallo welt noch weiter gehts hier");
        assert!(
            segments
                .iter()
                .all(|s| s.words.iter().any(|w| !w.text.trim().is_empty())),
            "kein unsichtbarer Block darf uebrig bleiben"
        );
    }

    /// Zwei Transkript-Zeilen werden erst NACH der Block-Bildung gemischt.
    /// Ein langer Block der einen Zeile kann dabei vor einem kurzen der
    /// anderen einsortiert werden und trotzdem spaeter enden -- verschmelzen
    /// wuerde die Woerter verschraenken.
    ///
    /// Dieser Zustand ist auf dem echten Aufrufpfad erreichbar, ohne dass
    /// irgendetwas mutiert werden muesste: `build_segments` laeuft je Zeile,
    /// `all_segments.sort_by_key` sortiert danach nach dem ERSTEN Wort.
    #[test]
    fn zwei_transkript_zeilen_verschraenken_die_woerter_nicht() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![
                RenderTranscriptInput {
                    started_at: Some(0),
                    words: vec![
                        word("a1", " eins", 0, 80, 0),
                        word("a2", " zwei", 100, 180, 0),
                        word("a3", " drei", 200, 280, 0),
                    ],
                    assignments: vec![],
                },
                RenderTranscriptInput {
                    started_at: Some(0),
                    words: vec![word("b1", " dazwischen", 100, 180, 0)],
                    assignments: vec![],
                },
            ],
            participant_human_ids: vec!["self".to_string(), "remote".to_string()],
            self_human_id: Some("self".to_string()),
            humans: vec![RenderTranscriptHuman {
                human_id: "self".to_string(),
                name: "Me".to_string(),
            }],
        });

        for segment in &segments {
            for paar in segment.words.windows(2) {
                assert!(
                    paar[0].start_ms <= paar[1].start_ms,
                    "Woerter stehen ausser der Reihe: {} vor {}",
                    paar[0].start_ms,
                    paar[1].start_ms
                );
            }
        }

        assert_eq!(
            segments.len(),
            2,
            "der spaeter beginnende Block darf nicht in den frueheren gezogen werden"
        );
        let woerter: usize = segments.iter().map(|s| s.words.len()).sum();
        assert_eq!(woerter, 4, "kein Wort geht verloren");
    }

    /// Zwei Bloecke desselben Menschen, die in der Anzeige direkt
    /// untereinander stehen, kommen als EIN Absatz heraus -- das ist der
    /// Fall, der als zerhacktes Transkript gemeldet wurde.
    ///
    /// Der lange Beitrag der Gegenseite beginnt frueh und dauert lange; er
    /// wird deshalb VOR beide Einwuerfe sortiert, und die Einwuerfe landen
    /// unmittelbar untereinander.
    #[test]
    fn zwei_einwuerfe_um_einen_langen_beitrag_herum_werden_ein_absatz() {
        // Die Gegenseite beginnt frueher als beide Einwuerfe und redet bis
        // 20 000 ms durch -- deshalb wird ihr Block VOR beide sortiert.
        let mut words = Vec::new();
        for i in 0..40 {
            let start = i * 500;
            words.push(word(&format!("remote-{i}"), " lang", start, start + 400, 1));
        }
        // Je vier Woerter ueber gut 2 s. Die Groesse stammt noch aus der
        // Zeit der Mikro-Konsolidierung, die kurze Bloecke selbst
        // eingesammelt und den Test damit auch ohne die Absatz-Phase gruen
        // gemacht haette; sie ist am 03.09.2026 gestrichen worden. Die
        // Masse bleiben, weil der Test mit ihnen misst, was er soll --
        // nachgemessen: ohne die Absatz-Phase liefert er drei Bloecke statt
        // zwei.
        for (marke, basis) in [("self-a", 5_000i64), ("self-b", 15_000)] {
            for i in 0..4 {
                let start = basis + i * 600;
                words.push(word(
                    &format!("{marke}-{i}"),
                    " wort",
                    start,
                    start + 500,
                    0,
                ));
            }
        }

        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![RenderTranscriptInput {
                started_at: Some(0),
                words,
                assignments: vec![],
            }],
            participant_human_ids: vec!["self".to_string(), "remote".to_string()],
            self_human_id: Some("self".to_string()),
            humans: vec![
                RenderTranscriptHuman {
                    human_id: "self".to_string(),
                    name: "Me".to_string(),
                },
                RenderTranscriptHuman {
                    human_id: "remote".to_string(),
                    name: "Remote".to_string(),
                },
            ],
        });

        assert_eq!(
            segments.len(),
            2,
            "ein Absatz je Mensch, nicht drei Bloecke"
        );
        assert_eq!(segments[0].speaker_label, "Remote");
        assert_eq!(segments[1].speaker_label, "Me");
        assert_eq!(
            segments[1].words.len(),
            8,
            "beide Einwuerfe in einem Absatz"
        );
    }

    #[test]
    fn keeps_missing_started_at_rows_anchored_at_zero() {
        let segments = render_transcript_segments(RenderTranscriptRequest {
            transcripts: vec![
                RenderTranscriptInput {
                    started_at: None,
                    words: vec![word("missing-start", " hello", 0, 100, 0)],
                    assignments: vec![],
                },
                RenderTranscriptInput {
                    started_at: Some(1_000),
                    words: vec![word("known-start", " later", 100, 200, 1)],
                    assignments: vec![],
                },
            ],
            participant_human_ids: vec!["self".to_string(), "remote".to_string()],
            self_human_id: Some("self".to_string()),
            humans: vec![RenderTranscriptHuman {
                human_id: "self".to_string(),
                name: "Me".to_string(),
            }],
        });

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "hello");
        assert_eq!(segments[0].start_ms, 0);
        assert_eq!(segments[1].text, "later");
        assert_eq!(segments[1].start_ms, 100);
    }
}
