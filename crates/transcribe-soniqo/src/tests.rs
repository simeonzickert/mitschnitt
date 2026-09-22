use owhisper_interface::stream;

use super::*;
use crate::responses::SYNTHETIC_BATCH_WORD_SECONDS;

#[test]
fn parses_model_ids() {
    assert_eq!(
        "soniqo-parakeet-streaming".parse::<SoniqoModel>().unwrap(),
        SoniqoModel::ParakeetStreaming
    );
    assert_eq!(
        "soniqo-qwen3-small".parse::<SoniqoModel>().unwrap(),
        SoniqoModel::Qwen3Small
    );
    assert_eq!(
        "soniqo-qwen3-large".parse::<SoniqoModel>().unwrap(),
        SoniqoModel::Qwen3Large
    );
}

#[test]
fn parakeet_batch_repo_matches_30s_coreml_artifact() {
    assert_eq!(
        SoniqoModel::ParakeetBatch.repo(),
        "aufklarer/Parakeet-TDT-v3-CoreML-INT8-30s"
    );
    assert_eq!(
        "aufklarer/Parakeet-TDT-v3-CoreML-INT8-30s"
            .parse::<SoniqoModel>()
            .unwrap(),
        SoniqoModel::ParakeetBatch
    );
    assert_eq!(
        "aufklarer/Parakeet-TDT-v3-CoreML-INT8"
            .parse::<SoniqoModel>()
            .unwrap(),
        SoniqoModel::ParakeetBatch
    );
}

#[test]
fn all_includes_available_model_variants() {
    assert_eq!(
        SoniqoModel::all(),
        &[
            SoniqoModel::ParakeetStreaming,
            SoniqoModel::ParakeetBatch,
            SoniqoModel::Omnilingual,
            SoniqoModel::Qwen3Small,
            SoniqoModel::Qwen3Large,
        ]
    );
}

#[test]
fn selectable_models_are_a_subset_of_all() {
    for model in SoniqoModel::selectable() {
        assert!(
            SoniqoModel::all().contains(model),
            "{model} is selectable but missing from all()"
        );
    }
}

#[test]
fn selectable_includes_advertised_models() {
    // Qwen3 ist am 31.08.2026 nach dem Vergleich an echtem Material wieder aus
    // der Auswahl genommen worden (Begruendung am SELECTABLE-Konstante). Die
    // Ladefaehigkeit bleibt erhalten und wird weiter geprueft -- siehe
    // qwen3_availability_follows_the_running_macos_version.
    assert_eq!(
        SoniqoModel::selectable(),
        &[SoniqoModel::ParakeetStreaming, SoniqoModel::ParakeetBatch]
    );
}

#[test]
fn parakeet_models_support_documented_european_languages() {
    let english = "en-US".parse().unwrap();
    let french = "fr".parse().unwrap();

    assert!(SoniqoModel::ParakeetStreaming.supports_language(&english));
    assert!(SoniqoModel::ParakeetBatch.supports_language(&english));
    assert!(SoniqoModel::ParakeetStreaming.supports_language(&french));
    assert!(SoniqoModel::ParakeetBatch.supports_language(&french));
}

#[test]
fn parakeet_models_reject_unsupported_languages() {
    let korean = "ko".parse().unwrap();

    assert!(!SoniqoModel::ParakeetStreaming.supports_language(&korean));
    assert!(!SoniqoModel::ParakeetBatch.supports_language(&korean));
}

#[test]
fn multilingual_models_support_non_english_languages() {
    let french = "fr".parse().unwrap();

    assert!(SoniqoModel::Omnilingual.supports_language(&french));
    assert!(SoniqoModel::Qwen3Small.supports_language(&french));
    assert!(SoniqoModel::Qwen3Large.supports_language(&french));
}

#[test]
fn live_support_is_gated_by_platform() {
    assert_eq!(
        SoniqoModel::ParakeetStreaming.supports_live_on_current_platform(),
        cfg!(all(target_os = "macos", target_arch = "aarch64")),
    );
    assert!(!SoniqoModel::ParakeetBatch.supports_live_on_current_platform());
}

#[test]
fn qwen3_platform_error_mentions_macos_15() {
    // The message is asserted directly: whether it *fires* now depends on the running
    // macOS, so building the error by hand keeps this test host-independent.
    let error = Error::RequiresMacOs15(SoniqoModel::Qwen3Small);

    assert_eq!(
        error.to_string(),
        "Qwen3 ASR 0.6B requires macOS 15 or newer."
    );
}

/// The old `const fn` could never return true for Qwen3 on any host. This pins the
/// runtime behaviour instead: on a macOS 15+ Apple Silicon box Qwen3 must be available.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn qwen3_availability_follows_the_running_macos_version() {
    let expected = crate::model::macos_version_at_least(15);

    assert_eq!(
        SoniqoModel::Qwen3Small.is_available_on_current_platform(),
        expected
    );
    assert_eq!(
        SoniqoModel::Qwen3Large.is_available_on_current_platform(),
        expected
    );
    // Parakeet carries no version requirement, so it is available regardless.
    assert!(SoniqoModel::ParakeetBatch.is_available_on_current_platform());

    // And the download/transcribe gate must agree with the selection gate.
    assert_eq!(
        ensure_supported_platform(SoniqoModel::Qwen3Small).is_ok(),
        expected
    );
}

#[test]
fn parses_major_version_from_system_version_plist() {
    let plist = r#"<plist version="1.0"><dict>
        <key>ProductBuildVersion</key><string>25C56</string>
        <key>ProductName</key><string>macOS</string>
        <key>ProductVersion</key><string>26.2</string>
        </dict></plist>"#;

    assert_eq!(crate::model::parse_product_major_version(plist), Some(26));
    assert_eq!(
        crate::model::parse_product_major_version("<plist><dict></dict></plist>"),
        None
    );
    // Unknown version must not be read as "new enough".
    assert!(!crate::model::macos_version_at_least(u32::MAX));
}

#[test]
fn streaming_model_uses_batch_model_for_file_transcription() {
    assert_eq!(
        SoniqoModel::ParakeetStreaming.batch_model(),
        SoniqoModel::ParakeetBatch
    );
    assert_eq!(
        SoniqoModel::ParakeetBatch.batch_model(),
        SoniqoModel::ParakeetBatch
    );
}

#[test]
fn batch_response_has_deepgram_shape() {
    let response =
        batch_response_from_text(SoniqoModel::ParakeetBatch, "hello world".to_string(), 2.0);

    assert_eq!(
        response.results.channels[0].alternatives[0].transcript,
        "hello world"
    );
    assert_eq!(response.results.channels[0].alternatives[0].words.len(), 2);
    assert_eq!(response.metadata["model_info"]["arch"], "soniqo");
    assert_eq!(response.metadata["duration"], 2.0);
    assert_eq!(response.metadata["timing_source"], "synthetic_text");
}

#[test]
fn batch_response_preserves_channel_indexes() {
    let response = batch_response_from_channels(
        SoniqoModel::Omnilingual,
        vec![
            FileTranscript::new("mic words".to_string(), 2.0),
            FileTranscript::new("speaker words".to_string(), 3.0),
        ],
    );

    assert_eq!(response.metadata["channels"], 2);
    assert_eq!(response.metadata["duration"], 3.0);
    assert_eq!(response.results.channels.len(), 2);
    assert_eq!(
        response.results.channels[0].alternatives[0].transcript,
        "mic words"
    );
    assert_eq!(
        response.results.channels[1].alternatives[0].transcript,
        "speaker words"
    );
    assert_eq!(
        response.results.channels[0].alternatives[0].words[0].channel,
        0
    );
    assert_eq!(
        response.results.channels[1].alternatives[0].words[0].channel,
        1
    );
}

#[test]
fn batch_response_uses_compact_synthetic_word_timing() {
    let response = batch_response_from_text(
        SoniqoModel::ParakeetBatch,
        "one two three four".to_string(),
        120.0,
    );
    let words = &response.results.channels[0].alternatives[0].words;

    assert_eq!(words[0].start, 0.0);
    assert_eq!(words[0].end, SYNTHETIC_BATCH_WORD_SECONDS);
    assert_eq!(words[3].end, 4.0 * SYNTHETIC_BATCH_WORD_SECONDS);
    assert!(words[3].end < 3.0);
    assert_eq!(response.metadata["duration"], 120.0);
}

#[test]
fn batch_response_offsets_synthetic_words_by_chunk_start() {
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![FileTranscript::from_chunks(
            vec![
                FileTranscriptChunk {
                    text: "early words".to_string(),
                    start_seconds: 0.0,
                    duration_seconds: 29.5,
                    words: Vec::new(),
                },
                FileTranscriptChunk {
                    text: "later words".to_string(),
                    start_seconds: 29.5,
                    duration_seconds: 29.5,
                    words: Vec::new(),
                },
            ],
            59.0,
        )],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    // Die Woerter fuellen die Dauer IHRES Abschnitts. Bis zum 02.09.2026 stand
    // hier zusaetzlich eine Decke von 0,4 s je Wort, und genau dieser Fall --
    // zwei Woerter auf 29,5 s -- war der, in dem sie greift: das zweite Wort
    // sass bei 0,4 s, die restlichen 28,7 s des Abschnitts trugen kein Wort.
    // Der Anker bleibt unveraendert der Abschnittsbeginn; das ist der Zweck
    // dieses Tests und er steht in Wort 3 unberuehrt.
    assert_eq!(words[0].start, 0.0);
    assert_eq!(words[1].start, 14.75);
    assert_eq!(words[2].start, 29.5);
    assert_eq!(words[3].start, 29.5 + 14.75);
    // Und kein Wort steht ueber die Grenze seines Abschnitts hinaus.
    assert_eq!(words[1].end, 29.5);
    assert_eq!(words[3].end, 59.0);
    // Fork: hier liegen echte Chunk-Zeiten vor (die Assertions darueber pruefen
    // genau das -- Wort 3 sitzt bei 29,5 s, weil sein Chunk dort beginnt).
    // Upstream stempelte trotzdem "synthetic_text", und das Frontend schaltet
    // bei diesem Etikett das Anspringen im Abspieler ab
    // (isTranscriptWordSeekable, apps/desktop/src/stt/timing.ts:45). Der
    // richtige Wert ist "provider_segment_interpolated": Segmentgrenzen echt,
    // Wortzeiten darin gleichverteilt.
    assert_eq!(
        response.metadata["timing_source"],
        "provider_segment_interpolated"
    );
}

#[test]
fn every_word_stays_inside_the_bounds_of_its_chunk() {
    // Ein langsam gesprochener Abschnitt: fuenf Woerter auf zehn Sekunden.
    // Bis zum 02.09.2026 endete sein Wortpaket nach 2 s (0,4 s je Wort) und
    // die uebrigen 8 s trugen nichts -- wer im Abspieler dorthin sprang, fand
    // Stille. Jetzt fuellen die Woerter den Abschnitt, und keins tritt heraus.
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![FileTranscript::from_chunks(
            vec![FileTranscriptChunk {
                text: "fuenf Woerter auf zehn Sekunden".to_string(),
                start_seconds: 100.0,
                duration_seconds: 10.0,
                words: Vec::new(),
            }],
            110.0,
        )],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    assert_eq!(words.len(), 5);
    assert_eq!(words[0].start, 100.0);
    assert_eq!(words[4].end, 110.0);
    for word in words {
        assert!(
            word.start >= 100.0 && word.end <= 110.0 && word.end > word.start,
            "Wort {word:?} liegt nicht innerhalb seines Abschnitts"
        );
    }
}

#[test]
fn batch_response_marks_text_only_channels_as_synthetic() {
    // Die Gegenprobe zum Test darueber: OHNE Chunks gibt es keine echte
    // Verankerung, dann ist "synthetic_text" korrekt und das Abspringen zu
    // Recht abgeschaltet. Faellt dieser Test, unterscheidet das Etikett die
    // beiden Faelle nicht mehr.
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![FileTranscript::new(
            "nur text ohne zeiten".to_string(),
            10.0,
        )],
    );

    assert_eq!(response.metadata["timing_source"], "synthetic_text");
}

#[test]
fn batch_response_aligns_words_to_diarized_speech() {
    let mut transcript = FileTranscript::from_chunks(
        vec![FileTranscriptChunk {
            text: "lex one two george three four".to_string(),
            start_seconds: 0.0,
            duration_seconds: 10.0,
            words: Vec::new(),
        }],
        10.0,
    );
    transcript.speaker_segments = vec![
        DiarizationSegment {
            start_seconds: 1.0,
            end_seconds: 4.0,
            speaker_index: 0,
        },
        DiarizationSegment {
            start_seconds: 6.0,
            end_seconds: 9.0,
            speaker_index: 1,
        },
    ];

    let response = batch_response_from_channels(SoniqoModel::ParakeetBatch, vec![transcript]);
    let words = &response.results.channels[0].alternatives[0].words;

    assert_eq!(
        words.iter().map(|word| word.speaker).collect::<Vec<_>>(),
        vec![Some(0), Some(0), Some(0), Some(1), Some(1), Some(1)]
    );
    assert!(words.windows(2).all(|pair| pair[0].start <= pair[1].start));
    assert!(words[0].start >= 1.0);
    assert!(words[3].start >= 6.0);
    assert_eq!(response.metadata["timing_source"], "diarized_speech");
}

#[test]
fn batch_response_normalizes_internal_whitespace() {
    let response = batch_response_from_text(
        SoniqoModel::ParakeetBatch,
        "eins zwei\n drei\tvier".to_string(),
        4.0,
    );
    let alternative = &response.results.channels[0].alternatives[0];

    assert_eq!(alternative.transcript, "eins zwei drei vier");
    assert_eq!(
        alternative
            .words
            .iter()
            .map(|word| word.word.as_str())
            .collect::<Vec<_>>(),
        vec!["eins", "zwei", "drei", "vier"]
    );
}

#[test]
fn live_response_keeps_source_channel() {
    let partial = LivePartial {
        source: "system".to_string(),
        text: "hello".to_string(),
        is_final: true,
    };

    let response = partial.into_stream_response(SoniqoModel::ParakeetStreaming, 0.0, 0.5);
    let stream::StreamResponse::TranscriptResponse { channel_index, .. } = response else {
        panic!("expected transcript response");
    };

    assert_eq!(channel_index, vec![1, 2]);
}

// --- Der Weg mit GEMESSENEN Wortzeiten (Fork 02.09.2026) --------------------
//
// Bis hierher hatte dieser Zweig keinen einzigen Test: die `words: Vec::new()`
// der Tests darueber pruefen genau das Gegenteil, naemlich den Rueckfall. Der
// gemessene Zweig ist aber der, den der Schalter `word_timings` einschaltet --
// und der, in dem der Sprecher-Hinweis verloren gehen kann.

fn measured_word(text: &str, start_seconds: f64, end_seconds: f64) -> TranscriptWord {
    TranscriptWord {
        text: text.to_string(),
        start_seconds,
        end_seconds,
        confidence: None,
    }
}

fn diarized(start_seconds: f64, end_seconds: f64, speaker_index: usize) -> DiarizationSegment {
    DiarizationSegment {
        start_seconds,
        end_seconds,
        speaker_index,
    }
}

fn measured_channel(
    words: Vec<TranscriptWord>,
    speaker_segments: Vec<DiarizationSegment>,
) -> FileTranscript {
    let text = words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let start_seconds = words.first().map(|word| word.start_seconds).unwrap_or(0.0);
    let end_seconds = words.last().map(|word| word.end_seconds).unwrap_or(1.0);
    let mut channel = FileTranscript::from_chunks(
        vec![FileTranscriptChunk {
            text,
            start_seconds,
            duration_seconds: end_seconds - start_seconds,
            words,
        }],
        end_seconds,
    );
    channel.speaker_segments = speaker_segments;
    channel
}

#[test]
fn measured_word_times_reach_the_response_unchanged() {
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![measured_channel(
            vec![
                measured_word("gemessen", 10.25, 10.80),
                measured_word("und", 11.90, 12.05),
                measured_word("belegt", 12.05, 12.70),
            ],
            Vec::new(),
        )],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    // Keine Gleichverteilung: die Luecke zwischen "gemessen" und "und" bleibt
    // stehen, weil sie echt ist.
    assert_eq!(
        words
            .iter()
            .map(|word| (word.word.as_str(), word.start, word.end))
            .collect::<Vec<_>>(),
        vec![
            ("gemessen", 10.25, 10.80),
            ("und", 11.90, 12.05),
            ("belegt", 12.05, 12.70),
        ]
    );
    assert_eq!(
        response.metadata.get("timing_source").unwrap(),
        "provider_word"
    );
}

#[test]
fn a_measured_word_beside_the_diarization_span_keeps_its_speaker() {
    // Der teure Fall. Abschnittsgrenzen kommen vom Sprach-Erkenner, die
    // Sprecher-Spannen vom Diarisierer -- zwei getrennte Detektoren. Das erste
    // und letzte Wort eines Abschnitts liegt regelmaessig knapp neben der
    // Spanne. Ein reines `find` lieferte dort `None`, und die Oberflaeche
    // schreibt den Sprecher-Hinweis nur bei einer Zahl: das Wort haengt dann
    // sprecherlos im Transkript.
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![measured_channel(
            vec![
                measured_word("davor", 4.80, 4.95),
                measured_word("mittendrin", 5.40, 5.90),
                measured_word("danach", 8.10, 8.40),
            ],
            vec![diarized(5.0, 8.0, 0), diarized(9.0, 12.0, 1)],
        )],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    assert_eq!(
        words
            .iter()
            .map(|word| (word.word.as_str(), word.speaker))
            .collect::<Vec<_>>(),
        vec![
            ("davor", Some(0)),
            ("mittendrin", Some(0)),
            ("danach", Some(0)),
        ],
        "kein Wort verliert seinen Sprecher, nur weil es knapp neben der Spanne liegt"
    );
}

#[test]
fn a_measured_word_between_two_speakers_takes_the_nearer_one() {
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![measured_channel(
            vec![measured_word("dazwischen", 8.60, 8.80)],
            vec![diarized(5.0, 8.0, 0), diarized(9.0, 12.0, 1)],
        )],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    // 0,60 s hinter Sprecher 0, 0,20 s vor Sprecher 1.
    assert_eq!(words[0].speaker, Some(1));
}

#[test]
fn a_measured_word_takes_the_speaker_it_overlaps_most() {
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![measured_channel(
            vec![measured_word("ueberlappend", 7.90, 9.50)],
            vec![diarized(5.0, 8.0, 0), diarized(8.0, 12.0, 1)],
        )],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    // 0,10 s bei Sprecher 0, 1,50 s bei Sprecher 1.
    assert_eq!(words[0].speaker, Some(1));
}

#[test]
fn a_measured_word_without_diarization_has_no_speaker() {
    // Die Gegenprobe: OHNE Diarisierung ist `None` die Wahrheit, nicht ein
    // Verlust. Faellt dieser Test, erfindet der naechstgelegene Sprecher einen.
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![measured_channel(
            vec![measured_word("allein", 1.0, 1.5)],
            Vec::new(),
        )],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    assert_eq!(words[0].speaker, None);
}

#[test]
fn a_measured_word_of_zero_length_still_lasts_a_millisecond() {
    // Der Waechter stand vorher auf `f64::EPSILON` und war rechnerisch
    // wirkungslos: 2,2e-16 relativ zu 1,0 verschwindet ab etwa vier Sekunden
    // ganz und darunter in der Millisekunden-Rundung der Oberflaeche.
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![measured_channel(
            vec![measured_word("punkt", 600.0, 600.0)],
            Vec::new(),
        )],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    // Gemessen wird an dem, was zaehlt: die Oberflaeche rundet auf
    // Millisekunden (`Math.round(word.end * 1000)`). Genau diese beiden Zahlen
    // muessen sich unterscheiden -- der Abstand selbst darf ruhig unter 0,001
    // liegen, das ist nur die Ungenauigkeit von f64 bei 600 Sekunden.
    assert!(
        (words[0].end * 1000.0).round() > (words[0].start * 1000.0).round(),
        "{:?} ueberlebt die Millisekunden-Rundung nicht",
        words[0]
    );
}

/// Die Nutzlast der Bruecke, wie Swift sie schreibt -- mit der Sicherheit je
/// Wort. Bewusst als JSON und nicht ueber die Rust-Typen gebaut: so misst der
/// Test die STRECKE (Feldname, Deserialisierung, Weitergabe) und nicht seine
/// eigene Konstruktion. Ein fehlendes Feld faellt hier durch, statt den Test
/// gar nicht erst uebersetzen zu lassen.
fn channel_from_bridge_json(json: &str) -> FileTranscript {
    serde_json::from_str(json).expect("Bruecken-Nutzlast ist gueltiges JSON")
}

const BRIDGE_CHANNEL_WITH_CONFIDENCE: &str = r#"{
  "text": "gemessen und belegt",
  "durationSeconds": 12.7,
  "chunks": [
    {
      "text": "gemessen und belegt",
      "startSeconds": 10.25,
      "durationSeconds": 2.45,
      "words": [
        { "text": "gemessen", "startSeconds": 10.25, "endSeconds": 10.80, "confidence": 0.91 },
        { "text": "und", "startSeconds": 11.90, "endSeconds": 12.05, "confidence": 0.42 },
        { "text": "belegt", "startSeconds": 12.05, "endSeconds": 12.70, "confidence": 0.77 }
      ]
    }
  ]
}"#;

#[test]
fn measured_word_confidence_reaches_the_response() {
    // Bis zum 03.09.2026 stand in `responses.rs` sechsmal `confidence: 1.0`
    // hart im Code. Der Dekoder rechnet die Zahl ohnehin je Wort aus -- sie kam
    // nur nie ueber die Bruecke. Eine erfundene Eins ist schlimmer als ein
    // leeres Feld: sie sieht aus wie eine Messung.
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![channel_from_bridge_json(BRIDGE_CHANNEL_WITH_CONFIDENCE)],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    assert_eq!(words.len(), 3);
    assert!((words[0].confidence - 0.91).abs() < 1e-9, "{:?}", words[0]);
    assert!((words[1].confidence - 0.42).abs() < 1e-9, "{:?}", words[1]);
    assert!((words[2].confidence - 0.77).abs() < 1e-9, "{:?}", words[2]);
    // Und daneben die Angabe, dass es MESSUNGEN sind -- ohne sie ist eine
    // 0,42 von einer aufgefuellten 1,0 nur zufaellig zu unterscheiden.
    assert_eq!(
        words
            .iter()
            .map(|word| word.measured_confidence)
            .collect::<Vec<_>>(),
        vec![Some(0.91), Some(0.42), Some(0.77)]
    );
}

#[test]
fn a_word_without_measured_confidence_stays_at_one_but_says_so() {
    // Die Gegenprobe -- und bis zum 03.09.2026 die halbe Wahrheit.
    //
    // Richtig bleibt: `confidence` behaelt die 1,0. Das Feld ist auf dem Draht
    // ein nackter `f64` ohne Leerstelle, und jede andere Zahl waere eine
    // Verhaltensaenderung fuer alle Anbieter.
    //
    // Die alte Fassung hoerte hier auf und schrieb damit genau den Fehler
    // fest, den sie zu pruefen vorgab: nach dem Speichern war "sicher
    // gemessen" von "nichts gewusst" nicht mehr zu unterscheiden. Die
    // erfundene Eins DARF dastehen -- sie muss nur als nicht gemessen
    // erkennbar bleiben.
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![measured_channel(
            vec![measured_word("ohne", 1.0, 1.5)],
            Vec::new(),
        )],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    assert_eq!(words[0].confidence, 1.0);
    assert_eq!(words[0].measured_confidence, None, "{:?}", words[0]);
}

#[test]
fn the_invented_one_and_a_measured_one_stay_apart() {
    // Der Falsifikator: zwei Woerter, beide mit `confidence == 1.0`, nur eines
    // gemessen. Wer die beiden nach dem Speichern nicht mehr trennen kann, baut
    // einen Filter auf NIEDRIGE Sicherheit, der genau invers greift -- er
    // ueberspringt die ungemessenen Woerter, weil sie maximal sicher aussehen.
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![channel_from_bridge_json(
            r#"{
  "text": "geraten gemessen",
  "durationSeconds": 2.0,
  "chunks": [
    {
      "text": "geraten gemessen",
      "startSeconds": 0.0,
      "durationSeconds": 2.0,
      "words": [
        { "text": "geraten", "startSeconds": 0.0, "endSeconds": 1.0 },
        { "text": "gemessen", "startSeconds": 1.0, "endSeconds": 2.0, "confidence": 1.0 }
      ]
    }
  ]
}"#,
        )],
    );
    let words = &response.results.channels[0].alternatives[0].words;

    assert_eq!(words.len(), 2);
    assert_eq!(
        words[0].confidence, words[1].confidence,
        "gleiche Draht-Zahl"
    );
    assert_eq!(words[0].measured_confidence, None, "{:?}", words[0]);
    assert_eq!(words[1].measured_confidence, Some(1.0), "{:?}", words[1]);
}

/// Eine wegen Laenge ausgelassene Sprechertrennung reist bis in die
/// Antwort-Metadaten. Ohne diesen Weg stuende sie nur im Protokoll, und die
/// Oberflaeche koennte einen ausgelassenen Schritt nicht von einem schlechten
/// Ergebnis unterscheiden.
#[test]
fn a_diarization_skipped_for_length_reaches_the_response_metadata() {
    let mut channel = FileTranscript::new("hallo welt".to_string(), 12.0);
    channel.diarization_skipped_over_seconds = Some(7200.0);

    let response = batch_response_from_channels(SoniqoModel::ParakeetBatch, vec![channel]);

    assert_eq!(
        response.metadata["diarization_skipped_over_seconds"],
        7200.0
    );
}

/// Und im Regelfall steht das Feld GAR NICHT da. Ein Feld, das immer
/// auftaucht, muss der Leser jedes Mal auswerten; die Abwesenheit ist hier
/// selbst die Aussage.
#[test]
fn a_normal_response_carries_no_skip_notice() {
    let response = batch_response_from_channels(
        SoniqoModel::ParakeetBatch,
        vec![FileTranscript::new("hallo welt".to_string(), 12.0)],
    );

    assert!(
        response
            .metadata
            .get("diarization_skipped_over_seconds")
            .is_none()
    );
}
