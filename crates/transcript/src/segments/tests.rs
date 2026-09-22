use crate::types::{
    ChannelProfile, IdentityAssignment, IdentityScope, Segment, SegmentBuilderOptions, SegmentKey,
};
use crate::types::{FinalizedWord, PartialWord, WordState};

use super::build_segments;

fn fw(text: &str, start: i64, end: i64, ch: i32) -> FinalizedWord {
    FinalizedWord {
        id: format!("w-{text}"),
        text: text.to_string(),
        start_ms: start,
        end_ms: end,
        channel: ch,
        state: WordState::Final,
        speaker_index: None,
    }
}

fn fw_si(text: &str, start: i64, end: i64, ch: i32, si: i32) -> FinalizedWord {
    FinalizedWord {
        id: format!("w-{text}"),
        text: text.to_string(),
        start_ms: start,
        end_ms: end,
        channel: ch,
        state: WordState::Final,
        speaker_index: Some(si),
    }
}

fn pw(text: &str, start: i64, end: i64, ch: i32) -> PartialWord {
    PartialWord {
        text: text.to_string(),
        start_ms: start,
        end_ms: end,
        channel: ch,
        speaker_index: None,
    }
}

fn pw_si(text: &str, start: i64, end: i64, ch: i32, si: i32) -> PartialWord {
    PartialWord {
        text: text.to_string(),
        start_ms: start,
        end_ms: end,
        channel: ch,
        speaker_index: Some(si),
    }
}

fn channel_human(human_id: &str, ch: ChannelProfile) -> IdentityAssignment {
    IdentityAssignment {
        human_id: human_id.to_string(),
        scope: IdentityScope::Channel { channel: ch },
    }
}

fn speaker_human(human_id: &str, ch: ChannelProfile, si: i32) -> IdentityAssignment {
    IdentityAssignment {
        human_id: human_id.to_string(),
        scope: IdentityScope::ChannelSpeaker {
            channel: ch,
            speaker_index: si,
        },
    }
}

fn words_human(human_id: &str, word_ids: &[&str]) -> IdentityAssignment {
    IdentityAssignment {
        human_id: human_id.to_string(),
        scope: IdentityScope::Words {
            word_ids: word_ids.iter().map(|id| id.to_string()).collect(),
        },
    }
}

fn key(ch: i32) -> SegmentKey {
    SegmentKey {
        channel: ChannelProfile::from(ch),
        speaker_index: None,
        speaker_human_id: None,
    }
}

fn key_speaker(ch: i32, si: i32) -> SegmentKey {
    SegmentKey {
        channel: ChannelProfile::from(ch),
        speaker_index: Some(si),
        speaker_human_id: None,
    }
}

fn key_speaker_human(ch: i32, si: i32, human_id: &str) -> SegmentKey {
    SegmentKey {
        channel: ChannelProfile::from(ch),
        speaker_index: Some(si),
        speaker_human_id: Some(human_id.to_string()),
    }
}

fn texts(seg: &Segment) -> Vec<&str> {
    seg.words.iter().map(|word| word.text.as_str()).collect()
}

fn is_finals(seg: &Segment) -> Vec<bool> {
    seg.words.iter().map(|word| word.is_final).collect()
}

#[test]
fn empty_input() {
    let result = build_segments(&[], &[], &[], None);
    assert!(result.is_empty());
}

#[test]
fn single_word() {
    let finals = vec![fw("0", 0, 100, 0)];
    let result = build_segments(&finals, &[], &[], None);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key, key(0));
    assert_eq!(texts(&result[0]), vec!["0"]);
    assert_eq!(is_finals(&result[0]), vec![true]);
}

#[test]
fn simple_multi_channel_without_merging() {
    let finals = vec![fw("0", 0, 100, 0)];
    let partials = vec![
        pw("1", 150, 200, 0),
        pw("2", 150, 200, 1),
        pw("3", 210, 260, 1),
    ];
    let result = build_segments(&finals, &partials, &[], None);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key, key(0));
    assert_eq!(texts(&result[0]), vec!["0", "1"]);
    assert_eq!(is_finals(&result[0]), vec![true, false]);
    assert_eq!(result[1].key, key(1));
    assert_eq!(texts(&result[1]), vec!["2", "3"]);
}

/// Umgeschrieben: der Einwurf der Gegenseite unterbricht den Redebeitrag
/// nicht mehr -- beide Woerter von Kanal 0 liegen in einem Block.
#[test]
fn einwurf_zerschneidet_den_redebeitrag_nicht() {
    let finals = vec![fw("0", 300, 400, 1)];
    let partials = vec![pw("1", 0, 100, 0), pw("2", 600, 700, 0)];
    let result = build_segments(&finals, &partials, &[], None);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key, key(0));
    assert_eq!(texts(&result[0]), vec!["1", "2"]);
    assert_eq!(result[1].key, key(1));
    assert_eq!(texts(&result[1]), vec!["0"]);
}

#[test]
fn sorted_by_start_ms() {
    let finals = vec![fw("2", 400, 450, 0)];
    let partials = vec![pw("0", 100, 150, 0), pw("1", 250, 300, 0)];
    let result = build_segments(&finals, &partials, &[], None);
    assert_eq!(result.len(), 1);
    assert_eq!(texts(&result[0]), vec!["0", "1", "2"]);
}

#[test]
fn does_not_merge_past_max_gap() {
    let finals = vec![
        fw("0", 0, 100, 0),
        fw("2", 2101, 2201, 0),
        fw("1", 150, 200, 1),
    ];
    let opts = SegmentBuilderOptions {
        max_gap_ms: Some(2000),
        ..Default::default()
    };
    let result = build_segments(&finals, &[], &[], Some(&opts));
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].key, key(0));
    assert_eq!(texts(&result[0]), vec!["0"]);
    assert_eq!(result[1].key, key(1));
    assert_eq!(texts(&result[1]), vec!["1"]);
    assert_eq!(result[2].key, key(0));
    assert_eq!(texts(&result[2]), vec!["2"]);
}

#[test]
fn merges_at_exact_threshold() {
    let finals = vec![fw("0", 0, 100, 0), fw("1", 2100, 2200, 0)];
    let opts = SegmentBuilderOptions {
        max_gap_ms: Some(2000),
        ..Default::default()
    };
    let result = build_segments(&finals, &[], &[], Some(&opts));
    assert_eq!(result.len(), 1);
    assert_eq!(texts(&result[0]), vec!["0", "1"]);
}

#[test]
fn three_distinct_channels() {
    let finals = vec![
        fw("0", 0, 100, 0),
        fw("1", 150, 250, 1),
        fw("2", 300, 400, 2),
    ];
    let result = build_segments(&finals, &[], &[], None);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].key, key(0));
    assert_eq!(result[1].key, key(1));
    assert_eq!(result[2].key, key(2));
}

#[test]
fn splits_by_speaker_within_channel() {
    let finals = vec![
        fw_si("0", 0, 100, 0, 0),
        fw_si("1", 150, 250, 0, 1),
        fw_si("2", 300, 400, 0, 0),
    ];
    let result = build_segments(&finals, &[], &[], None);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].key, key_speaker(0, 0));
    assert_eq!(result[1].key, key_speaker(0, 1));
    assert_eq!(result[2].key, key_speaker(0, 0));
}

/// Umgeschrieben: aus fuenf abwechselnden Woertern werden zwei Bloecke, nicht
/// fuenf -- genau der Fall, der am echten Gespraech 64 % Zwei-Wort-Schnipsel
/// erzeugte.
#[test]
fn schnelles_hin_und_her_ergibt_zwei_bloecke() {
    let finals = vec![
        fw("0", 0, 100, 0),
        fw("1", 150, 200, 1),
        fw("2", 250, 300, 0),
        fw("3", 350, 400, 1),
        fw("4", 450, 500, 0),
    ];
    let result = build_segments(&finals, &[], &[], None);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key, key(0));
    assert_eq!(texts(&result[0]), vec!["0", "2", "4"]);
    assert_eq!(result[1].key, key(1));
    assert_eq!(texts(&result[1]), vec!["1", "3"]);
}

#[test]
fn propagates_human_id_across_shared_speaker_index() {
    let finals = vec![fw_si("0", 0, 100, 0, 1), fw_si("1", 200, 300, 0, 1)];
    let assignments = vec![speaker_human("alice", ChannelProfile::DirectMic, 1)];
    let result = build_segments(&finals, &[], &assignments, None);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].key,
        SegmentKey {
            channel: ChannelProfile::DirectMic,
            speaker_index: Some(1),
            speaker_human_id: Some("alice".to_string()),
        }
    );
    assert_eq!(texts(&result[0]), vec!["0", "1"]);
}

#[test]
fn does_not_leak_human_id_across_channels_with_same_speaker_index() {
    let finals = vec![
        fw_si("0", 0, 100, 0, 0),
        fw_si("1", 200, 300, 1, 0),
        fw_si("2", 400, 500, 0, 0),
    ];
    let assignments = vec![
        speaker_human("john", ChannelProfile::DirectMic, 0),
        speaker_human("janet", ChannelProfile::RemoteParty, 0),
    ];
    let result = build_segments(&finals, &[], &assignments, None);

    // Umgeschrieben: der Punkt des Tests ist, dass dieselbe Sprecher-Nummer
    // auf zwei Kanaelen NICHT denselben Menschen bedeutet. Das gilt
    // unveraendert -- nur liegen Johns beide Woerter jetzt in einem Block.
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key.speaker_human_id.as_deref(), Some("john"));
    assert_eq!(texts(&result[0]), vec!["0", "2"]);
    assert_eq!(result[1].key.speaker_human_id.as_deref(), Some("janet"));
    assert_eq!(texts(&result[1]), vec!["1"]);
}

#[test]
fn partial_word_inherits_previous_segment_key() {
    let finals = vec![fw("0", 0, 90, 0)];
    let partials = vec![pw("1", 140, 220, 0)];
    let result = build_segments(&finals, &partials, &[], None);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key, key(0));
    assert_eq!(texts(&result[0]), vec!["0", "1"]);
    assert_eq!(is_finals(&result[0]), vec![true, false]);
}

#[test]
fn partial_with_intermittent_speaker_hint_stays_in_previous_segment() {
    let finals = vec![fw_si("0", 0, 100, 0, 0)];
    let partials = vec![pw("1", 150, 250, 0)];
    let result = build_segments(&finals, &partials, &[], None);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key, key_speaker(0, 0));
    assert_eq!(texts(&result[0]), vec!["0", "1"]);
}

/// Umgeschrieben: ein noch laufendes Wort kehrt in den Block seines eigenen
/// Kanals zurueck, statt einen dritten Block zu eroeffnen.
#[test]
fn laufendes_wort_kehrt_in_seinen_kanalblock_zurueck() {
    let finals = vec![fw("0", 0, 100, 0), fw("1", 150, 220, 1)];
    let partials = vec![pw("2", 230, 300, 0)];
    let result = build_segments(&finals, &partials, &[], None);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key, key(0));
    assert_eq!(texts(&result[0]), vec!["0", "2"]);
    assert_eq!(is_finals(&result[0]), vec![true, false]);
    assert_eq!(result[1].key, key(1));
    assert_eq!(texts(&result[1]), vec!["1"]);
}

#[test]
fn custom_max_gap_ms() {
    let finals = vec![
        fw("0", 0, 100, 0),
        fw("1", 500, 600, 0),
        fw("2", 1700, 1800, 0),
    ];
    let opts = SegmentBuilderOptions {
        max_gap_ms: Some(1000),
        ..Default::default()
    };
    let result = build_segments(&finals, &[], &[], Some(&opts));
    assert_eq!(result.len(), 2);
    assert_eq!(texts(&result[0]), vec!["0", "1"]);
    assert_eq!(texts(&result[1]), vec!["2"]);
}

/// Umgeschrieben: die Sprecher-Nummer wird weiter je Kanal geerbt, das
/// Ergebnis sind aber zwei Bloecke statt vier.
#[test]
fn laufende_woerter_erben_die_sprechernummer_ihres_kanals() {
    let finals = vec![fw_si("0", 0, 100, 0, 0), fw_si("1", 150, 250, 1, 1)];
    let partials = vec![pw("2", 300, 400, 0), pw("3", 450, 550, 1)];
    let result = build_segments(&finals, &partials, &[], None);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key, key_speaker(0, 0));
    assert_eq!(texts(&result[0]), vec!["0", "2"]);
    assert_eq!(result[1].key, key_speaker(1, 1));
    assert_eq!(texts(&result[1]), vec!["1", "3"]);
}

/// Umgeschrieben: zeitlich ueberlappende Kanaele ergeben zwei Bloecke, die
/// sich in der Zeit ueberlappen duerfen -- je Kanal einer.
#[test]
fn ueberlappende_kanaele_ergeben_je_kanal_einen_block() {
    let finals = vec![
        fw("0", 0, 100, 0),
        fw("1", 50, 150, 1),
        fw("2", 200, 300, 0),
        fw("3", 250, 350, 1),
    ];
    let result = build_segments(&finals, &[], &[], None);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key, key(0));
    assert_eq!(texts(&result[0]), vec!["0", "2"]);
    assert_eq!(result[1].key, key(1));
    assert_eq!(texts(&result[1]), vec!["1", "3"]);
}

/// Umgeschrieben: die Sprecher-Nummer des Erkenners landet weiter im
/// Schluessel; die beiden Woerter von Kanal 0 liegen jetzt in einem Block.
#[test]
fn auto_assign_based_on_provider_speaker_index() {
    let finals = vec![
        fw_si("0", 0, 100, 0, 0),
        fw_si("1", 100, 200, 1, 1),
        fw_si("2", 200, 300, 0, 0),
    ];
    let result = build_segments(&finals, &[], &[], None);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key, key_speaker(0, 0));
    assert_eq!(texts(&result[0]), vec!["0", "2"]);
    assert_eq!(result[1].key, key_speaker(1, 1));
}

#[test]
fn word_assignment_overrides_only_selected_words() {
    let finals = vec![
        fw_si("0", 0, 100, 1, 2),
        fw_si("1", 100, 200, 1, 2),
        fw_si("2", 200, 300, 1, 2),
        fw_si("3", 300, 400, 1, 2),
    ];
    let assignments = vec![
        speaker_human("alice", ChannelProfile::RemoteParty, 2),
        words_human("bob", &["w-1", "w-2"]),
    ];

    let result = build_segments(&finals, &[], &assignments, None);

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].key, key_speaker_human(1, 2, "alice"));
    assert_eq!(texts(&result[0]), vec!["0"]);
    assert_eq!(result[1].key, key_speaker_human(1, 2, "bob"));
    assert_eq!(texts(&result[1]), vec!["1", "2"]);
    assert_eq!(result[2].key, key_speaker_human(1, 2, "alice"));
    assert_eq!(texts(&result[2]), vec!["3"]);
}

#[test]
fn handles_partial_only_stream_with_speaker_and_assignment() {
    let partials = vec![pw_si("0", 0, 80, 0, 3), pw("1", 120, 200, 0)];
    let assignments = vec![speaker_human("alice", ChannelProfile::DirectMic, 3)];
    let result = build_segments(&[], &partials, &assignments, None);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].key,
        SegmentKey {
            channel: ChannelProfile::DirectMic,
            speaker_index: Some(3),
            speaker_human_id: Some("alice".to_string()),
        }
    );
    assert_eq!(texts(&result[0]), vec!["0", "1"]);
    assert_eq!(is_finals(&result[0]), vec![false, false]);
}

#[test]
fn propagates_direct_mic_channel_identity_forward() {
    let finals = vec![
        fw("0", 0, 100, 0),
        fw("1", 200, 300, 0),
        fw("2", 1200, 1300, 1),
        fw("3", 1500, 1600, 1),
        fw("4", 2601, 2701, 0),
    ];
    let assignments = vec![channel_human("carol", ChannelProfile::DirectMic)];
    let result = build_segments(&finals, &[], &assignments, None);
    // Umgeschrieben: Carols Identitaet gilt weiter fuer den ganzen Kanal --
    // ihre drei Woerter liegen jetzt aber in einem Block, weil die Pause von
    // 2301 ms unter max_gap_ms bleibt.
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key.speaker_human_id.as_deref(), Some("carol"));
    assert_eq!(texts(&result[0]), vec!["0", "1", "4"]);
    assert_eq!(result[1].key, key(1));
}

#[test]
fn applies_direct_mic_channel_identity_to_provider_speakers() {
    let finals = vec![fw_si("0", 0, 100, 0, 2)];
    let assignments = vec![channel_human("self", ChannelProfile::DirectMic)];
    let opts = SegmentBuilderOptions {
        complete_channels: Some(vec![ChannelProfile::DirectMic]),
        ..Default::default()
    };

    let result = build_segments(&finals, &[], &assignments, Some(&opts));

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key, key_speaker_human(0, 2, "self"));
}

#[test]
fn propagates_remote_party_identity_when_channel_marked_complete() {
    let finals = vec![fw("0", 0, 100, 1), fw("1", 200, 300, 1)];
    let assignments = vec![channel_human("remote", ChannelProfile::RemoteParty)];
    let opts = SegmentBuilderOptions {
        complete_channels: Some(vec![ChannelProfile::DirectMic, ChannelProfile::RemoteParty]),
        ..Default::default()
    };
    let result = build_segments(&finals, &[], &assignments, Some(&opts));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key.speaker_human_id.as_deref(), Some("remote"));
}

#[test]
fn partial_word_ignores_its_own_runtime_hint_and_keeps_previous_segment_key() {
    let finals = vec![fw_si("0", 0, 100, 0, 0)];
    let partials = vec![pw_si("1", 150, 250, 0, 1)];
    let result = build_segments(&finals, &partials, &[], None);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key, key_speaker(0, 0));
    assert_eq!(texts(&result[0]), vec!["0", "1"]);
}

/// Bleibt gruen, prueft aber seit der kanalbewussten Block-Bildung etwas
/// anderes als frueher: die beiden Bloecke entstehen jetzt schon in
/// `collect_segments`, die Konsolidierung absorbiert hier nichts mehr.
/// Umbenannt und um seine Gegenprobe ergaenzt: der Test hiess
/// `consolidates_rapid_crosstalk_micro_segments` und schrieb das Ergebnis
/// der Mikro-Konsolidierung zu. Seit die Block-Bildung an den letzten Block
/// DESSELBEN Kanals anhaengt, entsteht dieses Ergebnis schon dort -- der
/// Test war gruen und haette auch das vollstaendige Loeschen der
/// Konsolidierung ueberlebt.
///
/// Er bleibt stehen, weil die Zusage stimmt und wichtig ist: schnelles
/// Durcheinanderreden ergibt zwei Redebeitraege, nicht neun Schnipsel. Nur
/// die Zuschreibung war falsch.
///
/// Die Gegenprobe, die das frueher festhielt (dasselbe Ergebnis mit
/// abgeschalteter Konsolidierung), ist am 03.09.2026 entfallen. Sie
/// verglich zuletzt zwei identische Aufrufe: seit die Konsolidierung
/// gestrichen ist, gibt es in `build_segments` keine zweite Phase mehr, von
/// der sich diese Zusage abgrenzen liesse, und `SegmentBuilderOptions` hat
/// keinen Schalter, der sie abschalten koennte. Ein Vergleich ohne
/// Differenz kann nicht rot werden und haette eine spaeter
/// wiedereingefuehrte Konsolidierung sogar gedeckt. Was die Zusage traegt,
/// prueft der Rumpf unten scharf genug: Blockzahl, beide Schluessel und
/// beide Wortfolgen wortgenau.
#[test]
fn kanalbewusste_block_bildung_haelt_schnelles_durcheinanderreden_zusammen() {
    let finals = vec![
        fw("alright", 78000, 84000, 1),
        fw("mean", 84000, 84500, 0),
        fw("but", 85000, 85200, 1),
        fw("look", 85200, 85400, 0),
        fw("yeah", 85400, 85500, 1),
        fw("everyone", 85500, 86000, 0),
        fw("knows", 86000, 86500, 0),
        fw("the", 86500, 87000, 0),
        fw("truth", 87000, 105000, 0),
    ];
    let opts = SegmentBuilderOptions::default();
    let result = build_segments(&finals, &[], &[], Some(&opts));
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key, key(1));
    assert_eq!(texts(&result[0]), vec!["alright", "but", "yeah"]);
    assert_eq!(result[1].key, key(0));
    assert_eq!(
        texts(&result[1]),
        vec!["mean", "look", "everyone", "knows", "the", "truth"]
    );
}

/// Die Verschaerfung in `propagate_identity`: zwei Bloecke desselben
/// Menschen wachsen nur zusammen, wenn auch die Pause es zulaesst.
///
/// Vorher galt `max_gap_ms` fuer identitaetstragende Schluessel faktisch
/// nicht -- was der Erkenner in zwei Sprecher-Nummern zerlegt hatte, wurde
/// unabhaengig vom Abstand wieder eins. Seit die Block-Bildung kanalbewusst
/// arbeitet, waere daraus "alle Bloecke eines Kanals werden einer" geworden.
///
/// Verloren geht genau dieser Fall: gleicher Mensch, Pause ueber
/// `max_gap_ms`. Das ist gewollt -- eine Pause von fuenf Sekunden ist ein
/// neuer Redebeitrag, auch wenn dieselbe Person weiterspricht. Ohne Test
/// kippt es beim naechsten Umbau still zurueck. Auf echtem Material greift
/// die Stelle null Mal; sie ist nur hier gedeckt.
#[test]
fn gleicher_mensch_ueber_eine_lange_pause_bleibt_zwei_bloecke() {
    let finals = vec![fw_si("0", 0, 100, 2, 0), fw_si("1", 5100, 5200, 2, 1)];
    let assignments = vec![words_human("alice", &["w-0", "w-1"])];
    let result = build_segments(&finals, &[], &assignments, None);

    assert_eq!(
        result.len(),
        2,
        "fuenf Sekunden Pause trennen den Redebeitrag, auch beim selben Menschen"
    );
    assert_eq!(result[0].key.speaker_human_id.as_deref(), Some("alice"));
    assert_eq!(result[1].key.speaker_human_id.as_deref(), Some("alice"));

    // Gegenprobe: dieselben zwei Bloecke innerhalb der Pause wachsen sehr
    // wohl zusammen -- sonst pruefte der Test nur, dass nie etwas
    // zusammengefuehrt wird.
    let nah = vec![fw_si("0", 0, 100, 2, 0), fw_si("1", 150, 250, 2, 1)];
    let nah_assignments = vec![words_human("alice", &["w-0", "w-1"])];
    assert_eq!(
        build_segments(&nah, &[], &nah_assignments, None).len(),
        1,
        "innerhalb der Pause bleibt es ein Redebeitrag"
    );
}
#[test]
fn merges_adjacent_same_human_across_speaker_indexes() {
    let finals = vec![
        fw_si("0", 0, 100, 2, 0),
        fw_si("1", 150, 250, 2, 1),
        fw_si("2", 400, 500, 2, 0),
    ];
    let assignments = vec![words_human("alice", &["w-0", "w-1", "w-2"])];
    let result = build_segments(&finals, &[], &assignments, None);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key.speaker_human_id.as_deref(), Some("alice"));
    assert_eq!(texts(&result[0]), vec!["0", "1", "2"]);
}

#[test]
fn does_not_merge_same_human_across_another_speaker() {
    let finals = vec![
        fw_si("0", 0, 100, 2, 0),
        fw_si("1", 150, 250, 2, 1),
        fw_si("2", 400, 500, 2, 0),
    ];
    let assignments = vec![
        words_human("alice", &["w-0", "w-2"]),
        words_human("bob", &["w-1"]),
    ];
    let result = build_segments(&finals, &[], &assignments, None);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].key.speaker_human_id.as_deref(), Some("alice"));
    assert_eq!(result[1].key.speaker_human_id.as_deref(), Some("bob"));
    assert_eq!(result[2].key.speaker_human_id.as_deref(), Some("alice"));
}

/// Ohne Deckel waere ein Monolog nach der kanalbewussten Block-Bildung EIN
/// Block ueber die ganze Aufnahme -- die Wortzahl bricht ihn auf.
#[test]
fn deckel_teilt_einen_monolog_nach_wortzahl() {
    let finals = (0..25)
        .map(|i| fw(&format!("w{i}"), i * 100, i * 100 + 90, 0))
        .collect::<Vec<_>>();
    let opts = SegmentBuilderOptions {
        max_segment_words: Some(10),
        ..Default::default()
    };
    let result = build_segments(&finals, &[], &[], Some(&opts));
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].words.len(), 10);
    assert_eq!(result[1].words.len(), 10);
    assert_eq!(result[2].words.len(), 5);
    assert_eq!(
        result.iter().map(|seg| seg.words.len()).sum::<usize>(),
        finals.len(),
        "kein Wort darf beim Teilen verloren gehen"
    );
}

/// Zweiter Zahn desselben Deckels: auch ein langsam gesprochener Monolog mit
/// wenigen Woertern darf nicht beliebig lang werden.
#[test]
fn deckel_teilt_einen_monolog_nach_dauer() {
    let finals = vec![
        fw("a", 0, 1000, 0),
        fw("b", 2000, 3000, 0),
        fw("c", 4000, 5000, 0),
        fw("d", 6000, 7000, 0),
    ];
    let opts = SegmentBuilderOptions {
        max_segment_ms: Some(3500),
        ..Default::default()
    };
    let result = build_segments(&finals, &[], &[], Some(&opts));
    // "c" endet bei 5000 ms, das waeren 5000 ms ab Blockbeginn -- ueber dem
    // Deckel von 3500. Der Block schliesst also nach "b".
    assert_eq!(result.len(), 2);
    assert_eq!(texts(&result[0]), vec!["a", "b"]);
    assert_eq!(texts(&result[1]), vec!["c", "d"]);
}

/// Der Deckel greift auch dort, wo zwei Bloecke desselben Menschen wegen
/// verschiedener Sprecher-Nummern erst in `propagate_identity` zusammenkommen.
#[test]
fn deckel_greift_auch_beim_zusammenfuehren_gleicher_menschen() {
    let finals = vec![
        fw_si("0", 0, 100, 2, 0),
        fw_si("1", 150, 250, 2, 1),
        fw_si("2", 400, 500, 2, 0),
    ];
    let assignments = vec![words_human("alice", &["w-0", "w-1", "w-2"])];
    let opts = SegmentBuilderOptions {
        max_segment_words: Some(1),
        ..Default::default()
    };
    let result = build_segments(&finals, &[], &assignments, Some(&opts));
    assert_eq!(result.len(), 3);
    assert_eq!(
        result.iter().map(|seg| seg.words.len()).sum::<usize>(),
        3,
        "kein Wort darf beim Nicht-Zusammenfuehren verloren gehen"
    );
}

/// Die Deckel-Standardwerte stehen hier ausgeschrieben.
///
/// Alle uebrigen Deckel-Tests setzen eigene, kleine Grenzen oder lesen sie
/// aus den Vorgaben, statt sie zu nennen -- ein Vertipper in `Default`
/// (512_000 statt 512) faellt damit nirgends auf, weil sich jeder dieser
/// Tests mitverschiebt. Diese Zeilen sind der eine Ort, an dem die Zahlen
/// selbst stehen. Ihre Herleitung steht bei den Feldern in
/// `types/segment.rs`.
#[test]
fn die_deckel_standardwerte_stehen_fest() {
    let opts = SegmentBuilderOptions::default();
    assert_eq!(
        opts.max_segment_words,
        Some(512),
        "gemessenes Maximum 445 Woerter plus Reserve"
    );
    assert_eq!(
        opts.max_segment_ms,
        Some(300_000),
        "gemessenes Maximum 167 190 ms plus Reserve"
    );
}
