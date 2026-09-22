mod accumulator;
mod progressive;
mod simple;
mod upload;

use std::sync::Arc;

use owhisper_client::{AdapterKind, OpenAIAdapter};

use crate::{BatchEvent, BatchRuntime};

use progressive::run_progressive_batch_session;
use simple::{run_apple_speech_batch, run_direct_batch_for_adapter_kind, run_soniqo_batch};

pub use simple::tuning::{
    SoniqoChunkingMode, SoniqoTuning, seconds_to_samples, set_soniqo_tuning, soniqo_tuning,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum BatchProvider {
    #[serde(rename = "whispercpp")]
    #[strum(serialize = "whispercpp")]
    WhisperLocal,
    Deepgram,
    Soniox,
    AssemblyAI,
    Fireworks,
    OpenAI,
    OpenRouter,
    SiliconFlow,
    Zai,
    Gladia,
    ElevenLabs,
    Pyannote,
    DashScope,
    Mistral,
    #[serde(alias = "hyprnote")]
    Anarlog,
    Soniqo,
    AppleSpeech,
    AquaVoice,
    Cartesia,
    Cohere,
    #[serde(rename = "aws_transcribe")]
    #[strum(serialize = "aws_transcribe")]
    AwsTranscribe,
    #[serde(rename = "azure_speech")]
    #[strum(serialize = "azure_speech")]
    AzureSpeech,
    #[serde(rename = "google_cloud")]
    #[strum(serialize = "google_cloud")]
    GoogleCloud,
    #[serde(rename = "google_generative_ai")]
    #[strum(serialize = "google_generative_ai")]
    GoogleGenerativeAi,
    Groq,
    RevAi,
    Speechmatics,
    Together,
    Xai,
}

impl BatchProvider {
    pub fn to_adapter_kind(&self) -> Option<AdapterKind> {
        match self {
            Self::Deepgram => Some(AdapterKind::Deepgram),
            Self::Soniox => Some(AdapterKind::Soniox),
            Self::AssemblyAI => Some(AdapterKind::AssemblyAI),
            Self::Fireworks => Some(AdapterKind::Fireworks),
            Self::OpenAI => Some(AdapterKind::OpenAI),
            Self::OpenRouter => Some(AdapterKind::OpenRouter),
            Self::SiliconFlow => Some(AdapterKind::SiliconFlow),
            Self::Zai => Some(AdapterKind::Zai),
            Self::Gladia => Some(AdapterKind::Gladia),
            Self::ElevenLabs => Some(AdapterKind::ElevenLabs),
            Self::Pyannote => Some(AdapterKind::Pyannote),
            Self::Mistral => Some(AdapterKind::Mistral),
            Self::Anarlog => Some(AdapterKind::Anarlog),
            Self::AquaVoice => Some(AdapterKind::AquaVoice),
            Self::Cartesia => Some(AdapterKind::Cartesia),
            Self::Cohere => Some(AdapterKind::Cohere),
            Self::AwsTranscribe => Some(AdapterKind::AwsTranscribe),
            Self::AzureSpeech => Some(AdapterKind::AzureSpeech),
            Self::GoogleCloud => Some(AdapterKind::GoogleCloud),
            Self::GoogleGenerativeAi => Some(AdapterKind::GoogleGenerativeAi),
            Self::Groq => Some(AdapterKind::Groq),
            Self::RevAi => Some(AdapterKind::RevAi),
            Self::Speechmatics => Some(AdapterKind::Speechmatics),
            Self::Together => Some(AdapterKind::Together),
            Self::Xai => Some(AdapterKind::Xai),
            Self::WhisperLocal | Self::Soniqo | Self::AppleSpeech | Self::DashScope => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BatchParams {
    pub session_id: String,
    pub provider: BatchProvider,
    pub file_path: String,
    #[serde(default)]
    pub model: Option<String>,
    pub base_url: String,
    pub api_key: String,
    #[serde(default)]
    pub languages: Vec<anlg_language::Language>,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Die eingetragene Stichwortliste, Zeile fuer Zeile -- getrennt von `keywords`, und
    /// das ist der Punkt.
    ///
    /// `keywords` ist eine gemischte Tuete: Teilnehmernamen, Kalendernamen UND
    /// automatisch aus der Notiz gezogene Schlagwoerter
    /// (`apps/desktop/src/stt/useKeywords.ts:buildKeywords`). Als HINWEIS an
    /// ein Modell ist das richtig -- als Vorlage fuer ein Suchen-und-Ersetzen
    /// waere es gefaehrlich: ein beliebiges Wort aus der Notiz duerfte dann
    /// Woerter im Transkript ueberschreiben. Hier steht nur, was ein Mensch in
    /// die Stichwortliste eingetragen hat.
    ///
    /// Format je Zeile: `Sedacz => Sedatsch; Sedatz` oder nur `Webflow`.
    #[serde(default)]
    pub vocabulary: Vec<String>,
    #[serde(default)]
    pub num_speakers: Option<u32>,
    #[serde(default)]
    pub min_speakers: Option<u32>,
    #[serde(default)]
    pub max_speakers: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum BatchRunMode {
    Direct,
    Streamed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BatchRunOutput {
    pub session_id: String,
    pub mode: BatchRunMode,
    pub response: owhisper_interface::batch::Response,
}

pub async fn run_batch(
    runtime: Arc<dyn BatchRuntime>,
    params: BatchParams,
) -> crate::Result<BatchRunOutput> {
    runtime.emit(BatchEvent::BatchStarted {
        session_id: params.session_id.clone(),
    });

    let session_id = params.session_id.clone();
    let vocabulary = anlg_vocabulary::Vocabulary::parse(&params.vocabulary);
    let mut result = run_batch_inner(runtime.clone(), params).await;

    // Der Nachlauf sitzt hier, weil hier jeder Anbieter vorbeikommt -- und
    // VOR dem Ereignis, damit niemand die unkorrigierte Fassung zu sehen
    // bekommt und sie danach still ausgetauscht wird.
    if let Ok(output) = result.as_mut() {
        crate::vocabulary::apply_to_response(
            &mut output.response,
            &vocabulary,
            crate::vocabulary::vocabulary_options(),
        );
    }

    if let Err(error) = &result {
        let (code, message) = match error {
            crate::Error::BatchFailed(failure) => (failure.code(), failure.to_string()),
            _ => (crate::BatchErrorCode::Unknown, error.to_string()),
        };

        runtime.emit(BatchEvent::BatchFailed {
            session_id,
            code,
            error: message,
        });
    } else {
        let output = result.as_ref().unwrap();

        runtime.emit(BatchEvent::BatchResponse {
            session_id: output.session_id.clone(),
            response: output.response.clone(),
            mode: output.mode,
        });
        runtime.emit(BatchEvent::BatchCompleted {
            session_id: output.session_id.clone(),
        });
    }

    result
}

pub fn expects_progressive_batch(params: &BatchParams) -> bool {
    match params.provider {
        BatchProvider::WhisperLocal => true,
        BatchProvider::OpenAI => {
            OpenAIAdapter::supports_progressive_batch_model(params.model.as_deref())
        }
        _ => false,
    }
}

async fn run_batch_inner(
    runtime: Arc<dyn BatchRuntime>,
    params: BatchParams,
) -> crate::Result<BatchRunOutput> {
    let metadata_joined = tokio::task::spawn_blocking({
        let path = params.file_path.clone();
        move || anlg_audio_utils::audio_file_metadata(path)
    })
    .await;

    let metadata_result = match metadata_joined {
        Ok(result) => result,
        Err(err) => {
            let raw_error = format!("{err:?}");
            tracing::error!(error = %raw_error, "audio_metadata_task_join_failed");
            return Err(crate::BatchFailure::AudioMetadataJoinFailed.into());
        }
    };

    let metadata = match metadata_result {
        Ok(metadata) => metadata,
        Err(err) => {
            let raw_error = err.to_string();
            let message = format_user_friendly_error(&raw_error);
            tracing::error!(
                error = %raw_error,
                anarlog.error.user_message = %message,
                "failed_to_read_audio_metadata"
            );
            return Err(crate::BatchFailure::AudioMetadataReadFailed { message }.into());
        }
    };

    let listen_params = build_listen_params(&params, metadata.channels, metadata.sample_rate);

    match params.provider {
        BatchProvider::WhisperLocal => {
            run_progressive_batch_session(runtime, params, listen_params).await
        }
        BatchProvider::Soniqo => run_soniqo_batch(runtime, params, listen_params).await,
        BatchProvider::AppleSpeech => run_apple_speech_batch(runtime, params, listen_params).await,
        BatchProvider::OpenAI => {
            if OpenAIAdapter::supports_progressive_batch_model(listen_params.model.as_deref()) {
                run_progressive_batch_session(runtime, params, listen_params).await
            } else {
                run_direct_batch_for_adapter_kind(AdapterKind::OpenAI, params, listen_params).await
            }
        }
        BatchProvider::DashScope => Err(crate::BatchFailure::BatchCapabilityUnsupported {
            provider: batch_provider_label(BatchProvider::DashScope),
        }
        .into()),
        ref provider => {
            let adapter_kind = provider
                .to_adapter_kind()
                .expect("all non-special BatchProvider variants have an AdapterKind mapping");
            run_direct_batch_for_adapter_kind(adapter_kind, params, listen_params).await
        }
    }
}

fn build_listen_params(
    params: &BatchParams,
    channels: u8,
    sample_rate: u32,
) -> owhisper_interface::ListenParams {
    owhisper_interface::ListenParams {
        model: params.model.clone(),
        channels,
        sample_rate,
        languages: params.languages.clone(),
        keywords: params.keywords.clone(),
        // NUR die richtigen Schreibweisen verlassen das Geraet.
        //
        // Die rohen Zeilen tragen die Verhoerungen ("Sedacz => Sarec;
        // Seda das"), und aus `ListenParams` wird bei entfernten Anbietern ein
        // Abfrageparameter in der URL. Beim lokalen Dienst waere das harmlos,
        // bei einer eingetragenen fremden Adresse gingen Kundennamen samt
        // ihrer Verhoerungen ueber die Leitung. Fuer eine App, deren Kern
        // Datenhoheit ist, ist das die falsche Fehlerrichtung.
        //
        // Es geht nichts verloren: die Empfaengerseite baut daraus ohnehin nur
        // die Kontext-Vorgabe und ruft dafuer selbst `canonical_terms()`
        // (crates/transcribe-whisper-local/src/service/mod.rs). Die
        // Verhoerungen braucht ausschliesslich der Nachlauf hier im Haus.
        vocabulary: anlg_vocabulary::Vocabulary::parse(&params.vocabulary).canonical_terms(),
        num_speakers: params.num_speakers,
        min_speakers: params.min_speakers,
        max_speakers: params.max_speakers,
        custom_query: None,
    }
}

pub(super) fn batch_provider_label(provider: BatchProvider) -> String {
    provider.to_string()
}

pub(super) fn session_span(session_id: &str) -> tracing::Span {
    tracing::info_span!("session", anarlog.session.id = %session_id)
}

pub(super) fn format_user_friendly_error(error: &str) -> String {
    let error_lower = error.to_lowercase();

    if error_lower.contains("401") || error_lower.contains("unauthorized") {
        return "Authentication failed. Please check your API key in settings.".to_string();
    }
    if error_lower.contains("403") || error_lower.contains("forbidden") {
        return "Access denied. Your API key may not have permission for this operation."
            .to_string();
    }
    if error_lower.contains("429") || error_lower.contains("rate limit") {
        return "Rate limit exceeded. Please wait a moment and try again.".to_string();
    }
    if error_lower.contains("timeout") {
        return "Connection timed out. Please check your internet connection and try again."
            .to_string();
    }
    if error_lower.contains("connection refused")
        || error_lower.contains("failed to connect")
        || error_lower.contains("network")
    {
        return "Could not connect to the transcription service. Please check your internet connection.".to_string();
    }
    if error_lower.contains("413")
        || error_lower.contains("payload too large")
        || error_lower.contains("file too large")
        || error_lower.contains("upload limit")
        || error_lower.contains("audio duration")
    {
        return "This recording is too large for the selected transcription provider. Try another provider or split the recording.".to_string();
    }
    if error_lower.contains("invalid audio")
        || error_lower.contains("unsupported format")
        || error_lower.contains("codec")
    {
        return "The audio file format is not supported. Please try a different file.".to_string();
    }
    if error_lower.contains("file not found") || error_lower.contains("no such file") {
        return "Audio file not found. The recording may have been moved or deleted.".to_string();
    }

    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listen_params(model: Option<&str>) -> owhisper_interface::ListenParams {
        owhisper_interface::ListenParams {
            model: model.map(ToOwned::to_owned),
            languages: vec![anlg_language::ISO639::En.into()],
            ..Default::default()
        }
    }

    fn batch_params(provider: BatchProvider, base_url: &str) -> BatchParams {
        BatchParams {
            session_id: "session".to_string(),
            provider,
            file_path: "/tmp/audio.wav".to_string(),
            model: None,
            base_url: base_url.to_string(),
            api_key: "key".to_string(),
            languages: vec![anlg_language::ISO639::En.into()],
            keywords: vec![],
            vocabulary: vec![],
            num_speakers: None,
            min_speakers: None,
            max_speakers: None,
        }
    }

    /// Fork-Abweichung (03.09.2026): Die Verhoerungen bleiben im Haus.
    ///
    /// `build_listen_params` fuellt die Parameter, aus denen der HTTP-Adapter
    /// die URL baut (crates/owhisper-client/src/adapter/whispercpp/batch.rs).
    /// Stuende dort die rohe Zeile, gingen bei einer eingetragenen fremden
    /// Adresse Kundennamen samt Verhoerungen ueber die Leitung.
    #[test]
    fn build_listen_params_sendet_keine_verhoerung() {
        let mut params = batch_params(BatchProvider::Pyannote, "https://api.pyannote.ai");
        params.vocabulary = vec![
            "Sedacz => Sedatsch; Sarec".to_string(),
            "Webflow".to_string(),
        ];

        let listen_params = build_listen_params(&params, 2, 48_000);

        assert_eq!(listen_params.vocabulary, ["Sedacz", "Webflow"]);
        for term in &listen_params.vocabulary {
            assert!(!term.contains("=>"), "Pfeil in {term}");
            assert!(!term.contains("Sarec"), "Verhoerung in {term}");
        }
    }

    #[test]
    fn build_listen_params_preserves_num_speakers() {
        let mut params = batch_params(BatchProvider::Pyannote, "https://api.pyannote.ai");
        params.num_speakers = Some(3);

        let listen_params = build_listen_params(&params, 2, 48_000);

        assert_eq!(listen_params.num_speakers, Some(3));
        assert_eq!(listen_params.channels, 2);
        assert_eq!(listen_params.sample_rate, 48_000);
    }

    #[test]
    fn build_listen_params_preserves_speaker_range_options() {
        let mut params = batch_params(BatchProvider::Pyannote, "https://api.pyannote.ai");
        params.min_speakers = Some(2);
        params.max_speakers = Some(4);

        let listen_params = build_listen_params(&params, 1, 16_000);
        assert_eq!(listen_params.min_speakers, Some(2));
        assert_eq!(listen_params.max_speakers, Some(4));
        assert!(listen_params.custom_query.is_none());
    }

    // Die fuenf URL-Routing-Tests, die hier standen, pruefen `resolve_batch_adapter_kind`
    // und `supports_progressive_batch`. Beide Helfer hatten nach dem Wegfall von
    // `BatchProvider::Am` (Argmax-Strecke, 01.09.2026) keinen Aufrufer mehr in der
    // Produktion und sind entfallen. Der wichtigste Beweis lebt weiter, nur an seinem
    // richtigen Ort: dass eine Loopback-Adresse beim lokalen Server landet, steht in
    // `crates/owhisper-client/src/adapter/tests.rs` -- dort, wo die Aufloesung passiert.

    // F4: die Kennung bleibt auf dem Draht lesbar -- unter dem heutigen und
    // dem aelteren Namen --, auch wenn kein Client mehr dahintersteht.
    #[test]
    fn anarlog_and_its_old_name_still_deserialize() {
        for wire in ["\"anarlog\"", "\"hyprnote\""] {
            let provider: BatchProvider = serde_json::from_str(wire).unwrap();
            assert!(matches!(provider, BatchProvider::Anarlog), "{wire}");
            assert_eq!(provider.to_adapter_kind(), Some(AdapterKind::Anarlog));
        }
    }

    #[test]
    fn cloud_anarlog_batch_is_not_progressive() {
        let params = batch_params(BatchProvider::Anarlog, "https://api.anarlog.so/stt");

        assert!(!expects_progressive_batch(&params));
    }

    /// Der lokale whisper.cpp-Dienst laeuft immer progressiv, unabhaengig von der URL.
    /// Frueher lief dieselbe Zusage ueber `BatchProvider::Am` und eine Loopback-Adresse.
    #[test]
    fn local_whisper_batch_is_progressive() {
        let params = batch_params(BatchProvider::WhisperLocal, "http://localhost:50060/v1");

        assert!(expects_progressive_batch(&params));
    }

    #[test]
    fn provider_upload_limit_errors_are_explained() {
        let message = format_user_friendly_error(
            r#"UnexpectedStatus { status: 400, body: "Audio file exceeds the 25 MB multipart upload limit." }"#,
        );

        assert!(message.starts_with("This recording is too large"));
    }

    #[test]
    fn provider_duration_limit_errors_are_explained() {
        let message = format_user_friendly_error(
            r#"UnexpectedStatus { status: 400, body: "{\"error\":{\"message\":\"audio duration 1500.012 seconds is longer than 1400 seconds which is the maximum for this model\"}}" }"#,
        );

        assert!(message.starts_with("This recording is too large"));
    }
}
