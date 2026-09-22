use owhisper_interface::ListenParams;

use crate::adapter::deepgram_compat::{
    LanguageQueryStrategy, Serializer, TranscriptionMode, UrlQuery,
};

// Die Sprachliste hat ihr Zuhause in `anlg_language`. Sie lief frueher ueber
// `anlg_am` -- eine Umleitung durch das Argmax-Crate, das es nicht mehr gibt.
pub use anlg_language::PARAKEET_TDT_V3_LANGUAGE_CODES as PARAKEET_V3_LANGS;

pub struct LocalServerLanguageStrategy;

impl LanguageQueryStrategy for LocalServerLanguageStrategy {
    fn append_language_query<'a>(
        &self,
        query_pairs: &mut Serializer<'a, UrlQuery>,
        params: &ListenParams,
        _mode: TranscriptionMode,
    ) {
        let lang = pick_single_language(params);
        query_pairs.append_pair("language", lang.iso639().code());
        if !params.languages.is_empty() {
            query_pairs.append_pair("detect_language", "false");
        }
    }
}

fn pick_single_language(params: &ListenParams) -> anlg_language::Language {
    let model = params.model.as_deref().unwrap_or("");

    if model.contains("parakeet") && model.contains("v2") {
        anlg_language::ISO639::En.into()
    } else if model.contains("parakeet") && model.contains("v3") {
        params
            .languages
            .iter()
            .find(|lang| PARAKEET_V3_LANGS.contains(&lang.iso639().code()))
            .cloned()
            .unwrap_or_else(|| anlg_language::ISO639::En.into())
    } else {
        params
            .languages
            .first()
            .cloned()
            .unwrap_or_else(|| anlg_language::ISO639::En.into())
    }
}
