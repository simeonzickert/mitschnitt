use anlg_ws_client::client::Message;
use owhisper_interface::ListenParams;
use owhisper_interface::stream::StreamResponse;

use crate::adapter::RealtimeSttAdapter;
use crate::adapter::deepgram_compat::build_listen_ws_url;

use super::{
    LocalServerAdapter, keywords::LocalServerKeywordStrategy, language::LocalServerLanguageStrategy,
};

impl RealtimeSttAdapter for LocalServerAdapter {
    fn provider_name(&self) -> &'static str {
        "local-server"
    }

    fn is_supported_languages(
        &self,
        languages: &[anlg_language::Language],
        model: Option<&str>,
    ) -> bool {
        LocalServerAdapter::is_supported_languages_live(languages, model)
    }

    fn supports_native_multichannel(&self) -> bool {
        false
    }

    fn build_ws_url(&self, api_base: &str, params: &ListenParams, channels: u8) -> url::Url {
        build_listen_ws_url(
            api_base,
            params,
            channels,
            &LocalServerLanguageStrategy,
            &LocalServerKeywordStrategy,
        )
    }

    fn build_auth_header(&self, api_key: Option<&str>) -> Option<(&'static str, String)> {
        api_key.and_then(|k| crate::providers::Provider::Deepgram.build_auth_header(k))
    }

    fn keep_alive_message(&self) -> Option<Message> {
        Some(Message::Text(
            serde_json::to_string(&owhisper_interface::ControlMessage::KeepAlive)
                .unwrap()
                .into(),
        ))
    }

    fn finalize_message(&self) -> Message {
        Message::Text(
            serde_json::to_string(&owhisper_interface::ControlMessage::Finalize)
                .unwrap()
                .into(),
        )
    }

    fn parse_response(&self, raw: &str) -> Vec<StreamResponse> {
        match serde_json::from_str::<StreamResponse>(raw) {
            Ok(response) => vec![response],
            Err(_) => {
                if let Ok(error) = serde_json::from_str::<LocalServerError>(raw) {
                    tracing::error!(
                        error.type = %error.error_type,
                        error = %error.message,
                        "local_server_error"
                    );
                    vec![StreamResponse::ErrorResponse {
                        error_code: None,
                        error_message: format!("{}: {}", error.error_type, error.message),
                        provider: "local-server".to_string(),
                    }]
                } else {
                    tracing::warn!(
                        anarlog.payload.size_bytes = raw.len() as u64,
                        "local_server_unknown_message"
                    );
                    vec![]
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct LocalServerError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

#[cfg(test)]
mod tests {
    use anlg_language::ISO639;

    use super::LocalServerAdapter;

    use crate::test_utils::{UrlTestCase, run_url_test_cases};

    const API_BASE: &str = "ws://localhost:50060/v1";

    #[test]
    fn test_single_language_urls() {
        run_url_test_cases(
            &LocalServerAdapter::default(),
            API_BASE,
            &[
                UrlTestCase {
                    name: "english",
                    model: None,
                    languages: &[ISO639::En],
                    contains: &["language=en", "detect_language=false"],
                    not_contains: &["language=multi", "languages="],
                },
                UrlTestCase {
                    name: "empty_defaults_to_english",
                    model: None,
                    languages: &[],
                    contains: &["language=en"],
                    not_contains: &["detect_language=false"],
                },
            ],
        );
    }

    #[test]
    fn test_multi_language_urls() {
        run_url_test_cases(
            &LocalServerAdapter::default(),
            API_BASE,
            &[UrlTestCase {
                name: "multi_lang_picks_first",
                model: None,
                languages: &[ISO639::De, ISO639::Fr],
                contains: &["language=de", "detect_language=false"],
                not_contains: &["language=multi", "language=fr"],
            }],
        );
    }

    #[test]
    fn test_parakeet_v2_urls() {
        run_url_test_cases(
            &LocalServerAdapter::default(),
            API_BASE,
            &[UrlTestCase {
                name: "parakeet_v2_always_english",
                model: Some("parakeet-v2-something"),
                languages: &[ISO639::De],
                contains: &["language=en", "detect_language=false"],
                not_contains: &["language=de"],
            }],
        );
    }

    #[test]
    fn test_parakeet_v3_urls() {
        run_url_test_cases(
            &LocalServerAdapter::default(),
            API_BASE,
            &[
                UrlTestCase {
                    name: "parakeet_v3_supported_language",
                    model: Some("parakeet-v3-something"),
                    languages: &[ISO639::De],
                    contains: &["language=de", "detect_language=false"],
                    not_contains: &[],
                },
                UrlTestCase {
                    name: "parakeet_v3_unsupported_fallback",
                    model: Some("parakeet-v3-something"),
                    languages: &[ISO639::Ko],
                    contains: &["language=en", "detect_language=false"],
                    not_contains: &["language=ko"],
                },
                UrlTestCase {
                    name: "parakeet_v3_multi_lang_picks_first_supported",
                    model: Some("parakeet-v3-something"),
                    languages: &[ISO639::Ko, ISO639::Fr],
                    contains: &["language=fr", "detect_language=false"],
                    not_contains: &[],
                },
            ],
        );
    }
}
