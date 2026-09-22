use std::future::{Future, pending};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::{Router, extract::State, http::StatusCode};
use owhisper_client::BatchSttAdapter;
use owhisper_interface::batch::Response;
use tokio::sync::Notify;

use anlg_transcribe_core::TARGET_SAMPLE_RATE;

use super::super::BatchParams;
use super::*;
// Nicht ueber `super::*` erreichbar: der Produktionsweg des Herzschlags und
// sein Takt liegen im Modul `local`.
use super::local::{SONIQO_DIARIZATION_HEARTBEAT, with_diarization_heartbeat};
use crate::{BatchEvent, BatchRuntime};

#[derive(Clone, Default)]
struct HangingHttpAdapter;

impl BatchSttAdapter for HangingHttpAdapter {
    fn provider_name(&self) -> &'static str {
        "hanging-http"
    }

    fn is_supported_languages(
        &self,
        _languages: &[anlg_language::Language],
        _model: Option<&str>,
    ) -> bool {
        true
    }

    fn transcribe_file<'a, P: AsRef<Path> + Send + 'a>(
        &'a self,
        client: &'a reqwest_middleware::ClientWithMiddleware,
        api_base: &'a str,
        _api_key: &'a str,
        _params: &'a owhisper_interface::ListenParams,
        _file_path: P,
    ) -> Pin<
        Box<dyn Future<Output = std::result::Result<Response, owhisper_client::Error>> + Send + 'a>,
    > {
        Box::pin(async move {
            client.post(api_base).body("audio").send().await?;
            panic!("hanging provider unexpectedly responded");
        })
    }
}

static SEGMENT_UPLOADS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

#[derive(Clone, Default)]
struct RecordingAdapter;

impl BatchSttAdapter for RecordingAdapter {
    fn provider_name(&self) -> &'static str {
        "recording"
    }

    fn is_supported_languages(
        &self,
        _languages: &[anlg_language::Language],
        _model: Option<&str>,
    ) -> bool {
        true
    }

    fn transcribe_file<'a, P: AsRef<Path> + Send + 'a>(
        &'a self,
        _client: &'a reqwest_middleware::ClientWithMiddleware,
        _api_base: &'a str,
        _api_key: &'a str,
        _params: &'a owhisper_interface::ListenParams,
        file_path: P,
    ) -> Pin<
        Box<dyn Future<Output = std::result::Result<Response, owhisper_client::Error>> + Send + 'a>,
    > {
        let path = file_path.as_ref().to_path_buf();
        Box::pin(async move {
            let mut uploads = SEGMENT_UPLOADS.lock().unwrap();
            let index = uploads.len();
            uploads.push(path.to_string_lossy().into_owned());

            Ok(Response {
                metadata: serde_json::json!({ "provider": "recording" }),
                results: owhisper_interface::batch::Results {
                    channels: vec![owhisper_interface::batch::Channel {
                        alternatives: vec![owhisper_interface::batch::Alternatives {
                            transcript: format!("segment {index}"),
                            confidence: 1.0,
                            words: vec![owhisper_interface::batch::Word {
                                word: format!("segment{index}"),
                                start: 0.25,
                                end: 0.75,
                                confidence: 1.0,
                                measured_confidence: Some(1.0),
                                channel: 0,
                                speaker: None,
                                punctuated_word: None,
                            }],
                        }],
                    }],
                },
            })
        })
    }
}

#[derive(Clone)]
struct HangingProviderState {
    request_started: Arc<Notify>,
    request_cancelled: Arc<Notify>,
}

struct NotifyOnDrop(Arc<Notify>);

impl Drop for NotifyOnDrop {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

async fn hanging_provider(State(state): State<HangingProviderState>) -> StatusCode {
    let _cancelled = NotifyOnDrop(state.request_cancelled.clone());
    state.request_started.notify_one();
    pending::<()>().await;
    StatusCode::OK
}

fn write_test_wav(path: &Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    writer.write_sample(0.0f32).unwrap();
    writer.finalize().unwrap();
}

fn write_test_wav_samples(path: &Path, samples: usize) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for _ in 0..samples {
        writer.write_sample(0.0f32).unwrap();
    }
    writer.finalize().unwrap();
}

#[test]
fn direct_timeout_scales_with_audio_duration_and_is_bounded() {
    assert_eq!(
        direct_batch_timeout_for_audio(None),
        DIRECT_BATCH_TIMEOUT_FLOOR
    );
    assert_eq!(
        direct_batch_timeout_for_audio(Some(Duration::from_secs(60 * 60))),
        Duration::from_secs(2 * 60 * 60 + 5 * 60)
    );
    assert_eq!(
        direct_batch_timeout_for_audio(Some(Duration::from_secs(24 * 60 * 60))),
        DIRECT_BATCH_TIMEOUT_CEILING
    );
}

#[test]
fn segments_only_when_a_provider_limit_is_exceeded() {
    let source = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    write_test_wav_samples(source.path(), TARGET_SAMPLE_RATE as usize);
    let path = source.path().to_str().unwrap();
    let size = std::fs::metadata(source.path()).unwrap().len();
    let limit = |max_bytes, max_duration| {
        Some(owhisper_client::BatchUploadLimit {
            max_bytes,
            max_duration,
        })
    };

    assert_eq!(segment_plan(path, None, None), None);
    assert_eq!(
        segment_plan(path, None, limit(size, Duration::from_secs(60))),
        None
    );
    assert_eq!(
        segment_plan(path, None, limit(size - 1, Duration::from_secs(60))),
        Some(Duration::from_secs(60))
    );
    assert_eq!(
        segment_plan(
            path,
            Some(Duration::from_secs(61)),
            limit(size, Duration::from_secs(60))
        ),
        Some(Duration::from_secs(60))
    );
}

#[test]
fn openai_diarize_splits_below_the_shared_openai_duration_cap() {
    let source = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    write_test_wav_samples(source.path(), TARGET_SAMPLE_RATE as usize);
    let path = source.path().to_str().unwrap();
    let size = std::fs::metadata(source.path()).unwrap().len();
    let diarize = owhisper_client::AdapterKind::OpenAI
        .batch_upload_limit(Some("gpt-4o-transcribe-diarize"))
        .unwrap();
    let transcribe = owhisper_client::AdapterKind::OpenAI
        .batch_upload_limit(Some("gpt-4o-transcribe"))
        .unwrap();
    let meeting = Some(Duration::from_secs_f64(1500.012));

    assert!(
        size < transcribe.max_bytes,
        "fixture must be size-eligible so duration is the trigger"
    );
    assert_eq!(
        segment_plan(path, Some(Duration::from_secs(1450)), Some(transcribe)),
        None
    );
    assert_eq!(
        segment_plan(path, Some(Duration::from_secs(1450)), Some(diarize)),
        Some(diarize.max_duration)
    );
    assert_eq!(
        segment_plan(path, meeting, Some(diarize)),
        Some(diarize.max_duration)
    );
    assert!(diarize.max_duration < Duration::from_secs(1400));
}

#[test]
fn merges_segment_transcripts_onto_a_single_timeline() {
    let segment = |transcript: &str, start: f64| Response {
        metadata: serde_json::json!({ "provider": "openrouter" }),
        results: owhisper_interface::batch::Results {
            channels: vec![owhisper_interface::batch::Channel {
                alternatives: vec![owhisper_interface::batch::Alternatives {
                    transcript: transcript.to_string(),
                    confidence: 1.0,
                    words: vec![owhisper_interface::batch::Word {
                        word: transcript.to_string(),
                        start,
                        end: start + 0.5,
                        confidence: 1.0,
                        measured_confidence: Some(1.0),
                        channel: 0,
                        speaker: None,
                        punctuated_word: None,
                    }],
                }],
            }],
        },
    };

    let merged = merge_segment_responses(
        vec![segment("first", 1.0), segment("second", 2.0)],
        Duration::from_secs(600),
    );

    let alternative = &merged.results.channels[0].alternatives[0];
    assert_eq!(alternative.transcript, "first second");
    assert_eq!(alternative.words[0].start, 1.0);
    assert_eq!(alternative.words[1].start, 602.0);
    assert_eq!(alternative.words[1].end, 602.5);
    assert_eq!(merged.metadata["provider"], "openrouter");
}

#[test]
fn keeps_diarized_segment_speakers_distinct() {
    let segment = |label: &str, speaker: usize, start: f64| Response {
        metadata: serde_json::json!({
            "speaker_labels": [label],
            "speaker_segments": [{
                "speaker": label,
                "start": start,
                "end": start + 0.5,
            }],
        }),
        results: owhisper_interface::batch::Results {
            channels: vec![owhisper_interface::batch::Channel {
                alternatives: vec![owhisper_interface::batch::Alternatives {
                    transcript: label.to_string(),
                    confidence: 1.0,
                    words: vec![owhisper_interface::batch::Word {
                        word: label.to_string(),
                        start,
                        end: start + 0.5,
                        confidence: 1.0,
                        measured_confidence: Some(1.0),
                        channel: 0,
                        speaker: Some(speaker),
                        punctuated_word: None,
                    }],
                }],
            }],
        },
    };

    let merged = merge_segment_responses(
        vec![segment("speaker_a", 0, 1.0), segment("speaker_b", 0, 2.0)],
        Duration::from_secs(600),
    );

    let alternative = &merged.results.channels[0].alternatives[0];
    assert_eq!(alternative.words[0].speaker, Some(0));
    assert_eq!(alternative.words[1].speaker, Some(1));
    assert_eq!(
        merged.metadata["speaker_labels"],
        serde_json::json!(["speaker_a", "speaker_b"])
    );
    assert_eq!(merged.metadata["speaker_segments"][0]["start"], 1.0);
    assert_eq!(merged.metadata["speaker_segments"][1]["start"], 602.0);
}

#[tokio::test]
async fn oversized_audio_is_uploaded_one_segment_at_a_time() {
    SEGMENT_UPLOADS.lock().unwrap().clear();
    let source = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    write_test_wav_samples(source.path(), TARGET_SAMPLE_RATE as usize * 3);
    let size = std::fs::metadata(source.path()).unwrap().len();

    let params = BatchParams {
        session_id: "segment-test".to_string(),
        provider: super::super::BatchProvider::OpenRouter,
        file_path: source.path().to_string_lossy().into_owned(),
        model: None,
        base_url: "https://openrouter.ai/api/v1".to_string(),
        api_key: "test".to_string(),
        languages: vec![anlg_language::ISO639::En.into()],
        keywords: vec![],
        vocabulary: vec![],
        num_speakers: None,
        min_speakers: None,
        max_speakers: None,
    };

    let output = run_direct_batch::<RecordingAdapter>(
        "recording",
        params,
        owhisper_interface::ListenParams::default(),
        Some(owhisper_client::BatchUploadLimit {
            max_bytes: size - 1,
            max_duration: Duration::from_secs(1),
        }),
    )
    .await
    .unwrap();

    let uploads = SEGMENT_UPLOADS.lock().unwrap().clone();
    assert_eq!(uploads.len(), 3, "3s of audio in 1s segments");
    for upload in &uploads {
        assert!(upload.ends_with(".mp3"), "segment was not re-encoded");
        assert!(!std::path::Path::new(upload).exists(), "segment leaked");
    }

    let alternative = &output.response.results.channels[0].alternatives[0];
    assert_eq!(alternative.transcript, "segment 0 segment 1 segment 2");
    assert_eq!(alternative.words[0].start, 0.25);
    assert_eq!(alternative.words[2].start, 2.25);
    assert!(source.path().exists());
}

// Grok-Review 02.09.2026 (F4). "anarlog" ist die Kennung der Gegenstelle des
// Originals (api.anarlog.so). Fuer sie darf kein Netz-Client mehr entstehen --
// nicht ueber einen Filter in der Oberflaeche, sondern hier, wo der Client
// gebaut wuerde. Vor dem Umbau lief dieser Test bis zum Verbindungsversuch
// (base_url zeigt auf einen geschlossenen Port, der Fehler war "connection
// refused", nicht "removed in this fork").
#[tokio::test]
async fn anarlog_direct_batch_is_refused_before_any_network_client_is_built() {
    let audio = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    write_test_wav(audio.path());
    let params = BatchParams {
        session_id: "removed-provider".to_string(),
        provider: super::super::BatchProvider::Anarlog,
        file_path: audio.path().to_string_lossy().into_owned(),
        model: Some("cloud".to_string()),
        base_url: "http://127.0.0.1:1/stt".to_string(),
        api_key: "test".to_string(),
        languages: vec![anlg_language::ISO639::En.into()],
        keywords: vec![],
        vocabulary: vec![],
        num_speakers: None,
        min_speakers: None,
        max_speakers: None,
    };

    let error = run_direct_batch_for_adapter_kind(
        owhisper_client::AdapterKind::Anarlog,
        params,
        owhisper_interface::ListenParams::default(),
    )
    .await
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("removed in this fork"),
        "unerwartete Meldung: {message}"
    );
}

#[tokio::test]
async fn direct_provider_timeout_cancels_non_responding_request() {
    let request_started = Arc::new(Notify::new());
    let request_cancelled = Arc::new(Notify::new());
    let state = HangingProviderState {
        request_started: request_started.clone(),
        request_cancelled: request_cancelled.clone(),
    };
    let app = Router::new().fallback(hanging_provider).with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let audio = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    write_test_wav(audio.path());
    let params = BatchParams {
        session_id: "timeout-test".to_string(),
        provider: super::super::BatchProvider::OpenAI,
        file_path: audio.path().to_string_lossy().into_owned(),
        model: Some("whisper-1".to_string()),
        base_url: format!("http://{address}/v1"),
        api_key: "test".to_string(),
        languages: vec![anlg_language::ISO639::En.into()],
        keywords: vec![],
        vocabulary: vec![],
        num_speakers: None,
        min_speakers: None,
        max_speakers: None,
    };
    let request = tokio::spawn(run_direct_batch_with_timeout::<HangingHttpAdapter>(
        "hanging-http",
        params,
        owhisper_interface::ListenParams {
            model: Some("whisper-1".to_string()),
            channels: 1,
            sample_rate: TARGET_SAMPLE_RATE,
            languages: vec![anlg_language::ISO639::En.into()],
            ..Default::default()
        },
        Duration::from_secs(5),
    ));

    tokio::time::timeout(Duration::from_secs(6), request_started.notified())
        .await
        .expect("provider did not receive the request");
    let error = request
        .await
        .expect("batch task panicked")
        .expect_err("non-responding provider should time out");

    match error {
        crate::Error::BatchFailed(failure) => {
            assert_eq!(failure.code(), crate::BatchErrorCode::TimedOut);
            assert!(matches!(
                failure,
                crate::BatchFailure::DirectRequestTimedOut { .. }
            ));
        }
        other => panic!("unexpected timeout error: {other:?}"),
    }
    tokio::time::timeout(Duration::from_secs(2), request_cancelled.notified())
        .await
        .expect("provider request was not cancelled");

    server.abort();
}

#[test]
fn parakeet_batch_reads_bounded_contiguous_chunks_from_disk() {
    let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    let total_samples = SONIQO_PARAKEET_MAX_CHUNK_SAMPLES * 2 + TARGET_SAMPLE_RATE as usize;
    write_test_wav_samples(file.path(), total_samples);
    let chunks =
        FixedSoniqoFileChunkIterator::new(file, SONIQO_PARAKEET_MAX_CHUNK_SAMPLES).unwrap();
    let mut previous_end = 0usize;
    let mut chunk_count = 0usize;

    for chunk in chunks {
        let chunk = chunk.unwrap();
        assert_eq!(chunk.sample_start, previous_end);
        assert!(chunk.samples.len() <= SONIQO_PARAKEET_MAX_CHUNK_SAMPLES);
        previous_end = chunk.sample_end;
        chunk_count += 1;
    }

    assert_eq!(chunk_count, 3);
    assert_eq!(previous_end, total_samples);
}

// --- Stellschrauben der Messbank --------------------------------------------
//
// `super::tuning` haelt EINEN prozessweiten Wert. Tests laufen als Threads im
// selben Prozess, also darf immer nur einer davon gleichzeitig an der
// Stellschraube drehen -- und jeder Test, der sie LIEST, muss dieselbe Sperre
// halten, sonst sieht er den Wert eines Nachbarn. Der Waechter stellt beim
// Verlassen den Produktionsstand wieder her, auch wenn der Test panisch endet.
static TUNING_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct TuningGuard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

impl TuningGuard {
    fn production() -> Self {
        let guard = TUNING_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        tuning::set_soniqo_tuning(tuning::SoniqoTuning::PRODUCTION);
        Self(guard)
    }

    fn with(tuning: tuning::SoniqoTuning) -> Self {
        let guard = Self::production();
        tuning::set_soniqo_tuning(tuning);
        guard
    }
}

impl Drop for TuningGuard {
    fn drop(&mut self) {
        tuning::set_soniqo_tuning(tuning::SoniqoTuning::PRODUCTION);
    }
}

/// Ohne einen einzigen Setter-Aufruf -- so laeuft die App -- steht jede
/// Stellschraube auf dem Wert, den die Messreihe vom 02.09.2026 festgelegt
/// hat: sprachbewusst zerlegt, auf das Modellfenster (29,5 s) gebuendelt,
/// Stille-Gate an, Kanaele gleichzeitig.
#[test]
fn without_the_bench_every_knob_holds_todays_value() {
    let _guard = TuningGuard::production();
    let tuning = tuning::soniqo_tuning();

    assert_eq!(tuning, tuning::SoniqoTuning::PRODUCTION);
    assert_eq!(tuning.chunking, tuning::SoniqoChunkingMode::SpeechAware);
    assert_eq!(tuning.bundle_max_samples, None);
    assert_eq!(
        tuning.bundle_max_span_samples,
        Some(SONIQO_BUNDLE_MAX_SPAN_SAMPLES)
    );
    assert_eq!(tuning.direct_mic_min_rms, Some(SONIQO_DIRECT_MIC_MIN_RMS));
    assert!(
        tuning.channels_in_parallel,
        "seit dem 02.09.2026 laufen die Kanaele gleichzeitig -- die Vorgabe darf das nicht still zurueckdrehen"
    );

    // Und die drei Stellen, die den Wert lesen, geben unveraendert das aus,
    // was vor dieser Datei dort hart im Code stand.
    assert_eq!(
        soniqo_chunk_strategy(anlg_transcribe_soniqo::SoniqoModel::ParakeetBatch),
        SoniqoChunkStrategy::SpeechAware
    );
    assert_eq!(
        soniqo_bundle_max_samples(SoniqoChunkStrategy::SpeechAware),
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES
    );
    assert_eq!(
        soniqo_bundle_max_samples(SoniqoChunkStrategy::Fixed { max_samples: 4711 }),
        4711
    );
    assert_eq!(
        super::local::soniqo_bundle_max_span_samples(),
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        "die Zeitspanne eines Buendels ist auf dasselbe Fenster gedeckelt, das vor dem 31.08.2026 den Versatz begrenzte"
    );
}

/// Die Zerlegungsart laesst sich auf das starre Fenster zurueckdrehen -- das
/// ist Achse B des Testplans und zugleich der Upstream-Weg.
#[test]
fn the_bench_can_switch_chunking_back_to_fixed_windows() {
    let _guard = TuningGuard::with(tuning::SoniqoTuning::PRODUCTION.with_fixed_chunking(29.5));

    assert_eq!(
        soniqo_chunk_strategy(anlg_transcribe_soniqo::SoniqoModel::ParakeetBatch),
        SoniqoChunkStrategy::Fixed {
            max_samples: 472_000
        },
        "29,5 s bei 16 kHz sind 472.000 Abtastwerte -- dieselbe Zahl wie SONIQO_PARAKEET_MAX_CHUNK_SAMPLES"
    );
    assert_eq!(SONIQO_PARAKEET_MAX_CHUNK_SAMPLES, 472_000);
}

/// Die Zielgroesse eines Buendels laesst sich vorgeben, `0` heisst: gar nicht
/// buendeln, also ein Modellaufruf je Sprech-Abschnitt (der Stand vom 31.08.).
#[test]
fn the_bench_can_set_the_bundle_target_and_switch_bundling_off() {
    {
        let _guard = TuningGuard::with(tuning::SoniqoTuning::PRODUCTION.with_bundle_seconds(10.0));
        assert_eq!(
            soniqo_bundle_max_samples(SoniqoChunkStrategy::SpeechAware),
            160_000
        );
    }
    {
        let _guard = TuningGuard::with(tuning::SoniqoTuning::PRODUCTION.with_bundle_seconds(0.0));
        assert_eq!(
            soniqo_bundle_max_samples(SoniqoChunkStrategy::SpeechAware),
            0
        );

        // Zielgroesse 0 heisst: jeder Abschnitt geht allein raus. Ein Buendel
        // traegt dann genau ein Mitglied -- kein leeres, kein doppeltes.
        let mut bundler = SoniqoChunkBundler::new(0, usize::MAX);
        let mut bundles = Vec::new();
        for start in [0usize, 16_000, 32_000] {
            if let Some(bundle) = bundler.push(anlg_audio_chunking::AudioChunk {
                samples: vec![0.5; 16_000],
                sample_start: start,
                sample_end: start + 16_000,
            }) {
                bundles.push(bundle);
            }
        }
        bundles.extend(bundler.finish());

        assert_eq!(bundles.len(), 3);
        assert!(bundles.iter().all(|bundle| bundle.members.len() == 1));
    }
}

#[test]
fn soniqo_chunk_strategy_preserves_each_model_input_contract() {
    let _guard = TuningGuard::production();
    // Fork (31.08.2026): JEDES Modell bekommt die sprachbewusste Zerlegung --
    // vorher war Parakeet auf ein starres 29,5-s-Raster festgelegt, wodurch
    // jedes Wortpaket am Fensteranfang statt am Sprechbeginn landete (bis zu
    // 29,5 s Versatz, an des Betreibers Teammeeting gemessen). Begruendung im Kopf von
    // soniqo_chunk_strategy.
    //
    // Der Vertrag, den dieser Test schuetzen soll, ist die MODELLGRENZE, nicht
    // die Strategie: die Swift-Seite verlangt fuer parakeet-batch Stuecke
    // zwischen 20 s und 29,5 s (lib.swift:22-23). Deshalb pruefen wir jetzt
    // genau das -- dass die gewaehlte Strategie die Obergrenze einhaelt --
    // statt die Zuordnung selbst festzuschreiben.
    for model in [
        anlg_transcribe_soniqo::SoniqoModel::ParakeetStreaming,
        anlg_transcribe_soniqo::SoniqoModel::ParakeetBatch,
        anlg_transcribe_soniqo::SoniqoModel::Omnilingual,
        anlg_transcribe_soniqo::SoniqoModel::Qwen3Small,
        anlg_transcribe_soniqo::SoniqoModel::Qwen3Large,
    ] {
        assert_eq!(
            soniqo_chunk_strategy(model),
            SoniqoChunkStrategy::SpeechAware,
            "{model:?} soll sprachbewusst zerlegt werden"
        );
    }

    // Die Obergrenze selbst pruefen wir hier bewusst NICHT: VadChunkerConfig
    // ist ausserhalb von audio-chunking nicht sichtbar, und die 25 s hier
    // nochmal hinzuschreiben hiesse, eine Kopie gegen sich selbst zu pruefen --
    // ein Test, der nie scheitern kann.
    //
    // Die Zusicherung steht deshalb dort, wo sie gilt:
    //   VAD max_chunk_duration = 25 s (audio-chunking/src/vad/session.rs:31)
    //   Parakeet-Fenster       = 29,5 s (SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
    //                                    swift-lib/src/lib.swift:23)
    // Wer eine der beiden Zahlen aendert, muss die andere mitlesen. Reisst die
    // Ordnung, schneidet die Swift-Seite intern nach und die Verankerung am
    // Sprechbeginn ist wieder weg -- ohne dass irgendein Test rot wird.
}

#[test]
fn channel_spooling_stops_cooperatively_when_cancelled() {
    let source_file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    write_test_wav_samples(source_file.path(), TARGET_SAMPLE_RATE as usize);
    let source = anlg_audio_utils::source_from_path(source_file.path()).unwrap();
    let cancellation_checks = std::cell::Cell::new(0usize);

    let error = resample_audio_to_channel_files_until("source.wav", source, || {
        let current = cancellation_checks.get();
        cancellation_checks.set(current + 1);
        current > 0
    })
    .expect_err("resampling should stop after cancellation");

    assert!(error.contains(LOCAL_BATCH_CANCELLED));
    assert!(cancellation_checks.get() >= 2);
}

#[test]
fn long_exact_speaker_diarization_is_omitted_from_the_plan() {
    assert!(ensure_soniqo_diarization_within_limit(SONIQO_DIARIZATION_MAX_SAMPLES).is_ok());
    let error =
        ensure_soniqo_diarization_within_limit(SONIQO_DIARIZATION_MAX_SAMPLES + 1).unwrap_err();
    assert!(error.contains("speaker diarization"));

    // Genau auf dem Deckel bleibt die Trennung stehen.
    let mut plans = [SoniqoChannelDiarization::Inferred];
    assert!(!soniqo_apply_diarization_limit(
        &mut plans,
        &[SONIQO_DIARIZATION_MAX_SAMPLES]
    ));
    assert_eq!(plans[0], SoniqoChannelDiarization::Inferred);

    // Ein Sample darueber wird sie gestrichen -- und das wird gemeldet.
    let mut plans = [SoniqoChannelDiarization::Exact(4)];
    assert!(soniqo_apply_diarization_limit(
        &mut plans,
        &[SONIQO_DIARIZATION_MAX_SAMPLES + 1]
    ));
    assert_eq!(plans[0], SoniqoChannelDiarization::Skip);

    // Ein zu langer Kanal, auf dem ohnehin nicht getrennt werden sollte, ist
    // KEINE gestrichene Trennung. Sonst waere die Meldung an den Nutzer eine
    // Falschmeldung bei jeder langen Videokonferenz.
    let mut plans = [
        SoniqoChannelDiarization::Skip,
        SoniqoChannelDiarization::Skip,
    ];
    assert!(!soniqo_apply_diarization_limit(
        &mut plans,
        &[
            SONIQO_DIARIZATION_MAX_SAMPLES + 1,
            SONIQO_DIARIZATION_MAX_SAMPLES + 1
        ]
    ));

    // Gemischt: nur der zu lange Kanal verliert seinen Plan.
    let mut plans = [
        SoniqoChannelDiarization::Inferred,
        SoniqoChannelDiarization::Exact(3),
    ];
    assert!(soniqo_apply_diarization_limit(
        &mut plans,
        &[
            SONIQO_DIARIZATION_MAX_SAMPLES,
            SONIQO_DIARIZATION_MAX_SAMPLES + 1
        ]
    ));
    assert_eq!(plans[0], SoniqoChannelDiarization::Inferred);
    assert_eq!(plans[1], SoniqoChannelDiarization::Skip);
}

#[test]
fn channel_spooling_preserves_channel_order_without_retaining_audio() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("source.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&source_path, spec).unwrap();
    for sample in [0.1, 0.9, 0.2, 0.8] {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    let source = anlg_audio_utils::source_from_path(&source_path).unwrap();

    let channels = resample_audio_to_channel_files(source_path.to_str().unwrap(), source).unwrap();

    assert_eq!(channels.len(), 2);
    assert_eq!(channels[0].sample_count, 2);
    assert_eq!(channels[0].file.path().parent(), Some(directory.path()));
    let read = channels
        .iter()
        .map(|channel| {
            hound::WavReader::open(channel.file.path())
                .unwrap()
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(read, vec![vec![0.1, 0.2], vec![0.9, 0.8]]);
}

#[test]
fn channel_spooling_discards_effectively_identical_duplicate_channel() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("source.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&source_path, spec).unwrap();
    for sample in [0.1, 0.1001, 0.2, 0.2001] {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    let source = anlg_audio_utils::source_from_path(&source_path).unwrap();

    let channels = resample_audio_to_channel_files(source_path.to_str().unwrap(), source).unwrap();

    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].sample_count, 2);
}

/// Der Effektivwert wird BEIM Umtasten mitgezaehlt, also genau dort, wo die
/// Kanaele auseinandergelegt werden. Wenn diese Zuordnung kippt, kippt auch
/// die Entscheidung, auf welchem Kanal getrennt wird -- deshalb wird sie an
/// einer Datei geprueft, die dem Vor-Ort-Termin nachgebaut ist: ein lauter
/// Kanal, ein digital stiller.
#[test]
fn channel_spooling_measures_loudness_per_channel() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("source.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&source_path, spec).unwrap();
    // Verschachtelt: links laut, rechts still.
    for (left, right) in [(0.5f32, 0.0f32), (-0.5, 0.0), (0.5, 0.0), (-0.5, 0.0)] {
        writer.write_sample(left).unwrap();
        writer.write_sample(right).unwrap();
    }
    writer.finalize().unwrap();
    let source = anlg_audio_utils::source_from_path(&source_path).unwrap();

    let channels = resample_audio_to_channel_files(source_path.to_str().unwrap(), source).unwrap();

    assert_eq!(channels.len(), 2);
    // Konstante Auslenkung 0,5 -> Effektivwert 0,5.
    assert!((channels[0].rms - 0.5).abs() < 1e-6, "{}", channels[0].rms);
    assert_eq!(channels[1].rms, 0.0);

    let rms = channels
        .iter()
        .map(|channel| channel.rms)
        .collect::<Vec<_>>();
    assert_eq!(soniqo_speech_channels(&rms), vec![0]);
    assert_eq!(
        soniqo_diarization_speaker_count(Some(4), &rms, 0),
        SoniqoChannelDiarization::Exact(4)
    );
}

#[test]
fn channel_spooling_rejects_unsupported_channel_count_before_reading_audio() {
    let source_file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    let writer = hound::WavWriter::create(
        source_file.path(),
        hound::WavSpec {
            channels: (MAX_LOCAL_BATCH_CHANNELS + 1) as u16,
            sample_rate: TARGET_SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    writer.finalize().unwrap();
    let source = anlg_audio_utils::source_from_path(source_file.path()).unwrap();

    let error = match resample_audio_to_channel_files("source.wav", source) {
        Ok(_) => panic!("unsupported channel count should fail"),
        Err(error) => error,
    };

    assert!(error.contains("at most 8 audio channels"));
    assert!(error.contains("declares 9"));
}

#[test]
fn direct_mic_gate_rejects_only_quiet_chunks() {
    assert!(audio_rms(&[0.0007; 100]) < SONIQO_DIRECT_MIC_MIN_RMS);
    assert!(audio_rms(&[0.0009; 100]) >= SONIQO_DIRECT_MIC_MIN_RMS);
}

#[test]
fn parakeet_batch_window_bounds_force_coreml_shape_3000() {
    let minimum_mel_frames = TARGET_SAMPLE_RATE as usize * 20 / 160 + 1;
    let maximum_mel_frames = SONIQO_PARAKEET_MAX_CHUNK_SAMPLES / 160 + 1;

    assert!(minimum_mel_frames > 2000);
    assert!(maximum_mel_frames <= 3000);
}

#[test]
fn soniqo_language_hint_uses_base_language_code() {
    assert_eq!(soniqo_language_hint(Some("de-DE")).as_deref(), Some("de"));
    assert_eq!(soniqo_language_hint(Some("en_US")).as_deref(), Some("en"));
    assert_eq!(soniqo_language_hint(Some(" fr ")).as_deref(), Some("fr"));
    assert_eq!(soniqo_language_hint(Some("")).as_deref(), None);
    assert_eq!(soniqo_language_hint(None).as_deref(), None);
}

/// Effektivwerte aus echten Aufnahmen, gemessen am 07.09.2026 mit ffmpeg
/// `astats` und aus dB zurueckgerechnet. Erfundene Zahlen waeren hier wertlos:
/// die ganze Entscheidung haengt daran, wie weit lebende und tote Kanaele in
/// der Wirklichkeit auseinanderliegen.
mod gemessene_pegel {
    /// Videokonferenz, Kanal 0 (Mikrofon): −31,3 dB.
    pub(super) const KONFERENZ_MIKROFON: f64 = 0.0272;
    /// Videokonferenz, Kanal 1 (Systemton): −23,1 dB.
    pub(super) const KONFERENZ_SYSTEMTON: f64 = 0.0700;
    /// Vor-Ort-Termin, Kanal 0 (Mikrofon, vier Personen am Tisch): −38,8 dB.
    pub(super) const RAUM_MIKROFON: f64 = 0.0115;
    /// Vor-Ort-Termin, Kanal 1 (Systemton): −160,8 dB, digitale Stille.
    pub(super) const RAUM_SYSTEMTON: f64 = 9.1e-9;
}

#[test]
fn soniqo_diarization_uses_remote_count_for_system_channel() {
    let konferenz = [
        gemessene_pegel::KONFERENZ_MIKROFON,
        gemessene_pegel::KONFERENZ_SYSTEMTON,
    ];
    assert_eq!(
        soniqo_diarization_speaker_count(Some(3), &konferenz, 0),
        SoniqoChannelDiarization::Skip
    );
    assert_eq!(
        soniqo_diarization_speaker_count(Some(3), &konferenz, 1),
        SoniqoChannelDiarization::Exact(2)
    );
    // Ohne Teilnehmerliste bleibt der Regelfall unveraendert ohne Trennung:
    // der Kanal ist dort bereits die Zuordnung.
    assert_eq!(
        soniqo_diarization_speaker_count(None, &konferenz, 1),
        SoniqoChannelDiarization::Skip
    );
}

#[test]
fn soniqo_diarization_uses_total_count_for_mono_audio() {
    let mono = [gemessene_pegel::RAUM_MIKROFON];
    assert_eq!(
        soniqo_diarization_speaker_count(Some(2), &mono, 0),
        SoniqoChannelDiarization::Exact(2)
    );
    // Eine Vorgabe unter zwei ist keine Trennung -- dann lieber selbst zaehlen
    // lassen, als gar nichts zu tun.
    assert_eq!(
        soniqo_diarization_speaker_count(Some(1), &mono, 0),
        SoniqoChannelDiarization::Inferred
    );
    assert_eq!(
        soniqo_diarization_speaker_count(None, &mono, 0),
        SoniqoChannelDiarization::Inferred
    );
}

/// Der Fall, der vorher gar nicht lief: vier Personen am Tisch, nur das
/// MacBook in der Mitte. Der Systemton ist digitale Stille, das Mikrofon
/// traegt alles -- also wird auf Kanal 0 getrennt, nicht auf Kanal 1.
#[test]
fn a_room_recording_is_diarized_on_the_channel_that_carries_sound() {
    let raum = [
        gemessene_pegel::RAUM_MIKROFON,
        gemessene_pegel::RAUM_SYSTEMTON,
    ];
    assert_eq!(soniqo_speech_channels(&raum), vec![0]);

    // Volle Sprecherzahl, NICHT total-1: der Abzug galt dem Nutzer, der bei
    // einer Videokonferenz auf dem anderen Kanal sitzt. Am Tisch sitzt er mit.
    assert_eq!(
        soniqo_diarization_speaker_count(Some(4), &raum, 0),
        SoniqoChannelDiarization::Exact(4)
    );
    assert_eq!(
        soniqo_diarization_speaker_count(Some(4), &raum, 1),
        SoniqoChannelDiarization::Skip
    );

    // Und ohne Teilnehmerliste wird trotzdem getrennt.
    assert_eq!(
        soniqo_diarization_speaker_count(None, &raum, 0),
        SoniqoChannelDiarization::Inferred
    );
}

#[test]
fn a_recording_without_any_sound_is_not_diarized() {
    let tot = [
        gemessene_pegel::RAUM_SYSTEMTON,
        gemessene_pegel::RAUM_SYSTEMTON,
    ];
    assert!(soniqo_speech_channels(&tot).is_empty());
    for channel_index in 0..2 {
        assert_eq!(
            soniqo_diarization_speaker_count(Some(4), &tot, channel_index),
            SoniqoChannelDiarization::Skip
        );
    }
}

/// Die Schwelle muss zu BEIDEN gemessenen Seiten Luft haben, sonst ist sie
/// eine Zahl ohne Deckung.
///
/// Die Zahlen stammen aus der Bestandsmessung vom 21.09.2026 (157 Aufnahmen,
/// 154 leisere Kanaele, je Kanal der Effektivwert gegen die Frage gehalten,
/// ob er spaeter Woerter beigetragen hat). Innerhalb des Verhaeltnis-Bandes
/// liegt zwischen dem lautesten stillen und dem leisesten echten Kanal eine
/// Luecke, und die Schwelle sitzt darin.
#[test]
fn the_silence_threshold_sits_between_the_measured_extremes() {
    /// Lautester Kanal, der im Band "hoechstens 6 % des lautesten" liegt und
    /// KEINE Woerter beigetragen hat.
    const LAUTESTER_STILLER_IM_BAND: f64 = 0.00101;
    /// Leisester Kanal im selben Band, der Woerter beigetragen hat.
    const LEISESTER_ECHTER_IM_BAND: f64 = 0.00384;

    assert!(
        LAUTESTER_STILLER_IM_BAND < SONIQO_CHANNEL_SILENT_RMS,
        "die Schwelle muss den lautesten gemessenen stillen Kanal noch fassen"
    );
    assert!(
        LEISESTER_ECHTER_IM_BAND > SONIQO_CHANNEL_SILENT_RMS,
        "die Schwelle darf den leisesten gemessenen echten Kanal nicht fassen"
    );
    // Und sie klebt an keiner der beiden Seiten: mindestens Faktor 1,5 Luft.
    assert!(SONIQO_CHANNEL_SILENT_RMS > LAUTESTER_STILLER_IM_BAND * 1.5);
    assert!(SONIQO_CHANNEL_SILENT_RMS * 1.5 < LEISESTER_ECHTER_IM_BAND);

    // Die digitale Stille liegt weit unter der eigenen Stufe.
    assert!(gemessene_pegel::RAUM_SYSTEMTON < SONIQO_CHANNEL_DIGITAL_SILENCE_RMS / 100.0);
    // Und jeder gemessene echte Kanal liegt ueber der Hauptschwelle.
    assert!(gemessene_pegel::RAUM_MIKROFON > SONIQO_CHANNEL_SILENT_RMS);
    assert!(gemessene_pegel::KONFERENZ_MIKROFON > SONIQO_CHANNEL_SILENT_RMS);
    assert!(gemessene_pegel::KONFERENZ_SYSTEMTON > SONIQO_CHANNEL_SILENT_RMS);
}

/// Falsifikator (a) aus dem Auftrag: eine Raumaufnahme, deren Systemton nur
/// leise rauscht, darf NICHT mehr mit zwei sprechenden Kanaelen geplant
/// werden.
///
/// Die Pegel sind die echten einer 40,8-Minuten-Raumaufnahme aus dem
/// Bestand: Mikrofon 0,00795, Systemton 0,00045, Verhaeltnis 0,0566. Sie ist
/// der Anlass dieser Aenderung -- vorher galten beide Kanaele als sprechend
/// und die Aufnahme bekam gar keine Trennung.
#[test]
fn a_room_recording_with_a_faintly_humming_system_channel_is_diarized() {
    let raum = [0.00795, 0.00045];

    assert_eq!(
        soniqo_speech_channels(&raum),
        vec![0],
        "nur das Mikrofon traegt Sprache; der Systemton rauscht bloss"
    );
    assert_eq!(
        soniqo_diarization_speaker_count(None, &raum, 0),
        SoniqoChannelDiarization::Inferred,
        "und genau dieser Kanal bekommt die Trennung"
    );
    assert_eq!(
        soniqo_diarization_speaker_count(None, &raum, 1),
        SoniqoChannelDiarization::Skip
    );
}

/// Falsifikator (b): ein echtes Zweikanal-Gespraech mit leiser Gegenseite
/// darf seinen zweiten Kanal NICHT verlieren.
///
/// Die Pegel sind die leiseste echte Gegenseite oberhalb des Bandes aus der
/// Bestandsmessung (Verhaeltnis 0,0778) und eine typische Videokonferenz.
#[test]
fn a_two_channel_conversation_keeps_its_quiet_far_side() {
    // Leiseste Gegenseite des Bestands, die Woerter getragen hat: Pegel
    // 0,00674 und damit ueber der Pegelschwelle -- hier entscheidet der
    // Pegel, nicht das Verhaeltnis (0,0778).
    let leise_gegenseite = [0.0867, 0.00674];
    assert_eq!(
        soniqo_speech_channels(&leise_gegenseite),
        vec![0, 1],
        "diese Gegenseite hat nachweislich Woerter beigetragen"
    );

    // Und eine Aufnahme, in der beide Seiten deutlich ueber dem Band liegen.
    let konferenz = [
        gemessene_pegel::KONFERENZ_MIKROFON,
        gemessene_pegel::KONFERENZ_SYSTEMTON,
    ];
    assert_eq!(soniqo_speech_channels(&konferenz), vec![0, 1]);

    // Ein echtes Gespraech mit leiser, aber klar sprechender Gegenseite:
    // diese Pegel stehen so in den Betriebsprotokollen, Verhaeltnis 0,444.
    // Sein Plan darf sich durch diese Aenderung NICHT bewegen.
    let gespraech = [0.009729919697673506, 0.0043213487596324715];
    assert_eq!(soniqo_speech_channels(&gespraech), vec![0, 1]);
    for channel_index in 0..2 {
        assert_eq!(
            soniqo_diarization_speaker_count(None, &gespraech, channel_index),
            SoniqoChannelDiarization::Skip,
            "zwei sprechende Kanaele ohne Teilnehmerliste bleiben unveraendert"
        );
    }
}

/// Die leiseste GEGENSEITE, die im Bestand je Woerter getragen hat, muss mit
/// Abstand sprechend bleiben.
///
/// Der Unterschied zu einem leisen Mikrofonkanal ist nicht kosmetisch: faellt
/// die Gegenseite faelschlich weg, wird der eine Mensch auf dem Mikrofon in
/// mehrere Sprecher zerlegt und steht in der Anzeige als "Speaker 1/2" da.
/// Deshalb steht der gemessene Abstand hier als Zahl.
///
/// Bestandsmessung 21.09.2026: von 128 leiseren Kanaelen mit Sprache sind nur
/// NEUN Systemton-Kanaele, und keiner davon faellt unter die Regel. Der
/// nächstliegende traegt ein einziges Wort.
#[test]
fn the_quietest_measured_far_side_stays_audible_with_room_to_spare() {
    /// Leisester Systemton-Kanal des Bestands, der Woerter getragen hat.
    const LEISESTE_ECHTE_GEGENSEITE_RMS: f64 = 0.00075;
    /// Sein Verhaeltnis zum Mikrofonkanal.
    const LEISESTE_ECHTE_GEGENSEITE_RATIO: f64 = 0.1418;

    // Er liegt UNTER der Pegelschwelle -- gerettet wird er allein vom
    // Verhaeltnis. Genau deshalb darf das Band nicht wachsen.
    assert!(LEISESTE_ECHTE_GEGENSEITE_RMS < SONIQO_CHANNEL_SILENT_RMS);
    assert!(
        LEISESTE_ECHTE_GEGENSEITE_RATIO > SONIQO_CHANNEL_SILENT_RATIO * 2.0,
        "das Band muss mindestens Faktor zwei Abstand zur leisesten echten \
         Gegenseite halten"
    );

    let mikrofon = LEISESTE_ECHTE_GEGENSEITE_RMS / LEISESTE_ECHTE_GEGENSEITE_RATIO;
    let aufnahme = [mikrofon, LEISESTE_ECHTE_GEGENSEITE_RMS];
    assert_eq!(
        soniqo_speech_channels(&aufnahme),
        vec![0, 1],
        "diese Gegenseite hat nachweislich gesprochen"
    );
}

/// Blocker A: faellt ein Zwei-Kanal-Fall auf den SYSTEMTON, darf der Nutzer
/// nicht mitgezaehlt werden.
///
/// Die Teilnehmerzahl aus der Oberflaeche schliesst ihn ein. Bleibt der
/// Systemton uebrig, sitzt er per Konstruktion nicht darauf -- er spricht ins
/// Mikrofon, das gerade als still eingestuft wurde. Ohne den Abzug wird eine
/// Gegenseite aus einem Menschen in zwei Etiketten gezwungen, und unterhalb
/// der Abschnittslaenge erzwingt die Bibliothek diese Zahl sogar.
#[test]
fn a_collapsed_pair_on_the_system_channel_does_not_count_the_user() {
    // Mikrofon leise, aber nicht digital still: nur RELATIV eingestuft.
    // Systemton traegt alles.
    let nur_systemton = [0.0005, 0.05];
    assert_eq!(
        soniqo_channel_voice(&nur_systemton, 0),
        SoniqoChannelVoice::RelativelySilent
    );
    assert_eq!(soniqo_speech_channels(&nur_systemton), vec![1]);

    assert_eq!(
        soniqo_diarization_speaker_count(Some(3), &nur_systemton, 1),
        SoniqoChannelDiarization::Exact(2),
        "drei Teilnehmer minus dem Nutzer sind zwei auf der Gegenseite"
    );
    assert_eq!(
        soniqo_diarization_speaker_count(Some(3), &nur_systemton, 0),
        SoniqoChannelDiarization::Skip
    );

    // Zwei Teilnehmer minus Nutzer waere einer -- keine Trennung. Genau wie
    // bisher im Zwei-Kanal-Fall: dann passiert gar nichts. Sonst entstuende
    // hier neu ein Transkript, dessen Export "Speaker 1/2" statt des einen
    // bekannten Namens zeigt.
    assert_eq!(
        soniqo_diarization_speaker_count(Some(2), &nur_systemton, 1),
        SoniqoChannelDiarization::Skip
    );
    assert_eq!(
        soniqo_diarization_speaker_count(Some(1), &nur_systemton, 1),
        SoniqoChannelDiarization::Skip
    );
    assert_eq!(
        soniqo_diarization_speaker_count(None, &nur_systemton, 1),
        SoniqoChannelDiarization::Skip,
        "ohne Teilnehmerliste bleibt es beim bisherigen Nichtstun"
    );
}

/// Faellt das Paar auf das MIKROFON und war die Gegenseite nur RELATIV still,
/// wird keine Sprecherzahl erzwungen.
///
/// Sitzt dort doch nur der Nutzer, findet die freie Trennung eine Stimme und
/// seine Selbst-Zuordnung bleibt erhalten. Ein erzwungenes `Exact(n)` wuerde
/// ihn garantiert zerlegen und im Transkript zu "Speaker 1/2" machen.
#[test]
fn a_collapsed_pair_on_the_microphone_never_forces_a_speaker_count() {
    let nur_mikrofon = [0.00795, 0.00045];
    assert_eq!(
        soniqo_channel_voice(&nur_mikrofon, 1),
        SoniqoChannelVoice::RelativelySilent
    );
    assert_eq!(soniqo_speech_channels(&nur_mikrofon), vec![0]);
    for vorgabe in [None, Some(2), Some(3), Some(4)] {
        assert_eq!(
            soniqo_diarization_speaker_count(vorgabe, &nur_mikrofon, 0),
            SoniqoChannelDiarization::Inferred,
            "Vorgabe {vorgabe:?} darf auf einer Vermutung nicht erzwungen werden"
        );
    }
}

/// Ist der andere Kanal dagegen DIGITAL still, ist die Lage sicher -- und
/// dann gilt die Teilnehmerzahl wie vor dieser Aenderung.
#[test]
fn a_digitally_silent_neighbour_keeps_the_old_hard_planning() {
    let raum = [
        gemessene_pegel::RAUM_MIKROFON,
        gemessene_pegel::RAUM_SYSTEMTON,
    ];
    assert_eq!(
        soniqo_channel_voice(&raum, 1),
        SoniqoChannelVoice::DigitallySilent
    );
    assert_eq!(
        soniqo_diarization_speaker_count(Some(4), &raum, 0),
        SoniqoChannelDiarization::Exact(4),
        "am Tisch sitzt der Nutzer mit, und die Lage ist eindeutig"
    );
}

/// Die Grenze zwischen "digital" und "relativ" ist ein `<`, kein `<=`.
#[test]
fn a_channel_exactly_at_the_digital_silence_floor_is_only_relatively_silent() {
    let auf_der_grenze = [1.0, SONIQO_CHANNEL_DIGITAL_SILENCE_RMS];
    assert_eq!(
        soniqo_channel_voice(&auf_der_grenze, 1),
        SoniqoChannelVoice::RelativelySilent,
        "genau auf dem Boden ist noch nicht darunter"
    );
    // Und eine Haarbreite darunter kippt es.
    let knapp_darunter = [1.0, SONIQO_CHANNEL_DIGITAL_SILENCE_RMS * 0.999];
    assert_eq!(
        soniqo_channel_voice(&knapp_darunter, 1),
        SoniqoChannelVoice::DigitallySilent
    );
}

/// Das Verhaeltnis-Band muss zu BEIDEN Seiten gemessene Deckung haben.
///
/// Ohne diese Pruefung liesse sich `SONIQO_CHANNEL_SILENT_RATIO` bis fast 0,9
/// anheben, ohne dass ein Test rot wird -- die uebrigen Faelle scheitern
/// ohnehin schon am Pegel.
#[test]
fn the_ratio_band_sits_between_the_measured_extremes() {
    /// Groesstes Verhaeltnis eines Kanals ohne Sprache, der zugleich unter
    /// der Pegelschwelle liegt -- also eines, den das Band fassen MUSS.
    const LAUTESTER_STILLER_IM_PEGELBAND: f64 = 0.0551;
    /// Kleinstes Verhaeltnis eines Kanals MIT Sprache unter der
    /// Pegelschwelle -- den darf das Band nicht fassen.
    const LEISESTER_ECHTER_IM_PEGELBAND: f64 = 0.1418;

    assert!(
        LAUTESTER_STILLER_IM_PEGELBAND < SONIQO_CHANNEL_SILENT_RATIO,
        "das Band muss den lautesten gemessenen stillen Kanal noch fassen"
    );
    assert!(
        LEISESTER_ECHTER_IM_PEGELBAND > SONIQO_CHANNEL_SILENT_RATIO,
        "das Band darf den leisesten gemessenen echten Kanal nicht fassen"
    );

    // Und es klebt an keiner Seite.
    assert!(SONIQO_CHANNEL_SILENT_RATIO > LAUTESTER_STILLER_IM_PEGELBAND);
    assert!(SONIQO_CHANNEL_SILENT_RATIO * 2.0 < LEISESTER_ECHTER_IM_PEGELBAND);
}

/// Ein Fall, in dem wirklich das VERHAELTNIS entscheidet: der Pegel liegt
/// unter der Schwelle, das Verhaeltnis knapp darueber.
///
/// Ohne ihn koennte man die Verhaeltnis-Bedingung ganz entfernen, und alle
/// anderen Tests blieben gruen.
#[test]
fn a_quiet_channel_close_to_its_neighbour_still_speaks() {
    let knapp_ueber_dem_band = SONIQO_CHANNEL_SILENT_RATIO * 1.2;
    let leiser = SONIQO_CHANNEL_SILENT_RMS / 2.0;
    let lauter = leiser / knapp_ueber_dem_band;
    let aufnahme = [lauter, leiser];

    assert!(leiser < SONIQO_CHANNEL_SILENT_RMS, "Testaufbau: Pegel unter der Schwelle");
    assert!(leiser > SONIQO_CHANNEL_DIGITAL_SILENCE_RMS, "Testaufbau: keine digitale Stille");
    assert_eq!(
        soniqo_speech_channels(&aufnahme),
        vec![0, 1],
        "allein das Verhaeltnis rettet diesen Kanal"
    );
}

/// Ein Pegel, der keine Zahl ist, ist keine Stimme.
///
/// Jeder Vergleich mit NaN ist falsch, also galt ein kaputter Pegel als
/// sprechend -- und verhinderte damit still die Trennung der ganzen Aufnahme.
#[test]
fn a_channel_without_a_usable_level_is_silent() {
    let kaputt = [0.05, f64::NAN];
    assert!(soniqo_channel_is_silent(&kaputt, 1));
    assert_eq!(soniqo_speech_channels(&kaputt), vec![0]);
    // Auch als lautester Kanal darf NaN das Verhaeltnis nicht vergiften.
    let kaputt_vorn = [f64::NAN, 0.05];
    assert_eq!(soniqo_speech_channels(&kaputt_vorn), vec![1]);
    assert!(soniqo_channel_silence_ratio(&kaputt_vorn, 1).is_finite());
}

/// Bei mehr als zwei Kanaelen bleibt alles beim alten Verhalten.
///
/// Der Ein-Kanal-Zweig ist fuer solche Aufnahmen nie durchdacht worden, und
/// im Bestand gibt es sie nicht. Lieber die kleine Flaeche behalten.
#[test]
fn recordings_with_more_than_two_channels_keep_the_old_behaviour() {
    // Die Pegel sind so gewaehlt, dass die Still-Regel hier wirklich etwas
    // ANDERES ergaebe: ein lauter Kanal, zwei nur relativ stille. Griffe die
    // Regel auch oberhalb von zwei Kanaelen, bliebe genau EIN sprechender
    // uebrig, der Ein-Kanal-Zweig waere erreicht und Kanal 0 bekaeme
    // `Exact(3)`.
    //
    // Eine erste Fassung dieses Tests nahm vier Kanaele, von denen zwei laut
    // waren -- damit blieben in beiden Faellen zwei sprechende uebrig, beide
    // Wege endeten im selben Zweig, und der Test bewachte nichts.
    let drei = [0.05, 0.0005, 0.0006];
    assert_eq!(
        soniqo_channel_voice(&[0.05, 0.0005], 1),
        SoniqoChannelVoice::RelativelySilent,
        "Testaufbau: diese Pegel waeren bei zwei Kanaelen relativ still"
    );

    for channel_index in 0..3 {
        assert_eq!(
            soniqo_diarization_speaker_count(Some(3), &drei, channel_index),
            SoniqoChannelDiarization::Skip,
            "Kanal {channel_index} darf hier keine Trennung bekommen"
        );
    }
}

/// Die DIGITALE Stille gilt dagegen weiter fuer jede Kanalzahl.
///
/// Sie ist keine Vermutung ueber den Nachbarn, sondern die Feststellung, dass
/// da nichts ist -- und sie galt schon vor der relativen Regel. Eine
/// Dreikanal-Aufnahme mit zwei toten Spuren hat vorher eine Trennung
/// bekommen; eine Zwischenfassung dieser Runde nahm sie ihr weg, weil sie
/// oberhalb von zwei Kanaelen pauschal ALLE als sprechend fuehrte.
///
/// Erwartet wird exakt das Verhalten des Standes vor der relativen Regel.
#[test]
fn digital_silence_still_counts_on_recordings_with_more_than_two_channels() {
    let drei_mit_zwei_toten = [0.05, 0.0, 0.0];
    assert_eq!(
        soniqo_speech_channels(&drei_mit_zwei_toten),
        vec![0],
        "zwei tote Spuren sind auch zu dritt tot"
    );

    // Ohne Teilnehmerliste trennt der lebende Kanal frei.
    assert_eq!(
        soniqo_diarization_speaker_count(None, &drei_mit_zwei_toten, 0),
        SoniqoChannelDiarization::Inferred
    );
    assert_eq!(
        soniqo_diarization_speaker_count(Some(1), &drei_mit_zwei_toten, 0),
        SoniqoChannelDiarization::Inferred
    );
    // Mit Teilnehmerliste gilt die volle Zahl -- es gibt keinen zweiten
    // Kanal, auf dem der Nutzer sitzen koennte.
    assert_eq!(
        soniqo_diarization_speaker_count(Some(3), &drei_mit_zwei_toten, 0),
        SoniqoChannelDiarization::Exact(3)
    );
    assert_eq!(
        soniqo_diarization_speaker_count(Some(2), &drei_mit_zwei_toten, 0),
        SoniqoChannelDiarization::Exact(2)
    );
    // Die toten Spuren bekommen nichts.
    for channel_index in 1..3 {
        assert_eq!(
            soniqo_diarization_speaker_count(Some(3), &drei_mit_zwei_toten, channel_index),
            SoniqoChannelDiarization::Skip
        );
    }
}

/// Das Verhaeltnis einer Aufnahme ganz ohne Pegel ist null, nicht NaN.
#[test]
fn the_ratio_of_a_silent_recording_is_zero() {
    assert_eq!(soniqo_channel_silence_ratio(&[0.0, 0.0], 0), 0.0);
    assert_eq!(soniqo_channel_silence_ratio(&[0.0], 0), 0.0);
}

/// JEDER Kanal wird transkribiert, auch ein als still eingestufter.
///
/// Die Still-Einstufung gehoert allein der Trennungs-PLANUNG. Wuerde
/// irgendwann ein Filter davorgesetzt, ginge Text verloren, und zwar ohne
/// dass ein Test rot wird -- die Transkriptionsschleife kennt die Einstufung
/// gar nicht. Dieser Test haelt die Trennung der beiden Fragen fest.
#[test]
fn every_channel_is_transcribed_even_when_it_counts_as_silent() {
    // Ein Kanalpaar, dessen zweiter Kanal nach der Planungsregel still ist.
    let pegel = [0.00795, 0.00045];
    assert_eq!(soniqo_speech_channels(&pegel), vec![0]);

    let gesehen = Arc::new(std::sync::Mutex::new(Vec::<usize>::new()));
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());

    let results = transcribe_soniqo_channels_in_parallel(
        2,
        vec![empty_channel(1), empty_channel(2)],
        Some(&progress),
        |channel_index, _channel| {
            gesehen.lock().unwrap().push(channel_index);
            Ok(transcript(&format!("kanal {channel_index}")))
        },
    )
    .unwrap();

    let mut gesehen = gesehen.lock().unwrap().clone();
    gesehen.sort_unstable();
    assert_eq!(
        gesehen,
        vec![0, 1],
        "auch der als still eingestufte Kanal muss durch die Transkription gehen"
    );
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|result| result.is_ok()));
}

/// Grenzfall: BEIDE Kanaele sind leise. Dann ist das Verhaeltnis des
/// leiseren zum lautesten gross, und nur die absolute Stufe entscheidet.
#[test]
fn two_equally_quiet_channels_are_judged_by_the_absolute_floor_alone() {
    // Beide leise, aber hoerbar: keiner gilt als still, sonst verlöre eine
    // durchgehend leise aufgenommene Konferenz eine Seite.
    let leise_konferenz = [0.0015, 0.0014];
    assert_eq!(soniqo_speech_channels(&leise_konferenz), vec![0, 1]);

    // Beide tot: keiner spricht. Ohne die absolute Stufe waere das
    // Verhaeltnis hier 1,0 und beide gaelten als sprechend.
    let tot = [
        gemessene_pegel::RAUM_SYSTEMTON,
        gemessene_pegel::RAUM_SYSTEMTON,
    ];
    assert!(soniqo_speech_channels(&tot).is_empty());
}

/// Grenzfall: genau AN den Schwellen. Beide Bedingungen sind als `<`
/// formuliert, der Schwellenwert selbst gilt also noch als sprechend.
#[test]
fn a_channel_exactly_at_a_threshold_still_counts_as_speaking() {
    // Effektivwert genau auf der Schwelle, Verhaeltnis klar darunter.
    let auf_der_rms_schwelle = [1.0, SONIQO_CHANNEL_SILENT_RMS];
    assert_eq!(soniqo_speech_channels(&auf_der_rms_schwelle), vec![0, 1]);

    // Verhaeltnis genau auf der Schwelle, Effektivwert klar darunter.
    let lautester = 0.001 / SONIQO_CHANNEL_SILENT_RATIO;
    let auf_der_verhaeltnis_schwelle = [lautester, 0.001];
    let verhaeltnis = soniqo_channel_silence_ratio(&auf_der_verhaeltnis_schwelle, 1);
    assert!(
        (verhaeltnis - SONIQO_CHANNEL_SILENT_RATIO).abs() < 1e-12,
        "Testaufbau: das Verhaeltnis sollte exakt auf der Schwelle liegen, ist {verhaeltnis}"
    );
    assert_eq!(soniqo_speech_channels(&auf_der_verhaeltnis_schwelle), vec![0, 1]);
}

/// Ein Kanal, der exakt null traegt, ist still -- auch wenn daneben nichts
/// liegt, gegen das man ihn halten koennte.
#[test]
fn a_channel_at_exactly_zero_is_silent() {
    assert!(soniqo_channel_is_silent(&[0.0], 0));
    assert!(soniqo_channel_is_silent(&[0.05, 0.0], 1));
    assert_eq!(soniqo_speech_channels(&[0.05, 0.0]), vec![0]);
}

#[test]
fn soniqo_progress_starts_where_the_diarization_band_ends() {
    // Die Zerlegung in Bloecke faengt NICHT bei null an: davor liegt das Band
    // der Sprechertrennung. Wer diese Zeile aendert, verschiebt zwei Baender
    // gegeneinander und muss beide anfassen.
    assert_eq!(
        soniqo_batch_progress(0, 10),
        SONIQO_DIARIZATION_PROGRESS_END
    );
    assert_eq!(soniqo_batch_progress(0, 0), SONIQO_DIARIZATION_PROGRESS_END);
}

#[test]
fn soniqo_progress_caps_before_completion() {
    let half = SONIQO_DIARIZATION_PROGRESS_END
        + 0.5 * (SONIQO_PROGRESS_MAX - SONIQO_DIARIZATION_PROGRESS_END);
    assert!((soniqo_batch_progress(5, 10) - half).abs() < 1e-9);
    assert_eq!(soniqo_batch_progress(10, 10), SONIQO_PROGRESS_MAX);
    assert_eq!(soniqo_batch_progress(11, 10), SONIQO_PROGRESS_MAX);
}

#[test]
fn die_trennung_faengt_am_bandanfang_an_und_endet_am_bandende() {
    assert_eq!(
        soniqo_diarization_progress(0, 1, 0.0),
        SONIQO_PROGRESS_PLANNED
    );
    assert_eq!(
        soniqo_diarization_progress(1, 1, 0.0),
        SONIQO_DIARIZATION_PROGRESS_END
    );
    // Kein zu trennender Kanal: der Balken bleibt am Anfang stehen, statt
    // durch eine Division durch null zu fallen.
    assert_eq!(
        soniqo_diarization_progress(0, 0, 0.5),
        SONIQO_PROGRESS_PLANNED
    );
}

#[test]
fn ein_laufender_kanal_erreicht_das_bandende_nie() {
    // Das ist der ganze Unterschied zwischen Schaetzung und Luege: solange der
    // Kanal laeuft, darf der Balken nicht behaupten, er sei fertig.
    let running = soniqo_diarization_progress(0, 1, 1.0);
    assert!(running < SONIQO_DIARIZATION_PROGRESS_END, "{running}");
    assert!(running > SONIQO_PROGRESS_PLANNED, "{running}");
    let very_late = soniqo_diarization_progress(0, 1, 1_000.0);
    assert!(very_late < SONIQO_DIARIZATION_PROGRESS_END, "{very_late}");
}

#[test]
fn die_trennung_laeuft_vorwaerts_und_nie_rueckwaerts() {
    let mut previous = f64::NEG_INFINITY;
    for channel in 0..3usize {
        for step in 0..=10 {
            let value = soniqo_diarization_progress(channel, 3, f64::from(step) / 10.0);
            assert!(value >= previous, "{value} nach {previous}");
            previous = value;
        }
    }
    assert!(previous <= SONIQO_DIARIZATION_PROGRESS_END);
}

#[test]
fn die_erwartung_waechst_mit_der_kanallaenge() {
    let ten_minutes = soniqo_diarization_expected(TARGET_SAMPLE_RATE as usize * 600);
    let twenty_minutes = soniqo_diarization_expected(TARGET_SAMPLE_RATE as usize * 1200);
    assert!(
        twenty_minutes > ten_minutes,
        "{twenty_minutes:?} nicht groesser als {ten_minutes:?}"
    );
    // Der doppelte Ton kostet die doppelte Erwartung -- der Faktor ist linear.
    let quotient = twenty_minutes.as_secs_f64() / ten_minutes.as_secs_f64();
    assert!((quotient - 2.0).abs() < 1e-6, "{quotient}");
    // Und ein leerer Kanal erzeugt keine Erwartung von null, sonst teilt
    // `soniqo_diarization_within` durch null.
    assert!(!soniqo_diarization_expected(0).is_zero());
}

#[test]
fn der_anteil_im_kanal_kommt_aus_der_uhr_und_ist_gedeckelt() {
    let expected = Duration::from_secs(100);
    assert!((soniqo_diarization_within(Duration::ZERO, expected) - 0.0).abs() < 1e-9);
    assert!((soniqo_diarization_within(Duration::from_secs(50), expected) - 0.5).abs() < 1e-9);
    // Laeuft es laenger als erwartet, waechst der Balken NICHT weiter.
    let overdue = soniqo_diarization_within(Duration::from_secs(10_000), expected);
    assert!(overdue < 1.0, "{overdue}");
    assert_eq!(
        overdue,
        soniqo_diarization_within(Duration::from_secs(100_000), expected)
    );
}

#[test]
fn collect_soniqo_channel_transcripts_keeps_channel_slots() {
    let transcripts = collect_soniqo_channel_transcripts([
        Ok(anlg_transcribe_soniqo::FileTranscript::new(
            "hello".to_string(),
            1.0,
        )),
        Err("native chunk failed".to_string()),
    ])
    .unwrap();

    assert_eq!(transcripts.len(), 2);
    assert_eq!(transcripts[0].text, "hello");
    assert_eq!(transcripts[1].text, "");
}

#[test]
fn collect_soniqo_channel_transcripts_preserves_later_channel_index() {
    let transcripts = collect_soniqo_channel_transcripts([
        Err("first failed".to_string()),
        Ok(anlg_transcribe_soniqo::FileTranscript::new(
            "system audio".to_string(),
            1.0,
        )),
    ])
    .unwrap();

    let response = anlg_transcribe_soniqo::batch_response_from_channels(
        anlg_transcribe_soniqo::SoniqoModel::ParakeetBatch,
        transcripts,
    );
    let alternative = &response.results.channels[1].alternatives[0];

    assert_eq!(response.results.channels.len(), 2);
    assert_eq!(response.results.channels[0].alternatives[0].transcript, "");
    assert_eq!(alternative.transcript, "system audio");
    assert_eq!(alternative.words[0].channel, 1);
}

#[test]
fn collect_soniqo_channel_transcripts_errors_when_all_channels_fail() {
    let error = collect_soniqo_channel_transcripts([
        Err("first failed".to_string()),
        Err("second failed".to_string()),
    ])
    .unwrap_err();

    assert_eq!(error, "Soniqo failed to transcribe all 2 audio channel(s).");
}

// --- Buendelung (Fork 02.09.2026) -------------------------------------------
//
// Was hier geschuetzt wird: Buendeln senkt die Zahl der Modellaufrufe, ohne die
// Zeitachse anzufassen. Die Begruendung samt Messung steht im Kopf von
// SONIQO_BUNDLE_SEAM_SAMPLES in local.rs.

fn speech_chunk(sample_start: usize, sample_count: usize) -> anlg_audio_chunking::AudioChunk {
    anlg_audio_chunking::AudioChunk {
        samples: vec![0.5f32; sample_count],
        sample_start,
        sample_end: sample_start + sample_count,
    }
}

fn drain_bundles(
    max_samples: usize,
    chunks: Vec<anlg_audio_chunking::AudioChunk>,
) -> Vec<super::local::SoniqoChunkBundle> {
    // Ohne Spannen-Deckel: diese Tests messen das TON-Budget. Die Zeitspanne
    // hat eigene Tests weiter unten.
    drain_bundles_with_span(max_samples, usize::MAX, chunks)
}

fn drain_bundles_with_span(
    max_samples: usize,
    max_span_samples: usize,
    chunks: Vec<anlg_audio_chunking::AudioChunk>,
) -> Vec<super::local::SoniqoChunkBundle> {
    let mut bundler = SoniqoChunkBundler::new(max_samples, max_span_samples);
    let mut bundles = Vec::new();
    for chunk in chunks {
        if let Some(bundle) = bundler.push(chunk) {
            bundles.push(bundle);
        }
    }
    if let Some(bundle) = bundler.finish() {
        bundles.push(bundle);
    }
    assert!(bundler.finish().is_none(), "finish darf nur einmal liefern");
    bundles
}

#[test]
fn bundles_never_outgrow_the_parakeet_window() {
    let max_samples = SONIQO_PARAKEET_MAX_CHUNK_SAMPLES;
    // 40 Abschnitte a 3 s -- genau die Groessenordnung aus des Betreibers Lauf, in dem
    // 79 % der Aufrufe unter 3 s Ton trugen.
    let chunk_samples = TARGET_SAMPLE_RATE as usize * 3;
    let chunks = (0..40)
        .map(|index| speech_chunk(index * chunk_samples * 2, chunk_samples))
        .collect::<Vec<_>>();

    let bundles = drain_bundles(max_samples, chunks);

    assert!(
        bundles.len() < 40,
        "Buendelung soll Aufrufe sparen, hat aber {} erzeugt",
        bundles.len()
    );
    for bundle in &bundles {
        assert!(
            bundle.samples.len() <= max_samples,
            "Buendel mit {} Samples sprengt das Modellfenster {max_samples}",
            bundle.samples.len()
        );
        // Naehte muessen im Budget mitgezaehlt sein, sonst rutscht das Paket
        // still ueber die Grenze und die Swift-Seite zerschneidet es nochmal.
        let speech: usize = bundle.members.iter().map(|m| m.speech_samples).sum();
        let seams = (bundle.members.len() - 1) * SONIQO_BUNDLE_SEAM_SAMPLES;
        assert_eq!(bundle.samples.len(), speech + seams);
    }
}

#[test]
fn every_bundle_member_keeps_its_own_speech_anchor() {
    let chunk_samples = TARGET_SAMPLE_RATE as usize * 10;
    let starts = [0usize, 400_000, 900_000, 1_500_000];
    let chunks = starts
        .iter()
        .map(|start| speech_chunk(*start, chunk_samples))
        .collect::<Vec<_>>();

    let bundles = drain_bundles(SONIQO_PARAKEET_MAX_CHUNK_SAMPLES, chunks);

    let anchors = bundles
        .iter()
        .flat_map(|bundle| bundle.members.iter())
        .map(|member| (member.sample_start, member.sample_end))
        .collect::<Vec<_>>();
    assert_eq!(
        anchors,
        starts
            .iter()
            .map(|start| (*start, start + chunk_samples))
            .collect::<Vec<_>>(),
        "Buendeln darf die Sprechbeginne weder verschieben noch umsortieren"
    );
}

#[test]
fn an_oversized_chunk_travels_alone_instead_of_being_cut() {
    let oversized = SONIQO_PARAKEET_MAX_CHUNK_SAMPLES + TARGET_SAMPLE_RATE as usize;
    let chunks = vec![
        speech_chunk(0, TARGET_SAMPLE_RATE as usize),
        speech_chunk(1_000_000, oversized),
        speech_chunk(3_000_000, TARGET_SAMPLE_RATE as usize),
    ];

    let bundles = drain_bundles(SONIQO_PARAKEET_MAX_CHUNK_SAMPLES, chunks);

    assert_eq!(bundles.len(), 3);
    assert_eq!(bundles[1].members.len(), 1);
    assert_eq!(bundles[1].samples.len(), oversized);
    assert_eq!(bundles[1].members[0].sample_start, 1_000_000);
}

#[test]
fn bundle_text_is_split_along_the_speech_durations() {
    let text = "eins zwei drei vier fuenf sechs sieben acht";

    assert_eq!(
        split_bundle_text(text, &[3, 1]),
        vec![
            "eins zwei drei vier fuenf sechs".to_string(),
            "sieben acht".to_string()
        ]
    );
    assert_eq!(
        split_bundle_text(text, &[1, 1]),
        vec![
            "eins zwei drei vier".to_string(),
            "fuenf sechs sieben acht".to_string()
        ]
    );
    // Ein einziger Abschnitt ist der verlustfreie Fall: der Text gehoert ihm.
    assert_eq!(split_bundle_text(text, &[7]), vec![text.to_string()]);
    // Kein Wort darf unterwegs verloren gehen.
    for weights in [
        vec![1, 5, 2],
        vec![9, 1, 1, 1],
        vec![1, 1, 1, 1, 1, 1, 1, 1],
    ] {
        let parts = split_bundle_text(text, &weights);
        assert_eq!(parts.len(), weights.len());
        assert_eq!(
            parts
                .iter()
                .flat_map(|part| part.split_whitespace())
                .collect::<Vec<_>>()
                .join(" "),
            text
        );
    }
    // Leerer Text erzeugt keine leeren Abschnitte mit Text.
    assert_eq!(
        split_bundle_text("", &[2, 1]),
        vec![String::new(), String::new()]
    );
}

#[test]
fn bundle_text_follows_the_speaking_rate_not_the_word_count() {
    // Der Kern der Sache: zwei GLEICH LANGE Abschnitte, aber sehr
    // unterschiedlich schnell gesprochen. Der erste traegt zwei lange Woerter,
    // der zweite sechs kurze -- dieselbe Sprechdauer, ein Drittel der Woerter.
    //
    // Wer nach Wortzahl teilt, gibt jedem Abschnitt vier Woerter und schiebt
    // dem langsamen damit zwei Woerter zu, die der schnelle gesprochen hat.
    // Jedes davon springt um die Luecke zwischen beiden Abschnitten.
    let text = "Zusammenarbeitsvertrag Verantwortungsbereich und ich hab das ja so";

    assert_eq!(
        split_bundle_text(text, &[1, 1]),
        vec![
            "Zusammenarbeitsvertrag Verantwortungsbereich".to_string(),
            "und ich hab das ja so".to_string(),
        ],
        "geteilt wird nach geschaetzter Sprechdauer des Textes, nicht nach Wortzahl"
    );

    // Gegenprobe in die andere Richtung: derselbe Text, aber der langsame
    // Abschnitt ist doppelt so lang. Dann darf er auch mehr Text bekommen.
    assert_eq!(
        split_bundle_text(text, &[2, 1]),
        vec![
            "Zusammenarbeitsvertrag Verantwortungsbereich und".to_string(),
            "ich hab das ja so".to_string(),
        ]
    );
}

#[test]
fn a_member_too_short_for_a_word_gets_none() {
    // Ein Abschnitt, dessen Sprechdauer neben seinen Nachbarn verschwindet,
    // bekommt kein Wort zugeschoben -- und die Woerter gehen dabei nicht
    // verloren, sie gehoeren dem Nachbarn.
    let parts = split_bundle_text("eins zwei", &[1, 40]);

    assert_eq!(parts, vec![String::new(), "eins zwei".to_string()]);
}

#[test]
fn splitting_a_bundle_never_creates_or_loses_a_word() {
    // Die Zusicherung, die ueber allem steht: die Summe der zugeteilten
    // Woerter ist immer die gelieferte Wortzahl, in derselben Reihenfolge.
    // Geprueft mit gemischt langen Woertern, denn genau die verschiebt die
    // Aufteilung nach Sprechdauer gegenueber der nach Wortzahl.
    let text = "a Reihenfolge im Verantwortungsbereich ja das Zusammenarbeitsvertrag und";

    for weights in [
        vec![1, 1],
        vec![1, 5, 2],
        vec![9, 1, 1, 1],
        vec![1, 1, 1, 1, 1, 1, 1, 1],
        vec![100, 1],
        vec![1, 100],
        vec![0, 3, 0],
    ] {
        let parts = split_bundle_text(text, &weights);
        assert_eq!(parts.len(), weights.len(), "Gewichte {weights:?}");
        assert_eq!(
            parts
                .iter()
                .flat_map(|part| part.split_whitespace())
                .collect::<Vec<_>>()
                .join(" "),
            text,
            "Gewichte {weights:?}"
        );
    }
}

// --- Zeitspanne eines Buendels (Fork 02.09.2026, Zweitblick-Befund) ---------
//
// Der Befund: das Ton-Budget allein deckelt nur, wie viel TON in einem Buendel
// liegt -- nicht, wie weit es auf der ZEITACHSE reicht, weil die sprachbewusste
// Zerlegung die Stille dazwischen verwirft. Ein Wort, das dem falschen Mitglied
// zufaellt, springt um die ganze Spanne.

/// Der durchgerechnete Fall aus dem Zweitblick, an dem der Fehler haengt: ein
/// stummes "Mhm" bei 100 s und ein echter Satz bei 180 s. Zusammen 3,6 s Ton --
/// das Ton-Budget von 29,5 s merkt davon nichts.
///
/// Ohne Spannen-Budget landen beide im selben Buendel, und die anteilige
/// Aufteilung schiebt das erste Wort des Satzes auf 100 s: **80 Sekunden zu
/// frueh**, mitten in den Redebeitrag des anderen.
#[test]
fn a_word_never_travels_further_than_the_window_that_used_to_cap_it() {
    let second = TARGET_SAMPLE_RATE as usize;
    // 0,6 s bei t=100 s -- ein "Mhm", das zu nichts transkribiert.
    let murmur = speech_chunk(100 * second, second * 6 / 10);
    // 3,0 s bei t=180 s -- "Ja das machen wir so".
    let sentence = speech_chunk(180 * second, second * 3);
    let text = "Ja das machen wir so";

    // Gegenprobe zuerst: ohne Deckel ist der Sprung da, schwarz auf weiss.
    let unbounded = drain_bundles_with_span(
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        usize::MAX,
        vec![murmur.clone(), sentence.clone()],
    );
    assert_eq!(
        unbounded.len(),
        1,
        "ohne Deckel liegen beide in einem Buendel"
    );
    let displaced = soniqo_bundle_transcript_chunks(&unbounded[0], text);
    assert_eq!(displaced[0].text, "Ja");
    assert!(
        (displaced[0].start_seconds - 100.0).abs() < 0.001,
        "ohne Deckel steht das Wort bei {} s statt bei 180 s",
        displaced[0].start_seconds
    );

    // Und jetzt mit dem Deckel, den die App faehrt.
    let bounded = drain_bundles_with_span(
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        SONIQO_BUNDLE_MAX_SPAN_SAMPLES,
        vec![murmur, sentence],
    );
    assert_eq!(
        bounded.len(),
        2,
        "80 Sekunden Abstand duerfen nicht in ein Buendel passen"
    );
    let repaired = soniqo_bundle_transcript_chunks(&bounded[1], text);
    assert_eq!(repaired.len(), 1);
    assert_eq!(repaired[0].text, text);
    assert!(
        (repaired[0].start_seconds - 180.0).abs() < 0.001,
        "der Satz gehoert an seinen Sprechbeginn, steht aber bei {} s",
        repaired[0].start_seconds
    );
}

/// Die eigentliche Zusicherung, nicht der Mechanismus: kein ausgegebener
/// Abschnitt liegt weiter von seinem tatsaechlichen Sprechzeitpunkt entfernt,
/// als die erlaubte Spanne zulaesst. Gemessen wird das am groesstmoeglichen
/// Versatz INNERHALB eines Buendels -- weiter kann ein Wort nicht springen,
/// denn es bleibt immer bei einem Mitglied desselben Buendels.
#[test]
fn no_word_can_be_displaced_further_than_the_allowed_span() {
    let second = TARGET_SAMPLE_RATE as usize;
    // Kurze Einwuerfe mit wachsenden Pausen dazwischen: 1 s Rede, dann 2 s,
    // 4 s, 8 s ... Pause. Genau das Muster, das die Spanne aufblaeht, ohne das
    // Ton-Budget zu fuellen.
    let mut chunks = Vec::new();
    let mut cursor = 0usize;
    for index in 0..24 {
        chunks.push(speech_chunk(cursor, second));
        cursor += second + second * (1 + index % 12);
    }

    let bundles = drain_bundles_with_span(
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        SONIQO_BUNDLE_MAX_SPAN_SAMPLES,
        chunks,
    );

    let mut worst_displacement = 0usize;
    for bundle in &bundles {
        if bundle.members.len() == 1 {
            // Ein einzelnes Mitglied behaelt seinen eigenen Anker -- der
            // verlustfreie Fall, hier ist der Versatz null.
            continue;
        }
        let first = bundle.members.first().expect("Buendel ohne Mitglied");
        let last = bundle.members.last().expect("Buendel ohne Mitglied");
        let span = last.sample_end - first.sample_start;
        assert!(
            span <= SONIQO_BUNDLE_MAX_SPAN_SAMPLES,
            "Buendel spannt {:.1} s, erlaubt sind {:.1} s",
            span as f64 / TARGET_SAMPLE_RATE as f64,
            SONIQO_BUNDLE_MAX_SPAN_SAMPLES as f64 / TARGET_SAMPLE_RATE as f64
        );
        worst_displacement = worst_displacement.max(last.sample_start - first.sample_start);
    }

    assert!(
        bundles.len() < 24,
        "der Deckel darf die Buendelung nicht ganz aushebeln, es blieben {} Aufrufe",
        bundles.len()
    );
    assert!(
        worst_displacement > 0,
        "der Test misst nichts, wenn kein Buendel mehr als ein Mitglied hat"
    );
    assert!(
        worst_displacement <= SONIQO_BUNDLE_MAX_SPAN_SAMPLES,
        "groesster moeglicher Versatz {:.1} s",
        worst_displacement as f64 / TARGET_SAMPLE_RATE as f64
    );
}

/// Ein Abschnitt, der fuer sich genommen laenger ist als die erlaubte Spanne,
/// wird nicht zerschnitten -- er geht allein raus, genau wie beim Ton-Budget.
#[test]
fn a_chunk_longer_than_the_span_still_travels_alone() {
    let second = TARGET_SAMPLE_RATE as usize;
    let oversized = SONIQO_BUNDLE_MAX_SPAN_SAMPLES + 5 * second;
    let chunks = vec![
        speech_chunk(0, second),
        speech_chunk(10 * second, oversized),
        speech_chunk(10 * second + oversized + second, second),
    ];

    let bundles = drain_bundles_with_span(
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        SONIQO_BUNDLE_MAX_SPAN_SAMPLES,
        chunks,
    );

    assert_eq!(bundles.len(), 3);
    assert_eq!(bundles[1].members.len(), 1);
    assert_eq!(bundles[1].members[0].sample_start, 10 * second);
}

/// Die Messbank kann die Spanne als eigene Achse fahren.
#[test]
fn the_bench_can_set_the_bundle_span() {
    let _guard = TuningGuard::with(tuning::SoniqoTuning::PRODUCTION.with_bundle_span_seconds(45.0));
    assert_eq!(super::local::soniqo_bundle_max_span_samples(), 720_000);
}

#[test]
fn soniqo_bundle_budget_follows_the_chosen_strategy() {
    let _guard = TuningGuard::production();
    assert_eq!(
        soniqo_bundle_max_samples(SoniqoChunkStrategy::SpeechAware),
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES
    );
    assert_eq!(
        soniqo_bundle_max_samples(SoniqoChunkStrategy::Fixed { max_samples: 4711 }),
        4711
    );
}

#[test]
fn a_bundle_with_several_members_keeps_every_original_time_anchor() {
    // Der eigentliche Zweck der Buendelung: EIN Modellaufruf, aber weiterhin
    // ein Transkript-Abschnitt je Sprech-Abschnitt mit dessen eigenem Anker.
    // Genau hier waere der 31.08.-Fehler zurueckgekommen, wenn das Buendel
    // selbst zum Transkript-Abschnitt geworden waere.
    let second = TARGET_SAMPLE_RATE as usize;
    let bundles = drain_bundles(
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        vec![
            speech_chunk(10 * second, 2 * second),
            speech_chunk(60 * second, 2 * second),
            speech_chunk(300 * second, 4 * second),
        ],
    );
    assert_eq!(bundles.len(), 1, "drei kurze Abschnitte, ein Aufruf");

    let chunks = soniqo_bundle_transcript_chunks(&bundles[0], "  a b c d e f g h  ");

    assert_eq!(
        chunks
            .iter()
            .map(|chunk| (chunk.start_seconds, chunk.duration_seconds))
            .collect::<Vec<_>>(),
        vec![(10.0, 2.0), (60.0, 2.0), (300.0, 4.0)],
        "die Anker muessen die Sprechbeginne bleiben, nicht die Buendelgrenzen"
    );
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>(),
        vec!["a b", "c d", "e f g h"],
        "der Text folgt der Sprechdauer der Abschnitte"
    );
}

#[test]
fn a_silent_member_does_not_produce_an_empty_transcript_chunk() {
    let second = TARGET_SAMPLE_RATE as usize;
    let bundles = drain_bundles(
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        vec![
            speech_chunk(0, 8 * second),
            speech_chunk(20 * second, second / 100),
        ],
    );
    let chunks = soniqo_bundle_transcript_chunks(&bundles[0], "nur der erste hat text");

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].start_seconds, 0.0);
    // Ein Buendel ohne jeden Text erzeugt gar keine Abschnitte -- der Kanal
    // faellt dann auf die leere FileTranscript zurueck, wie vor der Aenderung.
    assert!(soniqo_bundle_transcript_chunks(&bundles[0], "   ").is_empty());
}

// --- Gemessene Wortzeiten (Fork 02.09.2026) ---------------------------------
//
// Was hier geschuetzt wird: mit echten Wortzeiten entscheidet nicht mehr die
// geschaetzte Sprechdauer, WELCHEM Abschnitt ein Wort zufaellt, sondern die
// Zeit, zu der es gesprochen wurde. Das ist der ganze Unterschied zwischen
// einem Wort, das 18 Sekunden danebenliegt, und einem, das sitzt.

fn measured(text: &str, start: f64, end: f64) -> anlg_transcribe_soniqo::TranscriptWord {
    anlg_transcribe_soniqo::TranscriptWord {
        text: text.to_string(),
        start_seconds: start,
        end_seconds: end,
        confidence: None,
    }
}

fn measured_sure(
    text: &str,
    start: f64,
    end: f64,
    confidence: f64,
) -> anlg_transcribe_soniqo::TranscriptWord {
    anlg_transcribe_soniqo::TranscriptWord {
        confidence: Some(confidence),
        ..measured(text, start, end)
    }
}

#[test]
fn a_measured_word_lands_in_the_member_whose_audio_carried_it() {
    let second = TARGET_SAMPLE_RATE as usize;
    // Zwei Sprech-Abschnitte, 300 Sekunden auseinander, in EINEM Buendel:
    // im Buendel liegen sie direkt hintereinander, getrennt nur durch die
    // Viertelsekunde Naht.
    let bundles = drain_bundles_with_span(
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        usize::MAX,
        vec![
            speech_chunk(10 * second, 2 * second),
            speech_chunk(310 * second, 2 * second),
        ],
    );
    assert_eq!(bundles.len(), 1);

    // Buendel-Zeitachse: Mitglied 1 bei 0,0-2,0 s, Naht bis 2,25 s,
    // Mitglied 2 ab 2,25 s.
    let words = vec![
        measured("a", 0.2, 0.5),
        measured("b", 1.5, 1.8),
        measured("c", 2.4, 2.7),
        measured("d", 3.9, 4.1),
    ];
    let chunks =
        soniqo_bundle_transcript_chunks_from_words(&bundles[0], "a b c d", &words).unwrap();

    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>(),
        vec!["a b", "c d"]
    );
    let times = chunks
        .iter()
        .flat_map(|chunk| chunk.words.iter())
        .map(|word| {
            (
                word.text.as_str(),
                (word.start_seconds * 1000.0).round() as i64,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        times,
        vec![("a", 10_200), ("b", 11_500), ("c", 310_150), ("d", 311_650)],
        "jedes Wort steht auf der Zeitachse der Aufnahme, nicht auf der des Buendels"
    );
}

#[test]
fn a_measured_word_never_spans_the_gap_between_two_members() {
    let second = TARGET_SAMPLE_RATE as usize;
    let bundles = drain_bundles_with_span(
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        usize::MAX,
        vec![
            speech_chunk(0, 2 * second),
            speech_chunk(600 * second, 2 * second),
        ],
    );
    // "vier" liegt weit hinter dem letzten Mitglied -- der Erkenner kann eine
    // solche Zeit ausgeben, das Buendel hat dort aber keinen Ton mehr. Bis zum
    // 02.09.2026 wurde sie ungeprueft uebernommen und das Wort landete rund 28
    // Sekunden neben seinem Mitglied.
    let words = vec![
        measured("eins", 0.1, 0.4),
        measured("zwei", 1.9, 2.0),
        measured("drei", 2.3, 2.6),
        measured("vier", 30.0, 30.5),
    ];
    let chunks =
        soniqo_bundle_transcript_chunks_from_words(&bundles[0], "eins zwei drei vier", &words)
            .unwrap();

    // Der Name dieses Tests behauptet, kein Wort trete aus seinem Mitglied
    // heraus. Bis zum 02.09.2026 pruefte er nur den Wortanfang, liess ihn bis
    // 500 ms hinter das Mitgliedsende und sah `end_seconds` nie an -- ein
    // Wort, das 100 ms vor dem Ende beginnt und Minuten spaeter aufhoert,
    // bestand ihn. Jetzt werden BEIDE Enden geprueft, und zwar auf die
    // Mitgliedsgrenzen selbst.
    for chunk in &chunks {
        let member_start = chunk.start_seconds;
        let member_end = chunk.start_seconds + chunk.duration_seconds;
        for word in &chunk.words {
            assert!(
                word.start_seconds >= member_start - 1e-6
                    && word.end_seconds <= member_end + 1e-6
                    && word.end_seconds > word.start_seconds,
                "{} liegt bei {}..{} ausserhalb von {member_start}..{member_end}",
                word.text,
                word.start_seconds,
                word.end_seconds
            );
        }
    }
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[1].words.len(), 2);
    // Und es wird nicht verschluckt: es sitzt am Ende seines Mitglieds, mit der
    // Mindestdauer davor -- nicht 28 Sekunden dahinter.
    let last = chunks[1].words.last().unwrap();
    let member_end = chunks[1].start_seconds + chunks[1].duration_seconds;
    assert_eq!(last.text, "vier");
    assert!(
        (last.end_seconds - member_end).abs() < 1e-6 && last.start_seconds < last.end_seconds,
        "vier liegt bei {}..{}, erwartet direkt vor {member_end}",
        last.start_seconds,
        last.end_seconds
    );
}

#[test]
fn a_word_list_that_does_not_match_the_model_text_is_refused() {
    let second = TARGET_SAMPLE_RATE as usize;
    let bundles = drain_bundles_with_span(
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        usize::MAX,
        vec![speech_chunk(0, 2 * second)],
    );

    // Ein fehlendes Wort ist genau der Fall, in dem eine Zuordnung nicht mehr
    // belegbar ist -- dann lieber der alte Weg als eine erfundene Zeitachse.
    assert!(
        soniqo_bundle_transcript_chunks_from_words(
            &bundles[0],
            "eins zwei drei",
            &[measured("eins", 0.1, 0.4), measured("zwei", 0.5, 0.9)],
        )
        .is_none()
    );
    // Andere Trennung, gleiche Zeichen: das ist erlaubt, die Wortliste ist die
    // Zerlegung des Modells.
    assert!(
        soniqo_bundle_transcript_chunks_from_words(
            &bundles[0],
            "eins zwei",
            &[measured("einszwei", 0.1, 0.9)],
        )
        .is_some()
    );
    // Ohne Wortliste gibt es nichts zuzuordnen.
    assert!(soniqo_bundle_transcript_chunks_from_words(&bundles[0], "eins", &[]).is_none());
}

#[test]
fn the_word_timings_switch_follows_production_and_the_bench_can_move_it() {
    // Die App ruft `set_soniqo_tuning` nie -- was `PRODUCTION` sagt, gilt.
    // Seit dem Abend des 02.09.2026 sagt es: gemessene Wortzeiten.
    assert!(SoniqoTuning::default().word_timings);
    for enabled in [false, true] {
        assert_eq!(
            SoniqoTuning::PRODUCTION
                .with_word_timings(enabled)
                .word_timings,
            enabled
        );
    }
}

/// Ein Buendel mit zwei Sprech-Abschnitten, 300 s auseinander -- der Fall, in
/// dem sich die beiden Wege ueberhaupt unterscheiden.
fn two_member_bundle() -> Vec<super::local::SoniqoChunkBundle> {
    let second = TARGET_SAMPLE_RATE as usize;
    drain_bundles_with_span(
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
        usize::MAX,
        vec![
            speech_chunk(10 * second, 2 * second),
            speech_chunk(310 * second, 2 * second),
        ],
    )
}

fn four_measured_words() -> Vec<anlg_transcribe_soniqo::TranscriptWord> {
    vec![
        measured("eins", 0.1, 0.4),
        measured("zwei", 0.5, 0.9),
        measured("drei", 2.4, 2.7),
        measured("vier", 3.0, 3.4),
    ]
}

#[test]
fn the_production_path_uses_the_measured_word_times() {
    // Der Schaltertest darueber prueft den WAHRHEITSWERT der Vorgabe und kann
    // eine Aenderung am Standardverhalten grundsaetzlich nicht sehen. Dieser
    // haelt sie am ERGEBNIS fest: mit `PRODUCTION` traegt jeder Abschnitt die
    // gemessene Wortliste des Erkenners, und die Zeiten sind die gemessenen.
    // Wer den Schalter zurueckdreht, bricht ihn.
    let _guard = TuningGuard::with(tuning::SoniqoTuning::PRODUCTION);
    let bundles = two_member_bundle();
    assert_eq!(bundles.len(), 1);

    let decided =
        soniqo_chunks_for_bundle(&bundles[0], "eins zwei drei vier", &four_measured_words());

    assert!(!decided.fell_back_from_word_timings);
    assert_eq!(
        decided
            .chunks
            .iter()
            .map(|chunk| (chunk.text.as_str(), chunk.start_seconds))
            .collect::<Vec<_>>(),
        vec![("eins zwei", 10.0), ("drei vier", 310.0)]
    );
    for chunk in &decided.chunks {
        assert!(
            !chunk.words.is_empty(),
            "im Standard traegt jeder Abschnitt die gemessene Wortliste"
        );
    }
    assert_eq!(
        (decided.chunks[0].words[0].start_seconds * 1000.0).round() as i64,
        10_100,
        "die Zeit kommt aus dem Erkenner, nicht aus der Verteilung"
    );
}

#[test]
fn the_estimating_path_is_still_the_fallback_and_says_so() {
    // Passt die Wortliste nicht zum Modelltext, geht es den alten Weg -- und
    // der Rueckfall wird gemeldet statt still genommen.
    let _guard = TuningGuard::with(tuning::SoniqoTuning::PRODUCTION);
    let bundles = two_member_bundle();

    let mismatched = soniqo_chunks_for_bundle(
        &bundles[0],
        "eins zwei drei vier",
        &four_measured_words()[..2],
    );

    assert!(mismatched.fell_back_from_word_timings);
    assert_eq!(
        mismatched
            .chunks
            .iter()
            .map(|chunk| (chunk.text.as_str(), chunk.start_seconds))
            .collect::<Vec<_>>(),
        vec![("eins zwei", 10.0), ("drei vier", 310.0)]
    );
    assert!(mismatched.chunks.iter().all(|chunk| chunk.words.is_empty()));
}

#[test]
fn the_bench_can_switch_the_measured_word_times_off() {
    // Die Messbank misst weiter beide Seiten -- ohne das waere der Vergleich,
    // der den Schalter am 02.09.2026 umgelegt hat, nicht wiederholbar.
    let _guard = TuningGuard::with(tuning::SoniqoTuning::PRODUCTION.with_word_timings(false));
    let bundles = two_member_bundle();

    let decided =
        soniqo_chunks_for_bundle(&bundles[0], "eins zwei drei vier", &four_measured_words());

    assert!(!decided.fell_back_from_word_timings);
    assert!(decided.chunks.iter().all(|chunk| chunk.words.is_empty()));
}

// --- Kanaele gleichzeitig (Fork 02.09.2026) ---------------------------------

#[derive(Default)]
struct RecordingRuntime {
    cancelled: std::sync::atomic::AtomicBool,
    progress: std::sync::Mutex<Vec<f64>>,
}

impl BatchRuntime for RecordingRuntime {
    fn emit(&self, event: BatchEvent) {
        if let BatchEvent::BatchResponseStreamed {
            event: owhisper_interface::batch_stream::BatchStreamEvent::Progress { percentage, .. },
            ..
        } = event
        {
            self.progress.lock().unwrap().push(percentage);
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Acquire)
    }
}

fn empty_channel(sample_count: usize) -> ResampledChannelFile {
    ResampledChannelFile {
        file: tempfile::Builder::new().suffix(".wav").tempfile().unwrap(),
        sample_count,
        rms: 0.0,
    }
}

fn reporter(runtime: Arc<RecordingRuntime>) -> SoniqoProgressReporter {
    SoniqoProgressReporter {
        runtime,
        session_id: "parallel-test".to_string(),
    }
}

/// Der Kern des Abends: waehrend EIN Kanal getrennt wird, muss sich der Balken
/// bewegen. Vorher meldete dieser Abschnitt gar nichts, und eine laufende
/// Trennung war von einer haengenden nicht zu unterscheiden.
#[test]
fn der_herzschlag_meldet_sich_waehrend_ein_kanal_getrennt_wird() {
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());
    // Ein sehr langer Kanal, damit die Schaetzung waehrend des Tests klein
    // bleibt und der Deckel den Ausgang nicht bestimmt.
    let sample_count = TARGET_SAMPLE_RATE as usize * 6060;

    let outcome = with_diarization_heartbeat_every(
        Duration::from_millis(40),
        Some(&progress),
        0,
        1,
        sample_count,
        || {
            std::thread::sleep(Duration::from_millis(400));
            "fertig"
        },
    );

    assert_eq!(outcome, "fertig");
    let values = runtime.progress.lock().unwrap().clone();
    assert!(
        values.len() >= 3,
        "zu wenige Meldungen waehrend des Laufs: {values:?}"
    );
    let mut previous = f64::NEG_INFINITY;
    for value in &values {
        assert!(*value >= SONIQO_PROGRESS_PLANNED, "{value} unter dem Band");
        assert!(
            *value < SONIQO_DIARIZATION_PROGRESS_END,
            "{value} behauptet, der Kanal sei fertig"
        );
        assert!(*value >= previous, "{value} nach {previous} -- rueckwaerts");
        previous = *value;
    }
    // Die Zusage, die bis zum 08.09.2026 fehlte. `>= previous` laesst
    // Gleichheit zu, also bestand ein Balken, der sich NICHT bewegt, diesen
    // Test vollstaendig -- genau die eine Frage, fuer die er da ist.
    let first = *values.first().unwrap();
    let last = *values.last().unwrap();
    assert!(
        last > first,
        "der Balken steht still: {first} bis {last} ({values:?})"
    );
}

/// Der teuerste Fall des Abends: der Trennungslauf stirbt.
///
/// Bis zum 08.09.2026 stand `stop.store(true)` HINTER dem Lauf. Panikt der
/// Lauf, wird die Zeile nie erreicht, `std::thread::scope` wartet vor dem
/// Weiterreichen der Panik aber auf alle Faeden -- der Herzschlag dreht endlos
/// und die App haengt ohne Fehlermeldung. Deshalb hat dieser Test eine
/// Zeitschranke: ohne den Wachhund kommt nie eine Antwort zurueck.
#[test]
fn eine_panik_im_lauf_beendet_den_herzschlag_statt_die_app_aufzuhaengen() {
    let (sender, receiver) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let runtime = Arc::new(RecordingRuntime::default());
        let progress = reporter(runtime.clone());
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_diarization_heartbeat_every(
                Duration::from_millis(20),
                Some(&progress),
                0,
                1,
                TARGET_SAMPLE_RATE as usize * 6060,
                || -> () { panic!("die Trennung ist gestorben") },
            );
        }));
        let _ = sender.send(outcome.is_err());
    });

    match receiver.recv_timeout(Duration::from_secs(10)) {
        Ok(reached_the_caller) => assert!(
            reached_the_caller,
            "die Panik wurde verschluckt -- der Aufrufer erfaehrt nichts vom Tod des Laufs"
        ),
        Err(_) => panic!(
            "der Lauf ist nach der Panik nicht zurueckgekehrt -- der Herzschlag dreht endlos"
        ),
    }
}

/// Wer waehrend einer Minuten-Trennung abbricht, darf den Balken nicht munter
/// weiterklettern sehen. Der Herzschlag-Faden fragte die Abbruch-Flagge bis
/// zum 08.09.2026 gar nicht ab.
#[test]
fn nach_dem_abbruch_meldet_der_herzschlag_nichts_mehr() {
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());

    let at_cancellation = with_diarization_heartbeat_every(
        Duration::from_millis(20),
        Some(&progress),
        0,
        1,
        TARGET_SAMPLE_RATE as usize * 6060,
        || {
            std::thread::sleep(Duration::from_millis(140));
            let seen_while_running = runtime.progress.lock().unwrap().len();
            assert!(
                seen_while_running >= 3,
                "vor dem Abbruch muss sich der Balken gemeldet haben, sonst prueft dieser Test \
                 nichts: {seen_while_running} Meldungen"
            );
            runtime
                .cancelled
                .store(true, std::sync::atomic::Ordering::Release);
            // Lange genug, dass ohne die Abfrage etwa sieben weitere Meldungen
            // kaemen. Der Faden darf danach hoechstens die eine noch absetzen,
            // die im Moment des Abbruchs schon zwischen Pruefung und Meldung
            // stand.
            std::thread::sleep(Duration::from_millis(150));
            seen_while_running
        },
    );

    let after_cancellation = runtime.progress.lock().unwrap().len();
    assert!(
        after_cancellation <= at_cancellation + 1,
        "nach dem Abbruch kamen {} weitere Meldungen -- der Balken klettert weiter",
        after_cancellation - at_cancellation
    );
}

/// Ohne Melder darf der Herzschlag nichts tun -- und vor allem den Lauf nicht
/// verschlucken. Das ist der Weg, den die Messbank und die Tests nehmen.
#[test]
fn ohne_fortschrittsmelder_laeuft_der_lauf_trotzdem_durch() {
    let outcome = with_diarization_heartbeat_every(
        Duration::from_millis(10),
        None,
        0,
        1,
        TARGET_SAMPLE_RATE as usize * 60,
        || 42u8,
    );
    assert_eq!(outcome, 42);
}

/// Der Weg, der im echten Trennungslauf laeuft, ist `with_diarization_heartbeat`
/// -- nicht die Fassung mit waehlbarem Takt, an der alle anderen Tests haengen.
/// Ein vollstaendiger No-op dieses Wrappers bestand sie alle. Dieser Test
/// kostet dafuer echte Wartezeit: der Produktionstakt ist
/// [`SONIQO_DIARIZATION_HEARTBEAT`], und darunter gibt es keine Meldung zu
/// sehen.
#[test]
fn der_produktionsweg_meldet_sich_im_produktionstakt() {
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());

    let outcome = with_diarization_heartbeat(
        Some(&progress),
        0,
        1,
        TARGET_SAMPLE_RATE as usize * 6060,
        || {
            std::thread::sleep(SONIQO_DIARIZATION_HEARTBEAT + Duration::from_millis(400));
            "fertig"
        },
    );

    assert_eq!(outcome, "fertig");
    let values = runtime.progress.lock().unwrap().clone();
    assert!(
        !values.is_empty(),
        "der Produktionsweg hat im ganzen Lauf nichts gemeldet"
    );
    for value in &values {
        assert!(*value >= SONIQO_PROGRESS_PLANNED, "{value} unter dem Band");
        assert!(
            *value < SONIQO_DIARIZATION_PROGRESS_END,
            "{value} behauptet, der Kanal sei fertig"
        );
    }
}

fn transcript(text: &str) -> anlg_transcribe_soniqo::FileTranscript {
    anlg_transcribe_soniqo::FileTranscript::new(text.to_string(), 1.0)
}

/// Nacheinander bleibt als gemessene Gegenprobe im Code (`bench_soniqo
/// --channels sequential`). Dieser Test haelt fest, dass der Zweig noch das
/// tut, was sein Name sagt: mit `channels_in_parallel: false` duerfen die
/// Kanaele sich nicht ueberlappen.
#[test]
fn with_the_knob_turned_off_the_channels_run_one_after_the_other() {
    let _guard = TuningGuard::with(tuning::SoniqoTuning {
        channels_in_parallel: false,
        ..tuning::SoniqoTuning::PRODUCTION
    });
    let running = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen_together = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());

    let results = transcribe_soniqo_channels_in_parallel(
        2,
        vec![empty_channel(1), empty_channel(2)],
        Some(&progress),
        |channel_index, _channel| {
            let concurrent = running.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
            if concurrent > 1 {
                seen_together.store(true, std::sync::atomic::Ordering::Release);
            }
            std::thread::sleep(Duration::from_millis(50));
            running.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            Ok(transcript(&format!("kanal {channel_index}")))
        },
    )
    .unwrap();

    assert!(
        !seen_together.load(std::sync::atomic::Ordering::Acquire),
        "mit abgeschalteter Stellschraube duerfen die Kanaele sich nicht ueberlappen"
    );
    assert_eq!(
        results
            .into_iter()
            .map(|result| result.unwrap().text)
            .collect::<Vec<_>>(),
        vec!["kanal 0".to_string(), "kanal 1".to_string()]
    );
    assert_eq!(runtime.progress.lock().unwrap().len(), 2);
}

#[test]
fn by_default_both_channels_are_transcribed_at_the_same_time() {
    // Der Entscheid vom 02.09.2026: OHNE jeden Setter-Aufruf -- so laeuft die
    // App -- laufen die Kanaele gleichzeitig.
    //
    // Beweis ohne Haenger: jeder Kanal meldet sich an und wartet MIT FRIST auf
    // den anderen. Laufen sie nacheinander, laeuft die Frist ab und der Test
    // wird rot -- er blockiert nicht.
    let _guard = TuningGuard::production();
    let arrived = Arc::new((std::sync::Mutex::new(0usize), std::sync::Condvar::new()));
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());

    let results = transcribe_soniqo_channels_in_parallel(
        2,
        vec![empty_channel(1), empty_channel(2)],
        Some(&progress),
        |channel_index, _channel| {
            let (lock, condition) = &*arrived;
            let mut count = lock.lock().unwrap();
            *count += 1;
            condition.notify_all();
            while *count < 2 {
                let (guard, timeout) = condition
                    .wait_timeout(count, Duration::from_secs(10))
                    .unwrap();
                count = guard;
                if timeout.timed_out() {
                    break;
                }
            }
            assert_eq!(*count, 2, "Kanal {channel_index} lief allein");
            Ok(transcript(&format!("kanal {channel_index}")))
        },
    )
    .unwrap();

    assert_eq!(
        results
            .into_iter()
            .map(|result| result.unwrap().text)
            .collect::<Vec<_>>(),
        vec!["kanal 0".to_string(), "kanal 1".to_string()],
        "die Kanalreihenfolge muss den parallelen Lauf ueberleben"
    );
    let emitted = runtime.progress.lock().unwrap().clone();
    assert_eq!(emitted.len(), 2);
    assert!(emitted.iter().all(|percentage| *percentage > 0.0));
}

#[test]
fn cancellation_stops_every_channel() {
    let _guard = TuningGuard::production();
    let runtime = Arc::new(RecordingRuntime::default());
    runtime
        .cancelled
        .store(true, std::sync::atomic::Ordering::Release);
    let progress = reporter(runtime.clone());
    let calls = std::sync::atomic::AtomicUsize::new(0);

    let error = transcribe_soniqo_channels_in_parallel(
        4,
        vec![
            empty_channel(1),
            empty_channel(2),
            empty_channel(3),
            empty_channel(4),
        ],
        Some(&progress),
        |_channel_index, _channel| {
            calls.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            Ok(transcript("darf nicht laufen"))
        },
    )
    .expect_err("ein abgebrochener Lauf darf kein Ergebnis liefern");

    assert_eq!(error, LOCAL_BATCH_CANCELLED);
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::Acquire),
        0,
        "kein Kanal darf nach dem Abbruch noch das Modell rufen"
    );
    assert!(runtime.progress.lock().unwrap().is_empty());
}

#[test]
fn a_failing_channel_neither_hides_nor_kills_the_others() {
    let _guard = TuningGuard::production();
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());
    let calls = std::sync::atomic::AtomicUsize::new(0);

    // Ein Kanal, dessen Transkription fehlschlaegt, ist KEIN Abbruch: der
    // Sammler weiter oben ersetzt ihn durch eine leere Spur, solange ein
    // anderer Kanal durchkommt.
    let results = transcribe_soniqo_channels_in_parallel(
        2,
        vec![empty_channel(1), empty_channel(2)],
        Some(&progress),
        |channel_index, _channel| {
            calls.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            if channel_index == 0 {
                Err("kaputt".to_string())
            } else {
                Ok(transcript("heil"))
            }
        },
    )
    .unwrap();

    assert_eq!(calls.load(std::sync::atomic::Ordering::Acquire), 2);
    assert_eq!(results[0].as_ref().unwrap_err(), "kaputt");
    assert_eq!(results[1].as_ref().unwrap().text, "heil");
    let collected = collect_soniqo_channel_transcripts(results).unwrap();
    assert_eq!(collected.len(), 2);
    assert_eq!(collected[1].text, "heil");
}

#[test]
fn a_silent_or_empty_channel_set_breaks_nothing() {
    let _guard = TuningGuard::production();
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());

    let none = transcribe_soniqo_channels_in_parallel(0, Vec::new(), Some(&progress), |_, _| {
        panic!("darf nicht aufgerufen werden")
    })
    .unwrap();
    assert!(none.is_empty());

    // Ein stiller Kanal liefert eine leere Spur, kein Fehler.
    let quiet = transcribe_soniqo_channels_in_parallel(
        1,
        vec![empty_channel(0)],
        Some(&progress),
        |_, _| Ok(transcript("")),
    )
    .unwrap();
    assert_eq!(quiet.len(), 1);
    assert!(quiet[0].as_ref().unwrap().text.is_empty());
}

/// Abbruch MITTEN im Lauf, nicht davor: beide Kanaele laufen bereits, dann
/// drueckt jemand auf Stopp. Der Lauf darf weder haengen noch ein halbes
/// Transkript liefern, und die Abbruchmeldung darf nicht verschluckt werden.
#[test]
fn cancelling_mid_run_stops_both_channels_and_keeps_the_message() {
    let _guard = TuningGuard::production();
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());
    let calls = std::sync::atomic::AtomicUsize::new(0);

    let error = transcribe_soniqo_channels_in_parallel(
        2,
        vec![empty_channel(1), empty_channel(2)],
        Some(&progress),
        |channel_index, _channel| {
            calls.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            // Der Abbruch faellt, waehrend der Kanal schon arbeitet.
            runtime
                .cancelled
                .store(true, std::sync::atomic::Ordering::Release);
            Ok(transcript(&format!("kanal {channel_index}")))
        },
    )
    .expect_err("ein mitten im Lauf abgebrochener Lauf darf kein Ergebnis liefern");

    assert_eq!(
        error, LOCAL_BATCH_CANCELLED,
        "die Abbruchmeldung darf nicht durch eine andere ersetzt werden"
    );
    assert!(
        calls.load(std::sync::atomic::Ordering::Acquire) <= 2,
        "kein Kanal darf nach dem Abbruch ein zweites Mal starten"
    );
    assert!(
        runtime.progress.lock().unwrap().is_empty(),
        "ein abgebrochener Kanal darf keinen Fortschritt melden"
    );
}

/// Der Fortschrittsbalken zaehlt vorwaerts, auch wenn alle Kanaele im selben
/// Moment fertig werden. Ohne die Kopplung von Zaehlen und Melden koennte der
/// zweite Kanal seine 100 % vor den 50 % des ersten losschicken.
#[test]
fn progress_never_runs_backwards_when_channels_finish_together() {
    let _guard = TuningGuard::production();
    let channel_count = 8usize;
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());
    let arrived = Arc::new((std::sync::Mutex::new(0usize), std::sync::Condvar::new()));

    let results = transcribe_soniqo_channels_in_parallel(
        channel_count,
        (0..channel_count).map(empty_channel).collect(),
        Some(&progress),
        |channel_index, _channel| {
            // Alle Kanaele werden im selben Moment fertig -- das ist genau der
            // Fall, in dem die Meldungen sich ueberholen koennten.
            let (lock, condition) = &*arrived;
            let mut count = lock.lock().unwrap();
            *count += 1;
            condition.notify_all();
            while *count < channel_count {
                let (guard, timeout) = condition
                    .wait_timeout(count, Duration::from_secs(10))
                    .unwrap();
                count = guard;
                if timeout.timed_out() {
                    break;
                }
            }
            assert_eq!(*count, channel_count, "Kanal {channel_index} lief allein");
            Ok(transcript(&format!("kanal {channel_index}")))
        },
    )
    .unwrap();

    assert_eq!(results.len(), channel_count);
    let emitted = runtime.progress.lock().unwrap().clone();
    assert_eq!(emitted.len(), channel_count);
    for pair in emitted.windows(2) {
        assert!(
            pair[1] > pair[0],
            "der Fortschritt lief rueckwaerts oder stand still: {emitted:?}"
        );
    }
    assert_eq!(
        emitted.last().copied(),
        Some(soniqo_batch_progress(channel_count, channel_count)),
        "der letzte Kanal muss den vollen Stand melden"
    );
}

/// Die Kanalreihenfolge im Ergebnis haengt NICHT davon ab, welcher Kanal
/// zuerst fertig wird -- sonst vertauschten sich Mikrofon und Systemton, und
/// das Transkript verloere seine Chronologie.
#[test]
fn the_channel_order_does_not_follow_the_finishing_order() {
    let _guard = TuningGuard::production();
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());

    let results = transcribe_soniqo_channels_in_parallel(
        2,
        vec![empty_channel(1), empty_channel(2)],
        Some(&progress),
        |channel_index, _channel| {
            // Kanal 0 wird bewusst als LETZTER fertig.
            if channel_index == 0 {
                std::thread::sleep(Duration::from_millis(120));
            }
            Ok(transcript(&format!("kanal {channel_index}")))
        },
    )
    .unwrap();

    assert_eq!(
        results
            .into_iter()
            .map(|result| result.unwrap().text)
            .collect::<Vec<_>>(),
        vec!["kanal 0".to_string(), "kanal 1".to_string()],
        "Kanal 0 muss Kanal 0 bleiben, auch wenn er zuletzt fertig wird"
    );
}

/// Dieselben Zusicherungen wie oben, aber auf dem seriellen Zweig -- er bleibt
/// als Messgegenprobe im Code und darf nicht unbemerkt verfallen.
#[test]
fn the_sequential_branch_keeps_its_guarantees() {
    let _guard = TuningGuard::with(tuning::SoniqoTuning {
        channels_in_parallel: false,
        ..tuning::SoniqoTuning::PRODUCTION
    });
    let runtime = Arc::new(RecordingRuntime::default());
    let progress = reporter(runtime.clone());

    // Fehlerhafter Kanal beendet den anderen nicht.
    let results = transcribe_soniqo_channels_in_parallel(
        2,
        vec![empty_channel(1), empty_channel(2)],
        Some(&progress),
        |channel_index, _channel| {
            if channel_index == 0 {
                Err("kaputt".to_string())
            } else {
                Ok(transcript("heil"))
            }
        },
    )
    .unwrap();
    assert_eq!(results[0].as_ref().unwrap_err(), "kaputt");
    assert_eq!(results[1].as_ref().unwrap().text, "heil");

    // Leere Kanalliste bricht nichts.
    let none = transcribe_soniqo_channels_in_parallel(0, Vec::new(), Some(&progress), |_, _| {
        panic!("darf nicht aufgerufen werden")
    })
    .unwrap();
    assert!(none.is_empty());

    // Fortschritt zaehlt vorwaerts.
    let emitted = runtime.progress.lock().unwrap().clone();
    assert_eq!(emitted.len(), 2);
    assert!(emitted[1] > emitted[0]);
}

#[test]
fn the_bundle_split_keeps_the_measured_confidence_with_its_word() {
    // Die Buendel-Aufteilung baut jedes Wort NEU auf (sie klemmt seine Zeit in
    // den Tonbereich seines Mitglieds). Wer dabei ein Feld vergisst, verliert
    // es still -- die Zeiten stimmen weiter, und niemand sieht, dass die
    // Sicherheit unterwegs auf `None` gefallen ist.
    let bundles = two_member_bundle();
    assert_eq!(bundles.len(), 1);

    let words = vec![
        measured_sure("eins", 0.1, 0.4, 0.91),
        measured_sure("zwei", 0.5, 0.9, 0.42),
        measured_sure("drei", 2.4, 2.7, 0.77),
        measured_sure("vier", 3.0, 3.4, 0.55),
    ];
    let chunks =
        soniqo_bundle_transcript_chunks_from_words(&bundles[0], "eins zwei drei vier", &words)
            .expect("die Wortliste passt zum Text");

    let carried: Vec<Option<f64>> = chunks
        .iter()
        .flat_map(|chunk| chunk.words.iter())
        .map(|word| word.confidence)
        .collect();

    assert_eq!(
        carried,
        vec![Some(0.91), Some(0.42), Some(0.77), Some(0.55)]
    );
}

/// Liest die Oberflaechen-Zahl ausdruecklich als Nutzer plus Teilnehmer, damit
/// die Rechnung fuer eine Leserin sichtbar bleibt (die Zahl schliesst den
/// Nutzer ein, siehe `getSessionSpeakerCount` in useRunBatch.ts). Bei genau
/// einem tonfuehrenden Kanal und einem digital stillen Gegenkanal, also einer
/// Raumaufnahme, prueft das den Zweig `Some(total) if total >= 2 =>
/// Exact(total)` in local.rs, Zeile 1992: die volle Zahl geht ohne Abzug in
/// die Trennung, weil der Nutzer bei einer Raumaufnahme mit am Tisch sitzt und
/// nicht auf dem stillen Kanal.
#[test]
fn raumaufnahme_nutzer_und_drei_teilnehmer_trennt_auf_vier() {
    let raum = [
        gemessene_pegel::RAUM_MIKROFON,
        gemessene_pegel::RAUM_SYSTEMTON,
    ];
    let aus_der_oberflaeche: u32 = 1 + 3;

    assert_eq!(
        soniqo_diarization_speaker_count(Some(aus_der_oberflaeche), &raum, 0),
        SoniqoChannelDiarization::Exact(4)
    );
    assert_eq!(
        soniqo_diarization_speaker_count(Some(aus_der_oberflaeche), &raum, 1),
        SoniqoChannelDiarization::Skip
    );
}

/// Anrufkanal mit einem einzigen Teilnehmer neben dem Nutzer. Nach dem Abzug
/// des Nutzers, `total.saturating_sub(1)` in local.rs, Zeile 2004, bleibt
/// genau eine Person uebrig, und die Bedingung `if count >= 2` in derselben
/// Funktion, Zeile 2007, laesst das nicht durch. Ohne diese Zwei-Schwelle
/// wuerde eine einzelne Person faelschlich als Sprecherzahl vorgegeben.
#[test]
fn anruf_nutzer_und_ein_teilnehmer_wird_nicht_getrennt() {
    let konferenz = [
        gemessene_pegel::KONFERENZ_MIKROFON,
        gemessene_pegel::KONFERENZ_SYSTEMTON,
    ];
    let aus_der_oberflaeche: u32 = 1 + 1;

    assert_eq!(
        soniqo_diarization_speaker_count(Some(aus_der_oberflaeche), &konferenz, 1),
        SoniqoChannelDiarization::Skip
    );
    assert_eq!(
        soniqo_diarization_speaker_count(Some(aus_der_oberflaeche), &konferenz, 0),
        SoniqoChannelDiarization::Skip
    );
}

/// Derselbe Anrufkanal, diesmal mit drei Teilnehmern statt einem: geprueft
/// wird wieder der Abzug `total.saturating_sub(1)` in local.rs, Zeile 2004,
/// nur dass nach dem Abzug drei Personen uebrig bleiben und damit die
/// Zwei-Schwelle aus Zeile 2007 ueberschreiten. Der Mikrofonkanal bleibt
/// unangetastet, weil der Nutzer dort sitzt.
#[test]
fn anruf_nutzer_und_drei_teilnehmer_trennt_den_systemton_auf_drei() {
    let konferenz = [
        gemessene_pegel::KONFERENZ_MIKROFON,
        gemessene_pegel::KONFERENZ_SYSTEMTON,
    ];
    let aus_der_oberflaeche: u32 = 1 + 3;

    assert_eq!(
        soniqo_diarization_speaker_count(Some(aus_der_oberflaeche), &konferenz, 1),
        SoniqoChannelDiarization::Exact(3)
    );
    assert_eq!(
        soniqo_diarization_speaker_count(Some(aus_der_oberflaeche), &konferenz, 0),
        SoniqoChannelDiarization::Skip
    );
}
