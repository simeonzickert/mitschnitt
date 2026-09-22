use std::collections::HashMap;

use crate::types::{ChannelProfile, Segment, SegmentBuilderOptions, SegmentKey, SegmentWord};

use super::model::{ProtoSegment, ResolvedWordFrame, SpeakerIdentity, SpeakerState};
use super::speakers::assign_complete_channel_human_id;

/// Grenzen, innerhalb derer ein Block weiterwachsen darf.
#[derive(Clone, Copy)]
pub(super) struct Blockgrenzen {
    max_gap_ms: i64,
    max_words: usize,
    max_ms: i64,
}

impl Blockgrenzen {
    fn aus(options: Option<&SegmentBuilderOptions>) -> Self {
        Self {
            max_gap_ms: options.and_then(|opts| opts.max_gap_ms).unwrap_or(3000),
            max_words: options
                .and_then(|opts| opts.max_segment_words)
                .unwrap_or(usize::MAX),
            max_ms: options
                .and_then(|opts| opts.max_segment_ms)
                .unwrap_or(i64::MAX),
        }
    }

    /// Darf `block` um `zusatz_woerter` Woerter wachsen, die bei
    /// `naechster_start_ms` beginnen und bei `neues_ende_ms` enden?
    ///
    /// Prueft Pause UND beide Deckel -- das ist die Regel fuers Anhaengen
    /// beim Sammeln und beim Zusammenfuehren.
    fn darf_wachsen(
        &self,
        block: &ProtoSegment,
        naechster_start_ms: i64,
        neues_ende_ms: i64,
        zusatz_woerter: usize,
    ) -> bool {
        let Some(letztes) = block.words.last() else {
            return false;
        };
        if naechster_start_ms - letztes.word.end_ms > self.max_gap_ms {
            return false;
        }
        self.deckel_halten(block, neues_ende_ms, zusatz_woerter)
    }

    fn deckel_halten(
        &self,
        block: &ProtoSegment,
        neues_ende_ms: i64,
        zusatz_woerter: usize,
    ) -> bool {
        if block.words.len() + zusatz_woerter > self.max_words {
            return false;
        }
        let start_ms = block
            .words
            .first()
            .map_or(neues_ende_ms, |erstes| erstes.word.start_ms);
        neues_ende_ms - start_ms <= self.max_ms
    }
}

/// Ein Wort haengt an den letzten Block DESSELBEN Kanals an, nicht an den
/// unmittelbar letzten Block ueberhaupt.
///
/// Vorher beendete ein einzelnes "ja" der Gegenseite den laufenden
/// Redebeitrag endgueltig: das naechste Wort des Sprechenden eroeffnete einen
/// neuen Block. An einem echten 23-Minuten-Gespraech gemessen ergab das 490
/// Bloecke mit einem Median von 2 Woertern, 64 % davon kuerzer als vier
/// Woerter -- ganze Saetze lagen in Schnipseln vor.
///
/// Bewusst KEINE kluegere Heuristik (kurzer Einwurf gegen echten
/// Sprecherwechsel): eine Tonspur ist eine Person, und wer weiterredet, redet
/// weiter. Die Trennung leisten Kanal, Sprecher-Identitaet, die Pause
/// (`max_gap_ms`) und der Deckel oben.
pub(super) fn collect_segments(
    frames: Vec<ResolvedWordFrame>,
    options: Option<&SegmentBuilderOptions>,
) -> Vec<ProtoSegment> {
    let grenzen = Blockgrenzen::aus(options);
    let mut segments: Vec<ProtoSegment> = Vec::new();
    let mut last_segment_by_channel: HashMap<ChannelProfile, usize> = HashMap::new();

    for frame in frames {
        let key = determine_key(&frame, &segments, &last_segment_by_channel);
        let kandidat = last_segment_by_channel.get(&key.channel).copied();

        let ziel = kandidat.filter(|&index| {
            segments[index].key == key
                && grenzen.darf_wachsen(&segments[index], frame.word.start_ms, frame.word.end_ms, 1)
        });

        if let Some(index) = ziel {
            segments[index].words.push(frame);
            continue;
        }

        let channel = key.channel;
        segments.push(ProtoSegment {
            key,
            words: vec![frame],
        });
        last_segment_by_channel.insert(channel, segments.len() - 1);
    }

    segments
}

/// Zwei Bloecke desselben Menschen wachsen zusammen, auch wenn der Erkenner
/// ihnen verschiedene Sprecher-Nummern gegeben hat.
///
/// Zieht dieselbe Kanal-Logik wie `collect_segments`: Nachbar ist der letzte
/// behaltene Block DESSELBEN Kanals, nicht der Vektor-Nachbar. Seit die
/// Block-Bildung kanalbewusst arbeitet, ist der Vektor-Nachbar naemlich fast
/// immer der andere Kanal, und diese Stelle wuerde nie wieder feuern.
///
/// Dass hier `Blockgrenzen` mitgeprueft werden, ist eine bewusste
/// Verschaerfung gegenueber vorher: frueher lagen zwei Bloecke desselben
/// Kanals nur dann nebeneinander, wenn die Pause zwischen ihnen laenger als
/// `max_gap_ms` war, und sie wurden trotzdem verschmolzen. Kanalbewusst
/// wuerde daraus "alle Bloecke eines Kanals werden einer" -- also gelten
/// Pause und Deckel auch hier.
pub(super) fn propagate_identity(
    segments: &mut Vec<ProtoSegment>,
    speaker_state: &SpeakerState,
    options: Option<&SegmentBuilderOptions>,
) {
    let grenzen = Blockgrenzen::aus(options);
    let mut write_index = 0;
    let mut last_kept_by_channel: HashMap<ChannelProfile, usize> = HashMap::new();

    for read_index in 0..segments.len() {
        assign_complete_channel_human_id(&mut segments[read_index], speaker_state);

        let kandidat = last_kept_by_channel
            .get(&segments[read_index].key.channel)
            .copied();

        let ziel = kandidat.filter(|&kept_index| {
            let Some(erstes) = segments[read_index].words.first() else {
                return false;
            };
            let Some(letztes) = segments[read_index].words.last() else {
                return false;
            };
            should_merge_adjacent_keys(&segments[kept_index].key, &segments[read_index].key)
                && grenzen.darf_wachsen(
                    &segments[kept_index],
                    erstes.word.start_ms,
                    letztes.word.end_ms,
                    segments[read_index].words.len(),
                )
        });

        if let Some(kept_index) = ziel {
            let words = std::mem::take(&mut segments[read_index].words);
            segments[kept_index].words.extend(words);
            continue;
        }

        let channel = segments[read_index].key.channel;
        if write_index != read_index {
            segments.swap(write_index, read_index);
        }
        last_kept_by_channel.insert(channel, write_index);
        write_index += 1;
    }

    segments.truncate(write_index);
}

pub(super) fn finalize_segments(proto_segments: Vec<ProtoSegment>) -> Vec<Segment> {
    proto_segments
        .into_iter()
        .map(|segment| Segment {
            key: segment.key,
            words: segment
                .words
                .into_iter()
                .map(|frame| SegmentWord {
                    text: frame.word.text,
                    start_ms: frame.word.start_ms,
                    end_ms: frame.word.end_ms,
                    channel: frame.word.channel,
                    is_final: frame.word.is_final,
                    id: frame.word.id,
                })
                .collect(),
        })
        .collect()
}

fn determine_key(
    frame: &ResolvedWordFrame,
    segments: &[ProtoSegment],
    last_segment_by_channel: &HashMap<ChannelProfile, usize>,
) -> SegmentKey {
    if !frame.word.is_final
        && let Some(&index) = last_segment_by_channel.get(&frame.word.channel)
    {
        return segments[index].key.clone();
    }

    create_segment_key(frame.word.channel, frame.identity.as_ref())
}

fn create_segment_key(channel: ChannelProfile, identity: Option<&SpeakerIdentity>) -> SegmentKey {
    SegmentKey {
        channel,
        speaker_index: identity.and_then(|value| value.speaker_index),
        speaker_human_id: identity.and_then(|value| value.human_id.clone()),
    }
}

fn should_merge_adjacent_keys(last: &SegmentKey, next: &SegmentKey) -> bool {
    if last.channel != next.channel {
        return false;
    }

    if let (Some(last_human), Some(next_human)) = (&last.speaker_human_id, &next.speaker_human_id)
        && last_human == next_human
    {
        return true;
    }

    last == next && next.has_speaker_identity()
}
