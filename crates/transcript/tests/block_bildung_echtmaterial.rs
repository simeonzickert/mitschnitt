//! Block-Bildung, gemessen an zwei echten Gespraechen.
//!
//! Die Vorlagen unter `tests/fixtures/` sind anonymisiert: sie tragen nur
//! Kanal und Wortzeiten, keinen Gespraechsinhalt. Fuer die Block-Bildung
//! reicht das genau aus -- sie liest ausschliesslich Kanal, Startzeit und
//! Endzeit; der Worttext geht nie in eine Entscheidung ein. Damit ist der
//! Falsifikator dauerhaft reproduzierbar, ohne dass ein Gespraech im Repo
//! liegt.
//!
//! Der Stand vor der kanalbewussten Block-Bildung, am 03.09.2026 selbst
//! nachgemessen (die Segment-Quellen aus `9e2447b67d^` in den Baum gelegt
//! und diese Vorlagen durchgerechnet):
//!
//!   G1  288 Bloecke, Median 5,5 Woerter, 32,6 % der Bloecke <= 3 Woerter
//!   G2  386 Bloecke, Median 8,0 Woerter, 35,5 % der Bloecke <= 3 Woerter
//!
//! Der Grund war, dass ein einzelnes "ja" der Gegenseite den laufenden
//! Redebeitrag endgueltig beendete.
//!
//! Die frueher hier stehenden Zahlen (304 / 436, Median 6 / 8) reproduzieren
//! nicht und sind ersetzt; sie stammten aus einer Zwischenfassung. Die
//! Commit-Botschaft von `9e2447b67d` nennt fuer G1 288 und trifft damit,
//! fuer G2 nennt sie 436 und liegt daneben -- gemessen sind 386.

use transcript::{
    ChannelProfile, FinalizedWord, MAX_BRUECKE_MS, RenderTranscriptHuman, RenderTranscriptInput,
    RenderTranscriptRequest, RenderTranscriptWordInput, RenderedTranscriptSegment, Segment,
    WordState, build_segments, channel_assignments_for_participants, derselbe_sprecher,
    render_transcript_segments, segment_options_for_participants,
};

const SELF_HUMAN_ID: &str = "human-selbst";
const REMOTE_HUMAN_ID: &str = "human-gegenueber";

/// Obergrenzen aus `SegmentBuilderOptions::default()`. Wird dort geschraubt,
/// muss diese Messung mitgezogen werden -- deshalb steht der Wert hier
/// ausgeschrieben und wird nicht importiert.
const DECKEL_WOERTER: usize = 512;
const DECKEL_MS: i64 = 300_000;

struct Vorlage {
    name: &'static str,
    woerter: Vec<RenderTranscriptWordInput>,
}

fn lade(name: &'static str, inhalt: &str) -> Vorlage {
    let woerter = inhalt
        .lines()
        .filter(|zeile| !zeile.starts_with('#') && !zeile.trim().is_empty())
        .enumerate()
        .map(|(index, zeile)| {
            let mut spalten = zeile.split('\t');
            let kanal: i32 = spalten
                .next()
                .expect("Kanal fehlt")
                .parse()
                .expect("Kanal ist keine Zahl");
            let start_ms: i64 = spalten
                .next()
                .expect("Startzeit fehlt")
                .parse()
                .expect("Startzeit ist keine Zahl");
            let end_ms: i64 = spalten
                .next()
                .expect("Endzeit fehlt")
                .parse()
                .expect("Endzeit ist keine Zahl");

            RenderTranscriptWordInput {
                id: format!("w{index}"),
                // Der Text ist bewusst ein Platzhalter: die Block-Bildung
                // liest ihn nie, und das Rendern verlangt nur, dass er nicht
                // leer ist.
                text: " wort".to_string(),
                start_ms,
                end_ms,
                channel: kanal,
                speaker_index: None,
            }
        })
        .collect::<Vec<_>>();

    assert!(
        !woerter.is_empty(),
        "Vorlage {name} ist leer -- fehlt die Datei unter tests/fixtures/?"
    );

    Vorlage { name, woerter }
}

fn vorlagen() -> Vec<Vorlage> {
    vec![
        lade("G1", include_str!("fixtures/gespraech-g1.tsv")),
        lade("G2", include_str!("fixtures/gespraech-g2.tsv")),
    ]
}

/// Baut die Bloecke ueber `build_segments` mit den PRODUKTIONS-Vorgaben.
fn baue(vorlage: &Vorlage) -> Vec<Segment> {
    let ids = vec![SELF_HUMAN_ID.to_string(), REMOTE_HUMAN_ID.to_string()];
    let options = segment_options_for_participants(&ids, Some(SELF_HUMAN_ID));

    let finals = vorlage
        .woerter
        .iter()
        .map(|wort| FinalizedWord {
            id: wort.id.clone(),
            text: wort.text.clone(),
            start_ms: wort.start_ms,
            end_ms: wort.end_ms,
            channel: wort.channel,
            state: WordState::Final,
            speaker_index: wort.speaker_index,
        })
        .collect::<Vec<_>>();
    let assignments = channel_assignments_for_participants(&ids, Some(SELF_HUMAN_ID));

    build_segments(&finals, &[], &assignments, Some(&options))
}

fn rendere(vorlage: &Vorlage) -> Vec<RenderedTranscriptSegment> {
    render_transcript_segments(RenderTranscriptRequest {
        transcripts: vec![RenderTranscriptInput {
            started_at: Some(0),
            words: vorlage.woerter.clone(),
            assignments: Vec::new(),
        }],
        participant_human_ids: vec![SELF_HUMAN_ID.to_string(), REMOTE_HUMAN_ID.to_string()],
        self_human_id: Some(SELF_HUMAN_ID.to_string()),
        humans: vec![
            RenderTranscriptHuman {
                human_id: SELF_HUMAN_ID.to_string(),
                name: "Selbst".to_string(),
            },
            RenderTranscriptHuman {
                human_id: REMOTE_HUMAN_ID.to_string(),
                name: "Gegenueber".to_string(),
            },
        ],
    })
}

fn median(mut werte: Vec<usize>) -> f64 {
    werte.sort_unstable();
    let mitte = werte.len() / 2;
    if werte.len().is_multiple_of(2) {
        (werte[mitte - 1] + werte[mitte]) as f64 / 2.0
    } else {
        werte[mitte] as f64
    }
}

/// Der Kern-Falsifikator: kein Wort geht verloren, und die Bloecke sind
/// ganze Redebeitraege statt Schnipsel.
///
/// Die Schranken stehen dicht an den am 03.09.2026 gemessenen Ist-Werten,
/// mit 10 % Reserve auf der Blockzahl:
///
///        Bloecke  Median  <= 3 Woerter
///   G1       84      28         4,8 %
///   G2      179      27        15,6 %
///
/// Zehn Prozent sind bewusst knapp gewaehlt. Sie lassen normale Nacharbeit
/// an den Schwellen durch, fangen aber den Fall, auf den es hier ankommt.
/// Neu hergeleitet am 03.09.2026, NACH dem Wegfall der
/// Mikro-Konsolidierung -- die alten Schranken sind nicht fortgeschrieben,
/// sondern aus diesen beiden Messungen abgeleitet:
///
///                          Bloecke   Median   <= 3 Woerter
///   voll                  84 / 179   28 / 27   4,8 / 15,6 %
///   ohne Absatz-Phase    143 / 296   15 / 10  24,5 / 34,8 %
///
/// Faellt die Absatz-Phase aus, steigt die Blockzahl um 70 % (G1) und 65 %
/// (G2) und der Anteil sehr kurzer Bloecke verfuenffacht sich -- jede der
/// drei Schranken faengt das mit Abstand.
///
/// Die Mikro-Konsolidierung, die frueher zwischen beiden Phasen lief, ist
/// am 03.09.2026 auf des Betreibers Entscheid gestrichen worden. Die Messung war
/// die Grundlage: mit ihr kam G1 auf dieselben 84 Bloecke, G2 auf 182 statt
/// 179 -- die Absatz-Phase raeumt eine Ebene spaeter dasselbe ab.
///
/// Die schaerfste der drei Kennzahlen ist der Anteil sehr kurzer Bloecke:
/// er misst genau das, was die Umbauten versprochen haben (vor der
/// kanalbewussten Block-Bildung 32,6 % / 35,5 %), und reagiert frueher als
/// der Median.
#[test]
fn bloecke_sind_ganze_redebeitraege() {
    struct Schranken {
        name: &'static str,
        max_bloecke: usize,
        min_median: f64,
        max_kurzanteil: f64,
    }

    let schranken = [
        Schranken {
            name: "G1",
            max_bloecke: 92,
            min_median: 25.0,
            max_kurzanteil: 7.0,
        },
        Schranken {
            name: "G2",
            max_bloecke: 197,
            min_median: 23.0,
            max_kurzanteil: 17.0,
        },
    ];

    for (vorlage, schranke) in vorlagen().iter().zip(schranken) {
        assert_eq!(vorlage.name, schranke.name, "Vorlagen-Reihenfolge kippt");

        let segmente = rendere(vorlage);
        let woerter_je_block = segmente
            .iter()
            .map(|segment| segment.words.len())
            .collect::<Vec<_>>();
        let woerter_gesamt: usize = woerter_je_block.iter().sum();
        let median_woerter = median(woerter_je_block.clone());
        let kurzanteil = 100.0 * woerter_je_block.iter().filter(|&&n| n <= 3).count() as f64
            / woerter_je_block.len() as f64;

        assert_eq!(
            woerter_gesamt,
            vorlage.woerter.len(),
            "{}: Woerter gingen verloren oder kamen dazu ({} rein, {} raus)",
            vorlage.name,
            vorlage.woerter.len(),
            woerter_gesamt
        );

        assert!(
            segmente.len() <= schranke.max_bloecke,
            "{}: {} Bloecke, erlaubt sind hoechstens {}",
            vorlage.name,
            segmente.len(),
            schranke.max_bloecke
        );

        assert!(
            median_woerter >= schranke.min_median,
            "{}: Median {} Woerter je Block, verlangt sind mindestens {}",
            vorlage.name,
            median_woerter,
            schranke.min_median
        );

        assert!(
            kurzanteil <= schranke.max_kurzanteil,
            "{}: {:.1} % der Bloecke haben hoechstens drei Woerter, erlaubt sind {} %",
            vorlage.name,
            kurzanteil,
            schranke.max_kurzanteil
        );
    }
}

/// Die Anzeige-Reihenfolge OHNE die Absatz-Phase -- also der Stand, den
/// `render_transcript_segments` bis zur Sortierzeile herstellt.
fn anzeige_ohne_absatz_phase(vorlage: &Vorlage) -> Vec<Segment> {
    let mut segmente = baue(vorlage);
    segmente.sort_by_key(|s| s.words.first().map(|w| w.start_ms).unwrap_or(i64::MAX));
    segmente
}

/// Die Absatz-Phase muss WIRKEN, nicht nur vorhanden sein: dieselbe Eingabe
/// laeuft zweimal, einmal mit und einmal ohne, und die Differenz muss von
/// null verschieden sein.
///
/// Gemessen am 03.09.2026, NACH dem Wegfall der Mikro-Konsolidierung:
/// G1 143 -> 84 Bloecke, G2 296 -> 179. Die frueher hier stehenden Zahlen
/// (124 -> 84 und 244 -> 179) galten fuer den Stand MIT Konsolidierung und
/// reproduzieren nicht mehr.
#[test]
fn die_absatz_phase_wirkt() {
    for vorlage in vorlagen() {
        let ohne = anzeige_ohne_absatz_phase(&vorlage);
        let mit = rendere(&vorlage);

        assert!(
            mit.len() < ohne.len(),
            "{}: die Absatz-Phase legt nichts zusammen ({} Bloecke mit, {} ohne)",
            vorlage.name,
            mit.len(),
            ohne.len()
        );

        let woerter: usize = mit.iter().map(|segment| segment.words.len()).sum();
        assert_eq!(
            woerter,
            vorlage.woerter.len(),
            "{}: die Absatz-Phase verliert Woerter",
            vorlage.name
        );
    }
}

/// Kein Block traegt Woerter ausser der Reihe.
///
/// Der Falsifikator, den die Absatz-Phase am ehesten reissen koennte: sie
/// haengt zwei Wortketten aneinander, und wenn die zweite frueher beginnt
/// als die erste endet, steht das Ergebnis durcheinander.
#[test]
fn woerter_stehen_in_jedem_block_chronologisch() {
    for vorlage in vorlagen() {
        for segment in rendere(&vorlage) {
            for paar in segment.words.windows(2) {
                assert!(
                    paar[0].start_ms <= paar[1].start_ms,
                    "{}: Wort bei {} ms steht vor Wort bei {} ms im selben Block",
                    vorlage.name,
                    paar[0].start_ms,
                    paar[1].start_ms
                );
            }
        }
    }
}

/// Belegt die Zahlen, mit denen `MAX_BRUECKE_MS` begruendet ist.
///
/// Der Kommentar an der Konstante behauptet eine Verteilung. Ohne diesen
/// Test waere das eine gespeicherte Praemisse, die in drei Wochen niemand
/// mehr nachrechnet -- hier rechnet sie das Material selbst nach.
///
/// Gezaehlt wird ueber `derselbe_sprecher`, also ueber das Praedikat, das
/// im Code tatsaechlich verschmilzt. Frueher stand hier
/// `paar[0].key == paar[1].key`: das ist eine ANDERE Menge (gleiche
/// `speaker_human_id` bei ungleicher `speaker_index` verschmilzt, ist aber
/// keine Schluesselgleichheit), und die Begruendung der Grenze haette sich
/// von ihr wegbewegen koennen, ohne dass es auffaellt. Auf diesem Material
/// fallen beide Mengen zusammen -- das prueft der Test gleich mit, damit
/// der Tag sichtbar wird, an dem sie es nicht mehr tun.
#[test]
fn luecken_verteilung_belegt_die_bruecken_grenze() {
    struct Erwartung {
        name: &'static str,
        paare: usize,
        min_luecke: i64,
        max_luecke: i64,
        ueber_der_grenze: Vec<i64>,
        /// Hoechste Zahl fremder Woerter in einer Luecke, die noch
        /// ueberbrueckt wird -- und niedrigste in einer, die es nicht mehr
        /// wird. Die zweite Achse der Grenz-Begruendung.
        max_fremdwoerter_drinnen: usize,
        min_fremdwoerter_draussen: Option<usize>,
    }

    let erwartungen = [
        Erwartung {
            name: "G1",
            paare: 59,
            min_luecke: 3_082,
            max_luecke: 22_431,
            ueber_der_grenze: vec![],
            max_fremdwoerter_drinnen: 87,
            min_fremdwoerter_draussen: None,
        },
        Erwartung {
            name: "G2",
            paare: 119,
            min_luecke: 3_002,
            max_luecke: 120_026,
            ueber_der_grenze: vec![61_546, 120_026],
            max_fremdwoerter_drinnen: 89,
            min_fremdwoerter_draussen: Some(140),
        },
    ];

    for (vorlage, erwartung) in vorlagen().iter().zip(erwartungen) {
        assert_eq!(vorlage.name, erwartung.name, "Vorlagen-Reihenfolge kippt");

        let segmente = anzeige_ohne_absatz_phase(vorlage);
        let luecke = |paar: &[Segment]| {
            paar[1].words.first().unwrap().start_ms - paar[0].words.last().unwrap().end_ms
        };

        let mut luecken: Vec<i64> = segmente
            .windows(2)
            .filter(|paar| derselbe_sprecher(&paar[0].key, &paar[1].key))
            .map(luecke)
            .collect();
        luecken.sort_unstable();

        let mut per_schluessel: Vec<i64> = segmente
            .windows(2)
            .filter(|paar| paar[0].key == paar[1].key)
            .map(luecke)
            .collect();
        per_schluessel.sort_unstable();
        assert_eq!(
            luecken, per_schluessel,
            "{}: Merge-Praedikat und Schluesselgleichheit erfassen nicht mehr dieselbe Menge -- \
             die Begruendung von MAX_BRUECKE_MS muss ueber `derselbe_sprecher` neu gemessen werden",
            vorlage.name
        );

        assert_eq!(
            luecken.len(),
            erwartung.paare,
            "{}: {} Anzeige-Nachbarn desselben Sprechers statt {}",
            vorlage.name,
            luecken.len(),
            erwartung.paare
        );
        assert_eq!(
            luecken[0], erwartung.min_luecke,
            "{}: kleinste Luecke",
            vorlage.name
        );
        assert_eq!(
            luecken[luecken.len() - 1],
            erwartung.max_luecke,
            "{}: groesste Luecke",
            vorlage.name
        );

        let darueber: Vec<i64> = luecken
            .iter()
            .copied()
            .filter(|&g| g > MAX_BRUECKE_MS)
            .collect();
        assert_eq!(
            darueber, erwartung.ueber_der_grenze,
            "{}: andere Paare liegen ueber der Bruecken-Grenze als begruendet",
            vorlage.name
        );

        // Zweite Achse: was liegt inhaltlich in der Luecke?
        //
        // Der Kommentar an `MAX_BRUECKE_MS` begruendet die Grenze nicht nur
        // ueber die Zeitverteilung, sondern auch darueber, dass die
        // ueberbrueckten Luecken hoechstens einen normalen Redebeitrag der
        // Gegenseite enthalten, die ausgeschlossenen dagegen ein Vielfaches.
        // Ohne diese Pruefung waere das eine gespeicherte Praemisse.
        let fremdwoerter = |von: i64, bis: i64, kanal| {
            segmente
                .iter()
                .filter(|s| s.key.channel != kanal)
                .flat_map(|s| s.words.iter())
                .filter(|w| w.start_ms >= von && w.start_ms < bis)
                .count()
        };

        let mut drinnen: Vec<usize> = Vec::new();
        let mut draussen: Vec<usize> = Vec::new();
        for paar in segmente.windows(2) {
            if !derselbe_sprecher(&paar[0].key, &paar[1].key) {
                continue;
            }
            let von = paar[0].words.last().unwrap().end_ms;
            let bis = paar[1].words.first().unwrap().start_ms;
            let fremd = fremdwoerter(von, bis, paar[0].key.channel);
            if bis - von > MAX_BRUECKE_MS {
                draussen.push(fremd);
            } else {
                drinnen.push(fremd);
            }
        }

        assert_eq!(
            drinnen.iter().copied().max(),
            Some(erwartung.max_fremdwoerter_drinnen),
            "{}: die ueberbrueckten Luecken tragen andere Mengen fremder Rede als begruendet",
            vorlage.name
        );
        assert_eq!(
            draussen.iter().copied().min(),
            erwartung.min_fremdwoerter_draussen,
            "{}: die ausgeschlossenen Luecken tragen andere Mengen fremder Rede als begruendet",
            vorlage.name
        );
        if let Some(min_draussen) = erwartung.min_fremdwoerter_draussen {
            assert!(
                erwartung.max_fremdwoerter_drinnen < min_draussen,
                "{}: drinnen und draussen ueberlappen bei der fremden Rede -- \
                 die zweite Achse traegt die Grenze dann nicht",
                vorlage.name
            );
        }

        // Die Grenze soll in einer echten Luecke der Verteilung sitzen,
        // nicht mitten in einer dichten Region.
        //
        // Gewertet werden nur Spruenge, deren UNTERE Seite noch unter der
        // Grenze liegt: der groesste Sprung ueberhaupt (61,5 -> 120,0 s)
        // trennt die beiden Ausreisser voneinander und taugt nicht als
        // Schnitt, weil beide Seiten ohnehin ausgeschlossen sind.
        if !erwartung.ueber_der_grenze.is_empty() {
            let (index, _) = luecken
                .windows(2)
                .enumerate()
                .filter(|(_, paar)| paar[0] < MAX_BRUECKE_MS)
                .max_by(|a, b| {
                    let quote = |p: &[i64]| p[1] as f64 / p[0] as f64;
                    quote(a.1).total_cmp(&quote(b.1))
                })
                .expect("mindestens ein Sprung unterhalb der Grenze");
            assert!(
                luecken[index + 1] > MAX_BRUECKE_MS,
                "{}: der groesste Sprung unterhalb der Grenze liegt zwischen {} und {} ms -- \
                 die Grenze {} sitzt nicht darin, sondern schneidet eine dichte Region",
                vorlage.name,
                luecken[index],
                luecken[index + 1],
                MAX_BRUECKE_MS
            );
        }
    }
}

/// Rauchmelder, kein Deckel-Nachweis: auf DIESEM Material greifen die
/// Standard-Deckel nie -- der laengste echte Block liegt bei 445 Woertern
/// und 167 190 ms (G1) beziehungsweise 298 Woertern und 144 086 ms (G2), der
/// Deckel bei 512 Woertern und 300 000 ms. Der Test waere auch mit einem
/// kaputten Deckel gruen.
///
/// Den echten Nachweis fuehren `deckel_teilt_einen_monolog_nach_wortzahl`,
/// `..._nach_dauer` und `deckel_greift_auch_beim_zusammenfuehren_gleicher_menschen`
/// in `segments/tests.rs` sowie `der_wort_deckel_haelt_auch_am_gleichstand`
/// und `der_zeit_deckel_haelt` in `segments/anzeige.rs` -- alle an
/// synthetischer Eingabe, die die Deckel wirklich
/// reisst. Hier bleibt der Test stehen, weil er billig ist und anschlaegt,
/// falls sich das Material oder die Kette einmal grundlegend aendert.
///
/// 300 000 ms sind fuenf Minuten, nicht zehn -- der frueher hier stehende
/// Satz war falsch.
#[test]
fn kein_block_ueber_dem_deckel() {
    for vorlage in vorlagen() {
        for segment in rendere(&vorlage) {
            assert!(
                segment.words.len() <= DECKEL_WOERTER,
                "{}: Block mit {} Woertern ueber dem Deckel {}",
                vorlage.name,
                segment.words.len(),
                DECKEL_WOERTER
            );
            assert!(
                segment.end_ms - segment.start_ms <= DECKEL_MS,
                "{}: Block ueber {} ms lang ({} ms)",
                vorlage.name,
                DECKEL_MS,
                segment.end_ms - segment.start_ms
            );
        }
    }
}

/// Bloecke ueberlappen sich nach der Aenderung in der Zeit -- ihre
/// Reihenfolge muss trotzdem der Startzeit folgen, sonst springt die Anzeige.
///
/// Ehrlich benannt: geprueft wird damit die Zusage von
/// `render_transcript_segments`, das am Ende selbst nach Startzeit sortiert,
/// nicht die der Block-Bildung. Der Test waere auch mit dem alten Code
/// gruen. Er bewacht die Sortierzeile, die jemand streichen koennte.
#[test]
fn bloecke_bleiben_nach_startzeit_sortiert() {
    for vorlage in vorlagen() {
        let segmente = rendere(&vorlage);
        for paar in segmente.windows(2) {
            assert!(
                paar[0].start_ms <= paar[1].start_ms,
                "{}: Block bei {} ms steht vor Block bei {} ms",
                vorlage.name,
                paar[0].start_ms,
                paar[1].start_ms
            );
        }
    }
}

/// Was die Block-Bildung wirklich zusichert -- und was sie ausdruecklich
/// NICHT zusichert.
///
/// ZUGESICHERT: je Kanal kommen genau dieselben Woerter in genau derselben
/// Reihenfolge heraus, die hineingegangen sind. Kennung, Text, Start, Ende
/// und Kanal jedes Wortes sind unveraendert; keines faellt weg, keines
/// kommt dazu, keines wechselt seinen Platz in der Kette seines Kanals.
///
/// NICHT ZUGESICHERT: die Chronologie ueber beide Kanaele hinweg. Weil ein
/// Wort an den letzten Block DESSELBEN Kanals anhaengt, ueberlappen sich
/// Bloecke in der Zeit -- waehrend die eine Seite spricht, wirft die andere
/// ein. Liest man das Ergebnis als eine einzige Kette, springt die
/// Wortstartzeit an den Blockgrenzen zurueck: am 03.09.2026 gemessen 47 Mal
/// (G1) und 75 Mal (G2). Das ist die gewollte Folge der Aenderung, kein
/// Fehler. Die Zeitmarke am Blockanfang (`start_label`) ist die Antwort
/// darauf, nicht eine Sortierung, die es wieder verschraenkt.
///
/// Die frueher hier gefuehrte Zusage "die Wortreihenfolge wird NULL
/// veraendert" stimmte global also nicht; kanalintern stimmt sie, und genau
/// das prueft dieser Test jetzt nach.
#[test]
fn je_kanal_bleiben_die_woerter_unveraendert() {
    for vorlage in vorlagen() {
        let segmente = baue(&vorlage);

        for kanal in [ChannelProfile::DirectMic, ChannelProfile::RemoteParty] {
            let erwartet = vorlage
                .woerter
                .iter()
                .filter(|wort| ChannelProfile::from(wort.channel) == kanal)
                .map(|wort| {
                    (
                        Some(wort.id.clone()),
                        wort.text.clone(),
                        wort.start_ms,
                        wort.end_ms,
                    )
                })
                .collect::<Vec<_>>();

            let bekommen = segmente
                .iter()
                .filter(|segment| segment.key.channel == kanal)
                .flat_map(|segment| segment.words.iter())
                .map(|wort| {
                    (
                        wort.id.clone(),
                        wort.text.clone(),
                        wort.start_ms,
                        wort.end_ms,
                    )
                })
                .collect::<Vec<_>>();

            assert_eq!(
                bekommen.len(),
                erwartet.len(),
                "{}: Kanal {:?} hat {} Woerter statt {}",
                vorlage.name,
                kanal,
                bekommen.len(),
                erwartet.len()
            );
            assert_eq!(
                bekommen, erwartet,
                "{}: Kanal {:?} -- Woerter, Zeiten oder ihre Reihenfolge haben sich geaendert",
                vorlage.name, kanal
            );
        }
    }
}

/// Ein Block gehoert genau einem Kanal -- die Zuordnung darf beim
/// Zusammenfassen nicht verrutschen.
///
/// Ehrlich benannt: der Kanal ist Teil des `SegmentKey`, und jedes Wort
/// haengt sich nur an einen Block mit passendem Schluessel. Die Zusage ist
/// damit weitgehend baulich wahr und dieser Test schwer rot zu bekommen. Er
/// bewacht den Tag, an dem jemand den Kanal aus dem Schluessel nimmt.
#[test]
fn ein_block_traegt_genau_einen_kanal() {
    for vorlage in vorlagen() {
        for segment in rendere(&vorlage) {
            for wort in &segment.words {
                assert_eq!(
                    wort.channel, segment.key.channel,
                    "{}: Wort aus einem fremden Kanal im Block",
                    vorlage.name
                );
            }
        }
    }
}
