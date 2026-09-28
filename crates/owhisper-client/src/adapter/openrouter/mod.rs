use std::path::Path;

use owhisper_interface::ListenParams;

use crate::adapter::openai_compatible_batch::{OpenAICompatibleBatchConfig, transcribe};
use crate::adapter::{
    BatchFuture, BatchSttAdapter, ClientWithMiddleware, LanguageQuality, LanguageSupport,
};

/// Direkt gegen `POST /api/v1/audio/transcriptions` mit
/// `response_format: "verbose_json"` + `timestamp_granularities: ["word","segment"]`
/// gemessen (28.09.2026, Fixture `crates/fixtures/audio/speech-de.mp3`):
/// diese sechs Modelle lieferten `words[]` mit echten `start`/`end`-Sekunden.
/// `openai/gpt-transcribe` antwortete stattdessen mit HTTP 400 ("does not
/// support response_format verbose_json").
///
/// Bewusst eine kleine, belegte Liste statt eines Rueckfalls auf `json` bei
/// genau diesem 400: `channel_split.rs` ruft `transcribe_file` einmal je
/// ~30-Sekunden-Paket auf (oft ein Dutzend Mal je Aufnahme). Ein
/// Rueckfall-Versuch pro Paket waere ein zweiter kostenpflichtiger Aufruf bei
/// JEDEM Paket eines nicht unterstuetzten Modells, nicht nur beim ersten --
/// OpenRouter traegt keinen Lauf-uebergreifenden Zustand, ueber den sich "das
/// hatten wir schon" einmalig merken liesse, ohne globalen Mutex mit eigenen
/// Race-Bedingungen einzufuehren. Ein Modell, das hier NICHT steht, bleibt
/// stattdessen beim bisherigen Weg (Text ohne Wortzeiten, Anzeige verteilt
/// nach Sprechanteil) -- kein lauter Fehlschlag, kein doppelt bezahlter
/// Aufruf, nur weiterhin das alte Verhalten.
const WORD_TIMESTAMP_MODELS: &[&str] = &[
    "google/gemini-3.5-transcribe",
    "microsoft/mai-transcribe-2",
    "x-ai/grok-stt-1.0",
    "deepgram/nova-3",
    "nvidia/parakeet-tdt-0.6b-v3",
    "openai/whisper-large-v3-turbo",
];

/// Eine Stelle fuer den Default -- `transcribe_file` und `wants_word_timestamps`
/// muessen sich hier immer einig sein, sonst weiss der Kanal-Weg vorher etwas
/// anderes, als die HTTP-Anfrage nachher tut.
const OPENROUTER_DEFAULT_MODEL: &str = "openai/gpt-transcribe";

#[derive(Clone, Default)]
pub struct OpenRouterAdapter;

impl OpenRouterAdapter {
    pub fn language_support_batch(_languages: &[anlg_language::Language]) -> LanguageSupport {
        LanguageSupport::Supported {
            quality: LanguageQuality::NoData,
        }
    }
}

impl BatchSttAdapter for OpenRouterAdapter {
    fn provider_name(&self) -> &'static str {
        "openrouter"
    }

    fn is_supported_languages(
        &self,
        languages: &[anlg_language::Language],
        _model: Option<&str>,
    ) -> bool {
        Self::language_support_batch(languages).is_supported()
    }

    fn transcribe_file<'a, P: AsRef<Path> + Send + 'a>(
        &'a self,
        client: &'a ClientWithMiddleware,
        api_base: &'a str,
        api_key: &'a str,
        params: &'a ListenParams,
        file_path: P,
    ) -> BatchFuture<'a> {
        let path = file_path.as_ref().to_path_buf();
        Box::pin(async move {
            transcribe(
                client,
                api_base,
                api_key,
                params,
                &path,
                OpenAICompatibleBatchConfig {
                    provider: "openrouter",
                    default_api_base: "https://openrouter.ai/api/v1",
                    default_model: OPENROUTER_DEFAULT_MODEL,
                    transcription_path: "audio/transcriptions",
                    response_format: Some("verbose_json"),
                    timestamp_field: Some("timestamp_granularities[]"),
                    include_language: true,
                    word_timestamp_models: Some(WORD_TIMESTAMP_MODELS),
                },
            )
            .await
        })
    }

    /// Dieselbe Liste, dieselbe Formel wie die HTTP-Anfrage oben (ZICK-330
    /// Nachtrag, 28.09.2026) -- der Kanal-Weg in listener2-core braucht diese
    /// Antwort VOR dem Aufruf, um die Paketgroesse zu waehlen.
    fn wants_word_timestamps(&self, model: Option<&str>) -> bool {
        crate::adapter::openai_compatible_batch::resolves_to_word_timestamps(
            Some(WORD_TIMESTAMP_MODELS),
            OPENROUTER_DEFAULT_MODEL,
            model,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn uses_openrouter_transcription_endpoint_without_model_specific_options() {
        let audio_file = crate::test_utils::sample_wav_file();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "Hello from OpenRouter."
            })))
            .mount(&server)
            .await;
        let params = ListenParams {
            model: Some("openai/gpt-4o-mini-transcribe".to_string()),
            languages: vec![anlg_language::ISO639::En.into()],
            ..Default::default()
        };

        let response = OpenRouterAdapter
            .transcribe_file(
                &crate::http_client::create_client(),
                &server.uri(),
                "test-key",
                &params,
                audio_file.path(),
            )
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        let body = String::from_utf8_lossy(&requests[0].body);
        assert!(body.contains("openai/gpt-4o-mini-transcribe"));
        assert!(body.contains("name=\"language\""));
        assert!(!body.contains("response_format"));
        assert_eq!(
            response.results.channels[0].alternatives[0].transcript,
            "Hello from OpenRouter."
        );
    }

    /// Gegenprobe zum Test oben: ein Modell aus `WORD_TIMESTAMP_MODELS`
    /// bekommt `verbose_json` + `timestamp_granularities[]=word` tatsaechlich
    /// angefragt.
    #[tokio::test]
    async fn requests_verbose_json_and_word_timestamps_for_measured_models() {
        let audio_file = crate::test_utils::sample_wav_file();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "Hello from OpenRouter.",
                "words": [
                    { "word": "Hello", "start": 0.0, "end": 0.4 },
                    { "word": "from", "start": 0.4, "end": 0.7 },
                    { "word": "OpenRouter.", "start": 0.7, "end": 1.3 }
                ]
            })))
            .mount(&server)
            .await;
        let params = ListenParams {
            model: Some("google/gemini-3.5-transcribe".to_string()),
            languages: vec![anlg_language::ISO639::En.into()],
            ..Default::default()
        };

        let response = OpenRouterAdapter
            .transcribe_file(
                &crate::http_client::create_client(),
                &server.uri(),
                "test-key",
                &params,
                audio_file.path(),
            )
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "ein Aufruf, kein Rueckfall-Versuch");
        let body = String::from_utf8_lossy(&requests[0].body);
        assert!(body.contains("google/gemini-3.5-transcribe"));
        assert!(body.contains("name=\"response_format\""));
        assert!(body.contains("verbose_json"));
        assert!(body.contains("name=\"timestamp_granularities[]\""));
        let word = &response.results.channels[0].alternatives[0].words[0];
        assert_eq!(word.start, 0.0);
        assert_eq!(word.end, 0.4);
    }
}
