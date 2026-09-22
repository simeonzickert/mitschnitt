use std::path::{Path, PathBuf};

// hypr-llm-sm (SupportedModel::AnarlogLLM) fehlt hier seit dem 02.09.2026
// mit Absicht (ZICK-263): es liesse sich nur ueber den S3-Speicher des
// Ursprungsprojekts laden. Der Enum-Wert bleibt, weil der Schluessel in
// Datenbanken liegt; eine bereits geladene hypr-llm.gguf bleibt auf der
// Platte, wird aber nicht mehr als Modell gelistet.
//
// E2 (Fix-Runde 1d, Grok): kein cfg-Split mehr. Upstream war die Liste
// unter `not(target_arch = "aarch64")` leer, waehrend list_supported_models()
// hart drei Modelle nannte -- seit ea4cee1815 iteriert es diese Liste, also
// bekam ein Nicht-ARM-Bau ploetzlich KEIN Modell mehr angeboten (und
// list_downloaded_models fand dort schon vorher keins). Llama und Gemma sind
// GGUF-Modelle fuer jede Architektur, die wir bauen; die Liste hier ist
// dieselbe wie die GgufLlm-Eintraege von LocalModel::all() (Test unten).
pub static SUPPORTED_MODELS: &[SupportedModel] = &[
    SupportedModel::Llama3p2_3bQ4,
    SupportedModel::Gemma3_4bQ4,
    // Qwen3 4B Instruct 2507, dazugekommen 22.09.2026 als zweiter Kandidat
    // fuer deutsche Zusammenfassungen. Standard bleibt Gemma; welches Modell
    // der Startblock laedt, steht in onboarding-models.ts.
    SupportedModel::Qwen3_4bInstruct2507Q4,
];

pub use anlg_local_model::GgufLlmModel as SupportedModel;

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ModelInfo {
    pub key: SupportedModel,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CustomModelInfo {
    pub path: String,
    pub name: String,
}

pub fn llm_models_dir(models_base: &Path) -> PathBuf {
    models_base.join("llm")
}

pub fn list_supported_models() -> Vec<ModelInfo> {
    SUPPORTED_MODELS.iter().map(supported_model_info).collect()
}

pub fn supported_model_info(model: &SupportedModel) -> ModelInfo {
    let description = match model {
        // Bis zum 02.09.2026 standen Llama und Gemma hier als "Deprecated"
        // hinter dem Modell des Ursprungsprojekts. Jetzt sind sie die
        // einzigen Modelle -- also steht da, was sie sind.
        SupportedModel::Llama3p2_3bQ4 => "Meta Llama 3.2 3B Instruct, Q4_K_M.",
        SupportedModel::Gemma3_4bQ4 => "Google Gemma 3 4B IT, Q4_K_M.",
        SupportedModel::Qwen3_4bInstruct2507Q4 => "Qwen3 4B Instruct 2507, Q4_K_M.",
        SupportedModel::AnarlogLLM => "No longer offered (yujonglee/hypr-llm-sm).",
    };

    ModelInfo {
        key: model.clone(),
        name: model.display_name().to_string(),
        description: description.to_string(),
        size_bytes: model.model_size(),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ModelIdentifier {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "mock-onboarding")]
    MockOnboarding,
}

#[cfg(test)]
mod tests {
    use super::*;

    // E2 (Fix-Runde 1d): auf jeder Architektur dieselben zwei Modelle, und
    // dieselben wie in LocalModel::all() -- ohne cfg, damit der Test auch
    // dort laeuft, wo die Liste frueher leer war.
    #[test]
    fn llama_and_gemma_are_offered_on_every_architecture() {
        let offered: Vec<SupportedModel> = list_supported_models()
            .into_iter()
            .map(|info| info.key)
            .collect();
        let all: Vec<SupportedModel> = anlg_local_model::LocalModel::all()
            .into_iter()
            .filter_map(|model| match model {
                anlg_local_model::LocalModel::GgufLlm(llm) => Some(llm),
                _ => None,
            })
            .collect();

        assert_eq!(
            offered,
            vec![
                SupportedModel::Llama3p2_3bQ4,
                SupportedModel::Gemma3_4bQ4,
                SupportedModel::Qwen3_4bInstruct2507Q4,
            ]
        );
        assert_eq!(
            offered, all,
            "SUPPORTED_MODELS und LocalModel::all() laufen auseinander"
        );
    }

    // ZICK-263 / ISA N7 (02.09.2026): das Modell des Ursprungsprojekts
    // (hypr-llm-sm, Schluessel AnarlogLLM) wird nicht mehr angeboten. Der
    // Schluessel selbst bleibt deserialisierbar -- das prueft
    // crates/local-model. Und was angeboten wird, traegt kein
    // "Deprecated"-Etikett mehr: Llama und Gemma sind die einzigen Modelle.
    #[test]
    fn the_upstream_model_is_not_offered_and_the_rest_is_not_deprecated() {
        let offered = list_supported_models();
        assert!(
            offered
                .iter()
                .all(|info| info.key != SupportedModel::AnarlogLLM),
            "hypr-llm-sm wird noch angeboten"
        );
        assert!(
            !SUPPORTED_MODELS.contains(&SupportedModel::AnarlogLLM),
            "hypr-llm-sm steht noch in SUPPORTED_MODELS"
        );
        for info in offered {
            assert!(
                !info.description.contains("Deprecated"),
                "{} traegt noch das Deprecated-Etikett",
                info.name
            );
        }
    }
}
