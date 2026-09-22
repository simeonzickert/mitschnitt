mod batch;
mod message;
mod recorded;
mod response;
mod streaming;

pub use recorded::*;
pub use streaming::*;

use std::path::Path;
use std::time::Duration;

use anlg_transcribe_core::TARGET_SAMPLE_RATE;
use owhisper_interface::ListenParams;
use owhisper_interface::stream::{Extra, Metadata, ModelInfo};

pub(crate) const DEFAULT_REDEMPTION_TIME: Duration = Duration::from_millis(400);

#[derive(Debug, Clone)]
pub(crate) struct Segment {
    pub text: String,
    pub start: f64,
    pub duration: f64,
    pub confidence: f64,
    pub language: Option<String>,
}

pub(crate) fn parse_listen_params(query: &str) -> Result<ListenParams, serde_html_form::de::Error> {
    serde_html_form::from_str(query)
}

pub(crate) fn build_metadata(model_path: &Path) -> Metadata {
    let model_name = model_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("whisper-local")
        .to_string();

    Metadata {
        model_info: ModelInfo {
            name: model_name,
            version: "1.0".to_string(),
            arch: "whisper-local".to_string(),
        },
        extra: Some(Extra::default().into()),
        ..Default::default()
    }
}

pub(crate) fn redemption_time(params: &ListenParams) -> Duration {
    params
        .custom_query
        .as_ref()
        .and_then(|q| q.get("redemption_time_ms"))
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_REDEMPTION_TIME)
}

/// Fork-Abweichung (31.08.2026): der Batch-Weg bekommt eine eigene Sitzung im
/// Batch-Modus. Der Live-Weg (`build_model_with_languages`) bleibt woertlich,
/// wie er war -- seine Parameter sind auf Latenz getrimmt und sollen es bleiben.
pub(crate) fn build_batch_model(
    loaded_model: &anlg_whisper_local::LoadedWhisper,
    params: &ListenParams,
) -> Result<anlg_whisper_local::Whisper, crate::Error> {
    let mut model = loaded_model
        .session_with_mode(
            params
                .languages
                .iter()
                .filter_map(|lang| lang.clone().try_into().ok())
                .collect(),
            anlg_whisper_local::TranscribeMode::Batch,
        )
        .map_err(crate::Error::from)?;

    // Fork-Abweichung (02.09.2026): Die Stichwortliste wird hier tatsaechlich
    // ANGEWENDET. `keywords` wurde in diesem Dienst seit jeher geparst und nie
    // benutzt -- es stand da und tat nichts. `vocabulary` ist der getrennte
    // Kanal, und aus ihm entsteht die Kontext-Vorgabe fuer den Dekoder.
    //
    // Bewusst NICHT `keywords`: dort stehen auch automatisch aus der Notiz
    // gezogene Schlagwoerter, und die dem Modell als erwuenschte Woerter
    // vorzulegen ist etwas anderes, als ihm Eigennamen beizubringen.
    //
    // Nur die richtigen Schreibweisen gehen hinein. Eine eingetragene
    // Verhoerung ("Sedatsch") als erwuenschtes Wort vorzulegen waere das
    // Gegenteil der Absicht -- `canonical_terms` schneidet sie ab.
    if !params.vocabulary.is_empty() {
        let vocabulary = anlg_vocabulary::Vocabulary::parse(&params.vocabulary);
        model.set_glossary(&vocabulary.canonical_terms());
    }

    Ok(model)
}

pub(crate) fn load_model(
    model_path: &Path,
) -> Result<anlg_whisper_local::LoadedWhisper, crate::Error> {
    anlg_whisper_local::LoadedWhisper::builder()
        .model_path(model_path.to_string_lossy().into_owned())
        .build()
        .map_err(crate::Error::from)
}

pub(crate) fn build_model_with_languages(
    loaded_model: &anlg_whisper_local::LoadedWhisper,
    languages: Vec<anlg_whisper::Language>,
) -> Result<anlg_whisper_local::Whisper, crate::Error> {
    loaded_model.session(languages).map_err(crate::Error::from)
}

/// Lautstaerke-Schwelle, unter der ein Abschnitt gar nicht erst ans Modell geht.
///
/// Derselbe Wert, den der Soniqo-Pfad seit jeher benutzt
/// (`SONIQO_DIRECT_MIC_MIN_RMS`, listener2-core) -- bewusst uebernommen statt neu
/// erfunden, damit beide Anbieter dieselbe Vorstellung von "still" haben.
///
/// An des Betreibers Teammeeting gemessen (31.08.2026, Mikrofonspur):
///   Stille (Sek. 0-100):   RMS 0.000122
///   Rede  (Sek. 402-448):  RMS 0.059282
/// Faktor 486 zwischen beidem, die Schwelle liegt sauber dazwischen.
const MIN_SPEECH_RMS: f32 = 0.0008;

/// Fork-Abweichung (31.08.2026): stille Abschnitte werden uebersprungen.
///
/// Whisper erfindet in Stille Text -- das bekannteste Muster ist ein
/// wiederholtes "Vielen Dank" (im Englischen "Thank you." oder Untertitel-
/// Abspaenne). In des Betreibers Transkript stand woertlich "Vielen Dank. Vielen Dank.
/// Dank. Danke Bo." Die ersten drei hat niemand gesagt; sie stammen aus den
/// sechs Minuten Stille, bevor er das Wort ergriff.
///
/// Wichtig, weil es die naheliegende Reparatur ausschliesst: die
/// Dekoder-Parameter helfen NICHT. An echtem Material geprueft (whisper-cli,
/// large-v3-turbo, derselbe Ausschnitt) blieb "Vielen Dank" unveraendert stehen
/// mit `--max-context 0`, mit `--no-speech-thold 0.8` und mit beidem zusammen.
/// Das Modell halluziniert nicht wegen falscher Schwellen, sondern weil ihm
/// ueberhaupt Stille vorgelegt wird.
///
/// Der Soniqo-Pfad hatte dieses Gate immer (bei des Betreibers Aufnahme uebersprang es
/// 37 von 42 Abschnitten der Mikrofonspur); dem Whisper-Pfad fehlte es. Die
/// sprachbewusste Zerlegung davor schneidet an Sprechpausen, wirft aber leise
/// Abschnitte nicht weg -- Atmen und Tastaturgeraeusche reichen Whisper.
fn chunk_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    (sum / samples.len() as f64).sqrt() as f32
}

pub(crate) fn transcribe_chunk(
    model: &mut anlg_whisper_local::Whisper,
    samples: &[f32],
    chunk_start_sec: f64,
) -> Result<Vec<Segment>, crate::Error> {
    let mode = model.mode();

    // Nur im Batch. Der Live-Pfad bekommt seine Abschnitte vom Strom und ist auf
    // Reaktionszeit gebaut; dort waere ein zusaetzliches Gate eine Verhaltens-
    // aenderung ohne Anlass.
    if mode == anlg_whisper_local::TranscribeMode::Batch {
        let rms = chunk_rms(samples);
        if rms < MIN_SPEECH_RMS {
            tracing::debug!(
                chunk.start_sec = chunk_start_sec,
                chunk.rms = rms,
                chunk.minimum_rms = MIN_SPEECH_RMS,
                "whisper_batch_silent_chunk_skipped"
            );
            return Ok(Vec::new());
        }
    }

    let raw_segments = model.transcribe(samples)?;
    let chunk_duration_sec = samples.len() as f64 / TARGET_SAMPLE_RATE as f64;

    Ok(build_chunk_segments_for_mode(
        raw_segments,
        chunk_start_sec,
        chunk_duration_sec,
        mode,
    ))
}

/// Fork-Abweichung (31.08.2026): im Batch-Modus werden die vom Dekoder
/// gelieferten Segmentzeiten UNVERAENDERT uebernommen (nur auf das Chunkfenster
/// begrenzt und um den Chunkanfang verschoben).
///
/// Der Bestandsweg darunter tut etwas anderes: er streckt die Segmente so, dass
/// das letzte exakt am Chunkende endet. Das ist sinnvoll, solange die Rohzeiten
/// wertlos sind -- und genau das waren sie im Live-Modus, wo
/// `no_timestamps(true)` gilt. Mit echten Zeitmarken waere dieselbe Streckung
/// eine Verfaelschung: eine Sprechpause am Chunkende wuerde in das letzte
/// Segment hineingezogen.
///
/// Faellt im Batch KEIN einziges brauchbares Zeitpaar an, greift bewusst der
/// alte Weg. Ein Chunk ohne Zeiten soll Text liefern, nicht verschwinden.
fn build_chunk_segments_for_mode(
    raw_segments: Vec<anlg_whisper_local::Segment>,
    chunk_start_sec: f64,
    chunk_duration_sec: f64,
    mode: anlg_whisper_local::TranscribeMode,
) -> Vec<Segment> {
    if mode.is_batch()
        && let Some(segments) =
            build_batch_chunk_segments(&raw_segments, chunk_start_sec, chunk_duration_sec)
    {
        return segments;
    }

    build_chunk_segments(raw_segments, chunk_start_sec, chunk_duration_sec)
}

/// `None`, wenn die Rohzeiten kein einziges brauchbares Paar hergeben -- dann
/// entscheidet der Aufrufer, worauf er zurueckfaellt.
fn build_batch_chunk_segments(
    raw_segments: &[anlg_whisper_local::Segment],
    chunk_start_sec: f64,
    chunk_duration_sec: f64,
) -> Option<Vec<Segment>> {
    if chunk_duration_sec <= 0.0 {
        return None;
    }

    let mut segments = Vec::with_capacity(raw_segments.len());

    for raw in raw_segments {
        let text = raw.text().trim().to_string();
        if text.is_empty() {
            continue;
        }

        let (start, end) = (raw.start(), raw.end());
        if !start.is_finite() || !end.is_finite() {
            continue;
        }

        let start = start.clamp(0.0, chunk_duration_sec);
        let end = end.clamp(0.0, chunk_duration_sec);
        if end <= start {
            continue;
        }

        segments.push(Segment {
            text,
            start: chunk_start_sec + start,
            duration: end - start,
            confidence: raw.confidence() as f64,
            language: raw.language().map(|value| value.to_string()),
        });
    }

    (!segments.is_empty()).then_some(segments)
}

fn build_chunk_segments(
    raw_segments: Vec<anlg_whisper_local::Segment>,
    chunk_start_sec: f64,
    chunk_duration_sec: f64,
) -> Vec<Segment> {
    if chunk_duration_sec <= 0.0 {
        return vec![];
    }

    let raw_segments: Vec<_> = raw_segments
        .into_iter()
        .filter_map(|segment| {
            let text = segment.text().trim().to_string();
            if text.is_empty() {
                return None;
            }

            Some((
                segment.start(),
                segment.end(),
                Segment {
                    text,
                    start: 0.0,
                    duration: 0.0,
                    confidence: segment.confidence() as f64,
                    language: segment.language().map(|value| value.to_string()),
                },
            ))
        })
        .collect();

    if raw_segments.is_empty() {
        return vec![];
    }

    if raw_segments.len() == 1 {
        return vec![Segment {
            start: chunk_start_sec,
            duration: chunk_duration_sec,
            ..raw_segments.into_iter().next().unwrap().2
        }];
    }

    let timings = normalize_raw_segment_timings(&raw_segments, chunk_duration_sec)
        .unwrap_or_else(|| synthetic_segment_timings(&raw_segments, chunk_duration_sec));

    raw_segments
        .into_iter()
        .zip(timings)
        .map(|((_, _, segment), (start_offset, duration))| Segment {
            start: chunk_start_sec + start_offset,
            duration,
            ..segment
        })
        .collect()
}

fn normalize_raw_segment_timings(
    raw_segments: &[(f64, f64, Segment)],
    chunk_duration_sec: f64,
) -> Option<Vec<(f64, f64)>> {
    let mut clamped_bounds = Vec::with_capacity(raw_segments.len());
    let mut previous_end = 0.0;

    for (start, end, _) in raw_segments {
        if !start.is_finite() || !end.is_finite() {
            return None;
        }

        let start = (*start).max(0.0).max(previous_end);
        let end = (*end).max(0.0);
        if end <= start {
            return None;
        }

        clamped_bounds.push((start, end));
        previous_end = end;
    }

    if previous_end <= 0.0 {
        return None;
    }

    let scale = chunk_duration_sec / previous_end;
    let mut timings = Vec::with_capacity(clamped_bounds.len());

    for (idx, (start, end)) in clamped_bounds.into_iter().enumerate() {
        let start = (start * scale).min(chunk_duration_sec);
        let end = if idx + 1 == raw_segments.len() {
            chunk_duration_sec
        } else {
            (end * scale).min(chunk_duration_sec)
        };

        if end <= start {
            return None;
        }

        timings.push((start, end - start));
    }

    Some(timings)
}

fn synthetic_segment_timings(
    raw_segments: &[(f64, f64, Segment)],
    chunk_duration_sec: f64,
) -> Vec<(f64, f64)> {
    let total_weight: usize = raw_segments
        .iter()
        .map(|(_, _, segment)| segment.text.split_whitespace().count().max(1))
        .sum();
    let mut cursor = 0.0;

    raw_segments
        .iter()
        .enumerate()
        .map(|(idx, (_, _, segment))| {
            let weight = segment.text.split_whitespace().count().max(1) as f64;
            let start = cursor;
            let end = if idx + 1 == raw_segments.len() {
                chunk_duration_sec
            } else {
                cursor + chunk_duration_sec * (weight / total_weight as f64)
            };
            cursor = end;
            (start, end - start)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fork-Abweichung (02.09.2026): Die Stichwortliste kommt als eigener
    /// Abfrageparameter an -- getrennt von `keywords`, das in diesem Dienst
    /// seit jeher geparst und nie benutzt wird.
    #[test]
    fn parse_with_vocabulary() {
        let params = parse_listen_params(
            "language=de&keywords=notiz-schlagwort\
             &vocabulary=Sedacz%20%3D%3E%20Sedatsch&vocabulary=Webflow",
        )
        .unwrap();

        assert_eq!(params.vocabulary, ["Sedacz => Sedatsch", "Webflow"]);
        // Die beiden Kanaele vermischen sich nicht.
        assert_eq!(params.keywords, ["notiz-schlagwort"]);
    }

    /// Nur die richtigen Schreibweisen gehen in die Kontext-Vorgabe. Eine
    /// eingetragene Verhoerung als erwuenschtes Wort vorzulegen waere das
    /// Gegenteil der Absicht.
    #[test]
    fn die_kontext_vorgabe_traegt_keine_verhoerung() {
        let vocabulary =
            anlg_vocabulary::Vocabulary::parse(["Sedacz => Sedatsch; Sedatz", "Webflow"]);
        let prompt = anlg_whisper_local::build_prompt(&vocabulary.canonical_terms());

        assert!(prompt.contains("Sedacz"));
        assert!(prompt.contains("Webflow"));
        assert!(!prompt.contains("Sedatsch"));
        assert!(!prompt.contains("Sedatz"));
    }

    #[test]
    fn parse_single_language() {
        let params = parse_listen_params("language=en").unwrap();
        assert_eq!(params.languages.len(), 1);
        assert_eq!(params.languages[0].iso639().code(), "en");
    }

    #[test]
    fn parse_multiple_languages() {
        let params = parse_listen_params("language=en&language=ko").unwrap();
        assert_eq!(params.languages.len(), 2);
        assert_eq!(params.languages[0].iso639().code(), "en");
        assert_eq!(params.languages[1].iso639().code(), "ko");
    }

    #[test]
    fn parse_no_languages() {
        let params = parse_listen_params("").unwrap();
        assert!(params.languages.is_empty());
    }

    #[test]
    fn parse_with_keywords() {
        let params = parse_listen_params("language=en&keywords=hello&keywords=world").unwrap();
        assert_eq!(params.languages.len(), 1);
        assert_eq!(params.keywords, vec!["hello", "world"]);
    }

    #[test]
    fn defaults_channels_and_sample_rate_when_omitted() {
        let params = parse_listen_params("language=en").unwrap();
        assert_eq!(params.channels, 1);
        assert_eq!(params.sample_rate, TARGET_SAMPLE_RATE);
    }

    /// Die Schwelle muss zwischen des Betreibers gemessener Stille und seiner Rede
    /// liegen. Beide Werte stammen aus seinem Teammeeting vom 31.08.2026
    /// (Mikrofonspur, Sekunde 0-100 gegen 402-448) und sind der Grund, warum
    /// das Gate ueberhaupt existiert -- Whisper erfand in dieser Stille ein
    /// dreifaches "Vielen Dank".
    ///
    /// Faellt dieser Test, wurde die Schwelle so verschoben, dass sie entweder
    /// Stille durchlaesst (Halluzinationen kehren zurueck) oder echte Rede
    /// verschluckt (Wortmeldungen verschwinden). Beides ist schlimmer als der
    /// Zustand davor.
    #[test]
    fn silence_gate_sits_between_measured_silence_and_measured_speech() {
        const MEASURED_SILENCE_RMS: f32 = 0.000_122;
        const MEASURED_SPEECH_RMS: f32 = 0.059_282;

        assert!(
            MEASURED_SILENCE_RMS < MIN_SPEECH_RMS,
            "Stille ({MEASURED_SILENCE_RMS}) muss unter der Schwelle ({MIN_SPEECH_RMS}) liegen"
        );
        assert!(
            MEASURED_SPEECH_RMS > MIN_SPEECH_RMS,
            "Rede ({MEASURED_SPEECH_RMS}) muss ueber der Schwelle ({MIN_SPEECH_RMS}) liegen"
        );
    }

    #[test]
    fn chunk_rms_separates_silence_from_speech() {
        // Digitale Stille.
        assert_eq!(chunk_rms(&[0.0; 1600]), 0.0);
        // Ein leiser Stoerpegel in der Groessenordnung von des Betreibers Stille.
        let faint: Vec<f32> = (0..1600)
            .map(|i| if i % 2 == 0 { 0.0001 } else { -0.0001 })
            .collect();
        assert!(chunk_rms(&faint) < MIN_SPEECH_RMS);
        // Sprache in der Groessenordnung seiner Aufnahme.
        let speech: Vec<f32> = (0..1600)
            .map(|i| if i % 2 == 0 { 0.06 } else { -0.06 })
            .collect();
        assert!(chunk_rms(&speech) > MIN_SPEECH_RMS);
        // Leerer Abschnitt darf nicht in Panik geraten.
        assert_eq!(chunk_rms(&[]), 0.0);
    }

    #[test]
    fn preserves_multiple_segments_with_normalized_timings() {
        let segments = build_chunk_segments(
            vec![
                anlg_whisper_local::Segment {
                    text: "hello".to_string(),
                    language: Some("en".to_string()),
                    start: 0.0,
                    end: 1.0,
                    confidence: 0.8,
                    ..Default::default()
                },
                anlg_whisper_local::Segment {
                    text: "again".to_string(),
                    language: Some("en".to_string()),
                    start: 1.5,
                    end: 2.0,
                    confidence: 1.0,
                    ..Default::default()
                },
            ],
            10.0,
            4.0,
        );

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start, 10.0);
        assert_eq!(segments[0].duration, 2.0);
        assert_eq!(segments[0].text, "hello");
        assert_eq!(segments[0].language.as_deref(), Some("en"));
        assert!((segments[0].confidence - 0.8).abs() < 1e-6);

        assert_eq!(segments[1].start, 13.0);
        assert_eq!(segments[1].duration, 1.0);
        assert_eq!(segments[1].text, "again");
        assert_eq!(segments[1].language.as_deref(), Some("en"));
        assert!((segments[1].confidence - 1.0).abs() < 1e-6);
    }

    /// Der Kern der Batch-Umstellung: im Live-Modus werden die Rohzeiten
    /// gestreckt, bis das letzte Segment am Chunkende endet -- im Batch-Modus
    /// nicht. Beide Modi laufen hier auf DENSELBEN Rohdaten, damit der Test
    /// den Unterschied misst und nicht zwei verschiedene Eingaben.
    #[test]
    fn batch_mode_keeps_the_decoder_timings_that_live_mode_stretches() {
        let raw = || {
            vec![
                anlg_whisper_local::Segment {
                    text: "erster Satz".to_string(),
                    language: Some("de".to_string()),
                    start: 1.0,
                    end: 3.0,
                    confidence: 1.0,
                    ..Default::default()
                },
                anlg_whisper_local::Segment {
                    text: "zweiter Satz".to_string(),
                    language: Some("de".to_string()),
                    start: 4.0,
                    end: 6.0,
                    confidence: 1.0,
                    ..Default::default()
                },
            ]
        };

        let batch = build_chunk_segments_for_mode(
            raw(),
            100.0,
            20.0,
            anlg_whisper_local::TranscribeMode::Batch,
        );
        assert_eq!(batch.len(), 2);
        // Unveraendert uebernommen, nur um den Chunkanfang verschoben.
        assert_eq!(batch[0].start, 101.0);
        assert_eq!(batch[0].duration, 2.0);
        assert_eq!(batch[1].start, 104.0);
        assert_eq!(batch[1].duration, 2.0);
        // Die Sprechpause zwischen 3 s und 4 s bleibt eine Pause.
        assert!(batch[0].start + batch[0].duration < batch[1].start);
        // Und das Segment wird NICHT bis ans Chunkende (120 s) gezogen.
        assert!(batch[1].start + batch[1].duration < 120.0);

        let live = build_chunk_segments_for_mode(
            raw(),
            100.0,
            20.0,
            anlg_whisper_local::TranscribeMode::Live,
        );
        assert_eq!(live.len(), 2);
        assert_eq!(live[1].start + live[1].duration, 120.0);
    }

    /// Wenn der Dekoder im Batch gar keine brauchbaren Zeiten liefert, darf der
    /// Text nicht verschwinden -- dann greift bewusst der alte Weg.
    #[test]
    fn batch_mode_falls_back_when_no_usable_decoder_timing_arrives() {
        let segments = build_chunk_segments_for_mode(
            vec![
                anlg_whisper_local::Segment {
                    text: "hello world".to_string(),
                    language: Some("en".to_string()),
                    start: 0.0,
                    end: 0.0,
                    confidence: 1.0,
                    ..Default::default()
                },
                anlg_whisper_local::Segment {
                    text: "again".to_string(),
                    language: Some("en".to_string()),
                    start: 0.0,
                    end: 0.0,
                    confidence: 1.0,
                    ..Default::default()
                },
            ],
            10.0,
            3.0,
            anlg_whisper_local::TranscribeMode::Batch,
        );

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start, 10.0);
        assert_eq!(segments[1].start, 12.0);
    }

    #[test]
    fn falls_back_to_synthetic_timings_when_raw_timings_are_invalid() {
        let segments = build_chunk_segments(
            vec![
                anlg_whisper_local::Segment {
                    text: "hello world".to_string(),
                    language: Some("en".to_string()),
                    start: 0.0,
                    end: 0.0,
                    confidence: 0.8,
                    ..Default::default()
                },
                anlg_whisper_local::Segment {
                    text: "again".to_string(),
                    language: Some("en".to_string()),
                    start: 0.0,
                    end: 0.0,
                    confidence: 1.0,
                    ..Default::default()
                },
            ],
            10.0,
            3.0,
        );

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start, 10.0);
        assert_eq!(segments[0].duration, 2.0);
        assert_eq!(segments[1].start, 12.0);
        assert_eq!(segments[1].duration, 1.0);
    }
}
