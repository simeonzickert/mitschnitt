use std::time::Duration;

use owhisper_client::{
    AdapterKind, AquaVoiceAdapter, AssemblyAIAdapter, AwsTranscribeAdapter, AzureSpeechAdapter,
    BatchSttAdapter, BatchUploadLimit, CartesiaAdapter, CohereAdapter, DeepgramAdapter,
    ElevenLabsAdapter, FireworksAdapter, GladiaAdapter, GoogleCloudAdapter,
    GoogleGenerativeAiAdapter, GroqAdapter, LocalServerAdapter, MistralAdapter, OpenAIAdapter,
    OpenRouterAdapter, PyannoteAdapter, RevAiAdapter, SiliconFlowAdapter, SonioxAdapter,
    SpeechmaticsAdapter, TogetherAdapter, XaiAdapter, ZaiAdapter,
};
use owhisper_interface::batch::{Alternatives, Channel, Response, Results};
use tracing::Instrument;

use super::super::upload::{audio_duration, segment_plan, split_batch_upload};
use super::super::{
    BatchParams, BatchRunMode, BatchRunOutput, format_user_friendly_error, session_span,
};
use super::channel_split::run_channel_split_batch;

pub(super) const DIRECT_BATCH_TIMEOUT_FLOOR: Duration = Duration::from_secs(15 * 60);
pub(super) const DIRECT_BATCH_TIMEOUT_CEILING: Duration = Duration::from_secs(6 * 60 * 60);
const DIRECT_BATCH_TIMEOUT_BUFFER: Duration = Duration::from_secs(5 * 60);
const DIRECT_BATCH_AUDIO_DURATION_MULTIPLIER: u32 = 2;

// `removed:` -- Kennungen, fuer die dieser Fork keinen Netz-Client mehr baut
// (Grok-Review 02.09.2026, F4). Der Match bleibt erschoepfend, also faellt ein
// Anbieter, der weder Adapter noch Eintrag hier hat, beim Bauen auf; und weil
// `AnarlogAdapter` keine `BatchSttAdapter`-Implementierung mehr hat, liesse
// sich `Anarlog` gar nicht in die Adapter-Liste zurueckstellen.
macro_rules! dispatch_batch {
    ($ak:expr, $provider:ident => $run:ident $args:tt,
     { $($var:ident => $adapter:ty),+ $(,)? },
     unsupported: [$($unsup:ident),* $(,)?],
     removed: [$($removed:ident),* $(,)?]
    ) => {
        match $ak {
            $(AdapterKind::$var => {
                let $provider = AdapterKind::$var.to_string();
                $run::<$adapter> $args .await
            })+
            $(AdapterKind::$unsup => {
                Err(crate::BatchFailure::DirectBatchUnsupported {
                    provider: AdapterKind::$unsup.to_string(),
                }.into())
            })*
            $(AdapterKind::$removed => {
                Err(crate::BatchFailure::DirectRequestFailed {
                    provider: AdapterKind::$removed.to_string(),
                    message: AdapterKind::$removed
                        .network_client_unavailable()
                        .unwrap_or("provider removed in this fork")
                        .to_string(),
                }.into())
            })*
        }
    };
}

/// Die eine Liste der Netz-Adapter, fuer beide Einstiege unten.
macro_rules! with_batch_adapters {
    ($ak:expr, $provider:ident => $run:ident $args:tt) => {
        dispatch_batch!($ak, $provider => $run $args, {
            LocalServer => LocalServerAdapter,
            Cartesia => CartesiaAdapter,
            Deepgram => DeepgramAdapter,
            Soniox => SonioxAdapter,
            AssemblyAI => AssemblyAIAdapter,
            Fireworks => FireworksAdapter,
            OpenAI => OpenAIAdapter,
            OpenRouter => OpenRouterAdapter,
            SiliconFlow => SiliconFlowAdapter,
            Zai => ZaiAdapter,
            Gladia => GladiaAdapter,
            ElevenLabs => ElevenLabsAdapter,
            Pyannote => PyannoteAdapter,
            Mistral => MistralAdapter,
            AquaVoice => AquaVoiceAdapter,
            Cohere => CohereAdapter,
            AwsTranscribe => AwsTranscribeAdapter,
            AzureSpeech => AzureSpeechAdapter,
            GoogleCloud => GoogleCloudAdapter,
            GoogleGenerativeAi => GoogleGenerativeAiAdapter,
            Groq => GroqAdapter,
            RevAi => RevAiAdapter,
            Speechmatics => SpeechmaticsAdapter,
            Together => TogetherAdapter,
            Xai => XaiAdapter,
        }, unsupported: [DashScope], removed: [Anarlog])
    };
}

pub(in crate::batch) async fn run_direct_batch_for_adapter_kind(
    adapter_kind: AdapterKind,
    params: BatchParams,
    listen_params: owhisper_interface::ListenParams,
) -> crate::Result<BatchRunOutput> {
    let limit = adapter_kind.batch_upload_limit(listen_params.model.as_deref());

    with_batch_adapters!(
        adapter_kind,
        provider => run_direct_batch(&provider, params, listen_params, limit)
    )
}

/// ZICK-330: laeuft dieser Anbieter ueber den Kanal-Weg (`channel_split.rs`)?
///
/// POSITIVLISTE (Opus-Zweitblick 26.09.2026): nur Anbieter, die reinen Text
/// bzw. einen Kanal OHNE Sprecher liefern. Der Kanal-Weg zerlegt in ~30-s-
/// Pakete und verwirft die Sprechernummern des Anbieters (sie waeren je Paket
/// neu vergeben); ein Anbieter mit eigener Sprechertrennung wuerde dadurch
/// seine Trennung verlieren, und job-basierte Anbieter bekaemen ~240 Jobs je
/// Stunde.
///
/// Geprueft am Adaptercode (26./27.09.2026):
/// - drin: OpenAI und OpenRouter (ausser `*-diarize`-Modellen), Groq,
///   Together, SiliconFlow, Zai (alle `openai_compatible_batch`, Sprecher nur
///   wenn das Modell sie liefert, und die gelisteten Standardmodelle tun das
///   nicht), Fireworks (`speaker: None`), Cohere und AquaVoice (kein Sprecher
///   im Adapter).
/// - AWS (Forge M4): der Adapter ruft ein OpenAI-kompatibles Gateway per
///   Multipart auf (`aws_transcribe/mod.rs`, `openai_compatible_batch`), kein
///   S3-Job, keine Diarisierung angefordert.
/// - xAI (Forge M4): fordert Diarisierung NUR an, wenn eine Sprecherzahl
///   gesetzt ist (`xai/batch.rs:52-57`). Ohne Sprecherzahl liefert es Text
///   und gehoert auf den Kanal-Weg, mit Sprecherzahl nicht.
/// - draussen: Mistral (schickt immer `diarize=true`), GoogleGenerativeAi
///   (`diarization_mode: speaker`), Deepgram (liefert selbst je Kanal eine
///   Spur), alle mit eigener Trennung (Soniox, Pyannote, Cartesia, Gladia,
///   ElevenLabs, AssemblyAI, Speechmatics, Azure, Google Cloud, Rev),
///   LocalServer, DashScope, Anarlog.
///
/// Wer einen Anbieter aufnimmt, prueft vorher, dass er keine Sprecher liefert.
pub(in crate::batch) fn adapter_splits_channels(
    adapter_kind: &AdapterKind,
    listen_params: &owhisper_interface::ListenParams,
) -> bool {
    let diarizing_model = listen_params
        .model
        .as_deref()
        .is_some_and(|model| model.to_lowercase().contains("diarize"));
    let speakers_requested = listen_params.num_speakers.is_some()
        || listen_params.min_speakers.is_some()
        || listen_params.max_speakers.is_some();
    match adapter_kind {
        AdapterKind::OpenAI
        | AdapterKind::OpenRouter
        | AdapterKind::Groq
        | AdapterKind::Together
        | AdapterKind::SiliconFlow
        | AdapterKind::Zai
        | AdapterKind::Fireworks
        | AdapterKind::Cohere
        | AdapterKind::AquaVoice
        | AdapterKind::AwsTranscribe => !diarizing_model,
        AdapterKind::Xai => !diarizing_model && !speakers_requested,
        _ => false,
    }
}

/// Kanal-Weg fuer einen Netz-Anbieter. Nicht `Done`: passt nicht (Mono,
/// hoechstens ein sprechender Kanal), der Aufrufer nimmt den alten Weg.
pub(in crate::batch) async fn run_channel_split_for_adapter_kind(
    adapter_kind: AdapterKind,
    runtime: std::sync::Arc<dyn crate::BatchRuntime>,
    params: &BatchParams,
    listen_params: &owhisper_interface::ListenParams,
) -> crate::Result<super::channel_split::ChannelSplitOutcome> {
    let limit = adapter_kind.batch_upload_limit(listen_params.model.as_deref());

    with_batch_adapters!(
        adapter_kind,
        provider => run_channel_split_batch(&provider, runtime, params, listen_params, limit)
    )
}

pub(super) async fn run_direct_batch<A: BatchSttAdapter>(
    provider: &str,
    params: BatchParams,
    listen_params: owhisper_interface::ListenParams,
    limit: Option<BatchUploadLimit>,
) -> crate::Result<BatchRunOutput> {
    let audio_duration = audio_duration(&params.file_path);
    let timeout = direct_batch_timeout_for_audio(audio_duration);

    match segment_plan(&params.file_path, audio_duration, limit) {
        Some(segment_duration) => {
            run_segmented_batch::<A>(provider, params, listen_params, segment_duration, timeout)
                .await
        }
        None => run_direct_batch_with_timeout::<A>(provider, params, listen_params, timeout).await,
    }
}

async fn run_segmented_batch<A: BatchSttAdapter>(
    provider: &str,
    params: BatchParams,
    mut listen_params: owhisper_interface::ListenParams,
    segment_duration: Duration,
    timeout: Duration,
) -> crate::Result<BatchRunOutput> {
    let segments = split_batch_upload(&params.file_path, segment_duration, provider).await?;
    listen_params.channels = 1;

    let mut responses = Vec::with_capacity(segments.paths().len());
    for path in segments.paths() {
        let mut segment_params = params.clone();
        segment_params.file_path = path.to_string_lossy().into_owned();

        let output = run_direct_batch_with_timeout::<A>(
            provider,
            segment_params,
            listen_params.clone(),
            timeout,
        )
        .await?;
        responses.push(output.response);
    }

    Ok(BatchRunOutput {
        session_id: params.session_id,
        mode: BatchRunMode::Direct,
        response: merge_segment_responses(responses, segment_duration),
    })
}

/// Segments are transcribed independently, so their timestamps restart at zero.
pub(super) fn merge_segment_responses(
    responses: Vec<Response>,
    segment_duration: Duration,
) -> Response {
    let mut metadata = serde_json::Value::Null;
    let mut speaker_labels = Vec::new();
    let mut speaker_segments = Vec::new();
    let mut speaker_offset = 0;
    let mut transcripts: Vec<String> = Vec::new();
    let mut words = Vec::new();

    for (index, response) in responses.into_iter().enumerate() {
        let offset = segment_duration.as_secs_f64() * index as f64;
        let segment_speaker_labels = response
            .metadata
            .get("speaker_labels")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        speaker_labels.extend(segment_speaker_labels.iter().cloned());
        speaker_segments.extend(
            response
                .metadata
                .get("speaker_segments")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .cloned()
                .map(|mut segment| {
                    for field in ["start", "end"] {
                        if let Some(value) = segment.get_mut(field)
                            && let Some(time) = value.as_f64()
                        {
                            *value = serde_json::json!(time + offset);
                        }
                    }
                    segment
                }),
        );
        if metadata.is_null() {
            metadata = response.metadata;
        }

        let Some(alternative) = response
            .results
            .channels
            .into_iter()
            .next()
            .and_then(|channel| channel.alternatives.into_iter().next())
        else {
            continue;
        };

        let transcript = alternative.transcript.trim();
        if !transcript.is_empty() {
            transcripts.push(transcript.to_string());
        }
        let segment_speaker_count = alternative
            .words
            .iter()
            .filter_map(|word| word.speaker)
            .max()
            .map_or(0, |speaker| speaker + 1)
            .max(segment_speaker_labels.len());
        words.extend(alternative.words.into_iter().map(|mut word| {
            word.start += offset;
            word.end += offset;
            word.speaker = word.speaker.map(|speaker| speaker + speaker_offset);
            word
        }));
        speaker_offset += segment_speaker_count;
    }

    if let Some(object) = metadata.as_object_mut() {
        if !speaker_labels.is_empty() {
            object.insert(
                "speaker_labels".to_string(),
                serde_json::Value::Array(speaker_labels),
            );
        }
        if !speaker_segments.is_empty() {
            object.insert(
                "speaker_segments".to_string(),
                serde_json::Value::Array(speaker_segments),
            );
        }
    }

    Response {
        metadata: if metadata.is_null() {
            serde_json::json!({})
        } else {
            metadata
        },
        results: Results {
            channels: vec![Channel {
                alternatives: vec![Alternatives {
                    transcript: transcripts.join(" "),
                    confidence: 1.0,
                    words,
                }],
            }],
        },
    }
}

/// Ein einzelner Aufruf, der Fehler noch in seiner rohen Form.
pub(super) enum DirectAttemptError {
    Provider(owhisper_client::Error),
    TimedOut,
}

impl DirectAttemptError {
    /// HTTP 429? Zuerst strukturiert (Statuscode am Fehler), sonst ueber die
    /// Meldung -- manche Adapter geben den Code nur als Text weiter.
    pub(super) fn is_rate_limited(&self) -> bool {
        let Self::Provider(error) = self else {
            return false;
        };
        if let Some(status) = provider_status(error) {
            return status == 429;
        }
        format_user_friendly_error(&format!("{error:?}")) == super::super::RATE_LIMIT_MESSAGE
    }

    pub(super) fn into_batch_error(self, provider: &str, timeout: Duration) -> crate::Error {
        match self {
            Self::Provider(err) => {
                let raw_error = format!("{err:?}");
                let message = format_user_friendly_error(&raw_error);
                tracing::error!(
                    error = %raw_error,
                    anarlog.error.user_message = %message,
                    "batch transcription failed"
                );
                crate::BatchFailure::DirectRequestFailed {
                    provider: provider.to_string(),
                    message,
                }
                .into()
            }
            Self::TimedOut => {
                tracing::error!(
                    timeout_seconds = timeout.as_secs(),
                    "batch transcription timed out"
                );
                crate::BatchFailure::DirectRequestTimedOut {
                    provider: provider.to_string(),
                    timeout_seconds: timeout.as_secs(),
                }
                .into()
            }
        }
    }
}

fn provider_status(error: &owhisper_client::Error) -> Option<u16> {
    match error {
        owhisper_client::Error::UnexpectedStatus { status, .. } => Some(status.as_u16()),
        owhisper_client::Error::ProviderFailure { status, .. } => status.map(|s| s.as_u16()),
        owhisper_client::Error::Http(error) => error.status().map(|s| s.as_u16()),
        _ => None,
    }
}

pub(super) async fn transcribe_once<A: BatchSttAdapter>(
    params: &BatchParams,
    listen_params: owhisper_interface::ListenParams,
    timeout: Duration,
) -> Result<owhisper_interface::batch::Response, DirectAttemptError> {
    let client = owhisper_client::BatchClient::<A>::builder()
        .api_base(params.base_url.clone())
        .api_key(params.api_key.clone())
        .params(listen_params)
        .build();

    tracing::debug!("transcribing file: {}", params.file_path);
    match tokio::time::timeout(timeout, client.transcribe_file(&params.file_path)).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(err)) => Err(DirectAttemptError::Provider(err)),
        Err(_) => Err(DirectAttemptError::TimedOut),
    }
}

pub(super) async fn run_direct_batch_with_timeout<A: BatchSttAdapter>(
    provider: &str,
    params: BatchParams,
    listen_params: owhisper_interface::ListenParams,
    timeout: Duration,
) -> crate::Result<BatchRunOutput> {
    let span = session_span(&params.session_id);

    async {
        let response = transcribe_once::<A>(&params, listen_params, timeout)
            .await
            .map_err(|error| error.into_batch_error(provider, timeout))?;
        tracing::info!("batch transcription completed");

        Ok(BatchRunOutput {
            session_id: params.session_id,
            mode: BatchRunMode::Direct,
            response,
        })
    }
    .instrument(span)
    .await
}

pub(super) fn direct_batch_timeout_for_audio(audio_duration: Option<Duration>) -> Duration {
    let timeout = audio_duration
        .map(|duration| {
            duration
                .saturating_mul(DIRECT_BATCH_AUDIO_DURATION_MULTIPLIER)
                .saturating_add(DIRECT_BATCH_TIMEOUT_BUFFER)
        })
        .unwrap_or(DIRECT_BATCH_TIMEOUT_FLOOR);

    timeout
        .max(DIRECT_BATCH_TIMEOUT_FLOOR)
        .min(DIRECT_BATCH_TIMEOUT_CEILING)
}
