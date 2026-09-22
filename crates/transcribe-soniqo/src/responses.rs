use owhisper_interface::{batch, stream};

use crate::{DiarizationSegment, FileTranscript, FileTranscriptChunk, SoniqoModel};

pub(crate) const SYNTHETIC_BATCH_WORD_SECONDS: f64 = 0.4;
const MIN_SYNTHETIC_DURATION_SECONDS: f64 = 0.05;
/// Untergrenze einer GEMESSENEN Wortdauer. Vorher stand hier `f64::EPSILON` --
/// 2,2e-16 und relativ zu 1,0: ab etwa vier Sekunden Aufnahmezeit ist die
/// Addition ein No-op, darunter verschwindet sie in der Millisekunden-Rundung
/// der Oberflaeche. Der Waechter las sich, als sei der Fall abgesichert, und
/// griff bei keinem einzigen Wort einer Zwanzig-Minuten-Aufnahme. Eine
/// Millisekunde ist die kleinste Dauer, die bis zur Anzeige ueberlebt.
pub const MIN_WORD_DURATION_SECONDS: f64 = 0.001;

pub fn stream_response_from_text(
    model: SoniqoModel,
    text: String,
    start: f64,
    duration: f64,
    is_final: bool,
    channel_index: &[i32],
) -> stream::StreamResponse {
    let text = normalize_transcript_text(&text);
    let duration = duration.max(MIN_SYNTHETIC_DURATION_SECONDS);
    let words = stream_words_from_text(&text, start, duration);

    stream::StreamResponse::TranscriptResponse {
        start,
        duration,
        is_final,
        speech_final: is_final,
        from_finalize: false,
        channel: stream::Channel {
            alternatives: vec![stream::Alternatives {
                transcript: text,
                words,
                confidence: 1.0,
                languages: vec![],
            }],
        },
        metadata: metadata(model),
        channel_index: channel_index.to_vec(),
    }
}

pub fn batch_response_from_text(
    model: SoniqoModel,
    text: String,
    duration_seconds: f64,
) -> batch::Response {
    batch_response_from_channels(model, vec![FileTranscript::new(text, duration_seconds)])
}

pub fn batch_response_from_channels(
    model: SoniqoModel,
    channels: Vec<FileTranscript>,
) -> batch::Response {
    let channels = if channels.is_empty() {
        vec![FileTranscript::new(String::new(), 0.05)]
    } else {
        channels
    };
    let duration_seconds = channels
        .iter()
        .map(|channel| channel.duration_seconds)
        .fold(0.05, f64::max);
    let has_diarization = channels
        .iter()
        .any(|channel| !channel.speaker_segments.is_empty());
    // Fork: ob echte Chunk-Zeiten vorliegen, entscheidet weiter unten pro Kanal
    // ueber batch_words_from_chunks vs. batch_words_from_text -- also muss es
    // auch das timing_source-Etikett entscheiden. Upstream fragte nur nach
    // Diarisierung und stempelte sonst "synthetic_text", selbst wenn die
    // Wortzeiten aus echten Chunks stammten. Folge im Frontend:
    // isTranscriptWordSeekable (apps/desktop/src/stt/timing.ts:45) haelt jedes
    // Wort fuer unbrauchbar und schaltet das Anspringen im Abspieler ab.
    let has_chunk_timings = channels.iter().any(|channel| !channel.chunks.is_empty());
    // Fork (02.09.2026): gemessene Wortzeiten verdienen ein eigenes Etikett.
    // "provider_segment_interpolated" waere hier gelogen -- die Wortzeiten sind
    // dann nicht mehr gleichverteilt, sondern kommen aus dem Erkenner.
    //
    // Grenze, ausdruecklich: das ist ein `any()` ueber ALLE Kanaele, und das
    // Etikett wird einmal je Antwort gestempelt. Eine gemischte Antwort (ein
    // Kanal mit gemessenen Woertern, einer ohne) etikettiert deshalb auch die
    // synthetischen Woerter als "provider_word". Fuer die Frage "darf man
    // dieses Wort anspringen?" ist das die richtige Fehlerrichtung. Fuer die
    // Frage "ist die Sicherheit dieses Wortes gemessen?" taugt es NICHT -- die
    // Antwort steht am Wort selbst, in `measured_confidence`.
    let has_word_timings = channels
        .iter()
        .any(|channel| channel.chunks.iter().any(|chunk| !chunk.words.is_empty()));
    let diarization_skipped_over_seconds = channels
        .iter()
        .find_map(|channel| channel.diarization_skipped_over_seconds);
    let diarization_failed = channels
        .iter()
        .any(|channel| channel.diarization_failed);
    let metadata = metadata_json(
        model,
        duration_seconds,
        channels.len() as u32,
        has_diarization,
        has_chunk_timings,
        has_word_timings,
        diarization_skipped_over_seconds,
        diarization_failed,
    );

    batch::Response {
        metadata,
        results: batch::Results {
            channels: channels
                .into_iter()
                .enumerate()
                .map(|(channel_index, channel)| {
                    let transcript = normalize_transcript_text(&channel.text);
                    let words = if channel.chunks.is_empty() {
                        let synthetic_duration = synthetic_text_duration(&transcript);
                        batch_words_from_text(
                            &transcript,
                            0.0,
                            synthetic_duration,
                            channel_index as i32,
                        )
                    } else {
                        batch_words_from_chunks(
                            &channel.chunks,
                            &channel.speaker_segments,
                            channel_index as i32,
                        )
                    };
                    batch::Channel {
                        alternatives: vec![batch::Alternatives {
                            words,
                            transcript,
                            confidence: 1.0,
                        }],
                    }
                })
                .collect(),
        },
    }
}

fn metadata(model: SoniqoModel) -> stream::Metadata {
    stream::Metadata {
        model_info: stream::ModelInfo {
            name: model.as_str().to_string(),
            version: "0.0.9".to_string(),
            arch: "soniqo".to_string(),
        },
        extra: Some(stream::Extra::default().into()),
        ..Default::default()
    }
}

fn metadata_json(
    model: SoniqoModel,
    duration_seconds: f64,
    channels: u32,
    has_diarization: bool,
    has_chunk_timings: bool,
    has_word_timings: bool,
    diarization_skipped_over_seconds: Option<f64>,
    diarization_failed: bool,
) -> serde_json::Value {
    let mut value = serde_json::to_value(metadata(model)).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert("duration".to_string(), serde_json::json!(duration_seconds));
        object.insert("channels".to_string(), serde_json::json!(channels));
        // Fork: dritter Fall ergaenzt. "provider_segment_interpolated" ist die
        // ehrliche Beschreibung dessen, was batch_words_from_chunks tut -- die
        // SEGMENT-Grenzen kommen echt vom Anbieter, die Wortzeiten darin sind
        // gleichverteilt. Das Frontend kennt genau diesen Wert
        // (getValidTimingSource, stt/timing.ts:51-53) und laesst solche Woerter
        // anspringen. "synthetic_text" bleibt fuer den Fall ohne jede
        // Chunk-Information reserviert und ist dort auch korrekt.
        object.insert(
            "timing_source".to_string(),
            serde_json::json!(if has_word_timings {
                "provider_word"
            } else if has_diarization {
                "diarized_speech"
            } else if has_chunk_timings {
                "provider_segment_interpolated"
            } else {
                "synthetic_text"
            }),
        );
        // Nur setzen, wenn es zutrifft. Ein Feld, das immer dasteht, muss der
        // Leser jedes Mal auswerten; eines, das nur im Ausnahmefall auftaucht,
        // ist die Nachricht selbst.
        if let Some(limit_seconds) = diarization_skipped_over_seconds {
            object.insert(
                "diarization_skipped_over_seconds".to_string(),
                serde_json::json!(limit_seconds),
            );
        }
        // Dieselbe Regel wie oben: nur im Ausnahmefall vorhanden, damit das
        // Dasein des Feldes schon die Nachricht ist.
        if diarization_failed {
            object.insert(
                "diarization_failed".to_string(),
                serde_json::json!(true),
            );
        }
    }
    value
}

fn stream_words_from_text(text: &str, start: f64, duration: f64) -> Vec<stream::Word> {
    let word_strs = split_words(text);
    let count = word_strs.len();

    if count == 0 {
        return Vec::new();
    }

    word_strs
        .into_iter()
        .enumerate()
        .map(|(index, word)| {
            let word_start = start + (index as f64 / count as f64) * duration;
            let word_end = if index + 1 == count {
                (start + duration - 0.05).max(word_start + 0.05)
            } else {
                start + ((index + 1) as f64 / count as f64) * duration
            };

            stream::Word {
                word: word.to_string(),
                start: word_start,
                end: word_end,
                // Der Live-Weg hat keine gemessene Sicherheit -- der
                // Streaming-Erkenner gibt in `lib.swift` keine heraus. Diese
                // 1,0 ist ein Fuellwert und wird bei der Umwandlung nach
                // `batch::Word` ausdruecklich NICHT als Messung uebernommen
                // (siehe `From<stream::Word>` in owhisper-interface).
                confidence: 1.0,
                speaker: None,
                punctuated_word: Some(word.to_string()),
                language: None,
            }
        })
        .collect()
}

fn batch_words_from_chunks(
    chunks: &[FileTranscriptChunk],
    speaker_segments: &[DiarizationSegment],
    channel: i32,
) -> Vec<batch::Word> {
    let mut words = batch_words_from_chunks_raw(chunks, speaker_segments, channel);

    // Die Glaettung braucht die ganze Wortfolge EINES Kanals, deshalb sitzt
    // sie hier und nicht in `speaker_for_word` -- die Funktion sieht immer nur
    // ein Wort. Begruendung und Messung bei [`smooth_speakers`].
    //
    // Sie laeuft NUR auf gemessenen Wortzeiten. Auf dem anderen Weg
    // (`batch_words_from_diarized_speech`) sind die Wortzeiten ueber die
    // Sprecher-Spannen gleichverteilt: das Ende eines Wortes ist der Anfang
    // des naechsten, jede Luecke ist 0, und ein ganzer Abschnitt wird damit zu
    // EINER pausenlosen Aeusserung. Die Regeln haengen aber an der Pause --
    // ohne sie wuerde die Glaettung genau die Sprecherwechsel einebnen, die
    // dieser Weg ueberhaupt nur transportiert. Gemischte Antworten (ein Chunk
    // mit gemessenen Woertern, einer ohne) bleiben ebenfalls unberuehrt; das
    // ist die sichere Richtung.
    let alle_gemessen = chunks.iter().all(|chunk| !chunk.words.is_empty());
    if !alle_gemessen {
        return words;
    }

    let mut smoothing: Vec<SmoothingWord> = words
        .iter()
        .map(|word| SmoothingWord {
            start_seconds: word.start,
            end_seconds: word.end,
            speaker: word.speaker,
        })
        .collect();
    let changed = smooth_speakers(&mut smoothing);
    if changed > 0 {
        for (word, smoothed) in words.iter_mut().zip(smoothing.iter()) {
            word.speaker = smoothed.speaker;
        }
        // Eine Protokollzeile statt eines zweiten Feldes: die Rohzuordnung im
        // Datenvertrag mitzufuehren haette `batch::Word` geaendert, und das
        // Feld reist bis in die Oberflaeche. Die Zahl hier reicht, um zu
        // sehen, wie stark die Regel greift.
        tracing::debug!(channel, changed, "speaker smoothing changed word speakers");
    }

    words
}

fn batch_words_from_chunks_raw(
    chunks: &[FileTranscriptChunk],
    speaker_segments: &[DiarizationSegment],
    channel: i32,
) -> Vec<batch::Word> {
    chunks
        .iter()
        .flat_map(|chunk| {
            // Fork: gemessene Wortzeiten schlagen jede Verteilung. Sie kommen
            // schon auf der Zeitachse der Aufnahme.
            if !chunk.words.is_empty() {
                return chunk
                    .words
                    .iter()
                    .map(|word| {
                        let start = word.start_seconds.max(0.0);
                        let end = word.end_seconds.max(start + MIN_WORD_DURATION_SECONDS);
                        batch::Word {
                            word: word.text.clone(),
                            start,
                            end,
                            // Fork (03.09.2026): die einzige Stelle im Fork, an
                            // der eine ECHTE Wortsicherheit vorliegt. Die
                            // uebrigen fuenf `1.0` in dieser Datei stehen auf
                            // Wegen ohne gemessene Woerter und bleiben, was sie
                            // waren.
                            //
                            // `confidence` bleibt der nackte Draht: das Feld
                            // ist nicht optional, jeder fehlende Wert wird dort
                            // zu 1,0 -- und danach ist "gemessen" von
                            // "unbekannt" nicht mehr zu unterscheiden. Die
                            // Unterscheidung reist deshalb daneben in
                            // `measured_confidence` mit, unveraendert als
                            // `Option`.
                            confidence: word.confidence.unwrap_or(1.0),
                            measured_confidence: word.confidence,
                            channel,
                            speaker: speaker_for_word(speaker_segments, start, end),
                            punctuated_word: Some(word.text.clone()),
                        }
                    })
                    .collect::<Vec<_>>();
            }

            let text = normalize_transcript_text(&chunk.text);
            if !speaker_segments.is_empty() {
                return batch_words_from_diarized_speech(
                    &text,
                    chunk.start_seconds,
                    chunk.duration_seconds,
                    channel,
                    speaker_segments,
                );
            }

            // Die Woerter eines Abschnitts fuellen SEINE ganze Dauer. Vorher
            // stand hier zusaetzlich eine Decke von 0,4 s je Wort
            // (`synthetic_text_duration`): wo langsam gesprochen wurde, endete
            // das Wortpaket weit vor dem Abschnittsende und drueckte alle
            // Woerter an den Anfang -- ein Abschnitt von 10 s mit fuenf
            // Woertern brachte sie alle in den ersten 2 s unter. Die Decke
            // gehoert dorthin, wo es gar keine gemessene Dauer gibt (siehe
            // `batch_words_from_text` ohne Abschnitte), nicht hierher.
            let duration = chunk.duration_seconds.max(MIN_SYNTHETIC_DURATION_SECONDS);
            let start = chunk.start_seconds.max(0.0);
            batch_words_from_text(&text, start, duration, channel)
        })
        .collect()
}

/// Welcher Sprecher redete waehrend dieses Wortes? Nur fuer den Weg mit
/// gemessenen Wortzeiten -- dort steht die Zeit fest, es ist also eine
/// Nachschlagefrage und keine Verteilungsfrage.
///
/// Fork (02.09.2026), zweiter Anlauf: die erste Fassung war ein reines `find`
/// auf `start <= t < end` und lieferte `None`, sobald die Wortzeit in keine
/// Sprecher-Spanne fiel. Das ist kein Randfall, sondern strukturell -- die
/// Abschnittsgrenzen kommen vom Sprach-Erkenner samt Vorlauf-Polster, die
/// Spannen vom Diarisierer; zwei getrennte Detektoren, und das erste und
/// letzte Wort eines Abschnitts liegt regelmaessig knapp daneben. Und `None`
/// ist hier kein "unbekannt", sondern Datenverlust: die Oberflaeche schreibt
/// den Sprecher-Hinweis nur bei einer Zahl
/// (`apps/desktop/src/store/zustand/listener/utils.ts`), das Wort haengt sonst
/// sprecherlos im Transkript. Der alte Weg ueber
/// [`batch_words_from_diarized_speech`] konnte das gar nicht: er presst die
/// Woerter auf die Vereinigung der Spannen, jedes Wort liegt per Konstruktion
/// in einer.
///
/// Deshalb wird jetzt die ganze Wortspanne befragt und im Zweifel die
/// NAECHSTGELEGENE Spanne genommen: erst die groesste Ueberlappung, sonst der
/// kleinste Abstand. `None` bleibt genau einem Fall vorbehalten -- es gibt
/// ueberhaupt keine Diarisierung. Dort ist es die Wahrheit.
/// Oeffentlich seit 10.09.2026: der Nachholweg auf einer FERTIGEN Aufnahme
/// muss dieselbe Zuordnung treffen wie der Stapellauf. Zwei Kopien dieser
/// Regel wuerden binnen einer Aenderung auseinanderlaufen, und der Unterschied
/// waere ein Wort, das je nach Weg einem anderen Menschen gehoert.
pub fn speaker_for_word(
    segments: &[DiarizationSegment],
    start_seconds: f64,
    end_seconds: f64,
) -> Option<usize> {
    segments
        .iter()
        .map(|segment| {
            let overlap =
                end_seconds.min(segment.end_seconds) - start_seconds.max(segment.start_seconds);
            let gap = if overlap > 0.0 {
                0.0
            } else {
                (segment.start_seconds - end_seconds)
                    .max(start_seconds - segment.end_seconds)
                    .max(0.0)
            };
            (gap, -overlap.max(0.0), segment.speaker_index)
        })
        .min_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        })
        .map(|(_, _, speaker_index)| speaker_index)
}

/// Ein Wort, so weit die Glaettung es kennen muss.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothingWord {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub speaker: Option<usize>,
}

/// Laenge eines Einsprengsels, das noch dem Rahmen zufallen darf.
///
/// Gemessen am 21.09.2026 ueber 10.344 A-B-A-Muster aus 73 getrennten
/// Transkripten: die Haelfte aller Einsprengsel ist 6 Woerter lang oder
/// kuerzer, und Gruppen mit wenig Redeanteil haben im Median 2 Woerter je
/// Block. Drei ist die konservative Grenze -- ein vollstaendiger Einwurf
/// ("Ja, genau.") passt hinein, ein halber Satz nicht mehr.
const SMOOTHING_MAX_INTERJECTION_WORDS: usize = 3;

/// Groesste Luecke, die noch als "keine Pause" gilt.
///
/// Gemessen am selben Material: 31 % aller Luecken vor einem Einsprengsel
/// sind exakt 0 ms, bei 300 ms sind 40 % erreicht, danach wird die Verteilung
/// flach. 300 ms ist auch die uebliche Untergrenze einer hoerbaren
/// Sprechpause -- darunter redet jemand weiter, darueber hat jemand
/// abgesetzt. Genau das ist der Unterschied zwischen einem Verhoerer und
/// einem echten Einwurf.
const SMOOTHING_MAX_GAP_SECONDS: f64 = 0.3;

/// Glaettet die Sprecherzuordnung ueber eine Wortfolge EINES Kanals.
///
/// # Das Problem
///
/// Die Zuordnung Wort zu Sprecher flackert. Gemessen an einer
/// 122,8-Minuten-Raumaufnahme mit vier Personen: 668 Bloecke, davon 26 % mit
/// hoechstens drei Woertern, 99 Ein-Wort-Bloecke, und ein einzelner Satz auf
/// vier Bloecke zweier Sprecher zerhackt. Ueber alle 66 getrennten
/// Transkripte des Bestandes liegen 40 bis 68 % der Bloecke bei hoechstens
/// drei Woertern. Das ist keine Eigenheit dieses Verfahrens -- ein
/// Cloud-Dienst zeigt an derselben Aufnahme fast dieselbe Struktur.
///
/// # Die Regel
///
/// **Einsprengsel.** Ein kurzer Lauf mit anderem Sprecher, eingerahmt vom
/// selben Sprecher, ohne Pause davor UND danach, gehoert dem Rahmen.
///
/// Hier stand bis zum 22.09.2026 eine zweite Regel ("innerhalb einer
/// pausenlosen Aeusserung entscheidet die Mehrheit"). Sie ist ausgebaut,
/// nachdem ihr Beitrag GEMESSEN wurde: an der 122,8-Minuten-Aufnahme haengte
/// sie ueber die erste Regel hinaus **ein einziges** Wort um (54 statt 53),
/// im Bestand ueber 112 Kanal-Stroeme 32 von 306, und am Anteil der Bloecke
/// mit hoechstens drei Woertern aenderte sie gar nichts (34,4 % in beiden
/// Faellen). Dafuer brachte sie eine eigene Fehlerflaeche mit: ohne
/// Laengengrenze ueberschrieb sie JEDEN inneren Minderheitslauf, und ohne
/// Kantenschutz frass sie genau die Einwuerfe, die die erste Regel schuetzt.
/// Eine Regel, die fast nichts tut und viel kaputtmachen kann, gehoert nicht
/// ins Haus.
///
/// # Was ueberleben muss
///
/// Ein echter Einwurf. Deshalb haengt beides an der Pause und nicht allein an
/// der Laenge: ein "Ja." NACH einer Pause bleibt, ein Sprecherwechsel an
/// einer Satzgrenze mit Absetzen bleibt, eine Rueckfrage mitten im Monolog
/// mit Luft davor bleibt. Nur was ohne Absetzen mitten in einer laufenden
/// Aeusserung steht, wird eingemeindet -- und das ist genau die Signatur
/// eines Verhoerers, nicht die eines Menschen, der etwas sagt.
///
/// Gibt zurueck, wie viele Woerter den Sprecher gewechselt haben.
pub fn smooth_speakers(words: &mut [SmoothingWord]) -> usize {
    // Die Regel liest Luecken zwischen benachbarten Woertern. Steht die Folge
    // nicht chronologisch, ist eine "Luecke" keine Aussage ueber eine Pause
    // mehr, sondern ueber die Sortierung -- dann lieber nichts tun als auf
    // einer falschen Grundlage umhaengen.
    if !is_chronological(words) {
        tracing::debug!(
            words = words.len(),
            "speaker smoothing skipped: word timings are not chronological"
        );
        return 0;
    }

    // Die Luecken einmal vorab: die Regel liest sie, waehrend sie die Sprecher
    // schreibt, und die Zeiten aendern sich dabei nicht.
    let gaps: Vec<f64> = (0..words.len())
        .map(|index| {
            if index == 0 {
                f64::INFINITY
            } else {
                words[index].start_seconds - words[index - 1].end_seconds
            }
        })
        .collect();

    // Und die Sprecher ebenfalls einmal vorab. Das ist kein Vorgriff auf
    // Geschwindigkeit, sondern eine Festlegung: ALLE Entscheidungen fallen auf
    // dem urspruenglichen Stand. Vorher las die Schleife `words` direkt und
    // damit teils schon eigene Ergebnisse -- bei A-B-A-C-A waere der Rahmen
    // fuer C erst durch das Umhaengen von B entstanden, und das Ergebnis haette
    // von der Reihenfolge abgehangen, in der die Schleife laeuft. Ein
    // Einsprengsel ist ein Einsprengsel im ORIGINAL oder gar nicht.
    let original: Vec<Option<usize>> = words.iter().map(|word| word.speaker).collect();
    let runs = speaker_runs(words);

    let mut changed = 0;
    for position in 1..runs.len().saturating_sub(1) {
        let (before_start, _) = runs[position - 1];
        let (run_start, run_end) = runs[position];
        let (after_start, _) = runs[position + 1];

        if original[before_start] != original[after_start]
            || original[before_start] == original[run_start]
        {
            continue;
        }
        if run_end - run_start + 1 > SMOOTHING_MAX_INTERJECTION_WORDS {
            continue;
        }
        if gaps[run_start] >= SMOOTHING_MAX_GAP_SECONDS
            || gaps[after_start] >= SMOOTHING_MAX_GAP_SECONDS
        {
            continue;
        }

        let frame = original[before_start];
        for word in words[run_start..=run_end].iter_mut() {
            if word.speaker != frame {
                word.speaker = frame;
                changed += 1;
            }
        }
    }

    changed
}

/// Groesste Rueckwaertsbewegung, die noch als Messrauschen durchgeht.
///
/// GEMESSEN, nicht gegriffen (22.09.2026): in der 122,8-Minuten-Aufnahme
/// ueberlappen 558 von 17.191 Wortpaaren (3,2 %), die groesste Ueberlappung
/// betraegt **0,112 s**, keine liegt unter -0,2 s. Im Bestand haben 89 von 112
/// Kanal-Stroemen mindestens eine Ueberlappung ueber 0,05 s. Der erste Wurf
/// dieser Konstante stand auf 0,05 s und schaltete die Glaettung damit
/// praktisch ueberall ab -- an der Testaufnahme von 140 umgehaengten Woertern
/// auf 0.
///
/// 0,2 s laesst das Messrauschen des Erkenners durch (Faktor 1,8 Reserve auf
/// den groessten gemessenen Wert) und bleibt klar unter
/// [`SMOOTHING_MAX_GAP_SECONDS`]. Was der Waechter fangen soll, ist eine
/// unsortierte Wortfolge, und die springt um Sekunden, nicht um Millisekunden.
const SMOOTHING_MAX_BACKWARD_SECONDS: f64 = 0.2;

/// Laeuft die Wortfolge vorwaerts?
fn is_chronological(words: &[SmoothingWord]) -> bool {
    words.windows(2).all(|pair| {
        pair[1].start_seconds - pair[0].end_seconds >= -SMOOTHING_MAX_BACKWARD_SECONDS
            && pair[1].start_seconds >= pair[0].start_seconds - SMOOTHING_MAX_BACKWARD_SECONDS
    })
}

/// Zusammenhaengende Laeufe gleichen Sprechers, als (erster, letzter) Index.
fn speaker_runs(words: &[SmoothingWord]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start = 0;
    for index in 1..=words.len() {
        if index == words.len() || words[index].speaker != words[start].speaker {
            runs.push((start, index - 1));
            start = index;
        }
    }
    runs
}

fn batch_words_from_diarized_speech(
    text: &str,
    start: f64,
    duration: f64,
    channel: i32,
    speaker_segments: &[DiarizationSegment],
) -> Vec<batch::Word> {
    let words = split_words(text);
    if words.is_empty() {
        return Vec::new();
    }

    let chunk_start = start.max(0.0);
    let chunk_end = chunk_start + duration.max(MIN_SYNTHETIC_DURATION_SECONDS);
    let mut spans = speaker_segments
        .iter()
        .filter_map(|segment| {
            let start = segment.start_seconds.max(chunk_start);
            let end = segment.end_seconds.min(chunk_end);
            (end > start).then_some((start, end, segment.speaker_index))
        })
        .collect::<Vec<_>>();
    spans.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.total_cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    let mut non_overlapping = Vec::with_capacity(spans.len());
    let mut previous_end = chunk_start;
    for (start, end, speaker) in spans {
        let start = start.max(previous_end);
        if end > start {
            non_overlapping.push((start, end, speaker));
            previous_end = end;
        }
    }
    let spans = non_overlapping;

    let active_duration = spans.iter().map(|(start, end, _)| end - start).sum::<f64>();
    if active_duration <= 0.0 {
        let duration =
            synthetic_text_duration(text).min(duration.max(MIN_SYNTHETIC_DURATION_SECONDS));
        return batch_words_from_text(text, chunk_start, duration, channel);
    }

    let position = |offset: f64| {
        let mut remaining = offset.clamp(0.0, active_duration);
        for (index, (start, end, speaker)) in spans.iter().enumerate() {
            let span_duration = end - start;
            if remaining <= span_duration || index + 1 == spans.len() {
                return ((start + remaining.min(span_duration)), index, *speaker);
            }
            remaining -= span_duration;
        }
        (chunk_end, spans.len() - 1, spans[spans.len() - 1].2)
    };
    let count = words.len();

    words
        .into_iter()
        .enumerate()
        .map(|(index, word)| {
            let start_offset = index as f64 / count as f64 * active_duration;
            let end_offset = (index + 1) as f64 / count as f64 * active_duration;
            let midpoint_offset = (start_offset + end_offset) / 2.0;
            let (mapped_start, _, _) = position(start_offset);
            let (mapped_end, _, _) = position(end_offset);
            let (midpoint, span_index, speaker) = position(midpoint_offset);
            let (span_start, span_end, _) = spans[span_index];
            let word_start = mapped_start.max(span_start).min(span_end);
            let word_end = mapped_end
                .min(span_end)
                .max((word_start + MIN_SYNTHETIC_DURATION_SECONDS).min(span_end));
            let (word_start, word_end) = if word_end > word_start {
                (word_start, word_end)
            } else {
                let half = MIN_SYNTHETIC_DURATION_SECONDS / 2.0;
                (
                    (midpoint - half).max(span_start),
                    (midpoint + half).min(span_end),
                )
            };

            batch::Word {
                word: word.to_string(),
                start: word_start,
                end: word_end.max(word_start + f64::EPSILON),
                confidence: 1.0,
                // Erfunden, nicht gemessen -- diese Woerter sind ueber die
                // Sprecherspanne verteilt, der Erkenner hat zu ihnen nichts
                // gesagt.
                measured_confidence: None,
                channel,
                speaker: Some(speaker),
                punctuated_word: Some(word.to_string()),
            }
        })
        .collect()
}

fn batch_words_from_text(text: &str, start: f64, duration: f64, channel: i32) -> Vec<batch::Word> {
    let word_strs = split_words(text);
    let count = word_strs.len();

    if count == 0 {
        return Vec::new();
    }

    word_strs
        .into_iter()
        .enumerate()
        .map(|(index, word)| batch::Word {
            word: word.to_string(),
            start: start + (index as f64 / count as f64) * duration,
            end: start + ((index + 1) as f64 / count as f64) * duration,
            confidence: 1.0,
            // Aus reinem Text gleichverteilt -- hier gibt es nichts zu messen.
            measured_confidence: None,
            channel,
            speaker: None,
            punctuated_word: Some(word.to_string()),
        })
        .collect()
}

fn split_words(text: &str) -> Vec<&str> {
    text.split_whitespace()
        .filter(|word| !word.is_empty())
        .collect()
}

fn normalize_transcript_text(text: &str) -> String {
    split_words(text).join(" ")
}

fn synthetic_text_duration(text: &str) -> f64 {
    let word_count = split_words(text).len();
    if word_count == 0 {
        MIN_SYNTHETIC_DURATION_SECONDS
    } else {
        (word_count as f64 * SYNTHETIC_BATCH_WORD_SECONDS).max(MIN_SYNTHETIC_DURATION_SECONDS)
    }
}

#[cfg(test)]
mod glaettung_tests {
    use super::*;

    /// Synthetische Woerter, keine echten Namen: `(start_ms, end_ms, sprecher)`.
    fn woerter(spec: &[(i64, i64, usize)]) -> Vec<SmoothingWord> {
        spec.iter()
            .map(|(start, end, speaker)| SmoothingWord {
                start_seconds: *start as f64 / 1000.0,
                end_seconds: *end as f64 / 1000.0,
                speaker: Some(*speaker),
            })
            .collect()
    }

    fn sprecher(words: &[SmoothingWord]) -> Vec<usize> {
        words.iter().filter_map(|word| word.speaker).collect()
    }

    /// Regel 1 greift: ein einzelnes Wort mit fremdem Sprecher, mitten in
    /// einer laufenden Aeusserung, ohne Absetzen davor oder danach.
    #[test]
    fn einsprengsel_ohne_pause_faellt_an_den_rahmen() {
        let mut words = woerter(&[
            (0, 300, 0),
            (300, 600, 0),
            (600, 900, 1),
            (900, 1200, 0),
            (1200, 1500, 0),
        ]);

        let changed = smooth_speakers(&mut words);

        assert_eq!(changed, 1);
        assert_eq!(sprecher(&words), vec![0, 0, 0, 0, 0]);
    }

    /// Regel 1 laesst los: derselbe Einwurf, aber mit deutlicher Pause davor.
    /// Das ist ein Mensch, der etwas sagt, kein Verhoerer.
    #[test]
    fn einwurf_nach_pause_bleibt_stehen() {
        let mut words = woerter(&[
            (0, 300, 0),
            (300, 600, 0),
            (1800, 2100, 1),
            (2100, 2400, 0),
            (2400, 2700, 0),
        ]);

        let changed = smooth_speakers(&mut words);

        assert_eq!(changed, 0, "der Einwurf hat Luft davor und gehoert ihm");
        assert_eq!(sprecher(&words), vec![0, 0, 1, 0, 0]);
    }

    /// Auch die Pause DANACH schuetzt: wer absetzt, nachdem er etwas gesagt
    /// hat, hat es gesagt.
    #[test]
    fn einwurf_mit_pause_danach_bleibt_stehen() {
        let mut words = woerter(&[
            (0, 300, 0),
            (300, 600, 0),
            (600, 900, 1),
            (2400, 2700, 0),
            (2700, 3000, 0),
        ]);

        let changed = smooth_speakers(&mut words);

        assert_eq!(changed, 0);
        assert_eq!(sprecher(&words), vec![0, 0, 1, 0, 0]);
    }

    /// Ein ganzer Satz ist kein Einsprengsel, auch wenn er pausenlos
    /// anschliesst.
    ///
    /// Fixture bewusst 3 + 4 + 3: der Rahmen hat die Mehrheit (sechs gegen
    /// vier), und die Aeusserung ist durchgehend pausenlos. Die erste Fassung
    /// stand auf 2 + 4 + 1 und bestand nur, weil der mittlere Lauf dort
    /// SELBST die Mehrheit war; sie haette die inzwischen ausgebaute
    /// Mehrheitsregel nicht gefangen. Ein Zweitblick hat das am 22.09.2026
    /// gefunden -- das Fixture bleibt scharf, auch ohne jene Regel.
    #[test]
    fn langer_beitrag_ohne_pause_bleibt_stehen() {
        let mut words = woerter(&[
            (0, 300, 0),
            (300, 600, 0),
            (600, 900, 0),
            (900, 1200, 1),
            (1200, 1500, 1),
            (1500, 1800, 1),
            (1800, 2100, 1),
            (2100, 2400, 0),
            (2400, 2700, 0),
            (2700, 3000, 0),
        ]);
        let runs = speaker_runs(&words);
        assert_eq!(runs.len(), 3, "drei Laeufe: Rahmen, Beitrag, Rahmen");
        assert_eq!(runs[1].1 - runs[1].0 + 1, 4, "vier Woerter, ueber der Grenze");

        let changed = smooth_speakers(&mut words);

        assert_eq!(changed, 0, "vier Woerter sind kein Einsprengsel");
        assert_eq!(sprecher(&words), vec![0, 0, 0, 1, 1, 1, 1, 0, 0, 0]);
    }

    /// Der Gegenbeweis zum Test darueber: derselbe Aufbau, aber der innere
    /// Lauf ist drei Woerter kurz. Dann greift die Regel.
    #[test]
    fn kurzer_innerer_lauf_faellt_an_den_rahmen() {
        let mut words = woerter(&[
            (0, 300, 0),
            (300, 600, 0),
            (600, 900, 0),
            (900, 1200, 1),
            (1200, 1500, 1),
            (1500, 1800, 1),
            (1800, 2100, 0),
            (2100, 2400, 0),
            (2400, 2700, 0),
        ]);

        let changed = smooth_speakers(&mut words);

        assert_eq!(changed, 3);
        assert_eq!(sprecher(&words), vec![0; 9]);
    }

    /// Die Regel holt auch einen Einsprengsel, dessen Rahmen in der
    /// umgebenden Aeusserung die MINDERHEIT ist -- das war der Fall, den die
    /// ausgebaute Mehrheitsregel nie konnte.
    #[test]
    fn einsprengsel_faellt_an_den_rahmen_auch_gegen_die_mehrheit() {
        let mut words = woerter(&[
            (0, 300, 1),
            (300, 600, 1),
            (600, 900, 1),
            (900, 1200, 1),
            (1200, 1500, 0),
            (1500, 1800, 2),
            (1800, 2100, 0),
        ]);

        let changed = smooth_speakers(&mut words);

        assert_eq!(changed, 1);
        assert_eq!(sprecher(&words), vec![1, 1, 1, 1, 0, 0, 0]);
    }

    /// Ein Ein-Wort-Einsprengsel mitten in einer pausenlosen Folge.
    #[test]
    fn ein_wort_einsprengsel_faellt_an_den_rahmen() {
        let mut words = woerter(&[
            (0, 300, 0),
            (300, 600, 0),
            (600, 900, 0),
            (900, 1200, 1),
            (1200, 1500, 0),
        ]);

        let changed = smooth_speakers(&mut words);

        assert!(changed >= 1);
        assert_eq!(sprecher(&words), vec![0, 0, 0, 0, 0]);
    }

    /// Zwei Aeusserungen, jede mit ihrem eigenen Sprecher, getrennt durch
    /// eine Pause. Hier darf nichts wandern.
    #[test]
    fn zwei_aeusserungen_behalten_ihre_sprecher() {
        let mut words = woerter(&[
            (0, 300, 0),
            (300, 600, 0),
            (300 + 2000, 600 + 2000, 1),
            (600 + 2000, 900 + 2000, 1),
        ]);

        let changed = smooth_speakers(&mut words);

        assert_eq!(changed, 0);
        assert_eq!(sprecher(&words), vec![0, 0, 1, 1]);
    }

    /// Rand: zu wenige Woerter, um irgendetwas zu entscheiden.
    #[test]
    fn zwei_woerter_bleiben_unberuehrt() {
        let mut words = woerter(&[(0, 300, 0), (300, 600, 1)]);

        assert_eq!(smooth_speakers(&mut words), 0);
        assert_eq!(sprecher(&words), vec![0, 1]);
    }

    /// Rand: zwei gleich lange Laeufe ohne Rahmen -- es gibt keinen
    /// Einsprengsel, also passiert nichts.
    #[test]
    fn zwei_gleich_lange_laeufe_bleiben_stehen() {
        let mut words = woerter(&[
            (0, 300, 0),
            (300, 600, 0),
            (600, 900, 1),
            (900, 1200, 1),
        ]);

        let changed = smooth_speakers(&mut words);

        assert_eq!(changed, 0, "kein Rahmen, kein Einsprengsel");
        assert_eq!(sprecher(&words), vec![0, 0, 1, 1]);
    }
}

#[cfg(test)]
mod glaettung_grenzen_tests {
    use super::*;
    use crate::{FileTranscript, FileTranscriptChunk, SoniqoModel};

    fn segment(start: f64, end: f64, speaker: usize) -> DiarizationSegment {
        DiarizationSegment {
            start_seconds: start,
            end_seconds: end,
            speaker_index: speaker,
        }
    }

    /// Unsortierte Wortzeiten sind keine Grundlage fuer eine Pausenregel.
    #[test]
    fn unsortierte_woerter_bleiben_unberuehrt() {
        let mut words = vec![
            SmoothingWord {
                start_seconds: 0.0,
                end_seconds: 0.3,
                speaker: Some(0),
            },
            SmoothingWord {
                start_seconds: 5.0,
                end_seconds: 5.3,
                speaker: Some(1),
            },
            SmoothingWord {
                start_seconds: 0.3,
                end_seconds: 0.6,
                speaker: Some(0),
            },
        ];
        let vorher: Vec<Option<usize>> = words.iter().map(|w| w.speaker).collect();

        assert_eq!(smooth_speakers(&mut words), 0);
        assert_eq!(
            words.iter().map(|w| w.speaker).collect::<Vec<_>>(),
            vorher
        );
    }

    /// Eine kleine Ueberlappung an der Wortgrenze ist Messrauschen und darf
    /// die Glaettung nicht abschalten.
    #[test]
    fn kleine_ueberlappung_schaltet_die_glaettung_nicht_ab() {
        let mut words = vec![
            SmoothingWord {
                start_seconds: 0.0,
                end_seconds: 0.31,
                speaker: Some(0),
            },
            SmoothingWord {
                start_seconds: 0.30,
                end_seconds: 0.60,
                speaker: Some(1),
            },
            SmoothingWord {
                start_seconds: 0.60,
                end_seconds: 0.90,
                speaker: Some(0),
            },
        ];

        assert_eq!(smooth_speakers(&mut words), 1);
    }

    /// Ohne gemessene Wortzeiten laeuft die Glaettung gar nicht: dort ist
    /// jede Luecke 0 und der ganze Abschnitt waere EINE Aeusserung.
    #[test]
    fn ohne_gemessene_wortzeiten_wird_nicht_geglaettet() {
        let chunk = FileTranscriptChunk {
            text: "eins zwei drei vier fuenf".to_string(),
            start_seconds: 0.0,
            duration_seconds: 5.0,
            words: Vec::new(),
        };
        let mut channel = FileTranscript::new("eins zwei drei vier fuenf".to_string(), 5.0);
        channel.chunks = vec![chunk];
        channel.speaker_segments = vec![
            segment(0.0, 2.0, 0),
            segment(2.0, 3.0, 1),
            segment(3.0, 5.0, 0),
        ];

        let response = batch_response_from_channels(SoniqoModel::ParakeetBatch, vec![channel]);
        let words = &response.results.channels[0].alternatives[0].words;

        // Der mittlere Sprecher ueberlebt -- geglaettet waere er weg.
        let sprecher: std::collections::BTreeSet<Option<usize>> =
            words.iter().map(|word| word.speaker).collect();
        assert!(
            sprecher.contains(&Some(1)),
            "ohne Glaettung bleibt Sprecher 1 stehen: {sprecher:?}"
        );
    }
}

/// Die Schwellen der Glaettung, beidseitig festgenagelt.
///
/// Jeder dieser Tests stirbt, wenn die zugehoerige Konstante in EINE Richtung
/// wandert -- und der jeweils andere, wenn sie in die andere wandert. Ohne
/// dieses Paar liesse sich eine Schwelle ueber einen weiten Bereich
/// verschieben, ohne dass etwas rot wird.
#[cfg(test)]
mod schwellen_tests {
    use super::*;

    fn kette(laufwoerter: usize, luecke_ms: i64) -> Vec<SmoothingWord> {
        let mut out = Vec::new();
        let mut t = 0i64;
        let push = |start: i64, ende: i64, sp: usize, out: &mut Vec<SmoothingWord>| {
            out.push(SmoothingWord {
                start_seconds: start as f64 / 1000.0,
                end_seconds: ende as f64 / 1000.0,
                speaker: Some(sp),
            });
        };
        for _ in 0..3 {
            push(t, t + 300, 0, &mut out);
            t += 300;
        }
        t += luecke_ms;
        for _ in 0..laufwoerter {
            push(t, t + 300, 1, &mut out);
            t += 300;
        }
        t += luecke_ms;
        for _ in 0..3 {
            push(t, t + 300, 0, &mut out);
            t += 300;
        }
        out
    }

    fn geaendert(mut words: Vec<SmoothingWord>) -> usize {
        smooth_speakers(&mut words)
    }

    #[test]
    fn wortgrenze_greift_bei_drei_und_nicht_bei_vier() {
        assert_eq!(SMOOTHING_MAX_INTERJECTION_WORDS, 3);
        assert_eq!(geaendert(kette(3, 0)), 3, "drei Woerter sind ein Einsprengsel");
        assert_eq!(geaendert(kette(4, 0)), 0, "vier sind keiner mehr");
    }

    #[test]
    fn pausengrenze_greift_unter_300ms_und_nicht_darueber() {
        assert!((SMOOTHING_MAX_GAP_SECONDS - 0.3).abs() < f64::EPSILON);
        assert_eq!(geaendert(kette(2, 299)), 2, "299 ms sind keine Pause");
        assert_eq!(geaendert(kette(2, 300)), 0, "300 ms sind eine");
    }

    #[test]
    fn rueckwaertsgrenze_laesst_messrauschen_durch_und_faengt_sortierfehler() {
        assert!((SMOOTHING_MAX_BACKWARD_SECONDS - 0.2).abs() < f64::EPSILON);

        let mut rauschen = kette(1, 0);
        // 199 ms Ueberlappung: noch Messrauschen.
        rauschen[3].start_seconds -= 0.199;
        assert_eq!(geaendert(rauschen), 1);

        let mut sortierfehler = kette(1, 0);
        sortierfehler[3].start_seconds -= 0.201;
        assert_eq!(geaendert(sortierfehler), 0);
    }
}
