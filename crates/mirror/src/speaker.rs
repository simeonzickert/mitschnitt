//! Turns stored transcript rows into the exact request the app renders from.
//!
//! The segmentation and the speaker labels themselves are NOT reimplemented
//! here: `anlg_transcript::render_transcript_segments` is the same function the
//! desktop calls through `renderTranscriptSegments`, so the mirror's transcript
//! reads the way the app shows it.
//!
//! What *is* reimplemented is the row-to-request conversion, which upstream
//! only has in TypeScript (`apps/desktop/src/stt/render-transcript.ts`). The
//! rules below are ported from that file one to one; when it changes, this has
//! to follow.

use serde_json::Value;

use anlg_db_app::SessionTranscriptRow;
use anlg_transcript::{
    ChannelProfile, IdentityAssignment, IdentityScope, RenderTranscriptHuman,
    RenderTranscriptInput, RenderTranscriptRequest, RenderTranscriptWordInput,
};

pub fn build_request(
    transcripts: &[SessionTranscriptRow],
    humans: Vec<RenderTranscriptHuman>,
    participant_human_ids: Vec<String>,
    self_human_id: Option<String>,
) -> Option<RenderTranscriptRequest> {
    let mut inputs = Vec::new();

    for transcript in transcripts {
        let words_json = parse_array(&transcript.words_json);
        let hints = parse_array(&transcript.speaker_hints_json);

        let mut words: Vec<RenderTranscriptWordInput> = Vec::new();
        for word in &words_json {
            let (Some(id), Some(text), Some(start_ms), Some(end_ms)) = (
                word.get("id").and_then(Value::as_str),
                word.get("text").and_then(Value::as_str),
                word.get("start_ms").and_then(Value::as_i64),
                word.get("end_ms").and_then(Value::as_i64),
            ) else {
                continue;
            };
            words.push(RenderTranscriptWordInput {
                id: id.to_string(),
                text: text.to_string(),
                start_ms,
                end_ms,
                channel: word
                    .get("channel")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .try_into()
                    .unwrap_or(0),
                speaker_index: None,
            });
        }

        if words.is_empty() {
            continue;
        }

        // Order matters and mirrors the TypeScript: provider indices are folded
        // into the words first, then automatic assignments, then user ones, so
        // a manual correction is the last word.
        let mut assignments = Vec::new();
        for hint in &hints {
            if hint_type(hint) == Some("provider_speaker_index") {
                apply_provider_index(hint, &mut words);
            }
        }
        for kind in ["automatic_speaker_assignment", "user_speaker_assignment"] {
            for hint in &hints {
                if hint_type(hint) == Some(kind)
                    && let Some(assignment) = speaker_assignment(hint, &words)
                {
                    assignments.push(assignment);
                }
            }
        }

        inputs.push(RenderTranscriptInput {
            started_at: Some(transcript.started_at_ms),
            words,
            assignments,
        });
    }

    if inputs.is_empty() {
        return None;
    }

    Some(RenderTranscriptRequest {
        transcripts: inputs,
        participant_human_ids,
        self_human_id,
        humans,
    })
}

fn parse_array(raw: &str) -> Vec<Value> {
    serde_json::from_str::<Vec<Value>>(raw).unwrap_or_default()
}

fn hint_type(hint: &Value) -> Option<&str> {
    hint.get("type").and_then(Value::as_str)
}

/// Hint values are stored either as a nested object or as a JSON string; the
/// app tolerates both, so the mirror has to as well.
fn hint_value(hint: &Value) -> Option<Value> {
    match hint.get("value") {
        Some(Value::String(raw)) => serde_json::from_str(raw).ok(),
        Some(value) => Some(value.clone()),
        None => None,
    }
}

fn apply_provider_index(hint: &Value, words: &mut [RenderTranscriptWordInput]) {
    let (Some(word_id), Some(value)) = (
        hint.get("word_id").and_then(Value::as_str),
        hint_value(hint),
    ) else {
        return;
    };
    let Some(speaker_index) = value.get("speaker_index").and_then(Value::as_i64) else {
        return;
    };
    let Some(word) = words.iter_mut().find(|word| word.id == word_id) else {
        return;
    };

    word.speaker_index = speaker_index.try_into().ok();
    if let Some(channel) = value.get("channel").and_then(Value::as_i64)
        && let Ok(channel) = channel.try_into()
    {
        word.channel = channel;
    }
}

fn speaker_assignment(
    hint: &Value,
    words: &[RenderTranscriptWordInput],
) -> Option<IdentityAssignment> {
    let value = hint_value(hint)?;
    let human_id = value.get("human_id").and_then(Value::as_str)?.to_string();

    if let Some(scope) = explicit_speaker_scope(&value) {
        return Some(IdentityAssignment { human_id, scope });
    }

    if value.get("scope").and_then(Value::as_str) == Some("segment")
        && let Some(word_ids) = value.get("word_ids").and_then(Value::as_array)
    {
        let word_ids: Vec<String> = word_ids
            .iter()
            .filter_map(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect();
        if !word_ids.is_empty() {
            return Some(IdentityAssignment {
                human_id,
                scope: IdentityScope::Words { word_ids },
            });
        }
    }

    let word_id = hint.get("word_id").and_then(Value::as_str)?;
    let word = words.iter().find(|word| word.id == word_id)?;
    let channel = ChannelProfile::from(word.channel);

    Some(IdentityAssignment {
        human_id,
        scope: match word.speaker_index {
            None => IdentityScope::Channel { channel },
            Some(speaker_index) => IdentityScope::ChannelSpeaker {
                channel,
                speaker_index,
            },
        },
    })
}

fn explicit_speaker_scope(value: &Value) -> Option<IdentityScope> {
    if value.get("scope").and_then(Value::as_str) != Some("speaker") {
        return None;
    }
    let channel = value.get("channel").and_then(Value::as_i64)?;
    if !(0..=2).contains(&channel) {
        return None;
    }
    let channel = ChannelProfile::from(i32::try_from(channel).ok()?);

    match value.get("speaker_index") {
        Some(Value::Null) | None => Some(IdentityScope::Channel { channel }),
        Some(index) => index
            .as_i64()
            .and_then(|index| i32::try_from(index).ok())
            .map(|speaker_index| IdentityScope::ChannelSpeaker {
                channel,
                speaker_index,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transcript(words_json: &str, hints_json: &str) -> SessionTranscriptRow {
        SessionTranscriptRow {
            id: "t1".into(),
            workspace_id: String::new(),
            owner_user_id: "me".into(),
            session_id: "s1".into(),
            source: "batch_transcription".into(),
            provider: "soniqo".into(),
            model: "soniqo-parakeet-batch".into(),
            language: String::new(),
            started_at_ms: 0,
            ended_at_ms: None,
            audio_attachment_id: String::new(),
            memo: String::new(),
            words_json: words_json.into(),
            speaker_hints_json: hints_json.into(),
            metadata_json: "{}".into(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    const TWO_WORDS: &str = r#"[
        {"id":"w1","text":"Hallo","start_ms":0,"end_ms":400,"channel":0},
        {"id":"w2","text":"Moin","start_ms":800,"end_ms":1200,"channel":1}
    ]"#;

    /// Messbank auf ECHTEM Bestand -- laeuft nur auf ausdruecklichen Aufruf.
    ///
    /// Fuehrt eine gespeicherte Transkript-Zeile durch genau die Kette, die
    /// auch die App faehrt: `build_request` (die Portierung von
    /// `render-transcript.ts`) und danach
    /// `anlg_transcript::render_transcript_segments`. Gibt aus, wie viele
    /// Bloecke welches Etikett tragen.
    ///
    /// Warum `#[ignore]` und Umgebungsvariablen statt eines gewoehnlichen
    /// Tests: die Eingabe sind echte Gespraechsdaten mit echten Namen, die
    /// nicht ins Repo gehoeren (ZICK-252). Und ein Test, der ohne seine Daten
    /// still gruen durchlaeuft, waere ein Erfolgsmelder ohne Deckung.
    ///
    /// Aufruf:
    /// ```text
    /// MITSCHNITT_PROBE_WORDS=<pfad> MITSCHNITT_PROBE_HINTS=<pfad> \
    ///   MITSCHNITT_PROBE_SELF=<human-id> MITSCHNITT_PROBE_PARTS=<id,id,...> \
    ///   cargo test -p mirror -- --ignored --nocapture etiketten_probe
    /// ```
    #[test]
    #[ignore = "braucht echte Bestandsdaten, siehe Doku am Test"]
    fn etiketten_probe_auf_echtem_bestand() {
        let words = std::fs::read_to_string(
            std::env::var("MITSCHNITT_PROBE_WORDS").expect("MITSCHNITT_PROBE_WORDS"),
        )
        .expect("words lesbar");
        let hints = std::fs::read_to_string(
            std::env::var("MITSCHNITT_PROBE_HINTS").expect("MITSCHNITT_PROBE_HINTS"),
        )
        .expect("hints lesbar");
        let self_id = std::env::var("MITSCHNITT_PROBE_SELF").ok().filter(|s| !s.is_empty());
        let parts: Vec<String> = std::env::var("MITSCHNITT_PROBE_PARTS")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Namen bewusst NICHT aus der Datenbank: das Etikett soll die
        // Kennung zeigen, nicht den Menschen. Wer hier einen Namen sieht,
        // sieht eine Zuordnung -- genau die Frage, um die es geht.
        let humans = Vec::new();

        let request = build_request(
            &[transcript(words.trim(), hints.trim())],
            humans,
            parts,
            self_id.clone(),
        )
        .expect("request");

        let woerter = request.transcripts[0].words.len();
        let mit_sprecher = request.transcripts[0]
            .words
            .iter()
            .filter(|w| w.speaker_index.is_some())
            .count();

        let segments = anlg_transcript::render_transcript_segments(request);

        let mut je_etikett: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for segment in &segments {
            *je_etikett.entry(segment.speaker_label.clone()).or_default() += 1;
        }

        println!("Woerter: {woerter}, davon mit Sprecher-Index: {mit_sprecher}");
        println!("Bloecke: {}", segments.len());
        println!("Verschiedene Etiketten: {}", je_etikett.len());
        for (etikett, anzahl) in &je_etikett {
            println!("  {etikett}: {anzahl} Bloecke");
        }
        if let Some(self_id) = self_id.as_ref() {
            println!(
                "Bloecke, die dem Selbst-Menschen zugeordnet sind: {}",
                segments
                    .iter()
                    .filter(|s| s.key.speaker_human_id.as_deref() == Some(self_id.as_str()))
                    .count()
            );
        }
    }

    #[test]
    fn words_and_channels_survive_the_conversion() {
        let request =
            build_request(&[transcript(TWO_WORDS, "[]")], vec![], vec![], None).expect("request");
        let words = &request.transcripts[0].words;
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].channel, 0);
        assert_eq!(words[1].channel, 1);
    }

    #[test]
    fn a_transcript_without_usable_words_is_dropped() {
        assert!(build_request(&[transcript("[]", "[]")], vec![], vec![], None).is_none());
        // Missing timestamps are the realistic broken case: the row exists, the
        // words are unusable, and the app skips them.
        let partial = r#"[{"id":"w1","text":"Hallo"}]"#;
        assert!(build_request(&[transcript(partial, "[]")], vec![], vec![], None).is_none());
    }

    #[test]
    fn a_hint_value_stored_as_a_json_string_is_parsed_like_an_object() {
        let as_string = r#"[{"word_id":"w2","type":"user_speaker_assignment","value":"{\"human_id\":\"ilja\"}"}]"#;
        let as_object =
            r#"[{"word_id":"w2","type":"user_speaker_assignment","value":{"human_id":"ilja"}}]"#;

        let from_string = build_request(&[transcript(TWO_WORDS, as_string)], vec![], vec![], None)
            .expect("request");
        let from_object = build_request(&[transcript(TWO_WORDS, as_object)], vec![], vec![], None)
            .expect("request");

        assert_eq!(
            from_string.transcripts[0].assignments,
            from_object.transcripts[0].assignments
        );
        assert_eq!(from_string.transcripts[0].assignments.len(), 1);
    }

    #[test]
    fn a_word_scoped_assignment_keeps_its_word_ids() {
        let hints = r#"[{"word_id":"w1","type":"user_speaker_assignment","value":{"human_id":"ilja","scope":"segment","word_ids":["w1","w2"]}}]"#;
        let request =
            build_request(&[transcript(TWO_WORDS, hints)], vec![], vec![], None).expect("request");

        assert_eq!(
            request.transcripts[0].assignments[0].scope,
            IdentityScope::Words {
                word_ids: vec!["w1".into(), "w2".into()]
            }
        );
    }

    // A provider index has to reach the word before the assignment is read, or
    // the assignment degrades from channel_speaker to channel scope.
    #[test]
    fn a_provider_index_narrows_the_assignment_scope() {
        let hints = r#"[
            {"word_id":"w2","type":"provider_speaker_index","value":{"speaker_index":3,"channel":1}},
            {"word_id":"w2","type":"user_speaker_assignment","value":{"human_id":"ilja"}}
        ]"#;
        let request =
            build_request(&[transcript(TWO_WORDS, hints)], vec![], vec![], None).expect("request");

        assert_eq!(
            request.transcripts[0].assignments[0].scope,
            IdentityScope::ChannelSpeaker {
                channel: ChannelProfile::RemoteParty,
                speaker_index: 3,
            }
        );
    }

    #[test]
    fn an_unrelated_hint_type_is_ignored() {
        let hints = r#"[{"word_id":"w1","type":"something_else","value":{"human_id":"ilja"}}]"#;
        let request =
            build_request(&[transcript(TWO_WORDS, hints)], vec![], vec![], None).expect("request");
        assert!(request.transcripts[0].assignments.is_empty());
    }

    #[test]
    fn broken_json_does_not_take_the_mirror_down() {
        assert!(
            build_request(
                &[transcript("{not json", "{not json")],
                vec![],
                vec![],
                None
            )
            .is_none()
        );
    }
}
