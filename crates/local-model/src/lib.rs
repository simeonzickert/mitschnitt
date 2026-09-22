use std::path::{Path, PathBuf};

use anlg_model_downloader::{DownloadableModel, Error};
pub use anlg_transcribe_soniqo::SoniqoModel;
pub use anlg_transcribe_speechanalyzer::AppleSpeechModel;
pub use anlg_whisper_local_model::WhisperModel;

pub const APPLE_SPEECH_DEFAULT_LOCALE: &str = "en-US";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, Eq, Hash, PartialEq)]
pub enum GgufLlmModel {
    Llama3p2_3bQ4,
    Gemma3_4bQ4,
    // Kandidat fuer den Sprachmodell-Vergleich (Recherche 22.09.2026), noch
    // NICHT Standard -- siehe Kommentarblock bei model_url() fuer Quelle und
    // Begruendung.
    Qwen3_4bInstruct2507Q4,
    #[serde(alias = "HyprLLM")]
    AnarlogLLM,
}

impl GgufLlmModel {
    pub fn file_name(&self) -> &str {
        match self {
            GgufLlmModel::Llama3p2_3bQ4 => "llm.gguf",
            GgufLlmModel::AnarlogLLM => "hypr-llm.gguf",
            GgufLlmModel::Gemma3_4bQ4 => "gemma-3-4b-it-Q4_K_M.gguf",
            GgufLlmModel::Qwen3_4bInstruct2507Q4 => "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        }
    }

    // Herkunft der Gewichte (Stand 01.09.2026). Der Fork zog seine Modelle
    // ueber den S3-Speicher des Originals -- eine Leitung, die uns niemand
    // schuldet, und die beim whisper.cpp-Pfad am 31.08.2026 bereits mit 403
    // geantwortet hat. Umgestellt wird nur, wo die Quelle BITGENAU dieselbe
    // Datei liefert; die Pruefsumme unten wird dabei NIE angepasst, denn sie
    // ist der Beweis. Gerechnet wurde jeweils die CRC32 der vollstaendig
    // geladenen Datei (crc32fast, wie in crates/file/src/lib.rs).
    //
    // - Llama-3.2-3B: umgestellt. Hugging Face liefert 2019377440 Bytes,
    //   CRC32 2831308098 -- identisch mit dem erwarteten Wert.
    // - hypr-llm-sm (AnarlogLLM): seit dem 02.09.2026 NICHT MEHR ANGEBOTEN
    //   (ZICK-263). Gleiche Groesse auf Hugging Face (1107409056), aber
    //   CRC32 378658602 statt 4037351144 -- keine bitgenaue Quelle, und der
    //   fremde Spiegel kommt nicht mehr in Frage. Der Enum-Wert bleibt, weil
    //   der Schluessel "AnarlogLLM" (Alias "HyprLLM") serialisiert in
    //   Datenbanken liegt; eine bereits geladene Datei hypr-llm.gguf bleibt
    //   unangetastet, wird aber nicht mehr als Modell gelistet.
    // - Gemma-3-4b: umgestellt. Hugging Face liefert 2489894016 Bytes,
    //   CRC32 2760830291 -- identisch mit dem erwarteten Wert.
    //
    // Qwen3-4B-Instruct-2507 (Recherche + Messung 22.09.2026, ISC-Auftrag
    // "Onboarding-Modell"): Kandidat fuer den Vergleich gegen Gemma 3 4B,
    // NICHT der neue Standard -- die Entscheidung zwischen beiden steht noch
    // aus, Standard bleibt Gemma (siehe apps/desktop/src/onboarding/
    // onboarding-models.ts, ausserhalb des Scopes dieser Aenderung).
    // Warum dieser Kandidat statt eines neueren Qwen3.5/3.6/3.8: Qwen3.5-4B
    // (Feb. 2026) ist ein multimodales Hybrid-Modell mit Gated-DeltaNet +
    // sparse MoE -- eine Architektur, die in llama.cpp erst seit kurzem und
    // nur in aktuellen Bauten laeuft (ggml-org/llama.cpp Issue #15940), also
    // ein unnoetiges Risiko fuer eine reine Text-Zusammenfassung. Qwen3-4B-
    // Instruct-2507 ist dagegen ein dichtes (dense) Transformer-Modell ohne
    // MoE, seit 06.08.2025 stabil in llama.cpp/GGUF verankert (Quelle:
    // huggingface.co/Qwen/Qwen3-4B-Instruct-2507, Modellkarte, Stand
    // 22.09.2026), Lizenz Apache-2.0 (klarer als Gemmas eigene Lizenz fuer
    // Weitergabe an Dritte), Kontextfenster 262144 Token nativ (Modellkarte
    // nennt "262,144 natively", erweiterbar bis 1010000 -- deutlich groesser
    // als Gemma 3 4B mit 128K).
    // Eine echte DEUTSCHE Zusammenfassungs-Kennzahl gibt es dafuer NICHT --
    // die offiziellen Benchmarks (MMLU-Pro, MMLU-Redux, C-Eval, GPQA) sind
    // englisch/chinesisch. Einziger gefundener deutscher Datenpunkt ist ein
    // Belebele-Lesetest (nicht Zusammenfassung) von 0.81 fuer Qwen3-4B aus
    // einer Drittstudie (arxiv "UberWeb"-Datensatzpapier), nicht aus Qwens
    // eigenem Bericht -- als Indiz zu lesen, nicht als Beleg.
    // Datei ueber unsloth/Qwen3-4B-Instruct-2507-GGUF geladen: das
    // OFFIZIELLE Qwen/Qwen3-4B-Instruct-2507-GGUF-Repo antwortete am
    // 22.09.2026 auf denselben Dateinamen mit HTTP 401 (kein Zugriff ohne
    // Anmeldung) -- unsloth ist dabei kein Ausweichen auf eine fremde
    // Quelle, sondern derselbe etablierte Requantisierer, den dieser Code
    // schon fuer Gemma oben verwendet. Hugging Face lieferte am 22.09.2026
    // 2497281120 Bytes, CRC32 1186510226 -- zweifach selbst gerechnet
    // (Python zlib.crc32 UND ein Rust-Referenzlauf mit derselben crc32fast-
    // Bibliothek wie in crates/file/src/lib.rs), beide Werte identisch.
    pub fn model_url(&self) -> Option<&str> {
        match self {
            GgufLlmModel::Llama3p2_3bQ4 => Some(
                "https://huggingface.co/lmstudio-community/Llama-3.2-3B-Instruct-GGUF/resolve/main/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
            ),
            // Keine Adresse mehr: die einzige bitgenaue Quelle war der
            // S3-Speicher des Ursprungsprojekts. Ein Download-Versuch
            // scheitert hier sauber, statt still dorthin zu gehen.
            GgufLlmModel::AnarlogLLM => None,
            GgufLlmModel::Gemma3_4bQ4 => Some(
                "https://huggingface.co/unsloth/gemma-3-4b-it-GGUF/resolve/main/gemma-3-4b-it-Q4_K_M.gguf",
            ),
            GgufLlmModel::Qwen3_4bInstruct2507Q4 => Some(
                "https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
            ),
        }
    }

    pub fn model_size(&self) -> u64 {
        match self {
            GgufLlmModel::Llama3p2_3bQ4 => 2019377440,
            GgufLlmModel::AnarlogLLM => 1107409056,
            GgufLlmModel::Gemma3_4bQ4 => 2489894016,
            GgufLlmModel::Qwen3_4bInstruct2507Q4 => 2497281120,
        }
    }

    pub fn model_checksum(&self) -> u32 {
        match self {
            GgufLlmModel::Llama3p2_3bQ4 => 2831308098,
            GgufLlmModel::AnarlogLLM => 4037351144,
            GgufLlmModel::Gemma3_4bQ4 => 2760830291,
            GgufLlmModel::Qwen3_4bInstruct2507Q4 => 1186510226,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            GgufLlmModel::Llama3p2_3bQ4 => "Llama 3.2 3B Q4",
            // Der Anzeigename hiess "Anarlog LLM" -- eine fremde Marke auf
            // einem Modell, das gar nicht von dort stammt. Es ist
            // `yujonglee/hypr-llm-sm` von Hugging Face. Eigene Marke waere
            // hier eine Falschbehauptung, also steht da jetzt der wahre Name.
            // Der serialisierte Wert "AnarlogLLM" bleibt (liegt in der DB).
            GgufLlmModel::AnarlogLLM => "hypr-llm-sm",
            GgufLlmModel::Gemma3_4bQ4 => "Gemma 3 4B Q4",
            GgufLlmModel::Qwen3_4bInstruct2507Q4 => "Qwen3 4B Instruct Q4",
        }
    }

    pub fn description(&self) -> String {
        let mb = self.model_size() as f64 / (1024.0 * 1024.0);
        if mb >= 1024.0 {
            format!("{:.1} GB", mb / 1024.0)
        } else {
            format!("{:.0} MB", mb)
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum LocalModelKind {
    Stt,
    Llm,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, Eq, Hash, PartialEq)]
#[serde(untagged)]
pub enum LocalModel {
    Soniqo(SoniqoModel),
    AppleSpeech(AppleSpeechModel),
    Whisper(WhisperModel),
    GgufLlm(GgufLlmModel),
}

impl std::fmt::Display for LocalModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocalModel::Soniqo(model) => write!(f, "{model}"),
            LocalModel::AppleSpeech(model) => write!(f, "{model}"),
            LocalModel::Whisper(model) => write!(f, "whisper-{model}"),
            LocalModel::GgufLlm(model) => write!(f, "llm-{model:?}"),
        }
    }
}

impl LocalModel {
    pub fn all() -> Vec<LocalModel> {
        let mut models = SoniqoModel::all()
            .iter()
            .copied()
            .map(LocalModel::Soniqo)
            .collect::<Vec<_>>();

        models.extend(
            AppleSpeechModel::all()
                .iter()
                .copied()
                .map(LocalModel::AppleSpeech),
        );

        models.extend([
            LocalModel::Whisper(WhisperModel::QuantizedTiny),
            LocalModel::Whisper(WhisperModel::QuantizedTinyEn),
            LocalModel::Whisper(WhisperModel::QuantizedBase),
            LocalModel::Whisper(WhisperModel::QuantizedBaseEn),
            LocalModel::Whisper(WhisperModel::QuantizedSmall),
            LocalModel::Whisper(WhisperModel::QuantizedSmallEn),
            LocalModel::Whisper(WhisperModel::QuantizedLargeTurbo),
        ]);

        models.extend([
            LocalModel::GgufLlm(GgufLlmModel::Llama3p2_3bQ4),
            LocalModel::GgufLlm(GgufLlmModel::Gemma3_4bQ4),
            // Zweiter auswaehlbarer Kandidat fuer den Sprachmodell-Vergleich
            // (siehe Kommentar bei GgufLlmModel::model_url()). Achtung: das
            // hartcodierte SUPPORTED_MODELS in crates/local-llm-core/src/
            // model.rs (ausserhalb dieses Scopes) zaehlt bislang exakt zwei
            // GgufLlm-Eintraege und vergleicht sich per Test gegen diese
            // Liste hier -- dieser dritte Eintrag macht jenen Test dort rot,
            // bis er nachgezogen wird.
            LocalModel::GgufLlm(GgufLlmModel::Qwen3_4bInstruct2507Q4),
        ]);

        models
    }

    pub fn kind(&self) -> &'static str {
        match self {
            LocalModel::Soniqo(_) => "stt-soniqo",
            LocalModel::AppleSpeech(_) => "stt-apple-speech",
            LocalModel::Whisper(_) => "stt-whisper",
            LocalModel::GgufLlm(_) => "llm",
        }
    }

    pub fn model_kind(&self) -> LocalModelKind {
        match self {
            LocalModel::Soniqo(_) | LocalModel::AppleSpeech(_) | LocalModel::Whisper(_) => {
                LocalModelKind::Stt
            }
            LocalModel::GgufLlm(_) => LocalModelKind::Llm,
        }
    }

    pub fn cli_name(&self) -> &'static str {
        match self {
            LocalModel::Soniqo(model) => model.as_str(),
            LocalModel::AppleSpeech(model) => model.as_str(),
            LocalModel::Whisper(WhisperModel::QuantizedTiny) => "whisper-tiny",
            LocalModel::Whisper(WhisperModel::QuantizedTinyEn) => "whisper-tiny-en",
            LocalModel::Whisper(WhisperModel::QuantizedBase) => "whisper-base",
            LocalModel::Whisper(WhisperModel::QuantizedBaseEn) => "whisper-base-en",
            LocalModel::Whisper(WhisperModel::QuantizedSmall) => "whisper-small",
            LocalModel::Whisper(WhisperModel::QuantizedSmallEn) => "whisper-small-en",
            LocalModel::Whisper(WhisperModel::QuantizedLargeTurbo) => "whisper-large-turbo",
            LocalModel::GgufLlm(GgufLlmModel::Llama3p2_3bQ4) => "llm-llama3-2-3b-q4",
            LocalModel::GgufLlm(GgufLlmModel::AnarlogLLM) => "llm-hypr-llm",
            LocalModel::GgufLlm(GgufLlmModel::Gemma3_4bQ4) => "llm-gemma3-4b-q4",
            LocalModel::GgufLlm(GgufLlmModel::Qwen3_4bInstruct2507Q4) => "llm-qwen3-4b-instruct-q4",
        }
    }

    pub fn install_path(&self, models_base: &Path) -> PathBuf {
        match self {
            LocalModel::Soniqo(model) => models_base.join("soniqo").join(model.as_str()),
            LocalModel::AppleSpeech(model) => models_base.join("apple-speech").join(model.as_str()),
            LocalModel::Whisper(model) => models_base.join("stt").join(model.file_name()),
            LocalModel::GgufLlm(model) => models_base.join("llm").join(model.file_name()),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            LocalModel::Soniqo(model) => model.display_name().to_string(),
            LocalModel::AppleSpeech(model) => model.display_name().to_string(),
            LocalModel::Whisper(model) => model.display_name().to_string(),
            LocalModel::GgufLlm(model) => model.display_name().to_string(),
        }
    }

    pub fn description(&self) -> String {
        match self {
            LocalModel::Soniqo(model) => model.description().to_string(),
            LocalModel::AppleSpeech(model) => model.description().to_string(),
            LocalModel::Whisper(model) => model.description(),
            LocalModel::GgufLlm(model) => model.description(),
        }
    }

    pub fn is_available_on_current_platform(&self) -> bool {
        let is_apple_silicon = cfg!(target_arch = "aarch64") && cfg!(target_os = "macos");

        match self {
            LocalModel::Soniqo(model) => model.is_available_on_current_platform(),
            LocalModel::AppleSpeech(model) => model.is_available_on_current_platform(),
            LocalModel::Whisper(_) => is_apple_silicon,
            LocalModel::GgufLlm(_) => cfg!(target_arch = "aarch64"),
        }
    }
}

impl DownloadableModel for GgufLlmModel {
    fn download_key(&self) -> String {
        format!("llm:{}", self.file_name())
    }

    fn download_url(&self) -> Option<String> {
        self.model_url().map(str::to_string)
    }

    fn download_checksum(&self) -> Option<u32> {
        Some(self.model_checksum())
    }

    fn download_destination(&self, models_base: &Path) -> PathBuf {
        models_base.join("llm").join(self.file_name())
    }

    fn is_downloaded(&self, models_base: &Path) -> Result<bool, Error> {
        let path = models_base.join("llm").join(self.file_name());
        if !path.exists() {
            return Ok(false);
        }

        let actual =
            anlg_file::file_size(&path).map_err(|e| Error::OperationFailed(e.to_string()))?;
        Ok(actual == self.model_size())
    }

    fn finalize_download(&self, _downloaded_path: &Path, _models_base: &Path) -> Result<(), Error> {
        Ok(())
    }

    fn delete_downloaded(&self, models_base: &Path) -> Result<(), Error> {
        let path = models_base.join("llm").join(self.file_name());
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| Error::DeleteFailed(e.to_string()))?;
        }
        Ok(())
    }
}

impl DownloadableModel for LocalModel {
    fn download_key(&self) -> String {
        match self {
            LocalModel::Soniqo(model) => format!("soniqo:{}", model.as_str()),
            LocalModel::AppleSpeech(model) => format!("apple-speech:{}", model.as_str()),
            LocalModel::Whisper(model) => format!("whisper:{}", model.file_name()),
            LocalModel::GgufLlm(model) => model.download_key(),
        }
    }

    fn download_url(&self) -> Option<String> {
        match self {
            LocalModel::Soniqo(_) | LocalModel::AppleSpeech(_) => None,
            LocalModel::Whisper(model) => Some(model.model_url().to_string()),
            LocalModel::GgufLlm(model) => model.download_url(),
        }
    }

    fn download_checksum(&self) -> Option<u32> {
        match self {
            LocalModel::Soniqo(_) | LocalModel::AppleSpeech(_) => None,
            LocalModel::Whisper(model) => Some(model.checksum()),
            LocalModel::GgufLlm(model) => model.download_checksum(),
        }
    }

    fn download_destination(&self, models_base: &Path) -> PathBuf {
        match self {
            LocalModel::Soniqo(model) => models_base.join("soniqo").join(model.as_str()),
            LocalModel::AppleSpeech(model) => models_base.join("apple-speech").join(model.as_str()),
            LocalModel::Whisper(model) => models_base.join("stt").join(model.file_name()),
            LocalModel::GgufLlm(model) => model.download_destination(models_base),
        }
    }

    fn is_downloaded(&self, models_base: &Path) -> Result<bool, Error> {
        match self {
            LocalModel::Soniqo(model) => anlg_transcribe_soniqo::is_model_downloaded(*model)
                .map_err(|e| Error::OperationFailed(e.to_string())),
            LocalModel::AppleSpeech(_) => match anlg_transcribe_speechanalyzer::settings_locale() {
                Ok(locale) => anlg_transcribe_speechanalyzer::is_model_downloaded(&locale)
                    .map_err(|e| Error::OperationFailed(e.to_string())),
                Err(anlg_transcribe_speechanalyzer::Error::NoSupportedSystemLanguage)
                | Err(anlg_transcribe_speechanalyzer::Error::UnsupportedPlatform) => Ok(false),
                Err(error) => Err(Error::OperationFailed(error.to_string())),
            },
            LocalModel::Whisper(model) => {
                Ok(models_base.join("stt").join(model.file_name()).exists())
            }
            LocalModel::GgufLlm(model) => model.is_downloaded(models_base),
        }
    }

    fn finalize_download(&self, downloaded_path: &Path, models_base: &Path) -> Result<(), Error> {
        match self {
            LocalModel::Soniqo(_) => Err(Error::FinalizeFailed(
                "Soniqo models are downloaded through the Soniqo bridge".to_string(),
            )),
            LocalModel::AppleSpeech(_) => Err(Error::FinalizeFailed(
                "Apple Speech assets are installed by macOS".to_string(),
            )),
            LocalModel::Whisper(_) => Ok(()),
            LocalModel::GgufLlm(model) => model.finalize_download(downloaded_path, models_base),
        }
    }

    fn delete_downloaded(&self, models_base: &Path) -> Result<(), Error> {
        match self {
            LocalModel::Soniqo(model) => anlg_transcribe_soniqo::delete_model(*model)
                .map_err(|e| Error::DeleteFailed(e.to_string())),
            // Only the reservation is ours to give back; macOS owns the asset files.
            LocalModel::AppleSpeech(_) => {
                let locale = anlg_transcribe_speechanalyzer::settings_locale()
                    .map_err(|e| Error::DeleteFailed(e.to_string()))?;
                anlg_transcribe_speechanalyzer::release_locale(&locale)
                    .map_err(|e| Error::DeleteFailed(e.to_string()))
            }
            LocalModel::Whisper(model) => {
                let model_path = models_base.join("stt").join(model.file_name());
                if model_path.exists() {
                    std::fs::remove_file(&model_path)
                        .map_err(|e| Error::DeleteFailed(e.to_string()))?;
                }
                Ok(())
            }
            LocalModel::GgufLlm(model) => model.delete_downloaded(models_base),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anarlog_llm_accepts_legacy_serialized_name() {
        assert_eq!(
            serde_json::from_str::<GgufLlmModel>("\"HyprLLM\"").unwrap(),
            GgufLlmModel::AnarlogLLM,
        );
        assert_eq!(
            serde_json::to_string(&GgufLlmModel::AnarlogLLM).unwrap(),
            "\"AnarlogLLM\"",
        );
    }

    // Wache gegen ein stilles Zurueckfallen auf den fremden Spiegel: jedes
    // angebotene GGUF-Modell laedt von Hugging Face, keines von einer
    // S3-Adresse. Bis zum 02.09.2026 nannte dieser Test hypr-llm-sm als
    // erlaubte Ausnahme; die ist mit ZICK-263 aufgeloest.
    #[test]
    fn gguf_model_urls_come_from_the_source() {
        let offered: Vec<GgufLlmModel> = LocalModel::all()
            .into_iter()
            .filter_map(|model| match model {
                LocalModel::GgufLlm(model) => Some(model),
                _ => None,
            })
            .collect();
        assert_eq!(
            offered.len(),
            3,
            "Modellliste hat sich veraendert: {offered:?}"
        );

        for model in offered {
            let url = model.model_url().expect("angebotenes Modell ohne Adresse");
            assert!(
                url.starts_with("https://huggingface.co/"),
                "{model:?} laedt wieder ueber fremde Infrastruktur: {url}"
            );
            assert!(
                !url.contains("amazonaws"),
                "{model:?} zeigt wieder auf einen S3-Speicher: {url}"
            );
        }
    }

    // ZICK-263 / ISA N7 (02.09.2026): das Modell des Ursprungsprojekts wird
    // nicht mehr angeboten und laedt nie mehr ueber dessen S3-Speicher. Der
    // serialisierte Schluessel bleibt lesbar (Test darueber), mehr nicht.
    #[test]
    fn the_upstream_model_is_no_longer_offered_and_never_downloads() {
        assert!(
            !LocalModel::all().contains(&LocalModel::GgufLlm(GgufLlmModel::AnarlogLLM)),
            "hypr-llm-sm steht noch in der Modellliste"
        );
        assert_eq!(GgufLlmModel::AnarlogLLM.model_url(), None);
        assert_eq!(
            GgufLlmModel::AnarlogLLM.download_url(),
            None,
            "hypr-llm-sm haette noch eine Download-Adresse"
        );
        assert_eq!(
            LocalModel::GgufLlm(GgufLlmModel::AnarlogLLM).download_url(),
            None
        );
    }

    #[test]
    fn soniqo_models_reject_generic_download_finalize() {
        let model = LocalModel::Soniqo(SoniqoModel::ParakeetStreaming);

        let error = model
            .finalize_download(Path::new("download"), Path::new("models"))
            .unwrap_err();

        assert!(error.to_string().contains("Soniqo bridge"));
    }
}
