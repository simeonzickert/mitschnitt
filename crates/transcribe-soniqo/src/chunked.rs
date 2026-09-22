//! Sprecher ueber Abschnittsgrenzen hinweg zusammenfuehren.
//!
//! **Warum es diese Datei gibt.** Die Sprechertrennung laeuft bis zum
//! 10.09.2026 genau einmal ueber einen ganzen Kanal. Das hat zwei Preise: die
//! teuerste Stufe (agglomeratives Clustern ueber die Einbettungen) waechst
//! quadratisch mit der Laenge, und oberhalb von etwa 150 Minuten stirbt der
//! PROZESS an einer CoreML-Zuteilung, die weder Swift noch Rust abfangen kann
//! (Belege bei `SONIQO_DIARIZATION_MAX_SAMPLES`). Deshalb galt ein Deckel von
//! 120 Minuten -- und eine echte Raumaufnahme vom 10.09. lag mit 122,8 Minuten
//! zwei Minuten und 47 Sekunden darueber und verlor seine komplette
//! Sprecherzuordnung.
//!
//! Wer den Deckel nur anhebt, verschiebt die Klippe und trifft sie spaeter.
//! Diese Datei nimmt ihr stattdessen die Grundlage: der Kanal wird in
//! Abschnitte zerlegt, die dauerhaft weit unter der Klippe liegen, jeder
//! Abschnitt einzeln getrennt, und danach werden die Sprecher wieder
//! zusammengefuehrt.
//!
//! **Das Zusammenfuehren raet nicht.** Das Verfahren liefert je Sprecher einen
//! 256-stelligen Stimm-Schwerpunkt (WeSpeaker-Einbettung, in
//! `DiarizationResult.speakerEmbeddings`). Zwei Abschnitte gehoeren demselben
//! Menschen, wenn ihre Schwerpunkte zueinander zeigen -- gemessen als
//! Kosinus-Aehnlichkeit, nicht geschaetzt aus Reihenfolge oder Redeanteil.
//!
//! Die Schwerpunkte kamen bis heute nie in Rust an: die Bruecke bekommt sie
//! vom Verfahren und liess sie in `encodeDiarizationJSON` einfach liegen.

use crate::{DiarizationSegment, Error, Result, SoniqoModel};

/// Abtastrate, mit der die Trennung rechnet.
///
/// Die Bruecke gibt sie nicht als Parameter herein, sondern setzt sie fest
/// (`soniqoFileTranscriptionSampleRate` in `swift-lib/src/lib.swift`). Wer sie
/// dort aendert, aendert sie hier mit -- sonst rutschen alle Zeiten.
pub const DIARIZATION_SAMPLE_RATE: u32 = 16_000;

/// Vorgabe fuer die Laenge EINES Abschnitts: 45 Minuten.
///
/// Weit unter der letzten Laenge, die ungeteilt nachweislich durchlief (120
/// min), und noch weiter unter der ersten, die den Prozess abgeschossen hat
/// (150 min). Die Begruendung mit der vollstaendigen Messreihe steht bei
/// `SONIQO_DIARIZATION_MAX_SAMPLES` in listener2-core.
pub const DEFAULT_CHUNK_SAMPLES: usize = DIARIZATION_SAMPLE_RATE as usize * 45 * 60;

/// Vorgabe fuer das kuerzeste allein getrennte Reststueck: 5 Minuten.
pub const DEFAULT_MIN_TAIL_SAMPLES: usize = DIARIZATION_SAMPLE_RATE as usize * 5 * 60;

/// Groesste Abschnittslaenge, die dieses Crate ueberhaupt zulaesst: 60 Minuten.
///
/// Der Absturzschutz gehoert hierher und nicht zum Aufrufer. Vorher war die
/// Abschnittslaenge ein nackter `usize` aus fremder Hand: `0` schaltete die
/// Zerlegung ganz ab, und ein grosser Wert liess wieder einen
/// Mehrstundenkanal in EINEN CoreML-Aufruf laufen -- also genau die Klippe,
/// gegen die diese Datei gebaut wurde. Eine Zusage, die der Aufrufer
/// versehentlich aufheben kann, ist keine.
///
/// 60 Minuten ist bewusst grosszuegiger als die Vorgabe (45) und bleibt
/// deutlich unter der letzten bestandenen Messung (120) und weit unter der
/// ersten gescheiterten (150).
pub const MAX_CHUNK_SAMPLES: usize = DIARIZATION_SAMPLE_RATE as usize * 60 * 60;

/// Laenge eines Stimm-Schwerpunkts.
///
/// WeSpeaker liefert 256 Werte. Steht hier als Zahl, weil sie geprueft wird:
/// eine abweichende Laenge ist keine Stimme, sondern ein Bruch in der Bruecke.
pub const EMBEDDING_DIMENSION: usize = 256;

/// Die geprueften Vorgaben fuer einen zerlegten Lauf.
///
/// Zwei nackte `usize` hintereinander (`chunk_samples`, `min_tail_samples`)
/// waren vertauschbar, ohne dass der Compiler etwas sagt -- aus 45 Minuten
/// waeren still 5 geworden. Sie reisen deshalb als benannte Einheit, und die
/// Einheit gibt es nur geprueft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPlan {
    chunk_samples: usize,
    min_tail_samples: usize,
}

impl ChunkPlan {
    /// Prueft die Vorgaben und lehnt ab, statt still etwas anderes zu tun.
    pub fn new(chunk_samples: usize, min_tail_samples: usize) -> Result<Self> {
        if chunk_samples == 0 {
            return Err(Error::Bridge(
                "chunk length must be greater than zero; a zero length would silently \
                 disable chunking and send the whole channel into one CoreML call"
                    .to_string(),
            ));
        }
        if chunk_samples > MAX_CHUNK_SAMPLES {
            return Err(Error::Bridge(format!(
                "chunk length {chunk_samples} exceeds the safe maximum of {MAX_CHUNK_SAMPLES} \
                 samples ({} minutes)",
                MAX_CHUNK_SAMPLES / DIARIZATION_SAMPLE_RATE as usize / 60
            )));
        }
        if min_tail_samples >= chunk_samples {
            return Err(Error::Bridge(format!(
                "minimum tail {min_tail_samples} must stay below the chunk length \
                 {chunk_samples}; otherwise every remainder is appended and the first \
                 chunk swallows the whole channel"
            )));
        }
        // Der LAENGSTE Abschnitt ist nicht `chunk_samples`, sondern
        // `chunk_samples` plus das angehaengte Reststueck (hoechstens
        // `min_tail_samples - 1`). Ohne diese Zeile waere ein Plan aus 60 und
        // 59 Minuten gueltig und erzeugte einen Abschnitt von fast zwei
        // Stunden -- der dann erst am nachgelagerten Waechter scheitert,
        // nachdem die Aufnahme schon gelesen ist. Die Grenze gehoert dorthin,
        // wo der Plan entsteht.
        let longest_chunk = chunk_samples + min_tail_samples.saturating_sub(1);
        if longest_chunk > MAX_CHUNK_SAMPLES {
            return Err(Error::Bridge(format!(
                "chunk length {chunk_samples} plus an appended tail of up to \
                 {} samples could produce a {longest_chunk}-sample chunk, above the safe \
                 maximum of {MAX_CHUNK_SAMPLES}",
                min_tail_samples.saturating_sub(1)
            )));
        }
        Ok(Self {
            chunk_samples,
            min_tail_samples,
        })
    }

    pub fn chunk_samples(&self) -> usize {
        self.chunk_samples
    }

    pub fn min_tail_samples(&self) -> usize {
        self.min_tail_samples
    }
}

impl Default for ChunkPlan {
    fn default() -> Self {
        // Die Vorgaben sind gueltig; ein Fehler hier waere ein Programmfehler
        // und kein Eingabefehler.
        Self {
            chunk_samples: DEFAULT_CHUNK_SAMPLES,
            min_tail_samples: DEFAULT_MIN_TAIL_SAMPLES,
        }
    }
}

/// Was zwischen zwei Abschnitten passieren darf.
///
/// Ohne das rechnete ein zerlegter Lauf nach einem Abbruch alle uebrigen
/// Abschnitte weiter -- bei einem Zwei-Stunden-Kanal also minutenlang gegen
/// den erklaerten Willen des Menschen davor. Und der Fortschrittsbalken hatte
/// nichts Echtes zu melden, weil niemand wusste, wann ein Abschnitt fertig
/// ist.
pub trait ChunkObserver {
    /// Wird vor jedem Abschnitt und nach jedem fertigen Abschnitt gerufen.
    /// `false` bricht den Lauf ab.
    fn should_continue(&mut self, finished_chunks: usize, total_chunks: usize) -> bool;

    /// Meldet, dass der zerlegte Lauf eine andere Sprecherzahl gefunden hat
    /// als vorgegeben war.
    ///
    /// Gemeldet, nicht behoben -- die Begruendung steht bei
    /// [`diarize_chunks_with`]. `highest_remaining_similarity` ist die
    /// hoechste Aehnlichkeit, die zwischen zwei der gefundenen Sprecher noch
    /// besteht: die Zahl, an der ein Mensch ablesen kann, ob zwei Etiketten
    /// plausibel derselbe Mensch sind (nahe 0,80 und darueber) oder ob da
    /// wirklich mehr Leute geredet haben (deutlich darunter).
    fn speaker_count_differs(
        &mut self,
        _expected: usize,
        _found: usize,
        _highest_remaining_similarity: Option<f32>,
    ) {
    }
}

/// Fuer Aufrufer ohne Abbruchweg und ohne Balken (Messbaenke, Tests).
pub struct IgnoreChunkProgress;

impl ChunkObserver for IgnoreChunkProgress {
    fn should_continue(&mut self, _finished_chunks: usize, _total_chunks: usize) -> bool {
        true
    }
}

impl<F> ChunkObserver for F
where
    F: FnMut(usize, usize) -> bool,
{
    fn should_continue(&mut self, finished_chunks: usize, total_chunks: usize) -> bool {
        self(finished_chunks, total_chunks)
    }
}

/// Der Lauf wurde abgebrochen, nicht fehlgeschlagen.
pub const CHUNKED_DIARIZATION_CANCELLED: &str = "speaker diarization was cancelled";

/// Trennt einen Kanal und zerlegt ihn dabei, wenn er zu lang ist.
///
/// Das ist der Weg, den JEDER Aufrufer nehmen soll -- der Stapellauf beim
/// Transkribieren wie das Nachholen auf einer fertigen Aufnahme. Wer
/// stattdessen [`crate::diarize_samples`] auf einem ganzen Zwei-Stunden-Kanal
/// aufruft, laeuft in die CoreML-Zuteilung, die den Prozess beendet.
///
/// Unter der Abschnittslaenge verhaelt sich die Funktion exakt wie ein
/// einzelner Lauf, inklusive einer vorgegebenen Sprecherzahl. Darueber wird
/// zerlegt, und dann wird die Vorgabe je Abschnitt NICHT angewendet. Am Ende
/// loest [`resolve_surplus_speakers`] ueberzaehlige Gruppen auf, aber nur
/// soweit sie erkennbar Rest-Cluster sind; was uebrig bleibt, wird gemeldet.
/// Die Begruendung mit der Messung steht bei [`diarize_chunks_with`].
pub fn diarize_samples_chunked(
    model: SoniqoModel,
    samples: &[f32],
    exact_speakers: Option<usize>,
    plan: ChunkPlan,
    observer: &mut dyn ChunkObserver,
) -> Result<Vec<DiarizationSegment>> {
    diarize_chunks_with(samples, exact_speakers, plan, observer, |chunk, chunk_speakers| {
        crate::diarize_samples_with_embeddings(model, chunk, chunk_speakers)
    })
}

/// Derselbe Lauf, aber mit dem Abschnitts-Aufruf als Naht.
///
/// # Warum eine vorgegebene Sprecherzahl hier nicht durch Verschmelzen erzwungen wird
///
/// Unter der Abschnittslaenge reicht die Vorgabe an die Bibliothek durch und
/// wird dort eingehalten -- daran aendert sich nichts. Im ZERLEGTEN Lauf wird
/// sie nicht durch Verschmelzen erzwungen, und das ist eine Entscheidung
/// gegen die naheliegende Loesung. Was stattdessen passiert, steht bei
/// [`resolve_surplus_speakers`]: eine ueberzaehlige Gruppe wird aufgeloest,
/// wenn sie erkennbar ein Rest-Cluster ist, und sonst bleibt sie stehen.
///
/// Am 21.09.2026 stand hier kurz ein `enforce_exact_speakers`, das nach dem
/// Zusammenfuehren die aehnlichsten globalen Sprecher verschmolz, bis die
/// Zahl passte. Es ist wieder ausgebaut, weil die Messung es widerlegt hat:
///
/// - **Es kann per Konstruktion nur Paare UNTER der Schwelle treffen.** Alles
///   ueber [`SPEAKER_MATCH_MIN_SIMILARITY`] hat [`match_speakers`] ohnehin
///   schon verschmolzen. Was uebrig bleibt, hat das Verfahren ausdruecklich
///   als verschiedene Stimmen erkannt.
/// - **Der einzige Messwert am echten Material bestaetigt das.** Die
///   Raumaufnahme vom 10.09.2026 (122,8 Minuten, ein Raummikrofon), Vorgabe
///   4: die eine noetige Verschmelzung lief bei **+0,6204** -- unter der
///   Schwelle 0,80 und sogar unter dem hoechsten je gemessenen FREMDpaar
///   (0,6358; derselbe Mensch lag bei 0,9635 bis 0,9946). Zusammengelegt
///   wurden zwei grosse Stimmen mit 2729 und 5949 Woertern.
/// - **Die Fehlerrichtungen sind ungleich.** Ein Sprecher zu viel ist
///   reparierbar: zwei Etiketten gehoeren demselben Menschen, das sieht man
///   und klickt es zusammen. Zwei zu einem verschmolzene Menschen sind es
///   nicht -- im Transkript steht dann, eine Person habe gesagt, was eine
///   andere gesagt hat, und nichts deutet darauf hin.
///
/// Eine Funktion, deren einziger gemessener Effekt eine Verschlechterung ist,
/// gehoert nicht als Option stehengelassen. Geblieben ist die Meldung ueber
/// [`ChunkObserver::speaker_count_differs`]: Vorgabe, gefundene Zahl und die
/// hoechste verbliebene Aehnlichkeit.
///
/// # Die Naht
///
/// Die Naht existiert nur, damit die Schleife OHNE CoreML testbar ist.
/// Vorher war jede Zeile zwischen "zerlegen" und "zusammenfuehren" nur an
/// einem echten Modell pruefbar, und genau dort sassen die teuersten Fehler:
/// der Absturzschutz (`&samples[start..end]` statt `samples`), der Offset und
/// die Abbruchpruefung. Mit der Naht toetet ein gewoehnlicher Test jeden
/// dieser Mutanten.
pub(crate) fn diarize_chunks_with<F>(
    samples: &[f32],
    exact_speakers: Option<usize>,
    plan: ChunkPlan,
    observer: &mut dyn ChunkObserver,
    mut run_chunk: F,
) -> Result<Vec<DiarizationSegment>>
where
    F: FnMut(&[f32], Option<usize>) -> Result<crate::Diarization>,
{
    // Ohne Ton gibt es nichts zu trennen. Vorher lief hier ein CoreML-Aufruf
    // mit einem leeren Ausschnitt.
    if samples.is_empty() {
        return Ok(Vec::new());
    }

    let bounds = chunk_bounds(samples.len(), plan.chunk_samples, plan.min_tail_samples);
    let total_chunks = bounds.len();

    let mut chunks = Vec::with_capacity(total_chunks);
    for (chunk_index, (start, end)) in bounds.iter().copied().enumerate() {
        if !observer.should_continue(chunk_index, total_chunks) {
            return Err(Error::Bridge(CHUNKED_DIARIZATION_CANCELLED.to_string()));
        }
        // Bei EINEM Abschnitt darf die vorgegebene Sprecherzahl direkt an das
        // Verfahren gehen -- es sieht dann den ganzen Kanal und kann die
        // Zusage selbst einhalten. Bei mehreren gilt sie fuer die Runde und
        // wird weiter unten global durchgesetzt; ein Abschnitt allein kann
        // nicht wissen, wer in den anderen sitzt.
        let chunk_speakers = if total_chunks == 1 { exact_speakers } else { None };
        let diarization = run_chunk_slice(&mut run_chunk, samples, start, end, chunk_speakers)?;

        // Unter der Abschnittslaenge geht das Ergebnis der Bibliothek
        // UNVERAENDERT zurueck -- nicht durch die Pruefung, nicht durch das
        // Zusammenfuehren, nicht durch das Sortieren.
        //
        // Das ist die Zusage "unterhalb von 45 Minuten aendert sich nichts",
        // und sie war einen halben Tag lang gebrochen, ohne dass es auffiel:
        // seit `stitch_chunks` auch den Einzelfall bearbeitete, konnten dort
        // Reihenfolge und Sprechernummern anders herauskommen als vorher, und
        // eine leere Schwerpunktliste haette einen Lauf scheitern lassen, der
        // bis dahin durchlief. Eine Zusage, die nur meistens gilt, ist keine.
        if total_chunks == 1 {
            if !observer.should_continue(1, 1) {
                return Err(Error::Bridge(CHUNKED_DIARIZATION_CANCELLED.to_string()));
            }
            return Ok(diarization.segments);
        }

        validate_chunk(&diarization, chunk_index)?;
        chunks.push(ChunkResult {
            offset_seconds: start as f64 / f64::from(DIARIZATION_SAMPLE_RATE),
            segments: diarization.segments,
            speaker_embeddings: diarization.speaker_embeddings,
        });
        if !observer.should_continue(chunk_index + 1, total_chunks) {
            return Err(Error::Bridge(CHUNKED_DIARIZATION_CANCELLED.to_string()));
        }
    }

    // Ab hier gilt `total_chunks > 1` -- der Einzelfall ist oben schon
    // zurueckgegeben.
    let (segments, speakers) = stitch_chunks(&chunks)?;
    let mut segments = segments;
    if let Some(expected) = exact_speakers {
        resolve_surplus_speakers(&mut segments, expected);
    }

    if let Some(expected) = exact_speakers
        && distinct_speakers(&segments) != expected
    {
        observer.speaker_count_differs(
            expected,
            distinct_speakers(&segments),
            highest_similarity_between(&speakers),
        );
    }
    Ok(segments)
}

/// Wie viele verschiedene Sprecher stehen noch in den Abschnitten?
///
/// Nach [`resolve_surplus_speakers`] ist das NICHT mehr `speakers.len()` --
/// eine aufgeloeste Gruppe bleibt in der Schwerpunktliste stehen, ihre
/// Abschnitte gehoeren aber jemand anderem.
fn distinct_speakers(segments: &[DiarizationSegment]) -> usize {
    segments
        .iter()
        .map(|segment| segment.speaker_index)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

/// Hoechster Redezeit-Anteil, bei dem eine ueberzaehlige Gruppe noch als
/// Rest-Cluster gelten darf.
///
/// Gemessen am 21.09.2026 an einem echten zerlegten Lauf ueber die
/// 122,8-Minuten-Raumaufnahme mit vier Personen (Vorgabe 4, 2231 Abschnitte,
/// fuenf gefundene Gruppen):
///
/// | Gruppe | Redezeit | mittlere Abschnittsdauer |
/// |--------|----------|--------------------------|
/// | Rest   |   6,7 %  | 0,78 s                   |
/// | naechste echte Stimme | 14,3 % | 2,74 s      |
/// | uebrige | 20,8 bis 33,7 % | 2,78 bis 4,16 s  |
///
/// Beide Schwellen liegen damit mittig im Graben, nicht am Rand: Faktor 2,1
/// beim Anteil, Faktor 3,5 bei der Dauer. Gegengeprueft ueber 335
/// Sprechergruppen aus 73 getrennten Transkripten (dort auf Wortebene, aus
/// den gespeicherten Hinweisen): Gruppen unter 8 % Wortanteil haben im Median
/// 2 Woerter je Block, Gruppen darueber 15.
const SURPLUS_MAX_SPEECH_SHARE: f64 = 0.08;

/// Hoechste mittlere Abschnittsdauer, bei der eine ueberzaehlige Gruppe noch
/// als Rest-Cluster gelten darf.
///
/// Wie oft eine ueberzaehlige Gruppe mindestens vorkommen muss, damit sie als
/// Rest-Cluster gilt.
///
/// Der dritte Teil der Schutzbedingung, und der einzige, der einen STILLEN
/// Menschen rettet. Anteil und Dauer allein tun das nicht: wer fuenfmal "ja"
/// sagt, hat WENIGER Redezeit und KUERZERE Abschnitte als der Rest-Cluster und
/// wuerde vor ihm aufgeloest. Der Trennwert steht woanders -- ein Rest-Cluster
/// besteht aus VIELEN Stuecken, verteilt ueber die ganze Aufnahme, ein stiller
/// Mensch aus wenigen.
///
/// Gemessen am zerlegten Lauf der 122,8-Minuten-Aufnahme (2,05 h): der
/// Rest-Cluster kommt auf **264 Abschnitte je Stunde**. Ein stiller Mensch mit
/// fuenf kurzen Einwuerfen ueber dieselbe Aufnahme kaeme auf 2,4. Dazwischen
/// liegt Faktor 110, die Schwelle ist also unkritisch; 60 ist gewaehlt, weil
/// sie sich auch ohne Messung begruenden laesst: ein Rest, der ueber die ganze
/// Aufnahme verstreut ist, taucht mindestens einmal pro Minute auf.
///
/// Als Rate und nicht als absolute Zahl, damit sie ueber Aufnahmelaengen
/// traegt. **Nicht** als Unterscheidung zwischen Rest und ECHTER Stimme
/// brauchbar: dort trennt sie gar nicht (gemessen 264 gegen 161 bis 250 je
/// Stunde) -- das tun Anteil und Dauer.
const SURPLUS_MIN_SEGMENTS_PER_HOUR: f64 = 60.0;

/// Der zweite Teil der Schutzbedingung, und der wichtigere: ein echter Gast,
/// der nicht im Kalender steht, redet WENIG, aber in zusammenhaengenden
/// Beitraegen. Ein Rest-Cluster besteht aus Fetzen. Die Messtabelle steht bei
/// [`SURPLUS_MAX_SPEECH_SHARE`]; dort liegt der Rest bei 0,78 s und die
/// naechste echte Stimme bei 2,74 s.
const SURPLUS_MAX_MEAN_SEGMENT_SECONDS: f64 = 2.5;

/// Loest ueberzaehlige Sprechergruppen auf, wenn sie erkennbar Rest-Cluster
/// sind -- und nur dann.
///
/// # Warum ueberhaupt
///
/// Die Trennung findet auf einer Raumaufnahme regelmaessig eine Gruppe mehr,
/// als Menschen anwesend waren: kurze Fetzen, die zu keiner Stimme richtig
/// passen, sammeln sich in einer eigenen Gruppe. Gemessen an der
/// 122,8-Minuten-Aufnahme vom 10.09.2026 (vier Personen, ein Raummikrofon):
/// die kleinste der fuenf Gruppen hielt 961 Woerter in 128 Bloecken, im
/// Mittel 7,5 Woerter je Block, und ihre Woerter verteilen sich bei einem
/// Cloud-Zweitgutachten gleichmaessig auf ALLE VIER echten Sprecher. Sie ist
/// kein Mensch, sie ist ein Rest.
///
/// # Warum so und nicht durch Verschmelzen
///
/// Am 21.09.2026 stand in dieser Datei kurz ein `enforce_exact_speakers`, das
/// die aehnlichsten globalen Sprecher verschmolz, bis die Zahl passte. Es ist
/// widerlegt und ausgebaut (Begruendung bei [`diarize_chunks_with`]): es traf
/// die beiden GROESSTEN Stimmen bei einer Aehnlichkeit von 0,62. Hier wird
/// niemand verschmolzen -- eine Fetzen-Gruppe wird VERTEILT, jedes ihrer
/// Abschnitte an den zeitlich naechsten Nachbarn einer behaltenen Gruppe.
/// Die Fehlerrichtung ist damit die gutartige: ein einzelner falsch
/// zugeordneter Fetzen statt zweier verschmolzener Menschen.
///
/// # Die Schutzbedingung, und warum sie tragend ist
///
/// Aufgeloest wird nur eine Gruppe, die KLEIN und FETZENHAFT ist. Beides
/// zusammen, nie eines allein: ein echter Gast, der nicht im Kalender steht,
/// ist klein, aber nicht fetzenhaft.
///
/// Das ist kein Beiwerk. Die vorgegebene Zahl kann von der Wirklichkeit
/// abweichen: sie stammt aus der Teilnehmerliste des Termins (im Stapellauf
/// inklusive des Menschen am Mikrofon; im Live-Weg nur die fernen
/// Teilnehmer, siehe `listener-core::expected_speakers_per_channel`). Ein
/// Gast ohne Kalendereintrag oder eine unvollstaendige Liste macht sie
/// zu klein. Ohne die Schutzbedingung wuerde hier eine echte
/// Stimme aufgeloest; mit ihr bleibt nach dem Rest-Cluster die naechste
/// Gruppe stehen (15,9 % Redeanteil, lange Beitraege), und die Abweichung
/// wird ueber [`ChunkObserver::speaker_count_differs`] gemeldet statt still
/// erzwungen.
///
/// Gibt die Zahl der aufgeloesten Gruppen zurueck.
pub(crate) fn resolve_surplus_speakers(
    segments: &mut [DiarizationSegment],
    expected: usize,
) -> usize {
    let total_seconds: f64 = segments.iter().map(segment_seconds).sum();
    if total_seconds <= 0.0 || segments.is_empty() {
        return 0;
    }

    let span_hours = {
        let first = segments
            .iter()
            .map(|segment| segment.start_seconds)
            .fold(f64::INFINITY, f64::min);
        let last = segments
            .iter()
            .map(|segment| segment.end_seconds)
            .fold(f64::NEG_INFINITY, f64::max);
        ((last - first) / 3600.0).max(f64::MIN_POSITIVE)
    };

    // Die Kennzahlen werden EINMAL gerechnet, vor jeder Aenderung.
    //
    // Frueher lief hier eine Schleife, die nach jeder Aufloesung neu mass --
    // und damit gegen sich selbst: die behaltenen Gruppen erben die Fetzen der
    // aufgeloesten, ihre mittlere Abschnittsdauer sinkt, und beim naechsten
    // Durchgang sieht eine echte Stimme fetzenhafter aus, als sie ist. Ein
    // Schutz, der mit jeder Runde weicher wird, ist keiner.
    let mut profile: Vec<SpeakerProfile> = segments
        .iter()
        .map(|segment| segment.speaker_index)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|speaker| {
            let own: Vec<&DiarizationSegment> = segments
                .iter()
                .filter(|segment| segment.speaker_index == speaker)
                .collect();
            let spoken: f64 = own.iter().copied().map(segment_seconds).sum();
            SpeakerProfile {
                speaker,
                share: spoken / total_seconds,
                mean_seconds: if own.is_empty() {
                    0.0
                } else {
                    spoken / own.len() as f64
                },
                segments_per_hour: own.len() as f64 / span_hours,
                spoken_seconds: spoken,
            }
        })
        .collect();

    if profile.len() <= expected || profile.len() < 2 {
        return 0;
    }

    // Von der kleinsten Redezeit aufwaerts.
    profile.sort_by(|left, right| left.spoken_seconds.total_cmp(&right.spoken_seconds));

    let mut doomed: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    for candidate in &profile {
        if profile.len() - doomed.len() <= expected {
            break;
        }
        if !candidate.is_rest_cluster() {
            // UEBERSPRINGEN, nicht abbrechen. Die erste Fassung brach hier ab,
            // mit der Begruendung "wir gehen von unten nach oben, groessere
            // sind erst recht geschuetzt". Das gilt fuer Anteil und Dauer,
            // aber NICHT fuer die Rate: ein stiller Mensch hat die kleinste
            // Redezeit von allen und ist trotzdem geschuetzt -- ein Abbruch
            // bei ihm haette den Rest-Cluster dahinter gerettet. Genau dieser
            // Fall stand als Test da und war rot.
            continue;
        }
        doomed.insert(candidate.speaker);
    }

    if doomed.is_empty() {
        return 0;
    }

    // Erst die Ziele einsammeln, dann umhaengen: ein Abschnitt darf nie an
    // eine Gruppe fallen, die in derselben Runde selbst aufgeloest wird.
    let keepers: Vec<(f64, f64, usize)> = segments
        .iter()
        .filter(|segment| !doomed.contains(&segment.speaker_index))
        .map(|segment| {
            (
                segment.start_seconds,
                segment.end_seconds,
                segment.speaker_index,
            )
        })
        .collect();
    if keepers.is_empty() {
        return 0;
    }

    for index in 0..segments.len() {
        if !doomed.contains(&segments[index].speaker_index) {
            continue;
        }
        let start = segments[index].start_seconds;
        let end = segments[index].end_seconds;
        let mut best: Option<(f64, bool, usize)> = None;
        for (keeper_start, keeper_end, speaker) in &keepers {
            let distance = (start - keeper_end).max(keeper_start - end).max(0.0);
            // Bei Gleichstand gewinnt der vorausgehende Abschnitt.
            let follows = *keeper_end > start;
            let key = (distance, follows, *speaker);
            let better = match best {
                None => true,
                Some(current) => {
                    key.0 < current.0 || (key.0 == current.0 && !key.1 && current.1)
                }
            };
            if better {
                best = Some(key);
            }
        }
        if let Some((_, _, speaker)) = best {
            segments[index].speaker_index = speaker;
        }
    }

    doomed.len()
}

/// Die Kennzahlen EINER Sprechergruppe, einmal gerechnet.
struct SpeakerProfile {
    speaker: usize,
    /// Anteil an der gesamten Redezeit.
    share: f64,
    /// Mittlere Dauer eines Abschnitts dieser Gruppe.
    mean_seconds: f64,
    /// Abschnitte je Stunde Aufnahmespanne.
    segments_per_hour: f64,
    /// Redezeit in Sekunden -- die Reihenfolge der Kandidaten.
    spoken_seconds: f64,
}

impl SpeakerProfile {
    /// Ist das ein Rest-Cluster, oder ein Mensch?
    ///
    /// Drei Bedingungen, alle noetig. Die ersten beiden sagen "klein und
    /// fetzenhaft", die dritte "und zwar ueberall" -- sie ist die, die einen
    /// STILLEN Menschen rettet.
    fn is_rest_cluster(&self) -> bool {
        // Ohne messbare Redezeit ist gar nichts zu beurteilen. Das ist kein
        // Randfall aus dem Lehrbuch: Abschnitte mit `start == end` ergeben
        // eine mittlere Dauer von 0 und wuerden den Dauer-Schutz sonst
        // aushebeln, statt ihn auszuloesen.
        if self.spoken_seconds <= 0.0 {
            return false;
        }

        self.share < SURPLUS_MAX_SPEECH_SHARE
            && self.mean_seconds < SURPLUS_MAX_MEAN_SEGMENT_SECONDS
            && self.segments_per_hour >= SURPLUS_MIN_SEGMENTS_PER_HOUR
    }
}

fn segment_seconds(segment: &DiarizationSegment) -> f64 {
    (segment.end_seconds - segment.start_seconds).max(0.0)
}

/// Der Ausschnitt ist eine eigene Zeile, damit ein Test sie beobachten kann.
///
/// Wer hier `samples` statt `&samples[start..end]` uebergibt, hebt den
/// Absturzschutz vollstaendig auf und schickt den GANZEN Kanal in jeden
/// Abschnittsaufruf -- der Fehler, gegen den diese Datei gebaut wurde, in
/// seiner unauffaelligsten Form.
fn run_chunk_slice<F>(
    run_chunk: &mut F,
    samples: &[f32],
    start: usize,
    end: usize,
    exact_speakers: Option<usize>,
) -> Result<crate::Diarization>
where
    F: FnMut(&[f32], Option<usize>) -> Result<crate::Diarization>,
{
    run_chunk(&samples[start..end], exact_speakers)
}

/// Prueft, was die Bruecke geliefert hat, BEVOR daraus Sprecher werden.
///
/// **Hier wird unterschieden zwischen einer kaputten Bruecke und einem
/// bekannten Ausgabefall.** Die Unterscheidung ist wichtig, weil die
/// Fehlerrichtungen ungleich sind -- dieselbe Ueberlegung wie beim Entscheid
/// gegen das Erzwingen der Sprecherzahl (siehe [`diarize_chunks_with`]).
///
/// **Laut scheitern** bei allem, was nur ein Bruch sein kann: falsche
/// Dimension, nicht-endliche Werte, ein Segment, das einen Sprecher nennt,
/// den es nicht gibt. Da stimmt eine Annahme dieser Datei nicht mehr, und
/// weiterzurechnen hiesse, Zahlen zu erfinden.
///
/// **Durchlassen** beim Nullvektor. Er ist kein Bruch, sondern ein
/// dokumentierter Ausgabefall: das Verfahren setzt selbst einen ein, wenn
/// ihm zu einem benutzten Sprecher der Schwerpunkt fehlt (`speech-swift`
/// v0.0.22, `DiarizationHelpers.swift:66-70`: `zeroEmbedding`). Bis zum
/// 21.09.2026 abends hat er hier den ganzen Lauf verworfen -- bei einem
/// Zwei-Stunden-Kanal also die komplette Sprecherzuordnung, wegen einer
/// einzigen fehlenden Stimme in einem von drei Abschnitten. Das ist genau
/// der Tausch, gegen den diese ganze Datei gebaut wurde.
///
/// Stattdessen wird ein solcher Sprecher in [`match_speakers`] als eigener,
/// nicht verbindbarer globaler Sprecher gefuehrt: seine Segmente bleiben
/// erhalten, er nimmt an keinem Aehnlichkeitsvergleich teil und lernt nichts
/// dazu. Er kann dadurch in zwei Abschnitten als zwei Sprecher erscheinen --
/// ein Etikett zu viel, und das ist im Transkript reparierbar. Keine
/// Sprecher zu haben ist es nicht.
fn validate_chunk(diarization: &crate::Diarization, chunk_index: usize) -> Result<()> {
    let embeddings = &diarization.speaker_embeddings;
    if embeddings.is_empty() {
        if diarization.segments.is_empty() {
            return Ok(());
        }
        // Ohne Schwerpunkte laesst sich dieser Abschnitt an keinen anderen
        // anschliessen. Weiterrechnen hiesse, Sprechernummern
        // aneinanderzureihen, die nichts miteinander zu tun haben -- also
        // demselben Menschen im naechsten Abschnitt einen zweiten Namen zu
        // geben. Das ist schlechter als keine Trennung, weil es im
        // Transkript wie eine Aussage aussieht.
        return Err(Error::Bridge(format!(
            "diarization returned no speaker embeddings for chunk {chunk_index}; \
             cannot stitch a segmented channel"
        )));
    }

    for (speaker_index, embedding) in embeddings.iter().enumerate() {
        if embedding.len() != EMBEDDING_DIMENSION {
            return Err(Error::Bridge(format!(
                "chunk {chunk_index}: speaker {speaker_index} has a {}-value embedding, \
                 expected {EMBEDDING_DIMENSION}",
                embedding.len()
            )));
        }
        if !embedding.iter().all(|value| value.is_finite()) {
            return Err(Error::Bridge(format!(
                "chunk {chunk_index}: speaker {speaker_index} has a non-finite embedding"
            )));
        }
        // Der Nullvektor faellt bewusst NICHT durch. Er wird weiter unten als
        // nicht verbindbarer Sprecher gefuehrt; die Begruendung steht im
        // Kopf dieser Funktion.
        if is_unusable_centroid(embedding) {
            tracing::info!(
                chunk.index = chunk_index,
                speaker.index = speaker_index,
                "soniqo_chunk_speaker_without_centroid"
            );
        }
    }

    if let Some(highest) = diarization
        .segments
        .iter()
        .map(|segment| segment.speaker_index)
        .max()
        && highest >= embeddings.len()
    {
        return Err(Error::Bridge(format!(
            "chunk {chunk_index}: a segment names speaker {highest}, but only {} \
             embeddings were returned",
            embeddings.len()
        )));
    }

    Ok(())
}

/// Die hoechste Aehnlichkeit, die zwischen zwei gefundenen Sprechern noch
/// besteht.
///
/// Sie beantwortet die eine Frage, die ein Mensch bei einer abweichenden
/// Sprecherzahl hat: sind zwei der Etiketten plausibel derselbe Mensch? Liegt
/// der Wert dicht unter der Schwelle, spricht das dafuer; liegt er weit
/// darunter, haben dort wirklich mehr Leute geredet.
fn highest_similarity_between(speakers: &[GlobalSpeaker]) -> Option<f32> {
    let mut highest: Option<f32> = None;
    for left in 0..speakers.len() {
        for right in (left + 1)..speakers.len() {
            let Some(similarity) =
                cosine_similarity(&speakers[left].centroid, &speakers[right].centroid)
            else {
                continue;
            };
            if highest.is_none_or(|current| similarity > current) {
                highest = Some(similarity);
            }
        }
    }
    highest
}

/// Das Ergebnis EINES Abschnitts, bevor es eingeordnet ist.
///
/// Die Zeiten in `segments` sind relativ zum Abschnittsanfang -- jeder Lauf
/// weiss nichts von seiner Lage im Kanal.
#[derive(Debug, Clone)]
pub(crate) struct ChunkResult {
    /// Verschiebung dieses Abschnitts gegen den Kanalanfang, in Sekunden.
    pub(crate) offset_seconds: f64,
    pub(crate) segments: Vec<DiarizationSegment>,
    /// Je lokalem Sprecher ein Stimm-Schwerpunkt. Leer, wenn die Bruecke
    /// keine geliefert hat -- dann ist Zusammenfuehren nicht moeglich, und
    /// der Aufrufer darf gar nicht erst zerlegt haben.
    pub(crate) speaker_embeddings: Vec<Vec<f32>>,
}

/// Ab welcher Kosinus-Aehnlichkeit zwei Schwerpunkte als derselbe Mensch
/// gelten.
///
/// **Gemessen, nicht geschaetzt (10.09.2026).** Der erste Wurf stand bei 0,60
/// und war zu niedrig -- die Messung an der echten Raumaufnahme hat ihn
/// widerlegt. `examples/diarize_chunks` ueber 122,8 Minuten in drei
/// Abschnitten, fuenf Sprecher je Abschnitt, 105 Paare:
///
/// | Paarart | niedrigster | hoechster |
/// |---|---|---|
/// | **derselbe Mensch** (zwischen Abschnitten, 15 Paare) | **+0,9635** | +0,9946 |
/// | **verschiedene Menschen** (innerhalb eines Abschnitts, 30 Paare) | +0,0717 | **+0,6358** |
///
/// Zwischen 0,6358 und 0,9635 liegt nichts. 0,80 sitzt mit Abstand in dieser
/// Luecke: gut ueber dem hoechsten bekannten Fremdpaar, gut unter dem
/// niedrigsten bekannten echten. Die alten 0,60 lagen UNTER dem hoechsten
/// Fremdpaar -- ein Wert, den nur die Exklusivsperre in [`match_speakers`]
/// noch gerettet haette.
///
/// Die Fehlerrichtung ist bewusst gewaehlt, weil die beiden Seiten nicht
/// gleichwertig sind: zwei Sprecher faelschlich zu verschmelzen legt einem
/// Menschen die Worte eines anderen in den Mund und ist im Transkript nicht
/// mehr zu erkennen. Sie faelschlich zu trennen erzeugt einen Sprecher zu
/// viel -- sichtbar, und von Hand in einem Griff zu beheben. Im Zweifel also
/// trennen.
///
/// Die Zahlen stammen aus EINER Aufnahme mit vier Menschen an einem
/// Raummikrofon. Wer eine Aufnahme mit deutlich aehnlicheren Stimmen oder
/// ueber Telefon misst, misst hier neu.
pub(crate) const SPEAKER_MATCH_MIN_SIMILARITY: f32 = 0.80;

/// Kosinus-Aehnlichkeit zweier Einbettungen.
///
/// Gibt `None` zurueck, wenn die Laengen nicht zusammenpassen oder eine Seite
/// der Nullvektor ist -- beides ist eine Aussage ueber die Eingabe und darf
/// nicht als "unaehnlich" (0.0) durchgehen, sonst wird aus einem kaputten
/// Schwerpunkt still ein neuer Sprecher.
pub(crate) fn cosine_similarity(left: &[f32], right: &[f32]) -> Option<f32> {
    if left.is_empty() || left.len() != right.len() {
        return None;
    }
    let mut dot = 0.0f64;
    let mut left_norm = 0.0f64;
    let mut right_norm = 0.0f64;
    for (a, b) in left.iter().zip(right.iter()) {
        let a = f64::from(*a);
        let b = f64::from(*b);
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm <= 0.0 || right_norm <= 0.0 {
        return None;
    }
    Some((dot / (left_norm.sqrt() * right_norm.sqrt())) as f32)
}

/// Ein global gefuehrter Sprecher ueber alle Abschnitte hinweg.
#[derive(Debug, Clone)]
pub(crate) struct GlobalSpeaker {
    /// Laufender, nach Sprechdauer gewichteter Mittelwert der Schwerpunkte.
    /// Gewichtet, weil ein Abschnitt mit vierzig Sekunden Rede weniger ueber
    /// eine Stimme aussagt als einer mit zwanzig Minuten.
    centroid: Vec<f32>,
    weight: f64,
}

/// Ein Schwerpunkt, mit dem sich nichts vergleichen laesst.
///
/// Der Nullvektor ist der Fall, den die Bibliothek selbst erzeugt
/// (`DiarizationHelpers.swift:66-70`). Er ist keine Stimme: jede
/// Aehnlichkeit zu ihm ist undefiniert, nicht null.
fn is_unusable_centroid(embedding: &[f32]) -> bool {
    embedding.is_empty()
        || !embedding
            .iter()
            .any(|value| value.is_finite() && *value != 0.0)
}

impl GlobalSpeaker {
    /// Ein Sprecher, dessen Schwerpunkt unbrauchbar ist, wird gefuehrt, aber
    /// nie verbunden: er bleibt fuer sich, und ein spaeterer Abschnitt legt
    /// ihm nichts in den Mund.
    fn is_matchable(&self) -> bool {
        !is_unusable_centroid(&self.centroid)
    }

    fn absorb(&mut self, embedding: &[f32], weight: f64) {
        if !self.is_matchable() || is_unusable_centroid(embedding) {
            return;
        }
        if embedding.len() != self.centroid.len() || weight <= 0.0 {
            return;
        }
        let total = self.weight + weight;
        if total <= 0.0 {
            return;
        }
        for (slot, value) in self.centroid.iter_mut().zip(embedding.iter()) {
            let merged = (f64::from(*slot) * self.weight + f64::from(*value) * weight) / total;
            *slot = merged as f32;
        }
        self.weight = total;
    }
}

/// Fuehrt die Abschnitte zu EINER Sprecherliste zusammen.
///
/// Die Zeiten werden dabei auf den Kanalanfang zurueckgerechnet, die
/// Sprechernummern global neu vergeben. Das Ergebnis ist nach Startzeit
/// sortiert und sieht fuer jeden Aufrufer aus wie das Ergebnis eines einzigen
/// Laufes -- das ist der ganze Zweck: hinter dieser Funktion darf niemand
/// mehr wissen muessen, dass zerlegt wurde.
pub(crate) fn stitch_chunks(
    chunks: &[ChunkResult],
) -> Result<(Vec<DiarizationSegment>, Vec<GlobalSpeaker>)> {
    let mut speakers: Vec<GlobalSpeaker> = Vec::new();
    let mut stitched: Vec<DiarizationSegment> = Vec::new();

    for (chunk_index, chunk) in chunks.iter().enumerate() {
        // Redeanteil je lokalem Sprecher -- das Gewicht, mit dem sein
        // Schwerpunkt in den globalen eingeht.
        let mut spoken_seconds = vec![0.0f64; chunk.speaker_embeddings.len()];
        for segment in &chunk.segments {
            if let Some(slot) = spoken_seconds.get_mut(segment.speaker_index) {
                *slot += (segment.end_seconds - segment.start_seconds).max(0.0);
            }
        }

        let mapping = match_speakers(&chunk.speaker_embeddings, &spoken_seconds, &mut speakers);

        for segment in &chunk.segments {
            // Vorher stand hier ein `continue`: ein Segment ohne Zuordnung
            // verschwand still, und der Mensch sah ein Stueck Gespraech ohne
            // Sprecher, ohne dass irgendwo etwas stand. Nach der Pruefung in
            // `validate_chunk` KANN das nicht mehr eintreten -- wenn doch, ist
            // eine Annahme dieser Datei gebrochen, und das gehoert laut
            // gesagt statt weggelassen.
            let Some(global_index) = mapping.get(segment.speaker_index).copied().flatten() else {
                return Err(Error::Bridge(format!(
                    "chunk {chunk_index}: segment names speaker {} but it could not be \
                     mapped to a global speaker",
                    segment.speaker_index
                )));
            };
            stitched.push(DiarizationSegment {
                start_seconds: segment.start_seconds + chunk.offset_seconds,
                end_seconds: segment.end_seconds + chunk.offset_seconds,
                speaker_index: global_index,
            });
        }
    }

    stitched.sort_by(|left, right| {
        left.start_seconds
            .total_cmp(&right.start_seconds)
            .then_with(|| left.end_seconds.total_cmp(&right.end_seconds))
            .then_with(|| left.speaker_index.cmp(&right.speaker_index))
    });
    Ok((stitched, speakers))
}

/// Ordnet die lokalen Sprecher EINES Abschnitts den global gefuehrten zu.
///
/// Gierig ueber die staerksten Paare zuerst, und in BEIDE Richtungen
/// exklusiv: ein globaler Sprecher nimmt hoechstens einen lokalen dieses
/// Abschnitts auf. Ohne diese zweite Sperre koennten zwei Menschen, die
/// derselben bekannten Stimme aehneln, auf denselben Namen zusammenfallen --
/// und genau das ist der Fehler, den man im Transkript nicht mehr sieht.
fn match_speakers(
    embeddings: &[Vec<f32>],
    spoken_seconds: &[f64],
    speakers: &mut Vec<GlobalSpeaker>,
) -> Vec<Option<usize>> {
    let mut mapping = vec![None; embeddings.len()];

    let mut candidates: Vec<(f32, usize, usize)> = Vec::new();
    for (local_index, embedding) in embeddings.iter().enumerate() {
        // Ein unbrauchbarer Schwerpunkt nimmt an keinem Vergleich teil. Ohne
        // diese Zeile entschiede `cosine_similarity` es zwar auch (sie gibt
        // fuer den Nullvektor `None`), aber dann stuende die Absicht
        // nirgends -- und sie ist der Kern: dieser Sprecher wird gefuehrt,
        // nicht verbunden.
        if is_unusable_centroid(embedding) {
            continue;
        }
        for (global_index, speaker) in speakers.iter().enumerate() {
            if !speaker.is_matchable() {
                continue;
            }
            if let Some(similarity) = cosine_similarity(embedding, &speaker.centroid) {
                if similarity >= SPEAKER_MATCH_MIN_SIMILARITY {
                    candidates.push((similarity, local_index, global_index));
                }
            }
        }
    }
    // Staerkste Aehnlichkeit zuerst; bei Gleichstand die kleineren Nummern,
    // damit dieselbe Eingabe immer dieselbe Zuordnung ergibt.
    candidates.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });

    let mut global_taken = vec![false; speakers.len()];
    for (_, local_index, global_index) in candidates {
        if mapping[local_index].is_some() || global_taken[global_index] {
            continue;
        }
        mapping[local_index] = Some(global_index);
        global_taken[global_index] = true;
    }

    for (local_index, embedding) in embeddings.iter().enumerate() {
        let weight = spoken_seconds.get(local_index).copied().unwrap_or(0.0);
        match mapping[local_index] {
            Some(global_index) => speakers[global_index].absorb(embedding, weight),
            None => {
                // Auch ein Sprecher ohne brauchbaren Schwerpunkt wird
                // gefuehrt. Frueher fiel er hier heraus, und seine Segmente
                // verschwanden -- ein Stueck Gespraech ohne Sprecher, ohne
                // dass irgendwo etwas stand. Er bekommt jetzt ein eigenes
                // Etikett, verbindet sich aber mit niemandem: `is_matchable`
                // haelt ihn aus jedem Vergleich heraus, in beide Richtungen.
                mapping[local_index] = Some(speakers.len());
                speakers.push(GlobalSpeaker {
                    centroid: embedding.clone(),
                    weight: weight.max(f64::EPSILON),
                });
            }
        }
    }

    mapping
}

/// Zerlegt eine Kanallaenge in Abschnittsgrenzen.
///
/// Der letzte Abschnitt wird dem vorletzten zugeschlagen, wenn er allein zu
/// kurz waere: ein Reststueck von zwanzig Sekunden traegt keine belastbare
/// Stimme, und ein Schwerpunkt daraus wuerde beim Zusammenfuehren mehr
/// Schaden anrichten als Nutzen.
pub(crate) fn chunk_bounds(
    sample_count: usize,
    chunk_samples: usize,
    min_tail_samples: usize,
) -> Vec<(usize, usize)> {
    if sample_count == 0 {
        return Vec::new();
    }
    if chunk_samples == 0 || sample_count <= chunk_samples {
        return vec![(0, sample_count)];
    }

    let mut bounds = Vec::new();
    let mut start = 0usize;
    while start < sample_count {
        let mut end = (start + chunk_samples).min(sample_count);
        if sample_count - end < min_tail_samples {
            end = sample_count;
        }
        bounds.push((start, end));
        start = end;
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(start: f64, end: f64, speaker: usize) -> DiarizationSegment {
        DiarizationSegment {
            start_seconds: start,
            end_seconds: end,
            speaker_index: speaker,
        }
    }

    fn embedding(values: &[f32]) -> Vec<f32> {
        values.to_vec()
    }

    #[test]
    fn eine_stimme_ueber_zwei_abschnitte_behaelt_eine_nummer() {
        // Derselbe Mensch redet in beiden Abschnitten -- im zweiten traegt er
        // lokal die Nummer 0, weil jeder Lauf bei 0 anfaengt. Genau das muss
        // das Zusammenfuehren aufloesen.
        let chunks = vec![
            ChunkResult {
                offset_seconds: 0.0,
                segments: vec![segment(0.0, 10.0, 0)],
                speaker_embeddings: vec![embedding(&[1.0, 0.0, 0.0])],
            },
            ChunkResult {
                offset_seconds: 100.0,
                segments: vec![segment(0.0, 10.0, 0)],
                speaker_embeddings: vec![embedding(&[0.99, 0.01, 0.0])],
            },
        ];

        let (stitched, _) = stitch_chunks(&chunks).unwrap();

        assert_eq!(stitched.len(), 2);
        assert_eq!(stitched[0].speaker_index, stitched[1].speaker_index);
        // Und die Zeit des zweiten Abschnitts ist auf den Kanalanfang
        // zurueckgerechnet, nicht bei null geblieben.
        assert_eq!(stitched[1].start_seconds, 100.0);
    }

    #[test]
    fn zwei_verschiedene_stimmen_bleiben_zwei_nummern() {
        let chunks = vec![
            ChunkResult {
                offset_seconds: 0.0,
                segments: vec![segment(0.0, 10.0, 0)],
                speaker_embeddings: vec![embedding(&[1.0, 0.0, 0.0])],
            },
            ChunkResult {
                offset_seconds: 100.0,
                segments: vec![segment(0.0, 10.0, 0)],
                speaker_embeddings: vec![embedding(&[0.0, 1.0, 0.0])],
            },
        ];

        let (stitched, _) = stitch_chunks(&chunks).unwrap();

        assert_eq!(stitched.len(), 2);
        assert_ne!(stitched[0].speaker_index, stitched[1].speaker_index);
    }

    #[test]
    fn ein_globaler_sprecher_nimmt_nicht_zwei_lokale_desselben_abschnitts() {
        // Beide lokalen Stimmen des zweiten Abschnitts aehneln der einen
        // bekannten. Ohne die Exklusivsperre wuerden beide auf denselben
        // Menschen fallen -- ein Fehler, den im Transkript niemand mehr
        // erkennt.
        let chunks = vec![
            ChunkResult {
                offset_seconds: 0.0,
                segments: vec![segment(0.0, 10.0, 0)],
                speaker_embeddings: vec![embedding(&[1.0, 0.0])],
            },
            ChunkResult {
                offset_seconds: 100.0,
                segments: vec![segment(0.0, 5.0, 0), segment(5.0, 10.0, 1)],
                speaker_embeddings: vec![embedding(&[1.0, 0.05]), embedding(&[1.0, 0.10])],
            },
        ];

        let (stitched, _) = stitch_chunks(&chunks).unwrap();

        let second_chunk: Vec<usize> = stitched
            .iter()
            .filter(|s| s.start_seconds >= 100.0)
            .map(|s| s.speaker_index)
            .collect();
        assert_eq!(second_chunk.len(), 2);
        assert_ne!(
            second_chunk[0], second_chunk[1],
            "zwei Sprecher desselben Abschnitts duerfen nie zusammenfallen"
        );
    }

    #[test]
    fn unaehnliche_stimme_wird_ein_neuer_sprecher_statt_der_naechstbeste() {
        // Knapp unter der Schwelle: das Verfahren darf NICHT den
        // aehnlichsten bekannten nehmen, nur weil es der aehnlichste ist.
        let a = embedding(&[1.0, 0.0]);
        let b = embedding(&[0.5, 1.0]);
        let similarity = cosine_similarity(&a, &b).unwrap();
        assert!(
            similarity < SPEAKER_MATCH_MIN_SIMILARITY,
            "Testaufbau kaputt: {similarity} liegt ueber der Schwelle"
        );

        let chunks = vec![
            ChunkResult {
                offset_seconds: 0.0,
                segments: vec![segment(0.0, 10.0, 0)],
                speaker_embeddings: vec![a],
            },
            ChunkResult {
                offset_seconds: 100.0,
                segments: vec![segment(0.0, 10.0, 0)],
                speaker_embeddings: vec![b],
            },
        ];

        let (stitched, _) = stitch_chunks(&chunks).unwrap();
        assert_ne!(stitched[0].speaker_index, stitched[1].speaker_index);
    }

    /// Nagelt die Schwelle an die Messung vom 10.09.2026 (Doku bei
    /// [`SPEAKER_MATCH_MIN_SIMILARITY`]). Wer sie senkt, laesst zwei
    /// verschiedene Menschen zusammenfallen; wer sie hebt, zerreisst einen
    /// Menschen in zwei. Beides soll hier auffallen und nicht erst im
    /// Transkript.
    #[test]
    fn die_schwelle_liegt_in_der_gemessenen_luecke() {
        const HOECHSTES_FREMDPAAR: f32 = 0.6358;
        const NIEDRIGSTES_ECHTES_PAAR: f32 = 0.9635;

        assert!(
            SPEAKER_MATCH_MIN_SIMILARITY > HOECHSTES_FREMDPAAR,
            "Schwelle {SPEAKER_MATCH_MIN_SIMILARITY} laesst zwei verschiedene \
             Menschen zusammenfallen (gemessen: {HOECHSTES_FREMDPAAR})"
        );
        assert!(
            SPEAKER_MATCH_MIN_SIMILARITY < NIEDRIGSTES_ECHTES_PAAR,
            "Schwelle {SPEAKER_MATCH_MIN_SIMILARITY} zerreisst einen Menschen \
             in zwei (gemessen: {NIEDRIGSTES_ECHTES_PAAR})"
        );
    }

    #[test]
    fn kosinus_meldet_unbrauchbare_eingaben_statt_null() {
        assert_eq!(cosine_similarity(&[], &[]), None);
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[1.0]), None);
        // Der Nullvektor ist keine Stimme. Wuerde er als 0.0 durchgehen,
        // erzeugte ein kaputter Schwerpunkt still einen neuen Sprecher.
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]), None);
    }

    #[test]
    fn abschnittsgrenzen_decken_den_kanal_lueckenlos_ab() {
        let bounds = chunk_bounds(1000, 300, 100);
        assert_eq!(bounds.first().unwrap().0, 0);
        assert_eq!(bounds.last().unwrap().1, 1000);
        for pair in bounds.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "keine Luecke, keine Ueberlappung");
        }
    }

    #[test]
    fn ein_zu_kurzes_reststueck_wird_zugeschlagen_statt_allein_getrennt() {
        // 1000 bei Abschnitt 300 ergaebe sonst einen Rest von 100.
        let bounds = chunk_bounds(1000, 300, 150);
        assert!(
            bounds.last().unwrap().1 - bounds.last().unwrap().0 >= 150,
            "Reststueck {:?} ist zu kurz fuer eine belastbare Stimme",
            bounds.last()
        );
        assert_eq!(bounds.last().unwrap().1, 1000);
    }

    #[test]
    fn ein_kanal_unter_der_abschnittslaenge_bleibt_ein_stueck() {
        assert_eq!(chunk_bounds(500, 1000, 100), vec![(0, 500)]);
    }

    // ---------------------------------------------------------------------
    // Ab hier: Tests ueber die NAHT, also ueber die ganze Schleife ohne
    // CoreML. Vorher war alles zwischen "zerlegen" und "zusammenfuehren" nur
    // an einem echten Modell pruefbar, und genau dort sassen die Mutanten,
    // die am 21.09.2026 ueberlebt haben.
    // ---------------------------------------------------------------------

    /// Ein gueltiger Schwerpunkt in der erwarteten Laenge.
    fn valid_embedding(seed: f32) -> Vec<f32> {
        let mut values = vec![0.0f32; EMBEDDING_DIMENSION];
        values[0] = 1.0;
        values[1] = seed;
        values
    }

    fn diarization(segments: Vec<DiarizationSegment>, embeddings: Vec<Vec<f32>>) -> crate::Diarization {
        crate::Diarization {
            segments,
            speaker_embeddings: embeddings,
        }
    }

    fn plan(chunk: usize, tail: usize) -> ChunkPlan {
        ChunkPlan::new(chunk, tail).expect("Testaufbau: ungueltiger Plan")
    }

    /// Bewacht: `chunk_bounds`-Zeile `sample_count <= chunk_samples`
    /// (chunked.rs) UND `run_chunk_slice`s `&samples[start..end]`.
    ///
    /// Das ist der Test, dessen Fehlen am 21.09. gemessen wurde: die alten
    /// Tests fuhren zwar lange Eingaben, pruefen aber nur Eigenschaften, die
    /// EIN Abschnitt genauso erfuellt (lueckenlos, endet am Kanalende). Der
    /// Mutant "nie zerlegen" blieb dadurch gruen.
    #[test]
    fn ein_langer_kanal_wird_wirklich_in_mehrere_aufrufe_zerlegt() {
        let samples = vec![0.0f32; 1000];
        let mut seen_lengths = Vec::new();
        let mut observer = IgnoreChunkProgress;

        let segments = diarize_chunks_with(
            &samples,
            None,
            plan(300, 100),
            &mut observer,
            |chunk, _| {
                seen_lengths.push(chunk.len());
                Ok(diarization(
                    vec![segment(0.0, 1.0, 0)],
                    vec![valid_embedding(0.0)],
                ))
            },
        )
        .unwrap();

        assert!(
            seen_lengths.len() > 1,
            "1000 Samples bei 300 je Abschnitt muessen mehr als einen Aufruf ergeben, \
             waren aber {seen_lengths:?}"
        );
        assert_eq!(
            seen_lengths.iter().sum::<usize>(),
            samples.len(),
            "die Abschnitte muessen den Kanal genau einmal abdecken"
        );
        assert!(
            seen_lengths.iter().all(|length| *length < samples.len()),
            "kein Abschnitt darf den GANZEN Kanal bekommen -- genau das ist der \
             Absturzschutz: {seen_lengths:?}"
        );
        assert_eq!(segments.len(), seen_lengths.len());
    }

    /// Bewacht: `offset_seconds: start as f64 / DIARIZATION_SAMPLE_RATE`
    /// und den Endzeit-Offset in `stitch_chunks`.
    ///
    /// Alle aelteren Tests setzen `offset_seconds` von Hand und koennen die
    /// Zeile, die ihn BERECHNET, deshalb nicht bewachen.
    #[test]
    fn die_zeiten_der_spaeteren_abschnitte_werden_auf_den_kanalanfang_gerechnet() {
        let chunk_samples = DIARIZATION_SAMPLE_RATE as usize * 60; // 60 s
        let samples = vec![0.0f32; chunk_samples * 2];
        let mut observer = IgnoreChunkProgress;

        let segments = diarize_chunks_with(
            &samples,
            None,
            plan(chunk_samples, chunk_samples / 2),
            &mut observer,
            |_, _| {
                Ok(diarization(
                    vec![segment(1.0, 2.0, 0)],
                    vec![valid_embedding(0.0)],
                ))
            },
        )
        .unwrap();

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start_seconds, 1.0);
        assert_eq!(segments[0].end_seconds, 2.0);
        // Zweiter Abschnitt beginnt bei 60 s, das Segment liegt 1 s darin.
        assert_eq!(segments[1].start_seconds, 61.0);
        assert_eq!(
            segments[1].end_seconds, 62.0,
            "auch die ENDzeit traegt den Offset; ohne sie waere das Segment rueckwaerts"
        );
    }

    /// Bewacht: `speakers[global_index].absorb(...)` in `match_speakers`.
    ///
    /// Erst mit einem DRITTEN Abschnitt wird der aktualisierte Schwerpunkt
    /// gebraucht; mit zweien reicht der erste Wert immer.
    #[test]
    fn der_globale_schwerpunkt_lernt_ueber_drei_abschnitte_dazu() {
        // Der Sprecher driftet langsam. Ohne `absorb` bleibt der globale
        // Schwerpunkt auf dem ersten Wert stehen und der dritte Abschnitt
        // faellt aus der Schwelle.
        let mut drift = 0.0f32;
        let mut observer = IgnoreChunkProgress;
        let samples = vec![0.0f32; 900];

        let segments = diarize_chunks_with(&samples, None, plan(300, 100), &mut observer, |_, _| {
            let mut values = vec![0.0f32; EMBEDDING_DIMENSION];
            values[0] = 1.0;
            values[1] = drift;
            drift += 0.62;
            Ok(diarization(vec![segment(0.0, 60.0, 0)], vec![values]))
        })
        .unwrap();

        let speakers: std::collections::BTreeSet<usize> =
            segments.iter().map(|s| s.speaker_index).collect();
        assert_eq!(
            speakers.len(),
            1,
            "der mitwandernde Schwerpunkt muss denselben Menschen halten, \
             gefunden: {speakers:?}"
        );
    }

    /// Bewacht: `sample_count - end < min_tail_samples` (das `<`, nicht `<=`).
    #[test]
    fn ein_reststueck_von_genau_der_mindestlaenge_bleibt_eigenstaendig() {
        // 1000 bei 300 je Abschnitt: 0-300, 300-600, 600-900, Rest 100.
        // Mit min_tail 100 ist der Rest GENAU die Mindestlaenge und bleibt
        // damit ein eigener Abschnitt.
        let bounds = chunk_bounds(1000, 300, 100);
        assert_eq!(
            bounds.last().copied(),
            Some((900, 1000)),
            "ein Rest von exakt der Mindestlaenge wird nicht angehaengt: {bounds:?}"
        );
    }

    /// Bewacht: die Pruefungen in `ChunkPlan::new`.
    #[test]
    fn ungueltige_zerlegungs_vorgaben_werden_abgelehnt_statt_still_gedeutet() {
        assert!(
            ChunkPlan::new(0, 0).is_err(),
            "chunk_samples = 0 wuerde die Zerlegung abschalten"
        );
        assert!(
            ChunkPlan::new(MAX_CHUNK_SAMPLES + 1, 100).is_err(),
            "ueber der Obergrenze laeuft wieder ein Mehrstundenkanal in EINEN Aufruf"
        );
        assert!(
            ChunkPlan::new(300, 300).is_err(),
            "min_tail >= chunk haengt jeden Rest an und frisst den ganzen Kanal"
        );
        // Genau die Vertauschung, die als nackte usize-Paare kompilierte:
        // aus 45 Minuten und 5 Minuten werden 5 und 45.
        assert!(
            ChunkPlan::new(DEFAULT_MIN_TAIL_SAMPLES, DEFAULT_CHUNK_SAMPLES).is_err(),
            "vertauschte Vorgaben muessen auffallen, nicht still 5-Minuten-Abschnitte ergeben"
        );
        assert!(ChunkPlan::new(DEFAULT_CHUNK_SAMPLES, DEFAULT_MIN_TAIL_SAMPLES).is_ok());
    }

    /// Sammelt, was der Lauf ueber eine abweichende Sprecherzahl meldet.
    #[derive(Default)]
    struct Abweichung {
        gemeldet: Vec<(usize, usize, Option<f32>)>,
    }

    impl ChunkObserver for Abweichung {
        fn should_continue(&mut self, _finished: usize, _total: usize) -> bool {
            true
        }
        fn speaker_count_differs(
            &mut self,
            expected: usize,
            found: usize,
            highest_remaining_similarity: Option<f32>,
        ) {
            self.gemeldet
                .push((expected, found, highest_remaining_similarity));
        }
    }

    /// Bewacht: dass die Vorgabe im zerlegten Lauf GEMELDET und nicht
    /// erzwungen wird -- also den Ausbau von `enforce_exact_speakers` und den
    /// `speakers.len() != expected`-Zweig.
    ///
    /// Der Fall ist genau der aus der Messung vom 21.09.2026: es wurden mehr
    /// Stimmen gefunden als vorgegeben, und sie sind einander unaehnlich. Wer
    /// hier wieder verschmilzt, legt zwei Menschen zusammen.
    #[test]
    fn eine_vorgegebene_sprecherzahl_wird_gemeldet_statt_erzwungen() {
        let samples = vec![0.0f32; 1500]; // fuenf Abschnitte a 300
        let mut chunk_index = 0usize;
        let mut observer = Abweichung::default();

        // In jedem Abschnitt genau eine Stimme, und alle fuenf zeigen in
        // verschiedene Richtungen -- also fuenf klar verschiedene Menschen.
        let segments = diarize_chunks_with(
            &samples,
            Some(4),
            plan(300, 100),
            &mut observer,
            |_, _| {
                let mut values = vec![0.0f32; EMBEDDING_DIMENSION];
                values[chunk_index * 10] = 1.0;
                chunk_index += 1;
                Ok(diarization(vec![segment(0.0, 10.0, 0)], vec![values]))
            },
        )
        .unwrap();

        let speakers: std::collections::BTreeSet<usize> =
            segments.iter().map(|s| s.speaker_index).collect();
        assert_eq!(
            speakers.len(),
            5,
            "fuenf unaehnliche Stimmen bleiben fuenf, auch wenn vier vorgegeben waren -- \
             gefunden: {speakers:?}"
        );
        assert_eq!(
            observer.gemeldet.len(),
            1,
            "die Abweichung muss genau einmal gemeldet werden: {:?}",
            observer.gemeldet
        );
        let (expected, found, highest) = observer.gemeldet[0];
        assert_eq!((expected, found), (4, 5));
        let highest = highest.expect("bei fuenf Sprechern gibt es Paare zum Vergleichen");
        assert!(
            highest < SPEAKER_MATCH_MIN_SIMILARITY,
            "die hoechste Restaehnlichkeit muss unter der Schwelle liegen, sonst haette \
             das Zusammenfuehren die beiden schon verschmolzen: {highest}"
        );
    }

    /// Bewacht: dass eine PASSENDE Sprecherzahl nichts meldet.
    #[test]
    fn eine_erfuellte_vorgabe_meldet_nichts() {
        let samples = vec![0.0f32; 900];
        let mut observer = Abweichung::default();

        diarize_chunks_with(&samples, Some(1), plan(300, 100), &mut observer, |_, _| {
            Ok(diarization(
                vec![segment(0.0, 10.0, 0)],
                vec![valid_embedding(0.0)],
            ))
        })
        .unwrap();

        assert!(
            observer.gemeldet.is_empty(),
            "ohne Abweichung gibt es nichts zu melden: {:?}",
            observer.gemeldet
        );
    }

    /// Bewacht: dass ein Nullvektor die Segmente BEHAELT statt den Lauf zu
    /// verwerfen -- und dass er sich mit niemandem verbindet.
    ///
    /// Kein gedachter Fall: `speech-swift` v0.0.22 setzt in
    /// `DiarizationHelpers.swift:66-70` selbst einen Nullvektor ein, wenn
    /// einem benutzten Sprecher der Schwerpunkt fehlt. Ihn abzulehnen hiesse,
    /// die Trennung eines Zwei-Stunden-Kanals wegen einer fehlenden Stimme in
    /// einem von drei Abschnitten ganz zu verlieren.
    #[test]
    fn ein_nullvektor_bleibt_ein_eigener_sprecher_und_verbindet_sich_mit_niemandem() {
        // Drei Abschnitte: der erste und der dritte tragen einen Nullvektor,
        // der zweite eine echte Stimme. Die beiden Nullvektor-Sprecher
        // duerfen NICHT miteinander verschmelzen -- sie sind nicht
        // vergleichbar, nicht gleich.
        let samples = vec![0.0f32; 900];
        let mut chunk_index = 0usize;
        let mut observer = IgnoreChunkProgress;

        let segments = diarize_chunks_with(&samples, None, plan(300, 100), &mut observer, |_, _| {
            let embedding = if chunk_index == 1 {
                valid_embedding(0.0)
            } else {
                vec![0.0f32; EMBEDDING_DIMENSION]
            };
            chunk_index += 1;
            Ok(diarization(vec![segment(0.0, 10.0, 0)], vec![embedding]))
        })
        .expect("ein Nullvektor darf den Lauf nicht verwerfen");

        assert_eq!(
            segments.len(),
            3,
            "kein Segment darf verlorengehen: {segments:?}"
        );
        let speakers: std::collections::BTreeSet<usize> =
            segments.iter().map(|s| s.speaker_index).collect();
        assert_eq!(
            speakers.len(),
            3,
            "zwei Nullvektor-Sprecher aus verschiedenen Abschnitten duerfen nicht \
             zusammenfallen, und keiner von ihnen mit der echten Stimme: {speakers:?}"
        );
    }

    /// Bewacht: `is_matchable` in beide Richtungen -- ein echter Sprecher
    /// darf einen Nullvektor-Sprecher ebenso wenig aufnehmen wie umgekehrt.
    #[test]
    fn ein_nullvektor_sprecher_nimmt_auch_keine_echte_stimme_auf() {
        let samples = vec![0.0f32; 600];
        let mut chunk_index = 0usize;
        let mut observer = IgnoreChunkProgress;

        let segments = diarize_chunks_with(&samples, None, plan(300, 100), &mut observer, |_, _| {
            // Erst der Nullvektor, dann eine echte Stimme.
            let embedding = if chunk_index == 0 {
                vec![0.0f32; EMBEDDING_DIMENSION]
            } else {
                valid_embedding(0.0)
            };
            chunk_index += 1;
            Ok(diarization(vec![segment(0.0, 10.0, 0)], vec![embedding]))
        })
        .unwrap();

        let speakers: std::collections::BTreeSet<usize> =
            segments.iter().map(|s| s.speaker_index).collect();
        assert_eq!(speakers.len(), 2, "die beiden gehoeren nicht zusammen");
    }

    /// Bewacht: Dimensions-, Endlichkeits- und Indexpruefung in
    /// `validate_chunk`.
    #[test]
    fn kaputte_schwerpunkte_lassen_den_lauf_laut_scheitern() {
        let samples = vec![0.0f32; 900];

        let cases: Vec<(&str, crate::Diarization)> = vec![
            (
                "expected 256",
                diarization(vec![segment(0.0, 1.0, 0)], vec![vec![1.0f32; 8]]),
            ),
            (
                "non-finite",
                diarization(vec![segment(0.0, 1.0, 0)], {
                    let mut values = valid_embedding(0.0);
                    values[3] = f32::NAN;
                    vec![values]
                }),
            ),
            (
                "embeddings were returned",
                diarization(vec![segment(0.0, 1.0, 5)], vec![valid_embedding(0.0)]),
            ),
        ];

        for (expected, payload) in cases {
            let mut observer = IgnoreChunkProgress;
            let mut payload = Some(payload);
            let result =
                diarize_chunks_with(&samples, None, plan(300, 100), &mut observer, |_, _| {
                    Ok(payload.take().unwrap_or_else(|| {
                        diarization(vec![segment(0.0, 1.0, 0)], vec![valid_embedding(0.0)])
                    }))
                });
            let error = result.expect_err("kaputte Eingabe muss scheitern");
            assert!(
                error.to_string().contains(expected),
                "erwartet '{expected}', bekam: {error}"
            );
        }
    }

    /// Bewacht: die beiden `observer.should_continue`-Aufrufe in
    /// `diarize_chunks_with`.
    #[test]
    fn ein_abbruch_stoppt_zwischen_den_abschnitten_statt_zu_ende_zu_rechnen() {
        let samples = vec![0.0f32; 3000]; // 10 Abschnitte a 300
        let mut calls = 0usize;
        // Nach dem ersten fertigen Abschnitt abbrechen.
        let mut observer = |finished: usize, _total: usize| finished < 1;

        let result = diarize_chunks_with(&samples, None, plan(300, 100), &mut observer, |_, _| {
            calls += 1;
            Ok(diarization(
                vec![segment(0.0, 1.0, 0)],
                vec![valid_embedding(0.0)],
            ))
        });

        let error = result.expect_err("ein Abbruch ist kein Erfolg");
        assert!(
            error.to_string().contains(CHUNKED_DIARIZATION_CANCELLED),
            "unerwartete Meldung: {error}"
        );
        assert_eq!(
            calls, 1,
            "nach dem Abbruch darf kein weiterer Abschnitt gerechnet werden"
        );
    }

    /// Bewacht: der Fortschritt meldet echte Abschnitte, nicht nur die Uhr.
    #[test]
    fn jeder_fertige_abschnitt_meldet_sich() {
        let samples = vec![0.0f32; 900];
        let mut finished_seen = Vec::new();
        let mut observer = |finished: usize, total: usize| {
            finished_seen.push((finished, total));
            true
        };

        diarize_chunks_with(&samples, None, plan(300, 100), &mut observer, |_, _| {
            Ok(diarization(
                vec![segment(0.0, 1.0, 0)],
                vec![valid_embedding(0.0)],
            ))
        })
        .unwrap();

        assert!(
            finished_seen.contains(&(3, 3)),
            "der letzte fertige Abschnitt muss gemeldet werden: {finished_seen:?}"
        );
        assert!(
            finished_seen.iter().all(|(_, total)| *total == 3),
            "die Gesamtzahl muss stimmen: {finished_seen:?}"
        );
    }

    /// Bewacht: der fruehe Ausstieg bei leerer Eingabe.
    #[test]
    fn ohne_ton_wird_das_verfahren_gar_nicht_erst_gerufen() {
        let mut calls = 0usize;
        let mut observer = IgnoreChunkProgress;
        let segments = diarize_chunks_with(&[], None, ChunkPlan::default(), &mut observer, |_, _| {
            calls += 1;
            Ok(diarization(Vec::new(), Vec::new()))
        })
        .unwrap();
        assert!(segments.is_empty());
        assert_eq!(calls, 0, "ein leerer Ausschnitt gehoert nicht in CoreML");
    }

    /// Bewacht: `let chunk_speakers = if total_chunks == 1 { exact_speakers }`.
    ///
    /// Unterhalb der Abschnittslaenge muss sich nichts geaendert haben -- die
    /// Vorgabe geht weiter direkt an das Verfahren.
    #[test]
    fn unter_der_abschnittslaenge_reist_die_vorgabe_unveraendert_durch() {
        let samples = vec![0.0f32; 100];
        let mut seen = None;
        let mut observer = IgnoreChunkProgress;

        diarize_chunks_with(
            &samples,
            Some(3),
            plan(300, 100),
            &mut observer,
            |chunk, exact| {
                seen = Some((chunk.len(), exact));
                Ok(diarization(
                    vec![segment(0.0, 1.0, 0)],
                    vec![valid_embedding(0.0)],
                ))
            },
        )
        .unwrap();

        assert_eq!(
            seen,
            Some((100, Some(3))),
            "bei einem Abschnitt bekommt das Verfahren die Zahl selbst"
        );
    }

    /// Bewacht: `if total_chunks == 1 { return Ok(diarization.segments) }`.
    ///
    /// Die Zusage lautet, dass sich unterhalb der Abschnittslaenge NICHTS
    /// aendert. Der Test setzt deshalb eine Eingabe vor, die jede der drei
    /// Bearbeitungen sichtbar machen wuerde: unsortierte Segmente (das
    /// Sortieren wuerde sie umstellen), eine LEERE Schwerpunktliste bei
    /// vorhandenen Segmenten (die Pruefung wuerde scheitern) und
    /// Sprechernummern, die beim Zusammenfuehren neu vergeben wuerden.
    #[test]
    fn unter_der_abschnittslaenge_kommt_das_ergebnis_unveraendert_zurueck() {
        let samples = vec![0.0f32; 100];
        let mut observer = IgnoreChunkProgress;

        let roh = vec![
            segment(50.0, 60.0, 7),
            segment(10.0, 20.0, 3),
            segment(30.0, 40.0, 7),
        ];

        let segments = diarize_chunks_with(
            &samples,
            None,
            plan(300, 100),
            &mut observer,
            |_, _| Ok(diarization(roh.clone(), Vec::new())),
        )
        .expect("ohne Zerlegung darf die leere Schwerpunktliste kein Fehler sein");

        assert_eq!(
            segments, roh,
            "Reihenfolge und Sprechernummern muessen bitgleich bleiben"
        );
    }

    /// Bewacht: die Reststueck-Obergrenze in `ChunkPlan::new`.
    #[test]
    fn ein_plan_dessen_reststueck_die_obergrenze_sprengt_wird_abgelehnt() {
        // 60 min Abschnitt plus bis zu 59 min Rest waere fast zwei Stunden in
        // EINEM Aufruf -- gueltig nach den alten drei Pruefungen.
        let stunde = DIARIZATION_SAMPLE_RATE as usize * 60 * 60;
        assert!(
            ChunkPlan::new(stunde, stunde - 1).is_err(),
            "der laengste Abschnitt waere fast doppelt so lang wie erlaubt"
        );
        // Genau an der Kante muss es noch gehen: 60 min plus 1 Sample Rest
        // ergibt hoechstens 60 min.
        assert!(ChunkPlan::new(stunde, 1).is_ok());
        assert!(ChunkPlan::new(stunde - 100, 101).is_ok());
    }
}

#[cfg(test)]
mod aufloesung_tests {
    use super::*;

    fn segment(start: f64, end: f64, speaker: usize) -> DiarizationSegment {
        DiarizationSegment {
            start_seconds: start,
            end_seconds: end,
            speaker_index: speaker,
        }
    }

    /// Zwei lange Sprecher und eine Gruppe aus kurzen Fetzen dazwischen. Die
    /// Fetzen sind klein UND kurz, also wird die Gruppe aufgeloest.
    #[test]
    fn kleine_fetzen_gruppe_wird_aufgeloest() {
        let mut segments = vec![
            segment(0.0, 60.0, 0),
            segment(60.5, 61.5, 2),
            segment(62.0, 120.0, 1),
            segment(120.5, 121.5, 2),
            segment(122.0, 180.0, 0),
        ];
        // Genug Fetzen, damit die Rate ueber SURPLUS_MIN_SEGMENTS_PER_HOUR
        // liegt -- drei Minuten Spanne verlangen mindestens drei Stueck.
        for index in 0..8 {
            let start = 62.0 + index as f64 * 6.0;
            segments.push(segment(start, start + 0.3, 2));
        }

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 1);
        assert_eq!(distinct_speakers(&segments), 2);
        // Der erste Fetzen folgt auf Sprecher 0 und faellt ihm zu, der zweite
        // auf Sprecher 1.
        assert_eq!(segments[1].speaker_index, 0);
        assert_eq!(segments[3].speaker_index, 1);
    }

    /// Der echte Gast, der nicht im Kalender steht: WENIG Redezeit, aber in
    /// einem zusammenhaengenden Beitrag. Er bleibt.
    #[test]
    fn kleine_gruppe_mit_langem_beitrag_bleibt() {
        let mut segments = vec![
            segment(0.0, 60.0, 0),
            segment(61.0, 71.0, 2),
            segment(72.0, 130.0, 1),
        ];

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 0, "zehn Sekunden am Stueck sind kein Fetzen");
        assert_eq!(distinct_speakers(&segments), 3);
    }

    /// Die grosse Gruppe bleibt auf jeden Fall -- auch wenn sie aus kurzen
    /// Abschnitten besteht.
    #[test]
    fn grosse_gruppe_bleibt_auch_bei_kurzen_abschnitten() {
        let mut segments = vec![segment(0.0, 20.0, 0), segment(21.0, 41.0, 1)];
        for index in 0..30 {
            let start = 42.0 + index as f64 * 2.0;
            segments.push(segment(start, start + 1.0, 2));
        }

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 0, "30 Sekunden Redezeit sind kein Rest-Cluster");
        assert_eq!(distinct_speakers(&segments), 3);
    }

    /// Passt die Zahl schon, wird nichts angefasst -- auch wenn eine Gruppe
    /// die Schutzbedingung erfuellen wuerde.
    #[test]
    fn nichts_passiert_wenn_die_zahl_stimmt() {
        let mut segments = vec![
            segment(0.0, 60.0, 0),
            segment(60.5, 61.5, 1),
            segment(62.0, 120.0, 0),
        ];
        let vorher = segments.clone();

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 0);
        assert_eq!(segments, vorher);
    }

    /// Zwei ueberzaehlige Fetzen-Gruppen werden nacheinander aufgeloest,
    /// beginnend mit der kleinsten.
    #[test]
    fn zwei_fetzen_gruppen_werden_beide_aufgeloest() {
        let mut segments = vec![
            segment(0.0, 60.0, 0),
            segment(62.0, 120.0, 1),
            segment(122.0, 180.0, 0),
        ];
        for index in 0..8 {
            let start = 60.5 + index as f64 * 7.0;
            segments.push(segment(start, start + 0.3, 2));
            segments.push(segment(start + 0.4, start + 0.7, 3));
        }

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 2);
        assert_eq!(distinct_speakers(&segments), 2);
    }

    /// Eine geschuetzte Gruppe stoppt die Kette NICHT, sie wird
    /// uebersprungen. Hier bleibt die 30-Sekunden-Stimme stehen, der
    /// Fetzen-Cluster geht -- und die Vorgabe wird trotzdem nicht erreicht,
    /// was der Fall ist, in dem die Vorgabe selbst zu klein war.
    #[test]
    fn geschuetzte_gruppe_stoppt_die_kette_nicht() {
        let mut segments = vec![
            segment(0.0, 60.0, 0),
            segment(62.0, 120.0, 1),
            segment(121.0, 151.0, 2),
            segment(152.0, 210.0, 0),
        ];
        for index in 0..8 {
            let start = 60.2 + index as f64 * 7.0;
            segments.push(segment(start, start + 0.2, 3));
        }

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 1, "nur der Fetzen, nicht die 30-Sekunden-Stimme");
        assert_eq!(distinct_speakers(&segments), 3);
        let uebrig: std::collections::BTreeSet<usize> =
            segments.iter().map(|s| s.speaker_index).collect();
        assert!(uebrig.contains(&2), "die stille Stimme bleibt: {uebrig:?}");
    }
}

#[cfg(test)]
mod schutzbedingung_tests {
    use super::*;

    fn segment(start: f64, end: f64, speaker: usize) -> DiarizationSegment {
        DiarizationSegment {
            start_seconds: start,
            end_seconds: end,
            speaker_index: speaker,
        }
    }

    /// Zwei Stunden Gespraech mit zwei Hauptrednern, einem Rest-Cluster (viele
    /// Fetzen, ueberall verteilt) und einem STILLEN Menschen (fuenfmal kurz
    /// "ja"). Der Rest geht, der Mensch bleibt.
    ///
    /// Das ist der Fall, den Anteil und Dauer allein falsch entscheiden: der
    /// stille Mensch hat WENIGER Redezeit (3,0 s gegen 108 s) und KUERZERE
    /// Abschnitte (0,6 s gegen 0,6 s) und waere deshalb der erste Kandidat.
    #[test]
    fn stiller_mensch_bleibt_rest_cluster_geht() {
        let mut segments = Vec::new();
        for index in 0..60 {
            let start = index as f64 * 120.0;
            segments.push(segment(start, start + 55.0, 0));
            segments.push(segment(start + 56.0, start + 110.0, 1));
        }
        // Rest-Cluster: 180 Fetzen ueber die ganze Aufnahme.
        for index in 0..180 {
            let start = index as f64 * 40.0 + 10.0;
            segments.push(segment(start, start + 0.6, 2));
        }
        // Stiller Mensch: fuenf kurze Einwuerfe.
        for index in 0..5 {
            let start = index as f64 * 1400.0 + 30.0;
            segments.push(segment(start, start + 0.6, 3));
        }

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 1, "genau eine Gruppe, nicht zwei");
        let uebrig: std::collections::BTreeSet<usize> =
            segments.iter().map(|s| s.speaker_index).collect();
        assert!(uebrig.contains(&3), "der stille Mensch bleibt: {uebrig:?}");
        assert!(!uebrig.contains(&2), "der Rest-Cluster geht: {uebrig:?}");
    }

    /// Dieselbe Gruppe, aber sie taucht nur selten auf: unter der Rate bleibt
    /// sie stehen, auch wenn Anteil und Dauer passen.
    #[test]
    fn seltene_gruppe_bleibt_trotz_kleiner_anteile() {
        let mut segments = vec![segment(0.0, 3000.0, 0), segment(3001.0, 6000.0, 1)];
        // Zehn Fetzen ueber 100 Minuten = 6 je Stunde, unter der Schwelle.
        for index in 0..10 {
            let start = index as f64 * 600.0 + 5.0;
            segments.push(segment(start, start + 0.5, 2));
        }

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 0);
        assert_eq!(distinct_speakers(&segments), 3);
    }

    /// Knapp UEBER der Rate wird dieselbe Gruppe aufgeloest -- die Schwelle
    /// ist damit von beiden Seiten festgenagelt.
    #[test]
    fn dieselbe_gruppe_geht_knapp_ueber_der_rate() {
        let mut segments = vec![segment(0.0, 3000.0, 0), segment(3001.0, 6000.0, 1)];
        // 110 Fetzen ueber dieselbe Spanne = 66 je Stunde, ueber der Schwelle.
        for index in 0..110 {
            let start = index as f64 * 54.0 + 5.0;
            segments.push(segment(start, start + 0.5, 2));
        }

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 1);
        assert_eq!(distinct_speakers(&segments), 2);
    }

    /// Abschnitte ohne Dauer ergeben eine mittlere Dauer von 0 und wuerden den
    /// Dauer-Schutz sonst aushebeln statt ausloesen.
    #[test]
    fn gruppe_ohne_messbare_redezeit_wird_nicht_aufgeloest() {
        let mut segments = vec![segment(0.0, 600.0, 0), segment(601.0, 1200.0, 1)];
        for index in 0..200 {
            let start = index as f64 * 6.0 + 1.0;
            segments.push(segment(start, start, 2));
        }

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 0, "ohne Redezeit ist nichts zu beurteilen");
        assert_eq!(distinct_speakers(&segments), 3);
    }

    /// Die Kennzahlen duerfen sich nicht dadurch aendern, dass eine andere
    /// Gruppe in derselben Runde aufgeloest wird. Hier erbt Sprecher 1 die
    /// Fetzen von Gruppe 2; wuerde danach neu gemessen, saehe er selbst
    /// fetzenhaft aus.
    #[test]
    fn kennzahlen_werden_nicht_durch_geerbte_fetzen_verdorben() {
        let mut segments = vec![segment(0.0, 1800.0, 0), segment(1801.0, 3600.0, 1)];
        for index in 0..120 {
            let start = index as f64 * 15.0 + 1802.0;
            segments.push(segment(start, start + 0.4, 2));
        }

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 1);
        let uebrig: std::collections::BTreeSet<usize> =
            segments.iter().map(|s| s.speaker_index).collect();
        assert_eq!(uebrig.len(), 2, "Sprecher 1 wird nicht selbst zum Opfer");
        assert!(uebrig.contains(&0) && uebrig.contains(&1));
    }

    /// Ein Abschnitt darf nie an eine Gruppe fallen, die in derselben Runde
    /// selbst aufgeloest wird.
    #[test]
    fn fetzen_fallen_nie_an_eine_ebenfalls_aufgeloeste_gruppe() {
        let mut segments = vec![segment(0.0, 1800.0, 0), segment(1801.0, 3600.0, 1)];
        for index in 0..100 {
            let start = index as f64 * 18.0 + 5.0;
            segments.push(segment(start, start + 0.4, 2));
            segments.push(segment(start + 0.5, start + 0.9, 3));
        }

        let resolved = resolve_surplus_speakers(&mut segments, 2);

        assert_eq!(resolved, 2);
        let uebrig: std::collections::BTreeSet<usize> =
            segments.iter().map(|s| s.speaker_index).collect();
        assert_eq!(uebrig, std::collections::BTreeSet::from([0, 1]));
    }
}

/// Die Schwellen der Aufloesung, beidseitig festgenagelt, und ihre
/// Verdrahtung in den echten Lauf.
#[cfg(test)]
mod verdrahtung_tests {
    use super::*;

    fn segment(start: f64, end: f64, speaker: usize) -> DiarizationSegment {
        DiarizationSegment {
            start_seconds: start,
            end_seconds: end,
            speaker_index: speaker,
        }
    }

    /// Zwei Hauptredner plus eine Fetzen-Gruppe mit einstellbarem Profil.
    fn lauf(fetzen: usize, fetzen_dauer: f64, spanne: f64) -> Vec<DiarizationSegment> {
        let mut segments = vec![
            segment(0.0, spanne / 2.0 - 1.0, 0),
            segment(spanne / 2.0, spanne - 1.0, 1),
        ];
        for index in 0..fetzen {
            let start = index as f64 * (spanne / fetzen as f64) + 0.5;
            segments.push(segment(start, start + fetzen_dauer, 2));
        }
        segments
    }

    #[test]
    fn anteilsgrenze_greift_knapp_darunter_und_nicht_darueber() {
        assert!((SURPLUS_MAX_SPEECH_SHARE - 0.08).abs() < f64::EPSILON);

        // 200 Fetzen ueber eine Stunde, Anteil knapp unter der Grenze.
        let mut drunter = lauf(200, 1.2, 3600.0);
        assert_eq!(resolve_surplus_speakers(&mut drunter, 2), 1);

        // Dieselbe Gruppe, nur laenger je Stueck -- Anteil ueber der Grenze.
        let mut drueber = lauf(200, 1.7, 3600.0);
        assert_eq!(resolve_surplus_speakers(&mut drueber, 2), 0);
    }

    #[test]
    fn dauergrenze_greift_knapp_darunter_und_nicht_darueber() {
        assert!((SURPLUS_MAX_MEAN_SEGMENT_SECONDS - 2.5).abs() < f64::EPSILON);

        // Wenige lange Fetzen: der Anteil bleibt klein, die Dauer entscheidet.
        let mut drunter = lauf(70, 2.4, 3600.0);
        assert_eq!(resolve_surplus_speakers(&mut drunter, 2), 1);

        let mut drueber = lauf(70, 2.6, 3600.0);
        assert_eq!(resolve_surplus_speakers(&mut drueber, 2), 0);
    }

    #[test]
    fn ratengrenze_greift_knapp_darueber_und_nicht_darunter() {
        assert!((SURPLUS_MIN_SEGMENTS_PER_HOUR - 60.0).abs() < f64::EPSILON);

        // Spanne 3600 s = 1 h: 61 Abschnitte liegen ueber der Rate, 59 darunter.
        let mut drueber = lauf(61, 0.5, 3600.0);
        assert_eq!(resolve_surplus_speakers(&mut drueber, 2), 1);

        let mut drunter = lauf(59, 0.5, 3600.0);
        assert_eq!(resolve_surplus_speakers(&mut drunter, 2), 0);
    }

    /// Der Test, der fehlte: die Aufloesung muss im ECHTEN Lauf haengen.
    ///
    /// Alle anderen rufen `resolve_surplus_speakers` direkt. Wer den Aufruf in
    /// `diarize_chunks_with` loescht, blieb damit gruen -- genau das hat ein
    /// Mutantenlauf am 22.09.2026 nachgewiesen.
    #[test]
    fn der_zerlegte_lauf_loest_wirklich_auf() {
        let samples = vec![0.1f32; 40];
        let plan = ChunkPlan::new(16, 8).expect("plan");
        let mut observer = IgnoreChunkProgress;

        // Jeder Abschnitt liefert zwei lange Sprecher und viele kurze Fetzen.
        let segments = diarize_chunks_with(&samples, Some(2), plan, &mut observer, |chunk, _| {
            let laenge = chunk.len() as f64 / f64::from(DIARIZATION_SAMPLE_RATE);
            let mut segs = vec![
                DiarizationSegment {
                    start_seconds: 0.0,
                    end_seconds: laenge * 0.45,
                    speaker_index: 0,
                },
                DiarizationSegment {
                    start_seconds: laenge * 0.5,
                    end_seconds: laenge * 0.95,
                    speaker_index: 1,
                },
            ];
            for index in 0..40 {
                let start = index as f64 * (laenge / 40.0);
                segs.push(DiarizationSegment {
                    start_seconds: start,
                    end_seconds: start + laenge * 0.0005,
                    speaker_index: 2,
                });
            }
            Ok(crate::Diarization {
                segments: segs,
                speaker_embeddings: (0..3)
                    .map(|k| {
                        let mut v = vec![0.0f32; 256];
                        v[k] = 1.0;
                        v
                    })
                    .collect(),
            })
        })
        .expect("lauf");

        assert_eq!(
            distinct_speakers(&segments),
            2,
            "die Fetzen-Gruppe muss im echten Lauf verschwinden"
        );
    }

    /// Ein einzelner Abschnitt geht unveraendert zurueck -- auch MIT Vorgabe.
    /// Dort haelt die Bibliothek die Zahl selbst ein, und die Aufloesung hat
    /// nichts zu suchen.
    #[test]
    fn einzelner_abschnitt_bleibt_bitgleich_auch_mit_vorgabe() {
        let samples = vec![0.1f32; 8];
        let plan = ChunkPlan::new(16, 4).expect("plan");
        let mut observer = IgnoreChunkProgress;

        let roh = vec![
            DiarizationSegment {
                start_seconds: 0.0,
                end_seconds: 1.0,
                speaker_index: 0,
            },
            DiarizationSegment {
                start_seconds: 1.0,
                end_seconds: 1.05,
                speaker_index: 2,
            },
            DiarizationSegment {
                start_seconds: 1.1,
                end_seconds: 2.0,
                speaker_index: 1,
            },
        ];

        let segments = diarize_chunks_with(&samples, Some(2), plan, &mut observer, |_, _| {
            Ok(crate::Diarization {
                segments: roh.clone(),
                speaker_embeddings: (0..3)
                    .map(|k| {
                        let mut v = vec![0.0f32; 256];
                        v[k] = 1.0;
                        v
                    })
                    .collect(),
            })
        })
        .expect("lauf");

        assert_eq!(segments.len(), roh.len());
        for (links, rechts) in segments.iter().zip(roh.iter()) {
            assert_eq!(links.speaker_index, rechts.speaker_index);
            assert_eq!(links.start_seconds, rechts.start_seconds);
            assert_eq!(links.end_seconds, rechts.end_seconds);
        }
    }
}
