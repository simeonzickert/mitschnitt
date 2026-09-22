pub use anlg_local_model::{AppleSpeechModel, LocalModel, SoniqoModel, WhisperModel};

pub static SUPPORTED_MODELS: &[LocalModel] = &[
    // Qwen3 am 31.08.2026 wieder aus der Auswahl genommen (Entscheid nach
    // Vergleich an echtem Material -- Begruendung an SoniqoModel::SELECTABLE).
    // Diese Liste muss mit selectable() uebereinstimmen, ein Test wacht darueber.
    LocalModel::Soniqo(SoniqoModel::ParakeetStreaming),
    LocalModel::Soniqo(SoniqoModel::ParakeetBatch),
    LocalModel::AppleSpeech(AppleSpeechModel::Default),
    // Fork-Abweichung (31.08.2026): Whisper large-v3-turbo als lokaler
    // BATCH-Anbieter neben Parakeet, nicht statt Parakeet. Anlass ist eine
    // Messung an echtem Material (des Betreibers 20-Minuten-Teammeeting, Sekunde
    // 370-460): Whisper traf dort den Eigennamen "Bo", den Parakeet und
    // Qwen3 beide als "mich" hoerten, setzte Satzzeichen und loeste den Genitiv
    // auf -- bei rund 19-facher Echtzeit auf der GPU.
    //
    // Nur diese eine Whisper-Groesse steht in der Auswahl. Die kleineren
    // Varianten existieren weiter als `WhisperModel`, sind aber nicht das,
    // wofuer Whisper hier antritt (Qualitaet, nicht Tempo).
    LocalModel::Whisper(WhisperModel::QuantizedLargeTurbo),
];

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SttModelType {
    Soniqo,
    AppleSpeech,
    Whispercpp,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SttModelInfo {
    pub key: LocalModel,
    pub display_name: String,
    pub description: String,
    pub size_bytes: Option<u64>,
    pub model_type: SttModelType,
    pub supports_realtime: bool,
    pub recommended_memory_bytes: u64,
}

const GIB: u64 = 1024 * 1024 * 1024;

const fn recommended_memory_bytes(model: &LocalModel) -> u64 {
    match model {
        LocalModel::Soniqo(SoniqoModel::ParakeetStreaming) => 8 * GIB,
        LocalModel::Soniqo(SoniqoModel::ParakeetBatch | SoniqoModel::Omnilingual) => 16 * GIB,
        // Qwen3 weights are 600 MB / 1.7 GB and upstream caps the MLX scratch pool at
        // min(4 GB, 25% of RAM) for the 1.7B variant. Upstream's own load-time warning
        // threshold is 24 GB total RAM (`Qwen3ASRMemory.largeModelRAMWarningThresholdGB`,
        // speech-swift 0.0.22), not 32 — the previous 32 GiB here was an unsourced guess
        // that shut the model out of machines upstream considers sufficient.
        LocalModel::Soniqo(SoniqoModel::Qwen3Small) => 8 * GIB,
        LocalModel::Soniqo(SoniqoModel::Qwen3Large) => 24 * GIB,
        LocalModel::AppleSpeech(_) => 8 * GIB,
        LocalModel::Whisper(_) => 8 * GIB,
        LocalModel::GgufLlm(_) => unreachable!(),
    }
}

pub fn stt_model_info(model: &LocalModel) -> SttModelInfo {
    match model {
        LocalModel::Soniqo(value) => SttModelInfo {
            key: model.clone(),
            display_name: value.display_name().to_string(),
            description: value.description().to_string(),
            size_bytes: Some(value.size_bytes()),
            model_type: SttModelType::Soniqo,
            supports_realtime: value.supports_live(),
            recommended_memory_bytes: recommended_memory_bytes(model),
        },
        LocalModel::AppleSpeech(value) => SttModelInfo {
            key: model.clone(),
            display_name: value.display_name().to_string(),
            description: value.description().to_string(),
            // macOS installs and shares the assets, so there is nothing for us to download.
            size_bytes: None,
            model_type: SttModelType::AppleSpeech,
            supports_realtime: value.supports_live(),
            recommended_memory_bytes: recommended_memory_bytes(model),
        },
        LocalModel::Whisper(value) => SttModelInfo {
            key: model.clone(),
            display_name: value.display_name().to_string(),
            description: value.description(),
            size_bytes: Some(value.model_size_bytes()),
            model_type: SttModelType::Whispercpp,
            supports_realtime: false,
            recommended_memory_bytes: recommended_memory_bytes(model),
        },
        LocalModel::GgufLlm(_) => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_models_include_soniqo_models_from_rust_source_of_truth() {
        let supported_soniqo_models = SUPPORTED_MODELS
            .iter()
            .filter_map(|model| match model {
                LocalModel::Soniqo(value) => Some(*value),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(supported_soniqo_models, SoniqoModel::selectable());
    }

    #[test]
    fn soniqo_model_info_comes_from_soniqo_metadata() {
        for model in SoniqoModel::all() {
            let info = stt_model_info(&LocalModel::Soniqo(*model));

            assert_eq!(info.key, LocalModel::Soniqo(*model));
            assert_eq!(info.display_name, model.display_name());
            assert_eq!(info.description, model.description());
            assert_eq!(info.size_bytes, Some(model.size_bytes()));
            assert_eq!(info.supports_realtime, model.supports_live());
            assert!(info.recommended_memory_bytes >= 8 * GIB);
            assert!(matches!(info.model_type, SttModelType::Soniqo));
        }
    }

    #[test]
    fn every_supported_model_has_unique_hardware_and_runtime_metadata() {
        let mut keys = std::collections::HashSet::new();

        for model in SUPPORTED_MODELS {
            let info = stt_model_info(model);
            let supports_realtime = match model {
                LocalModel::Soniqo(model) => model.supports_live(),
                LocalModel::AppleSpeech(model) => model.supports_live(),
                LocalModel::Whisper(_) => false,
                LocalModel::GgufLlm(_) => unreachable!(),
            };

            assert!(keys.insert(info.key.cli_name()));
            assert!(!info.display_name.trim().is_empty());
            assert!(!info.description.trim().is_empty());
            assert_eq!(info.supports_realtime, supports_realtime);
            assert!(info.recommended_memory_bytes > 0);
        }
    }

    /// Mirrors exactly what the `list_supported_models` Tauri command returns, which is
    /// the list the settings picker renders. Asserting the constant alone would not
    /// prove the models survive the platform filter — that filter is what used to
    /// Prueft dasselbe wie der Qwen3-Test darunter, nur in der Zielrichtung:
    /// Whisper large-v3-turbo muss den Plattformfilter UEBERLEBEN und in der
    /// gerenderten Liste ankommen. Die Konstante allein zu pruefen wuerde
    /// nichts beweisen -- genau dieser Filter hat Qwen3 einmal verschluckt.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn whisper_large_turbo_reaches_the_settings_picker_on_this_host() {
        let rendered = SUPPORTED_MODELS
            .iter()
            .filter(|model| model.is_available_on_current_platform())
            .map(|model| stt_model_info(model))
            .collect::<Vec<_>>();

        let whisper = rendered
            .iter()
            .find(|info| info.key.cli_name() == "whisper-large-turbo")
            .unwrap_or_else(|| {
                let names = rendered
                    .iter()
                    .map(|info| info.key.cli_name())
                    .collect::<Vec<_>>();
                panic!("Whisper large-v3-turbo fehlt in der Auswahl: {names:?}")
            });

        assert!(matches!(whisper.model_type, SttModelType::Whispercpp));
        // Whisper ist hier ausdruecklich ein BATCH-Anbieter. Stuende hier true,
        // wuerde die Oberflaeche ihn fuer Live-Mitschnitt anbieten, wo er nichts
        // zu suchen hat.
        assert!(!whisper.supports_realtime);
        // Der Download-Weg haengt an dieser Groesse: ohne sie zeigt die
        // Oberflaeche keinen Fortschritt und keine Groesse an.
        assert_eq!(whisper.size_bytes, Some(874_188_075));

        // Parakeet bleibt daneben stehen, nicht darunter.
        let names = rendered
            .iter()
            .map(|info| info.key.cli_name())
            .collect::<Vec<_>>();
        assert!(names.contains(&"soniqo-parakeet-batch"), "{names:?}");
        assert!(names.contains(&"soniqo-parakeet-streaming"), "{names:?}");
    }

    /// swallow Qwen3 unconditionally.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn qwen3_models_reach_the_settings_picker_on_this_host() {
        let rendered = SUPPORTED_MODELS
            .iter()
            .filter(|model| model.is_available_on_current_platform())
            .map(|model| stt_model_info(model).key.cli_name())
            .collect::<Vec<_>>();

        // Qwen3 ist seit dem 31.08.2026 bewusst NICHT mehr in der Auswahl
        // (Entscheid nach Vergleich an echtem Material, Begruendung an
        // SoniqoModel::SELECTABLE). Der Test prueft deshalb jetzt die
        // Gegenrichtung -- und bleibt wertvoll: faellt er, ist Qwen3
        // versehentlich zurueck in die Oberflaeche gerutscht.
        assert!(
            !rendered.contains(&"soniqo-qwen3-small"),
            "Qwen3 0.6B ist wieder in der Auswahl, obwohl es raus sollte: {rendered:?}"
        );
        assert!(
            !rendered.contains(&"soniqo-qwen3-large"),
            "Qwen3 1.7B ist wieder in der Auswahl, obwohl es raus sollte: {rendered:?}"
        );
        // Der Weg selbst muss aber intakt bleiben: die Ladefaehigkeit haengt
        // an der Laufzeit-Versionspruefung, nicht an der Auswahl. Faellt DAS,
        // sind die vier Sperren zurueck und ein Wiederanschalten waere wieder
        // ein Tagesprojekt statt zwei Zeilen.
        assert!(
            anlg_local_model::SoniqoModel::Qwen3Large.is_available_on_current_platform(),
            "die Laufzeit-Versionspruefung fuer Qwen3 ist kaputt"
        );
        assert!(rendered.contains(&"soniqo-parakeet-batch"));
    }
}
