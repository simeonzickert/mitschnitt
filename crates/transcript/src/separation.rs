//! Namen nur dort, wo eine Sprechertrennung sie traegt (ZICK-312, Rust-Seite).
//!
//! Laeuft keine Trennung (z. B. ein Cloud-Transkript von vor dem Kanal-Weg,
//! alles auf Kanal 0), macht die Anzeige aus jedem Mikrofon-Block den Nutzer
//! selbst. Wer das Etikett weiterreicht -- der Markdown-Spiegel, ein
//! Sprachmodell --, schreibt damit jede Aussage dem Nutzer zu. Hier steht die
//! EINE Rust-Fassung der Regel; die Oberflaeche traegt dieselbe Regel in
//! `apps/desktop/src/stt/speaker-separation.ts` (Zusammenfassung und Chat).
//! Aendert sich eine, muss die andere folgen.
//!
//! Die Regel:
//! - Getrennt ist ein Transkript mit Woertern auf mehr als einem Kanal, mehr
//!   als einem Anbieter-Sprecher oder mehr als einem automatisch zugeordneten
//!   Menschen. Ohne Woerter laesst sich nichts beurteilen; dann bleibt alles,
//!   wie es ist. Woerter OHNE Kanalangabe gelten als ungetrennt (Forge M5,
//!   fail-closed): der Renderer legt sie auf Kanal 0 = Nutzer.
//! - Ungetrennt und andere Teilnehmer dabei: jeder Block heisst
//!   [`UNSEPARATED_SPEAKER_LABEL`] -- AUSSER den Bloecken, die ein Mensch von
//!   Hand zugeordnet hat (Opus-Zweitblick 26.09.2026: eine einzelne
//!   Handzuordnung darf nicht alle Namen wieder einschalten).

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::RenderedTranscriptSegment;

/// Wortgleich mit `UNSEPARATED_SPEAKER_LABEL` in `speaker-separation.ts` und
/// den Regeln in `enhance.system.md.jinja` / `chat.system.md.jinja`.
pub const UNSEPARATED_SPEAKER_LABEL: &str = "Unknown speaker";

/// Eine gespeicherte Transkript-Zeile, so wie sie in der Datenbank steht
/// (`words_json`, `speaker_hints_json`, beide schon als JSON-Arrays gelesen).
pub struct SeparationRow<'a> {
    pub words: &'a [Value],
    pub hints: &'a [Value],
}

fn hint_type(hint: &Value) -> Option<&str> {
    hint.get("type").and_then(Value::as_str)
}

/// Hinweiswerte liegen als Objekt oder als JSON-Text vor; beides zaehlt.
fn hint_value(hint: &Value) -> Option<Value> {
    match hint.get("value") {
        Some(Value::String(raw)) => serde_json::from_str(raw).ok(),
        Some(value @ Value::Object(_)) => Some(value.clone()),
        _ => None,
    }
}

/// Traegt das Transkript eine echte Sprechertrennung? Handzuordnungen zaehlen
/// hier NICHT -- sie benennen einzelne Bloecke, sie trennen nicht.
pub fn transcript_has_speaker_separation(rows: &[SeparationRow<'_>]) -> bool {
    let mut channels = HashSet::new();
    let mut provider_speakers = HashSet::new();
    let mut assigned_humans = HashSet::new();
    let mut has_words = false;

    for row in rows {
        for word in row.words {
            has_words = true;
            if let Some(channel) = word.get("channel").and_then(Value::as_i64) {
                channels.insert(channel);
            }
        }
        for hint in row.hints {
            let Some(value) = hint_value(hint) else {
                continue;
            };
            match hint_type(hint) {
                Some("provider_speaker_index") => {
                    if let Some(index) = value.get("speaker_index").and_then(Value::as_i64) {
                        let channel = value.get("channel").map(Value::to_string);
                        provider_speakers.insert((channel, index));
                    }
                }
                Some("automatic_speaker_assignment") => {
                    if let Some(human) = value.get("human_id").and_then(Value::as_str) {
                        assigned_humans.insert(human.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    if !has_words {
        return true;
    }
    channels.len() > 1 || provider_speakers.len() > 1 || assigned_humans.len() > 1
}

/// Sollen die Etiketten neutralisiert werden? Ja, wenn andere Menschen dabei
/// waren und das Transkript keine Trennung traegt.
pub fn should_neutralize_speaker_labels(
    rows: &[SeparationRow<'_>],
    participant_human_ids: &[String],
    self_human_id: Option<&str>,
) -> bool {
    let others_present = participant_human_ids
        .iter()
        .any(|id| !id.is_empty() && Some(id.as_str()) != self_human_id);
    others_present && !transcript_has_speaker_separation(rows)
}

/// Alle Wort-Kennungen, die eine Handzuordnung abdeckt. Abgebildet wie die
/// Zuordnung selbst (`render-transcript.ts`, `normalizeSpeakerHint`): Bereich
/// "segment" = genau diese Woerter, Bereich "speaker" = alle Woerter dieses
/// Kanals (und Anbieter-Sprechers), alte Hinweise ohne Bereich = Kanal bzw.
/// Kanal und Sprecher des Ankerworts.
pub fn manually_assigned_word_ids(rows: &[SeparationRow<'_>]) -> HashSet<String> {
    let mut covered = HashSet::new();

    for row in rows {
        // (Kanal, Anbieter-Sprecher) je Wort, so wie die Anzeige sie sieht.
        let mut word_keys: HashMap<String, (i64, Option<i64>)> = HashMap::new();
        let mut order = Vec::new();
        for word in row.words {
            let Some(id) = word.get("id").and_then(Value::as_str) else {
                continue;
            };
            let channel = word.get("channel").and_then(Value::as_i64).unwrap_or(0);
            word_keys.insert(id.to_string(), (channel, None));
            order.push(id.to_string());
        }
        for hint in row.hints {
            if hint_type(hint) != Some("provider_speaker_index") {
                continue;
            }
            let (Some(word_id), Some(value)) = (
                hint.get("word_id").and_then(Value::as_str),
                hint_value(hint),
            ) else {
                continue;
            };
            let Some(index) = value.get("speaker_index").and_then(Value::as_i64) else {
                continue;
            };
            if let Some(key) = word_keys.get_mut(word_id) {
                key.1 = Some(index);
                if let Some(channel) = value.get("channel").and_then(Value::as_i64) {
                    key.0 = channel;
                }
            }
        }

        let cover_matching =
            |covered: &mut HashSet<String>, channel: i64, speaker: Option<Option<i64>>| {
                for id in &order {
                    let (word_channel, word_speaker) = word_keys[id];
                    let speaker_matches = speaker.is_none_or(|wanted| word_speaker == wanted);
                    if word_channel == channel && speaker_matches {
                        covered.insert(id.clone());
                    }
                }
            };

        for hint in row.hints {
            if hint_type(hint) != Some("user_speaker_assignment") {
                continue;
            }
            let Some(value) = hint_value(hint) else {
                continue;
            };
            if value.get("human_id").and_then(Value::as_str).is_none() {
                continue;
            }
            match value.get("scope").and_then(Value::as_str) {
                Some("segment") => {
                    if let Some(ids) = value.get("word_ids").and_then(Value::as_array) {
                        let ids = ids
                            .iter()
                            .filter_map(Value::as_str)
                            .filter(|id| !id.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<_>>();
                        if !ids.is_empty() {
                            covered.extend(ids);
                            continue;
                        }
                    }
                }
                Some("speaker") => {
                    if let Some(channel) = value.get("channel").and_then(Value::as_i64) {
                        match value.get("speaker_index") {
                            Some(Value::Number(index)) => {
                                cover_matching(&mut covered, channel, Some(index.as_i64()));
                            }
                            // `null`: der ganze Kanal.
                            _ => cover_matching(&mut covered, channel, None),
                        }
                        continue;
                    }
                }
                _ => {}
            }
            // Alter Hinweis: Kanal (und Sprecher) des Ankerworts.
            if let Some(anchor) = hint.get("word_id").and_then(Value::as_str)
                && let Some(&(channel, speaker)) = word_keys.get(anchor)
            {
                match speaker {
                    Some(index) => cover_matching(&mut covered, channel, Some(Some(index))),
                    None => cover_matching(&mut covered, channel, None),
                }
            }
        }
    }

    covered
}

/// Setzt in ungetrennten Transkripten jedes Etikett auf
/// [`UNSEPARATED_SPEAKER_LABEL`], ausser in Bloecken mit Handzuordnung.
pub fn neutralize_unseparated_labels(
    rows: &[SeparationRow<'_>],
    participant_human_ids: &[String],
    self_human_id: Option<&str>,
    segments: &mut [RenderedTranscriptSegment],
) {
    if !should_neutralize_speaker_labels(rows, participant_human_ids, self_human_id) {
        return;
    }
    let manual = manually_assigned_word_ids(rows);
    for segment in segments {
        let assigned_by_hand = segment
            .words
            .iter()
            .any(|word| word.id.as_ref().is_some_and(|id| manual.contains(id)));
        if !assigned_by_hand {
            segment.speaker_label = UNSEPARATED_SPEAKER_LABEL.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn words(channels: &[i64]) -> Vec<Value> {
        channels
            .iter()
            .enumerate()
            .map(|(index, channel)| json!({ "id": format!("w{index}"), "channel": channel }))
            .collect()
    }

    #[test]
    fn ein_kanal_ohne_hinweise_ist_ungetrennt() {
        let words = words(&[0, 0, 0]);
        let rows = [SeparationRow {
            words: &words,
            hints: &[],
        }];
        assert!(!transcript_has_speaker_separation(&rows));
        assert!(should_neutralize_speaker_labels(
            &rows,
            &["me".to_string(), "max".to_string()],
            Some("me")
        ));
        assert!(!should_neutralize_speaker_labels(
            &rows,
            &["me".to_string()],
            Some("me")
        ));
    }

    #[test]
    fn zwei_kanaele_sind_getrennt() {
        let words = words(&[0, 1]);
        let rows = [SeparationRow {
            words: &words,
            hints: &[],
        }];
        assert!(transcript_has_speaker_separation(&rows));
    }

    #[test]
    fn ohne_kanalangabe_gilt_ungetrennt() {
        let words = vec![json!({ "id": "w0" }), json!({ "id": "w1" })];
        let rows = [SeparationRow {
            words: &words,
            hints: &[],
        }];
        assert!(!transcript_has_speaker_separation(&rows));

        // Mehrere Anbieter-Sprecher tragen die Trennung auch ohne Kanal.
        let hints = vec![
            json!({"word_id": "w0", "type": "provider_speaker_index", "value": {"speaker_index": 0}}),
            json!({"word_id": "w1", "type": "provider_speaker_index", "value": {"speaker_index": 1}}),
        ];
        let rows = [SeparationRow {
            words: &words,
            hints: &hints,
        }];
        assert!(transcript_has_speaker_separation(&rows));
    }

    #[test]
    fn eine_handzuordnung_schaltet_keine_trennung_ein() {
        let words = words(&[0, 0, 0, 0]);
        let hints = vec![json!({
            "word_id": "w1",
            "type": "user_speaker_assignment",
            "value": "{\"human_id\":\"max\",\"scope\":\"segment\",\"word_ids\":[\"w1\",\"w2\"]}",
        })];
        let rows = [SeparationRow {
            words: &words,
            hints: &hints,
        }];
        assert!(
            !transcript_has_speaker_separation(&rows),
            "eine Handzuordnung ist keine Trennung"
        );
        let manual = manually_assigned_word_ids(&rows);
        assert_eq!(manual, HashSet::from(["w1".to_string(), "w2".to_string()]));
    }

    #[test]
    fn bereich_speaker_deckt_den_ganzen_kanal() {
        let words = words(&[0, 0, 1]);
        let hints = vec![json!({
            "word_id": "w0",
            "type": "user_speaker_assignment",
            "value": { "human_id": "me", "scope": "speaker", "channel": 0, "speaker_index": null },
        })];
        let rows = [SeparationRow {
            words: &words,
            hints: &hints,
        }];
        assert_eq!(
            manually_assigned_word_ids(&rows),
            HashSet::from(["w0".to_string(), "w1".to_string()])
        );
    }
}
