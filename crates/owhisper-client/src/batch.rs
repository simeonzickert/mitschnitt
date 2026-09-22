use std::marker::PhantomData;
use std::path::Path;

use owhisper_interface::ListenParams;
use owhisper_interface::batch::Response as BatchResponse;
use reqwest_middleware::ClientWithMiddleware;

use crate::adapter::BatchSttAdapter;
use crate::error::Error;
use crate::http_client::create_client;
use crate::{DeepgramAdapter, ListenClientBuilder, normalize_listen_params};

pub struct BatchClientBuilder<A: BatchSttAdapter = DeepgramAdapter> {
    api_base: Option<String>,
    api_key: Option<String>,
    params: Option<ListenParams>,
    _marker: PhantomData<A>,
}

impl Default for BatchClientBuilder {
    fn default() -> Self {
        Self {
            api_base: None,
            api_key: None,
            params: None,
            _marker: PhantomData,
        }
    }
}

impl<A: BatchSttAdapter> BatchClientBuilder<A> {
    pub fn api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = Some(api_base.into());
        self
    }

    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn params(mut self, params: ListenParams) -> Self {
        self.params = Some(params);
        self
    }

    pub fn adapter<B: BatchSttAdapter>(self) -> BatchClientBuilder<B> {
        BatchClientBuilder {
            api_base: self.api_base,
            api_key: self.api_key,
            params: self.params,
            _marker: PhantomData,
        }
    }

    pub fn build(self) -> BatchClient<A> {
        BatchClient::new(
            self.api_base.expect("api_base is required"),
            self.api_key.unwrap_or_default(),
            self.params.unwrap_or_default(),
        )
    }
}

#[derive(Clone)]
pub struct BatchClient<A: BatchSttAdapter = DeepgramAdapter> {
    client: ClientWithMiddleware,
    api_base: String,
    api_key: String,
    params: ListenParams,
    _marker: PhantomData<A>,
}

impl<A: BatchSttAdapter> BatchClient<A> {
    pub fn builder() -> BatchClientBuilder<A> {
        BatchClientBuilder {
            api_base: None,
            api_key: None,
            params: None,
            _marker: PhantomData,
        }
    }

    // C5 (Opus 7, Review 02.09.2026): bis hierher wurde einer URL der
    // Gegenstelle des Originals `?provider=<adapter>` angehaengt -- das Proxy-
    // Protokoll, mit dem api.anarlog.so den Ziel-Anbieter waehlte. Dieser Fork
    // baut keinen Client gegen das Original; die URL bleibt, wie sie kam.
    pub fn new(api_base: String, api_key: String, params: ListenParams) -> Self {
        let params = normalize_listen_params(params);

        Self {
            client: create_client(),
            api_base,
            api_key,
            params,
            _marker: PhantomData,
        }
    }

    pub async fn transcribe_file<P: AsRef<Path> + Send>(
        &self,
        file_path: P,
    ) -> Result<BatchResponse, Error> {
        A::default()
            .transcribe_file(
                &self.client,
                &self.api_base,
                &self.api_key,
                &self.params,
                file_path,
            )
            .await
    }
}

impl<A: crate::RealtimeSttAdapter + BatchSttAdapter> ListenClientBuilder<A> {
    pub fn build_batch(self) -> BatchClient<A> {
        let params = self.normalized_params();
        let api_base = self.api_base.expect("api_base is required");
        BatchClient::new(api_base, self.api_key.unwrap_or_default(), params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeepgramAdapter, OpenAIAdapter};
    use anlg_language::{ISO639, Language};

    #[test]
    fn does_not_inject_provider_for_direct_provider_url() {
        let client = BatchClient::<OpenAIAdapter>::builder()
            .api_base("https://api.openai.com/v1")
            .api_key("test")
            .build();

        assert_eq!(client.api_base, "https://api.openai.com/v1");
    }

    // C5 (Opus 7, Review 02.09.2026): eine von Hand eingetragene `custom`-URL
    // auf die Gegenstelle des Originals bekommt keinen Proxy-Parameter mehr
    // angehaengt -- der Client kennt das Protokoll nicht mehr. Ob so eine URL
    // ueberhaupt einen Client bekommt, entscheidet from_url_and_languages
    // (-> Anarlog -> `removed:`-Arm), nicht dieser Bau.
    #[test]
    fn leaves_a_hand_entered_url_of_the_original_untouched() {
        for api_base in [
            "https://api.anarlog.so/stt",
            "https://api.anarlog.so/stt?provider=anarlog&model=whisper-1",
        ] {
            let client = BatchClient::<DeepgramAdapter>::builder()
                .api_base(api_base)
                .api_key("test")
                .build();

            assert_eq!(client.api_base, api_base);
        }
    }

    #[test]
    fn normalizes_languages_when_constructed() {
        let client = BatchClient::<DeepgramAdapter>::builder()
            .api_base("https://api.deepgram.com/v1")
            .api_key("test")
            .params(ListenParams {
                languages: vec![
                    Language::with_region(ISO639::En, "US"),
                    Language::with_region(ISO639::En, "GB"),
                    ISO639::En.into(),
                    Language::with_region(ISO639::Ko, "KR"),
                ],
                ..Default::default()
            })
            .build();

        assert_eq!(client.params.languages.len(), 2);
        assert_eq!(client.params.languages[0].iso639(), ISO639::En);
        assert_eq!(client.params.languages[0].region(), None);
        assert_eq!(client.params.languages[1].iso639(), ISO639::Ko);
        assert_eq!(client.params.languages[1].region(), Some("KR"));
    }
}
