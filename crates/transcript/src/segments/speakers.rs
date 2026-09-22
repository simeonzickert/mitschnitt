use std::collections::{HashMap, HashSet};

use crate::types::{ChannelProfile, IdentityAssignment, IdentityScope};
use crate::types::{SegmentBuilderOptions, SegmentKey};

use super::model::{
    NormalizedWord, ProtoSegment, ResolvedWordFrame, SpeakerIdentity, SpeakerState,
};

pub(super) fn create_speaker_state(
    assignments: &[IdentityAssignment],
    normalized_words: &[NormalizedWord],
    options: Option<&SegmentBuilderOptions>,
) -> SpeakerState {
    let complete_channels = options
        .and_then(|opts| opts.complete_channels.clone())
        .or_else(|| SegmentBuilderOptions::default().complete_channels)
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut assignment_by_word_index: HashMap<usize, SpeakerIdentity> = HashMap::new();
    let mut human_id_by_scoped_speaker: HashMap<(ChannelProfile, i32), String> = HashMap::new();

    for assignment in assignments {
        if let IdentityScope::ChannelSpeaker {
            channel,
            speaker_index,
        } = assignment.scope
        {
            human_id_by_scoped_speaker
                .insert((channel, speaker_index), assignment.human_id.clone());
        }
    }

    for word in normalized_words {
        if let Some(speaker_index) = word.speaker_index {
            let entry = assignment_by_word_index.entry(word.order).or_default();
            entry.speaker_index = Some(speaker_index);

            if let Some(human_id) = human_id_by_scoped_speaker.get(&(word.channel, speaker_index)) {
                entry.human_id = Some(human_id.clone());
            }
        }
    }

    for assignment in assignments {
        if let IdentityScope::Words { word_ids } = &assignment.scope {
            for word in normalized_words {
                let Some(word_id) = word.id.as_ref() else {
                    continue;
                };
                if !word_ids.iter().any(|id| id == word_id) {
                    continue;
                }

                let entry = assignment_by_word_index.entry(word.order).or_default();
                entry.human_id = Some(assignment.human_id.clone());
            }
        }
    }

    let mut human_id_by_channel: HashMap<ChannelProfile, String> = HashMap::new();
    for assignment in assignments {
        if let IdentityScope::Channel { channel } = assignment.scope {
            human_id_by_channel.insert(channel, assignment.human_id.clone());
        }
    }

    SpeakerState {
        assignment_by_word_index,
        human_id_by_scoped_speaker,
        human_id_by_channel,
        last_speaker_by_channel: HashMap::new(),
        complete_channels,
        // Vorgabe schlaegt Selbstberechnung: wer ueber mehrere Transkript-
        // Zeilen rendert, kennt die ganze Sitzung, dieser Aufruf nur seine
        // Zeile. Siehe `SegmentBuilderOptions::multi_speaker_channels`.
        multi_speaker_channels: options
            .and_then(|opts| opts.multi_speaker_channels.clone())
            .map(|channels| channels.into_iter().collect())
            .unwrap_or_else(|| multi_speaker_channels(normalized_words)),
    }
}

/// Auf welchen Kanaelen hat die Trennung mehr als einen Sprecher gefunden?
///
/// Fork (08.09.2026). Bewusst ueber die ganze Wortliste gerechnet und nicht je
/// Block: die Frage ist eine Aussage ueber den KANAL, und ein einzelner Block
/// kann sie nicht beantworten. Wer sie am Einzelfall entscheidet, kommt zu dem
/// Schluss, dass jeder getrennte Sprecher der Selbst-Mensch ist -- genau der
/// Fehler, der des Betreibers Vier-Personen-Aufnahme viermal seinen eigenen Namen
/// gegeben hat.
fn multi_speaker_channels(words: &[NormalizedWord]) -> HashSet<ChannelProfile> {
    let mut speakers_by_channel: HashMap<ChannelProfile, HashSet<i32>> = HashMap::new();
    for word in words {
        if let Some(speaker_index) = word.speaker_index {
            speakers_by_channel
                .entry(word.channel)
                .or_default()
                .insert(speaker_index);
        }
    }

    speakers_by_channel
        .into_iter()
        .filter(|(_, speakers)| speakers.len() > 1)
        .map(|(channel, _)| channel)
        .collect()
}

pub(super) fn resolve_identities(
    words: &[NormalizedWord],
    speaker_state: &mut SpeakerState,
) -> Vec<ResolvedWordFrame> {
    words
        .iter()
        .map(|word| {
            let assignment = speaker_state
                .assignment_by_word_index
                .get(&word.order)
                .cloned();
            let identity = apply_identity_rules(word, assignment.as_ref(), speaker_state);
            remember_identity(word, assignment.as_ref(), &identity, speaker_state);

            ResolvedWordFrame {
                word: word.clone(),
                identity: (!identity.is_empty()).then_some(identity),
            }
        })
        .collect()
}

pub(super) fn assign_complete_channel_human_id(segment: &mut ProtoSegment, state: &SpeakerState) {
    if segment.key.speaker_human_id.is_some() {
        return;
    }

    // Fork (08.09.2026): "dieser Kanal gehoert diesem Menschen" ist nur dann
    // eine wahre Aussage, wenn die Trennung auf diesem Kanal nicht mehrere
    // Sprecher gefunden hat. Hat sie es, waere die grobe Kanal-Aussage eine
    // Behauptung ueber Menschen, die niemand geprueft hat.
    //
    // Ein Kanal mit genau EINEM gefundenen Sprecher bleibt bewusst draussen --
    // dort ist "ein Kanal, ein Mensch" weiterhin wahr, und der Selbst-Mensch
    // behaelt seinen Namen.
    //
    // Anlass: eine Zwei-Stunden-Raumaufnahme mit vier Personen. Ein
    // einziger Kanal, vier sauber getrennte Sprecher in der Datenbank
    // (Index 0..3, 746/5727/6888/3663 Woerter) -- und jeder einzelne trug im
    // Transkript SEINEN Namen, weil die Kanal-Zuweisung
    // (`channel_assignments_for_participants`, types/speaker.rs) den
    // Selbst-Menschen bedingungslos auf DirectMic legt.
    if state.multi_speaker_channels.contains(&segment.key.channel) {
        return;
    }

    let channel = segment.key.channel;
    if !state.complete_channels.contains(&channel) {
        return;
    }

    if let Some(human_id) = state.human_id_by_channel.get(&channel) {
        segment.key = SegmentKey {
            channel,
            speaker_index: segment.key.speaker_index,
            speaker_human_id: Some(human_id.clone()),
        };
    }
}

fn apply_identity_rules(
    word: &NormalizedWord,
    assignment: Option<&SpeakerIdentity>,
    state: &SpeakerState,
) -> SpeakerIdentity {
    let mut identity = assignment.cloned().unwrap_or_default();

    if let (Some(speaker_index), None) = (identity.speaker_index, &identity.human_id)
        && let Some(human_id) = state
            .human_id_by_scoped_speaker
            .get(&(word.channel, speaker_index))
    {
        identity.human_id = Some(human_id.clone());
    }

    // Fork (08.09.2026): die kanalweite Zuweisung greift nicht mehr auf einem
    // Kanal, auf dem die Trennung mehrere Sprecher gefunden hat. Dort ist "ein
    // Kanal, ein Mensch" schlicht unwahr, und das Ueberschreiben des feineren
    // Sprechers mit dem groberen Kanal ist Datenverlust, der sich als Wissen
    // tarnt.
    //
    // **Was hier baulich gilt, und was nicht** (ehrlich gefasst am
    // 21.09.2026; die fruehere Fassung behauptete mehr, als heute stimmt).
    //
    // BAULICH garantiert ist allein die Bedingung eine Zeile tiefer: die
    // kanalweite Zuordnung greift genau dann nicht, wenn dieser Kanal MEHR
    // ALS EINEN Sprecher-Index traegt (`multi_speaker_channels`). Das haengt
    // an den Woertern selbst und an keiner Annahme ueber die Planung.
    //
    // NICHT mehr garantiert ist der Satz, der hier frueher stand -- "beim
    // Zwei-Kanal-Gespraech traegt Kanal 0 nie einen Sprecher-Index". Er
    // stimmte, solange nur `channel_index == 1` eine Sprecherzahl bekam. Seit
    // dem 07.09.2026 wird bei einer Raumaufnahme auf dem MIKROFON getrennt
    // (der Systemton ist dort digitale Stille), und seit dem 21.09.2026 kann
    // auch ein Zwei-Kanal-Fall auf das Mikrofon fallen, wenn die Gegenseite
    // als Rauschen erkannt wird.
    //
    // Fuer die Raumaufnahme ist das GEWOLLT: dort sitzen mehrere Menschen an
    // einem Mikrofon, Kanal 0 soll mehrere Sprecher tragen, und der Nutzer
    // ist einer von ihnen. Die kanalweite Zuordnung waere dort schlicht
    // falsch -- sie wuerde allen Anwesenden seinen Namen geben.
    //
    // Der Fall, den es zu vermeiden gilt, ist der andere: eine echte, sehr
    // leise Gegenseite wird faelschlich fuer Rauschen gehalten, und der eine
    // Mensch am Mikrofon wird in mehrere Sprecher zerlegt -- dann verliert er
    // hier seinen Namen und steht als "Speaker 1/2" da, auch im Export.
    // Dagegen steht KEINE bauliche Garantie, sondern eine bewusste Wahl in
    // der Planung: faellt ein Zwei-Kanal-Fall auf Kanal 0, weil die
    // Gegenseite nur RELATIV still ist, wird dort nie eine Sprecherzahl
    // erzwungen (`soniqo_diarization_speaker_count` gibt `Inferred` statt
    // `Exact(n)`). Die freie Trennung findet auf einem Kanal mit einem
    // einzigen Menschen in aller Regel eine Stimme, und dann greift die
    // Zuordnung unten weiter.
    //
    // Gemessen (21.09.2026, 157 Aufnahmen): von 128 leiseren Kanaelen mit
    // Sprache sind neun Systemton-Kanaele, und keiner davon faellt unter die
    // Still-Regel -- der naechstliegende haelt Faktor 2,4 Abstand zum
    // Verhaeltnis-Band. Das ist eine Messung, keine Garantie.
    if identity.human_id.is_none()
        && !state.multi_speaker_channels.contains(&word.channel)
        && state.complete_channels.contains(&word.channel)
        && let Some(human_id) = state.human_id_by_channel.get(&word.channel)
    {
        identity.human_id = Some(human_id.clone());
    }

    if !(word.is_final || identity.speaker_index.is_some() && identity.human_id.is_some())
        && let Some(last) = state.last_speaker_by_channel.get(&word.channel)
    {
        if identity.speaker_index.is_none() {
            identity.speaker_index = last.speaker_index;
        }
        if identity.human_id.is_none() {
            identity.human_id = last.human_id.clone();
        }
    }

    identity
}

fn remember_identity(
    word: &NormalizedWord,
    assignment: Option<&SpeakerIdentity>,
    identity: &SpeakerIdentity,
    state: &mut SpeakerState,
) {
    let has_explicit_assignment = assignment
        .map(|value| value.speaker_index.is_some() || value.human_id.is_some())
        .unwrap_or(false);

    if let (Some(speaker_index), Some(human_id)) = (identity.speaker_index, &identity.human_id) {
        state
            .human_id_by_scoped_speaker
            .insert((word.channel, speaker_index), human_id.clone());
    }

    if state.complete_channels.contains(&word.channel)
        && identity.speaker_index.is_none()
        && let Some(human_id) = identity.human_id.clone()
    {
        state.human_id_by_channel.insert(word.channel, human_id);
    }

    if (!word.is_final || identity.speaker_index.is_some() || has_explicit_assignment)
        && !identity.is_empty()
    {
        state
            .last_speaker_by_channel
            .insert(word.channel, identity.clone());
    }
}
