//! Zwei in der ANZEIGE unmittelbar aufeinanderfolgende Bloecke desselben
//! Sprechers werden ein Absatz.
//!
//! Warum das eine eigene Phase ist und nicht in `build_segments` gehoert:
//! die Anzeige-Reihenfolge entsteht dort noch gar nicht. `build_segments`
//! laeuft je Transkript-Zeile; erst `render_transcript_segments` mischt alle
//! Zeilen und sortiert sie nach der Startzeit des ersten Wortes
//! (`render.rs`, `all_segments.sort_by_key`). Vorher ist "Nachbar in der
//! Anzeige" keine definierte Groesse.
//!
//! Warum es sie ueberhaupt braucht: Bloecke haengen am letzten Block
//! DESSELBEN Kanals, begrenzt durch `max_gap_ms` (3000). Waehrend der andere
//! redet, schweigt der eine zwangslaeufig laenger als drei Sekunden -- sein
//! naechster Satz eroeffnet also einen neuen Block. Weil der lange Beitrag
//! des anderen nach Startzeit einsortiert wird, landet er VOR beiden, und die
//! zwei Einwuerfe stehen unmittelbar untereinander: derselbe Name zweimal,
//! ein zerhackter Absatz.
//!
//! Diese Phase haengt `max_gap_ms` NICHT aus. Sie stellt eine strikt
//! staerkere Bedingung als die Pause allein: zwischen den beiden Bloecken
//! darf in der Anzeige nichts stehen. Steht dort ein Block des anderen, wird
//! nicht verschmolzen, egal wie kurz die Pause ist.

use crate::types::{Segment, SegmentBuilderOptions, SegmentKey};

/// Groesste Pause, die ein Absatz ueberbruecken darf.
///
/// Gemessen am 03.09.2026 ueber die beiden Vorlagen unter `tests/fixtures/`
/// (G1 = 3355 Woerter / 23,5 min, G2 = 8207 Woerter / 63,8 min), auf der
/// Anzeige-Reihenfolge und ueber `derselbe_sprecher` -- also ueber genau
/// das Praedikat, das weiter unten verschmilzt, nicht ueber
/// Schluesselgleichheit. Auf diesem Material fallen beide Mengen zusammen
/// (59 = 59 und 119 = 119 Paare, gemessen), weil kein Wort eine
/// Sprecher-Nummer traegt; sobald Diarisierung dazukommt, tun sie es nicht
/// mehr, und dann gilt die hier gemessene Menge.
///
///   G1  59 Anzeige-Nachbarn, Luecke 3,08 s bis 22,43 s
///   G2  119 Anzeige-Nachbarn, Luecke 3,00 s bis 120,03 s
///
/// EHRLICH BENANNT: diese Grenze ruht auf EINEM Gespraech. G1 liegt
/// vollstaendig unter 23 s -- dort gaebe es nichts zu schneiden, jeder Wert
/// ab 25 s ergibt dieselben 84 Bloecke. Nur G2 reicht ueber die Grenze
/// hinaus, und nur dort ist sie ueberhaupt gepruefbar.
///
/// In G2 liegen 117 von 119 Paaren darunter; ausgeschlossen bleiben genau
/// zwei: 61,5 s und 120,0 s. Der Wert sitzt in der Luecke zwischen 34,5 s
/// und 61,5 s (Faktor 1,78) -- dem groessten Sprung der Verteilung
/// unterhalb des Ausreisserpaars; der einzige groessere (61,5 -> 120,0 s,
/// Faktor 1,95) trennt die beiden Ausreisser voneinander und taugt nicht
/// als Grenze.
///
/// Die zweite, unabhaengige Achse bestaetigt denselben Schnitt: zaehlt man
/// die FREMDEN Woerter, die in der Luecke liegen, tragen alle
/// eingeschlossenen Paare zwischen 0 und 89, die beiden ausgeschlossenen
/// 140 und 260. Zwischen 20,3 s und 34,5 s liegt eine homogene Region
/// (35 bis 89 Fremdwoerter) -- jede Grenze darin schnitte mitten hindurch
/// und trennte Gleichartiges. Zwei bis vier Minuten fremde Rede sind kein
/// Absatz, sondern ein neuer Redebeitrag.
///
/// Jeder Wert zwischen 34 523 und 61 546 ms ergibt dasselbe Ergebnis;
/// 40 000 ist der naechstliegende runde. Der frueher hier stehende Wert
/// (30 000, begruendet mit einem Sprung von 29,6 auf 61,5 s) galt nur
/// solange die Mikro-Konsolidierung lief: sie hatte die Paare bei 31,9 /
/// 32,7 / 34,5 s vorher absorbiert. Nach ihrem Wegfall existiert jener
/// Sprung nicht mehr.
///
/// Reproduzieren: `luecken_verteilung_belegt_die_bruecken_grenze` in
/// `tests/block_bildung_echtmaterial.rs`.
pub const MAX_BRUECKE_MS: i64 = 40_000;

/// Faellt der Absatz zusammen, was in der Anzeige nebeneinander steht.
///
/// Erwartet `segments` in Anzeige-Reihenfolge; die Reihenfolge bleibt
/// erhalten, es fallen nur Eintraege weg.
pub fn verschmelze_anzeige_nachbarn(
    segments: &mut Vec<Segment>,
    bruecke_ms: i64,
    options: Option<&SegmentBuilderOptions>,
) {
    let max_words = options
        .and_then(|opts| opts.max_segment_words)
        .unwrap_or(usize::MAX);
    let max_ms = options
        .and_then(|opts| opts.max_segment_ms)
        .unwrap_or(i64::MAX);

    let mut write = 0usize;

    for read in 0..segments.len() {
        let darf = write > 0
            && darf_verschmelzen(
                &segments[write - 1],
                &segments[read],
                bruecke_ms,
                max_words,
                max_ms,
            );

        if darf {
            let words = std::mem::take(&mut segments[read].words);
            segments[write - 1].words.extend(words);
            continue;
        }

        if write != read {
            segments.swap(write, read);
        }
        write += 1;
    }

    segments.truncate(write);
}

fn darf_verschmelzen(
    vorheriger: &Segment,
    naechster: &Segment,
    bruecke_ms: i64,
    max_words: usize,
    max_ms: i64,
) -> bool {
    let (Some(vorheriges_erstes), Some(vorheriges_letztes)) =
        (vorheriger.words.first(), vorheriger.words.last())
    else {
        return false;
    };
    let (Some(naechstes_erstes), Some(naechstes_letztes)) =
        (naechster.words.first(), naechster.words.last())
    else {
        return false;
    };

    if !derselbe_sprecher(&vorheriger.key, &naechster.key) {
        return false;
    }

    // Die Woerter im verschmolzenen Block muessen chronologisch bleiben.
    // Auf beiden Vorlagen greift diese Bedingung nie (0 von 59 und 0 von 119
    // Paaren, gemessen 03.09.2026) -- sie steht fuer den Fall, dass mehrere
    // Transkript-Zeilen gemischt werden und ein frueher beginnender Block
    // spaeter endet. Der Nachweis, dass sie wirkt, laeuft ueber
    // `zwei_transkript_zeilen_verschraenken_die_woerter_nicht` in
    // `render.rs`, das genau diesen Weg ohne Kunstgriff faehrt.
    if vorheriges_letztes.start_ms > naechstes_erstes.start_ms {
        return false;
    }

    if naechstes_erstes.start_ms - vorheriges_letztes.end_ms > bruecke_ms {
        return false;
    }

    if vorheriger.words.len() + naechster.words.len() > max_words {
        return false;
    }

    naechstes_letztes.end_ms - vorheriges_erstes.start_ms <= max_ms
}

/// Derselbe Mensch, oder -- wo kein Mensch bekannt ist -- derselbe Kanal mit
/// derselben Sprecher-Nummer.
///
/// Deckungsgleich mit `should_merge_adjacent_keys` in `collect.rs`: ein Block
/// ohne jede Sprecher-Identitaet wird nicht verschmolzen, sonst wuerde ein
/// Kanal ohne Diarisierung zu einem einzigen Block.
///
/// Oeffentlich, damit die Messung in `tests/block_bildung_echtmaterial.rs`
/// GENAU dieses Praedikat zaehlt statt es nachzubauen. Ein nachgebautes
/// Praedikat misst irgendwann eine andere Menge als der Code verschmilzt --
/// die Begruendung der Bruecken-Grenze haenge dann in der Luft.
pub fn derselbe_sprecher(vorheriger: &SegmentKey, naechster: &SegmentKey) -> bool {
    if vorheriger.channel != naechster.channel {
        return false;
    }

    if let (Some(links), Some(rechts)) = (&vorheriger.speaker_human_id, &naechster.speaker_human_id)
        && links == rechts
    {
        return true;
    }

    vorheriger == naechster && naechster.has_speaker_identity()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChannelProfile, SegmentWord};

    fn wort(start_ms: i64, end_ms: i64, channel: ChannelProfile) -> SegmentWord {
        SegmentWord {
            text: " wort".to_string(),
            start_ms,
            end_ms,
            channel,
            is_final: true,
            id: Some(format!("w{start_ms}")),
        }
    }

    /// Ein Block mit `anzahl` Woertern ab `start_ms`, ein Wort je 100 ms.
    fn block(
        channel: ChannelProfile,
        human: Option<&str>,
        start_ms: i64,
        anzahl: usize,
    ) -> Segment {
        Segment {
            key: SegmentKey {
                channel,
                speaker_index: None,
                speaker_human_id: human.map(str::to_string),
            },
            words: (0..anzahl)
                .map(|i| {
                    let s = start_ms + i as i64 * 100;
                    wort(s, s + 80, channel)
                })
                .collect(),
        }
    }

    fn deckel(max_words: usize, max_ms: i64) -> SegmentBuilderOptions {
        SegmentBuilderOptions {
            max_segment_words: Some(max_words),
            max_segment_ms: Some(max_ms),
            ..Default::default()
        }
    }

    fn woerter_gesamt(segments: &[Segment]) -> usize {
        segments.iter().map(|s| s.words.len()).sum()
    }

    /// Die Kernzusage: zwei Anzeige-Nachbarn desselben Menschen werden EIN
    /// Absatz -- auch wenn die Pause dazwischen `max_gap_ms` reisst, denn
    /// genau deshalb liegen sie ueberhaupt getrennt vor.
    #[test]
    fn anzeige_nachbarn_desselben_menschen_werden_ein_absatz() {
        let mut segmente = vec![
            block(ChannelProfile::DirectMic, Some("mads"), 0, 3),
            block(ChannelProfile::DirectMic, Some("mads"), 6_000, 3),
        ];

        verschmelze_anzeige_nachbarn(&mut segmente, MAX_BRUECKE_MS, None);

        assert_eq!(segmente.len(), 1, "die beiden Bloecke sind ein Absatz");
        assert_eq!(woerter_gesamt(&segmente), 6, "kein Wort geht verloren");
        let starts: Vec<i64> = segmente[0].words.iter().map(|w| w.start_ms).collect();
        assert_eq!(starts, vec![0, 100, 200, 6_000, 6_100, 6_200]);
    }

    /// Drei Nachbarn werden ein Block, nicht zwei.
    #[test]
    fn die_verschmelzung_kettet_ueber_mehrere_nachbarn() {
        let mut segmente = vec![
            block(ChannelProfile::DirectMic, Some("mads"), 0, 2),
            block(ChannelProfile::DirectMic, Some("mads"), 6_000, 2),
            block(ChannelProfile::DirectMic, Some("mads"), 12_000, 2),
        ];

        verschmelze_anzeige_nachbarn(&mut segmente, MAX_BRUECKE_MS, None);

        assert_eq!(segmente.len(), 1);
        assert_eq!(woerter_gesamt(&segmente), 6);
    }

    /// Die Bruecken-Grenze steht hier als Zahl.
    ///
    /// Jeder andere Test setzt einen eigenen, kleinen Wert oder rechnet
    /// relativ zu `MAX_BRUECKE_MS` -- ein Vertipper in der Konstante
    /// (400_000 statt 40_000) verschiebt sie alle mit und faellt dort
    /// nirgends auf. Dieselbe Klasse wie bei den Deckeln, wo
    /// `die_deckel_standardwerte_stehen_fest` genau deshalb existiert.
    ///
    /// Ein Zwischenversuch, den Schwellentest einfach mit `MAX_BRUECKE_MS`
    /// statt mit 5 000 ms zu fahren, half NICHT: er prueft die Schwelle
    /// relativ zur Konstante und ueberlebt den Vertipper. Gemessen am
    /// 03.09.2026 -- er ist deshalb wieder entfallen.
    ///
    /// Zweiter, unabhaengiger Waechter ist
    /// `luecken_verteilung_belegt_die_bruecken_grenze`: bei 400 000 ms
    /// laege kein Paar mehr ueber der Grenze statt zweier.
    #[test]
    fn die_bruecken_grenze_steht_fest() {
        assert_eq!(
            MAX_BRUECKE_MS, 40_000,
            "40 s sitzen in der Luecke zwischen 34,5 s und 61,5 s -- \
             Herleitung im Kommentar an der Konstante"
        );
    }

    /// Die Bruecken-Grenze ist die Grenze -- exakt darauf wird noch
    /// verschmolzen, einen Millisekunde darueber nicht mehr.
    #[test]
    fn die_bruecken_grenze_trennt_genau_am_schwellenwert() {
        // Erster Block endet bei 80 ms, zweiter beginnt bei 80 + Bruecke.
        let auf_der_grenze = {
            let mut s = vec![
                block(ChannelProfile::DirectMic, Some("mads"), 0, 1),
                block(ChannelProfile::DirectMic, Some("mads"), 80 + 5_000, 1),
            ];
            verschmelze_anzeige_nachbarn(&mut s, 5_000, None);
            s.len()
        };
        let einen_ms_darueber = {
            let mut s = vec![
                block(ChannelProfile::DirectMic, Some("mads"), 0, 1),
                block(ChannelProfile::DirectMic, Some("mads"), 81 + 5_000, 1),
            ];
            verschmelze_anzeige_nachbarn(&mut s, 5_000, None);
            s.len()
        };

        assert_eq!(auf_der_grenze, 1, "auf der Grenze wird verschmolzen");
        assert_eq!(einen_ms_darueber, 2, "eine Millisekunde darueber nicht");
    }

    /// Der Unterschied zu `max_gap_ms`: steht in der Anzeige ein Block des
    /// anderen dazwischen, wird NICHT verschmolzen -- egal wie kurz die
    /// Pause ist. Genau das macht diese Phase zu einer Anzeige-Phase und
    /// nicht zu einer heimlichen Aufweichung der Pausenregel.
    #[test]
    fn ein_fremder_block_dazwischen_verhindert_den_absatz() {
        let mut segmente = vec![
            block(ChannelProfile::DirectMic, Some("mads"), 0, 3),
            block(ChannelProfile::RemoteParty, Some("tom"), 400, 3),
            block(ChannelProfile::DirectMic, Some("mads"), 800, 3),
        ];

        verschmelze_anzeige_nachbarn(&mut segmente, MAX_BRUECKE_MS, None);

        assert_eq!(segmente.len(), 3, "der fremde Block trennt");
        assert_eq!(woerter_gesamt(&segmente), 9);
    }

    /// Ein Block traegt nie zwei Kanaele und nie zwei Menschen.
    #[test]
    fn weder_kanal_noch_mensch_werden_vermischt() {
        let mut kanaele = vec![
            block(ChannelProfile::DirectMic, Some("mads"), 0, 2),
            block(ChannelProfile::RemoteParty, Some("mads"), 6_000, 2),
        ];
        verschmelze_anzeige_nachbarn(&mut kanaele, MAX_BRUECKE_MS, None);
        assert_eq!(kanaele.len(), 2, "zwei Kanaele bleiben zwei Bloecke");

        let mut menschen = vec![
            block(ChannelProfile::DirectMic, Some("mads"), 0, 2),
            block(ChannelProfile::DirectMic, Some("tom"), 6_000, 2),
        ];
        verschmelze_anzeige_nachbarn(&mut menschen, MAX_BRUECKE_MS, None);
        assert_eq!(menschen.len(), 2, "zwei Menschen bleiben zwei Bloecke");
    }

    /// Ohne jede Sprecher-Identitaet wird nicht verschmolzen -- sonst wuerde
    /// ein undiarisierter Kanal zu einem einzigen Block.
    #[test]
    fn bloecke_ohne_sprecher_identitaet_bleiben_getrennt() {
        let mut segmente = vec![
            block(ChannelProfile::DirectMic, None, 0, 2),
            block(ChannelProfile::DirectMic, None, 6_000, 2),
        ];

        verschmelze_anzeige_nachbarn(&mut segmente, MAX_BRUECKE_MS, None);

        assert_eq!(segmente.len(), 2);
    }

    /// Der Wort-Deckel gilt auch hier -- und er gilt am exakten
    /// Gleichstand noch.
    ///
    /// Die frueher hier stehende Fassung pruefte nur 5 + 5 gegen einen
    /// Deckel von 9. Damit ueberlebte der Mutant `>` -> `>=`, denn
    /// 10 >= 9 gilt genauso wie 10 > 9. Erst der Fall Summe == Deckel
    /// trennt die beiden.
    #[test]
    fn der_wort_deckel_haelt_auch_am_gleichstand() {
        let bloecke = || {
            vec![
                block(ChannelProfile::DirectMic, Some("mads"), 0, 5),
                block(ChannelProfile::DirectMic, Some("mads"), 6_000, 5),
            ]
        };

        let mut darueber = bloecke();
        verschmelze_anzeige_nachbarn(&mut darueber, MAX_BRUECKE_MS, Some(&deckel(9, i64::MAX)));
        assert_eq!(darueber.len(), 2, "10 Woerter reissen den Deckel von 9");
        assert_eq!(woerter_gesamt(&darueber), 10);

        let mut genau_drauf = bloecke();
        verschmelze_anzeige_nachbarn(
            &mut genau_drauf,
            MAX_BRUECKE_MS,
            Some(&deckel(10, i64::MAX)),
        );
        assert_eq!(
            genau_drauf.len(),
            1,
            "genau auf dem Deckel von 10 wird noch verschmolzen"
        );
        assert_eq!(woerter_gesamt(&genau_drauf), 10);
    }

    /// Der Zeit-Deckel gilt auch hier -- gemessen ueber den GESAMTEN
    /// verschmolzenen Block, vom ersten Wort des ersten bis zum letzten des
    /// zweiten.
    #[test]
    fn der_zeit_deckel_haelt() {
        let mut segmente = vec![
            block(ChannelProfile::DirectMic, Some("mads"), 0, 2),
            block(ChannelProfile::DirectMic, Some("mads"), 6_000, 2),
        ];

        // Der verschmolzene Block liefe von 0 bis 6 180 ms.
        verschmelze_anzeige_nachbarn(
            &mut segmente,
            MAX_BRUECKE_MS,
            Some(&deckel(usize::MAX, 6_179)),
        );
        assert_eq!(segmente.len(), 2, "6 180 ms reissen den Deckel von 6 179");

        let mut knapp_darunter = vec![
            block(ChannelProfile::DirectMic, Some("mads"), 0, 2),
            block(ChannelProfile::DirectMic, Some("mads"), 6_000, 2),
        ];
        verschmelze_anzeige_nachbarn(
            &mut knapp_darunter,
            MAX_BRUECKE_MS,
            Some(&deckel(usize::MAX, 6_180)),
        );
        assert_eq!(knapp_darunter.len(), 1, "genau auf dem Deckel geht es noch");
    }

    /// Woerter duerfen im verschmolzenen Block nicht ausser der Reihe stehen.
    ///
    /// Der Zustand ist im echten Aufrufpfad erreichbar, sobald mehrere
    /// Transkript-Zeilen gemischt werden: `render_transcript_segments` baut
    /// die Bloecke je Zeile und sortiert erst danach nach der Startzeit des
    /// ERSTEN Wortes. Ein langer Block der einen Zeile kann dann vor einem
    /// kurzen der anderen stehen und trotzdem spaeter enden. Der
    /// Integrationstest `zwei_transkript_zeilen_verschraenken_die_woerter_nicht`
    /// in `render.rs` faehrt genau diesen Weg ohne Kunstgriff.
    #[test]
    fn woerter_ausser_der_reihe_werden_nicht_verschmolzen() {
        let mut segmente = vec![
            block(ChannelProfile::DirectMic, Some("mads"), 0, 3),
            block(ChannelProfile::DirectMic, Some("mads"), 100, 1),
        ];

        verschmelze_anzeige_nachbarn(&mut segmente, MAX_BRUECKE_MS, None);

        assert_eq!(
            segmente.len(),
            2,
            "der zweite Block beginnt vor dem letzten Wort des ersten"
        );
    }
}
