use std::collections::{HashMap, HashSet};

use crate::{ChannelProfile, Segment, SegmentKey};

#[derive(Debug, Clone, Default)]
pub struct SpeakerLabelContext {
    pub self_human_id: Option<String>,
    pub human_name_by_id: HashMap<String, String>,
    /// Kanaele, auf denen die Trennung mehr als einen Sprecher gefunden hat
    /// (Fork, 08.09.2026). Gegenstueck zu `SpeakerState::multi_speaker_channels`
    /// -- dort entscheidet es ueber die Zuweisung, hier ueber das Etikett.
    ///
    /// Leer heisst "ein Kanal, ein Mensch", also das Verhalten von vorher.
    pub multi_speaker_channels: HashSet<ChannelProfile>,
}

/// Vergibt fortlaufende Nummern an Sprecher ohne Namen.
///
/// Fork (21.09.2026): der geerbte Deckel auf die Teilnehmerzahl ist RAUS.
/// Er stammt aus dem Cloud-Live-Pfad (upstream 617683dc7e, "cap provider
/// speaker labels to the current participant count") und sollte verhindern,
/// dass ein Anbieter, der seine Sprechernummern ueber Neuverbindungen
/// hochzaehlt, endlos neue Etiketten erzeugt. Er tat das per
/// `next_index.min(max)`, also indem er JEDEN ueberzaehligen Schluessel auf
/// dieselbe letzte erlaubte Nummer legte.
///
/// Gemessen an einer 122,8-Minuten-Raumaufnahme mit vier Personen: die
/// Trennung fand fuenf Stimmgruppen, der Deckel warf die beiden Hauptredner
/// (zusammen 60 % aller Woerter) unter EIN Etikett. Ein Etikett zu viel kann
/// ein Mensch zusammenfuehren, zwei verschmolzene Menschen kann er nicht mehr
/// trennen -- die Anzeige darf nie zwei verschiedene Sprecher-Schluessel auf
/// dasselbe Etikett legen.
///
/// Ehrlich zum Preis (nachgetragen 22.09.2026, nachdem ein Zweitblick die
/// erste Fassung dieses Absatzes am Code widerlegt hat): der Zweck des
/// Deckels entfaellt ERSATZLOS. `soniox_speaker_index` aus demselben Commit
/// ist eine reine Format-Normalisierung (String "1" wird zur Zahl 0) und
/// fuehrt KEINE Zuordnung ueber Neuverbindungen. Nummeriert ein Anbieter nach
/// einem Reconnect neu, entstehen ab jetzt unbegrenzt neue Etiketten.
///
/// Das ist bewusst so entschieden, nicht uebersehen: der Deckel loeste dieses
/// Problem ohnehin nicht, er kaschierte es und mischte die Reconnect-Fetzen
/// dabei unter einen echten Sprecher. Ein Etikett zu viel ist reparierbar,
/// zwei verschmolzene Menschen sind es nicht.
#[derive(Debug, Clone, Default)]
pub struct SpeakerLabeler {
    unknown_speaker_map: HashMap<SegmentKey, usize>,
    next_index: usize,
}

impl SpeakerLabeler {
    pub fn new() -> Self {
        Self {
            unknown_speaker_map: HashMap::new(),
            next_index: 1,
        }
    }

    pub fn from_segments(segments: &[Segment], ctx: Option<&SpeakerLabelContext>) -> Self {
        let mut labeler = Self::new();
        for segment in segments {
            if !segment.key.is_known_speaker(ctx) {
                labeler.unknown_speaker_number(&segment.key);
            }
        }
        labeler
    }

    pub fn label_for(&mut self, key: &SegmentKey, ctx: Option<&SpeakerLabelContext>) -> String {
        render_speaker_label(key, ctx, Some(self))
    }

    pub fn unknown_speaker_number(&mut self, key: &SegmentKey) -> usize {
        if let Some(existing) = self.unknown_speaker_map.get(key) {
            return *existing;
        }

        // Jeder neue Schluessel bekommt eine eigene Nummer. Kein Deckel: siehe
        // Kopfkommentar am Typ.
        let next = self.next_index;
        self.unknown_speaker_map.insert(key.clone(), next);
        self.next_index += 1;
        next
    }
}

impl SegmentKey {
    pub fn is_known_speaker(&self, ctx: Option<&SpeakerLabelContext>) -> bool {
        if self.speaker_human_id.is_some() {
            return true;
        }

        // Fork (08.09.2026): der Mehrsprecher-Vorbehalt ergaenzt. Ohne ihn galt
        // JEDER DirectMic-Block als bekannter Sprecher, auch die vier getrennten
        // Sprecher einer Raumaufnahme -- keiner von ihnen bekam eine
        // Sprecher-Nummer vom Labeler, weil `from_segments` sie hier
        // aussortierte. Die Fehlerrichtung ist wichtig: wer faelschlich als
        // bekannt gilt, bekommt einen falschen NAMEN; wer faelschlich als
        // unbekannt gilt, bekommt eine Nummer. Das zweite ist heilbar.
        //
        // Aus dem `matches!` ausgeschrieben, weil die neue Bedingung `ctx`
        // selbst braucht und nicht nur die gebundenen Felder.
        let Some(ctx) = ctx else {
            return false;
        };

        ctx.self_human_id.is_some()
            && self.channel == ChannelProfile::DirectMic
            && !ctx.multi_speaker_channels.contains(&self.channel)
    }
}

pub fn render_speaker_label(
    key: &SegmentKey,
    ctx: Option<&SpeakerLabelContext>,
    mut labeler: Option<&mut SpeakerLabeler>,
) -> String {
    if let Some(ctx) = ctx {
        if let Some(human_id) = key.speaker_human_id.as_ref() {
            if let Some(name) = ctx.human_name_by_id.get(human_id) {
                return name.clone();
            }
            return human_id.clone();
        }

        // Fork (08.09.2026): siehe `is_known_speaker`. Diese beiden Bedingungen
        // MUESSEN deckungsgleich bleiben -- `SpeakerLabeler::from_segments`
        // vergibt die Nummern nach der einen, `render_speaker_label` liest nach
        // der anderen. Laufen sie auseinander, faellt ein Block hier ans Ende
        // durch und bekommt eine Nummer, die der Labeler nie reserviert hat.
        if key.channel == ChannelProfile::DirectMic
            && !ctx.multi_speaker_channels.contains(&key.channel)
            && let Some(self_human_id) = ctx.self_human_id.as_ref()
        {
            if let Some(name) = ctx.human_name_by_id.get(self_human_id) {
                return name.clone();
            }
            return "You".to_string();
        }
    } else if let Some(human_id) = key.speaker_human_id.as_ref() {
        return human_id.clone();
    }

    if let Some(labeler) = labeler.as_mut() {
        return format!("Speaker {}", labeler.unknown_speaker_number(key));
    }

    let channel_label = match key.channel {
        ChannelProfile::DirectMic => "A",
        ChannelProfile::RemoteParty => "B",
        ChannelProfile::MixedCapture => "C",
    };

    match key.speaker_index {
        Some(index) => format!("Speaker {}", index + 1),
        None => format!("Speaker {channel_label}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChannelProfile;

    fn direct_mic_key() -> SegmentKey {
        SegmentKey {
            channel: ChannelProfile::DirectMic,
            speaker_index: None,
            speaker_human_id: None,
        }
    }

    #[test]
    fn renders_direct_mic_as_you_when_self_exists_without_name() {
        let ctx = SpeakerLabelContext {
            self_human_id: Some("self".to_string()),
            human_name_by_id: HashMap::new(),
            ..Default::default()
        };

        assert_eq!(
            render_speaker_label(&direct_mic_key(), Some(&ctx), None),
            "You"
        );
    }

    #[test]
    fn preserves_unknown_speaker_numbers_across_calls() {
        let mut labeler = SpeakerLabeler::new();
        let a = SegmentKey {
            channel: ChannelProfile::DirectMic,
            speaker_index: Some(7),
            speaker_human_id: None,
        };
        let b = SegmentKey {
            channel: ChannelProfile::RemoteParty,
            speaker_index: Some(9),
            speaker_human_id: None,
        };

        assert_eq!(labeler.label_for(&a, None), "Speaker 1");
        assert_eq!(labeler.label_for(&b, None), "Speaker 2");
        assert_eq!(labeler.label_for(&a, None), "Speaker 1");
    }

    /// Vorher hiess dieser Test `caps_unknown_speaker_numbers` und schrieb das
    /// Verschmelzen fest: der dritte Sprecher bekam bei zwei Teilnehmern
    /// dasselbe Etikett wie der zweite. Umgeschrieben auf die Zusage, die jetzt
    /// gilt -- mehr Stimmgruppen als Teilnehmer sind ein Anzeigefall, kein
    /// Grund, zwei Menschen zusammenzulegen.
    #[test]
    fn never_merges_two_speaker_keys_onto_one_label() {
        let mut labeler = SpeakerLabeler::new();
        let keys: Vec<SegmentKey> = (0..5)
            .map(|index| SegmentKey {
                channel: ChannelProfile::MixedCapture,
                speaker_index: Some(index),
                speaker_human_id: None,
            })
            .collect();

        let etiketten: Vec<String> = keys
            .iter()
            .map(|key| labeler.label_for(key, None))
            .collect();

        assert_eq!(
            etiketten,
            vec![
                "Speaker 1", "Speaker 2", "Speaker 3", "Speaker 4", "Speaker 5"
            ],
            "fuenf Schluessel, fuenf Etiketten: {etiketten:?}"
        );

        let verschiedene: HashSet<&str> = etiketten.iter().map(String::as_str).collect();
        assert_eq!(verschiedene.len(), 5);
    }

    /// Die Kernzusage am echten Fall: vier Teilnehmer in der Sitzung, fuenf
    /// Stimmgruppen aus der Trennung. Frueher wurden daraus vier Etiketten mit
    /// einem doppelt belegten.
    ///
    /// Wichtig zur Einordnung: fuenf Etiketten sind hier NICHT das Ziel,
    /// sondern die ehrliche Anzeige eines Fehlers, der eine Ebene tiefer sitzt
    /// -- die fuenfte Gruppe ist ein Restcluster aus kurzen Fetzen und gehoert
    /// beim Erzeugen der Hinweise aufgeloest, nicht in der Anzeige
    /// weggedeckelt. Diese Schicht hat nur eine Aufgabe: den Fehler nicht
    /// schlimmer machen, indem sie zwei Menschen zusammenlegt.
    #[test]
    fn five_speaker_keys_with_four_participants_get_five_labels() {
        let segments: Vec<Segment> = (0..5)
            .map(|index| Segment {
                key: SegmentKey {
                    channel: ChannelProfile::MixedCapture,
                    speaker_index: Some(index),
                    speaker_human_id: None,
                },
                words: Vec::new(),
            })
            .collect();

        let ctx = SpeakerLabelContext {
            self_human_id: Some("self".to_string()),
            human_name_by_id: HashMap::new(),
            multi_speaker_channels: HashSet::from([ChannelProfile::MixedCapture]),
        };

        let mut labeler = SpeakerLabeler::from_segments(&segments, Some(&ctx));
        let etiketten: HashSet<String> = segments
            .iter()
            .map(|segment| labeler.label_for(&segment.key, Some(&ctx)))
            .collect();

        assert_eq!(etiketten.len(), 5, "{etiketten:?}");
    }

    #[test]
    fn treats_direct_mic_with_provider_speaker_as_self() {
        let ctx = SpeakerLabelContext {
            self_human_id: Some("self".to_string()),
            human_name_by_id: HashMap::new(),
            ..Default::default()
        };
        let key = SegmentKey {
            channel: ChannelProfile::DirectMic,
            speaker_index: Some(2),
            speaker_human_id: None,
        };

        assert!(key.is_known_speaker(Some(&ctx)));
        assert_eq!(render_speaker_label(&key, Some(&ctx), None), "You");
    }
}
