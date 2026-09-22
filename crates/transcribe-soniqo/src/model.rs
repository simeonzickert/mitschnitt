use std::str::FromStr;

use crate::{Error, Result};

pub const LOCAL_BASE_URL: &str = "soniqo://local";

/// Major version of the running macOS, read once from the system version plist.
///
/// `None` when it cannot be determined (non-macOS, or an unreadable/!unexpected plist).
/// Reading the plist directly avoids the `sw_vers` compatibility shim that reports
/// `10.16` to processes linked against older SDKs.
#[cfg(target_os = "macos")]
fn macos_major_version() -> Option<u32> {
    static CACHED: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();

    *CACHED.get_or_init(|| {
        let plist =
            std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist").ok()?;
        parse_product_major_version(&plist)
    })
}

#[cfg(not(target_os = "macos"))]
fn macos_major_version() -> Option<u32> {
    None
}

/// Extracts the major number of `ProductVersion` from a SystemVersion plist.
/// Split out from the file read so it can be tested without touching the host.
pub(crate) fn parse_product_major_version(plist: &str) -> Option<u32> {
    let after_key = plist.split("<key>ProductVersion</key>").nth(1)?;
    let value = after_key
        .split("<string>")
        .nth(1)?
        .split("</string>")
        .next()?;

    value.trim().split('.').next()?.parse().ok()
}

/// Whether the running macOS is at least `major`. Unknown versions are treated as
/// too old: refusing a model that would work is recoverable, offering one that
/// cannot load is not.
pub(crate) fn macos_version_at_least(major: u32) -> bool {
    macos_major_version().is_some_and(|version| version >= major)
}

pub fn is_local_base_url(base_url: &str) -> bool {
    base_url.trim_end_matches('/') == LOCAL_BASE_URL
}

pub fn is_loopback_http_base_url(base_url: &str) -> bool {
    let Some(rest) = base_url
        .trim()
        .strip_prefix("http://")
        .or_else(|| base_url.trim().strip_prefix("https://"))
    else {
        return false;
    };

    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();

    let host = authority
        .strip_prefix('[')
        .and_then(|value| value.split(']').next())
        .unwrap_or_else(|| authority.split(':').next().unwrap_or_default());

    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|addr| addr.is_loopback())
}

pub fn local_model_from_request(base_url: &str, model: &str) -> Option<SoniqoModel> {
    let model = model.parse().ok()?;

    if is_local_base_url(base_url) || is_loopback_http_base_url(base_url) {
        Some(model)
    } else {
        None
    }
}

#[derive(
    Debug, Clone, Copy, serde::Serialize, serde::Deserialize, specta::Type, Eq, Hash, PartialEq,
)]
pub enum SoniqoModel {
    #[serde(rename = "soniqo-parakeet-streaming")]
    ParakeetStreaming,
    #[serde(rename = "soniqo-parakeet-batch")]
    ParakeetBatch,
    #[serde(rename = "soniqo-omnilingual")]
    Omnilingual,
    #[serde(rename = "soniqo-qwen3-small")]
    Qwen3Small,
    #[serde(rename = "soniqo-qwen3-large")]
    Qwen3Large,
}

impl SoniqoModel {
    const ALL: &'static [Self] = &[
        Self::ParakeetStreaming,
        Self::ParakeetBatch,
        Self::Omnilingual,
        Self::Qwen3Small,
        Self::Qwen3Large,
    ];

    const KNOWN: &'static [Self] = &[
        Self::ParakeetStreaming,
        Self::ParakeetBatch,
        Self::Omnilingual,
        Self::Qwen3Small,
        Self::Qwen3Large,
    ];

    // Entscheid 31.08.2026 nach einem Vergleich an echtem Material
    // (20-Minuten-Teammeeting, dieselbe Aufnahme dreimal transkribiert):
    // Qwen3 Large findet zwar mehr von seinen kurzen Einwuerfen (9 Sprech-
    // abschnitte gegen 1), trifft aber die Eigennamen schlechter (der Selbstname 1x
    // gegen 4x, Bo 1x gegen 2x) und liest sich rauher ("auch auch", "mal
    // mal"). Parakeet ist zudem schneller und sparsamer. Der Benchmark-Vorteil
    // von Qwen3 auf FLEURS (6,81 % gegen 12,33 % Fehlerrate) hat sich auf
    // Konferenzton NICHT bestaetigt -- ein Beleg dafuer, dass vorgelesene
    // Testsaetze und ein echtes Teammeeting verschiedene Dinge messen.
    //
    // Die Freischaltung bleibt vollstaendig im Code (Laufzeit-Versionspruefung,
    // Swift-Ladepfad, Build-Ziel 15.0) -- nur die Auswahl ist zurueckgenommen.
    // Wer Qwen3 wieder anbieten will, ergaenzt hier zwei Zeilen; es ist keine
    // Wiederholung der vier Sperren noetig.
    const SELECTABLE: &'static [Self] = &[Self::ParakeetStreaming, Self::ParakeetBatch];

    pub const fn all() -> &'static [Self] {
        Self::ALL
    }

    pub const fn selectable() -> &'static [Self] {
        Self::SELECTABLE
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ParakeetStreaming => "soniqo-parakeet-streaming",
            Self::ParakeetBatch => "soniqo-parakeet-batch",
            Self::Omnilingual => "soniqo-omnilingual",
            Self::Qwen3Small => "soniqo-qwen3-small",
            Self::Qwen3Large => "soniqo-qwen3-large",
        }
    }

    // Herkunft und Lizenz dieser fuenf Adressen, gemessen am 01.09.2026 gegen die
    // Hugging-Face-API. Anlass war des Betreibers Auftrag "Argmax raus und parakeet batch und
    // live bitte umbauen" -- der Umbau auf eine freie Quelle wurde GEPRUEFT und
    // ABGELEHNT, die Gruende stehen hier, damit sie niemand zweimal herleiten muss.
    //
    // `aufklarer` ist NICHT ein fremder Spiegel wie anarlogs S3-Speicher, sondern die
    // Modell-Heimat der Bibliothek, die sie laedt: `soniqo/speech-swift` (Apache-2.0),
    // eingebunden in `swift-lib/Package.swift`. Der Download passiert dort, nicht hier
    // -- diese Zeichenketten reichen wir nur durch, die zweite Fassung derselben Werte
    // steht in `swift-lib/src/lib.swift`. Wer hier etwas aendert, muss dort mitziehen.
    //
    // Lizenzstand:
    //   Parakeet-EOU-120M-CoreML-INT8 ........ NVIDIA Open Model License
    //                                          (Basis nvidia/parakeet_realtime_eou_120m-v1)
    //   Omnilingual-ASR-CTC-300M-...-10s ..... Apache-2.0 (Basis facebook/omniASR-CTC-300M)
    //   Pyannote-Community-1-CoreML .......... CC-BY-4.0, mit LICENSE-Datei
    //     (der Sprechertrenner, den `ParakeetBatch` zusaetzlich laedt -- er steht nicht
    //      in dieser Liste, weil speech-swift ihn selbst als Standard mitbringt)
    //   Parakeet-TDT-v3-CoreML-INT8-30s ...... KEINE Lizenzangabe, kein README.
    //
    // Der letzte Punkt ist die einzige offene Flanke, und sie ist kleiner, als sie
    // aussieht: die beiden Geschwister-Fassungen derselben Umwandlung,
    // `aufklarer/Parakeet-TDT-v3-CoreML-INT8` und `...-iOS-5s`, tragen beide CC-BY-4.0
    // und beschreiben dieselbe Architektur (FastConformer, 24 Schichten, 1024 hidden --
    // exakt die Werte in der config.json des 30s-Repos). Der 30-Sekunden-Variante fehlt
    // die Modellkarte, nicht die Lizenz. Das ist etwas anderes als Argmax, wo eine
    // Lizenz DA war und Weitergabe ausdruecklich untersagte.
    //
    // WARUM NICHT AUF `FluidInference/parakeet-tdt-0.6b-v3-coreml` (CC-BY-4.0)
    // UMGESTELLT WURDE -- gemessen, nicht vermutet. Der Lader in speech-swift
    // (`ParakeetASRModel.fromPretrained`) erwartet `config.json`, `vocab.json` und die
    // drei Verzeichnisse `encoder/decoder/joint.mlmodelc`. Das FluidInference-Repo hat
    // keines davon so: es liefert `Encoder.mlmodelc`, `ParakeetEncoder_15s.mlmodelc`,
    // `Melspectrogram_15s.mlmodelc`, `JointDecisionv3.mlmodelc` und
    // `parakeet_v3_vocab.json`. Entscheidend ist aber nicht der Dateiname, sondern die
    // Signatur des Rechengraphen. Aus den beiden `model.mil` abgelesen:
    //
    //   aufklarer:       func main<ios17>(tensor<int32,[1]> length,
    //                                     tensor<fp32,[1,128,3000]> mel)
    //   FluidInference:  func main<ios17>(tensor<fp32,[1,128,1501]> mel,
    //                                     tensor<int32,[1]> mel_length)
    //
    // Andere Namen, andere Reihenfolge, und vor allem ein anderes Fenster: 3000
    // Mel-Rahmen sind 30 Sekunden, 1501 sind 15. Der Batch-Weg schiebt Bloecke von bis
    // zu 29,5 Sekunden hinein (`parakeetBatchMaximumChunkSeconds`). Das FluidInference-
    // Modell wuerde sie nicht annehmen. Ein Tausch hiesse also, den fremden Lader
    // umzuschreiben -- ausdruecklich nicht der Auftrag.
    //
    // Fuer den Live-Weg gilt dasselbe schaerfer: `FluidInference/parakeet-realtime-eou-
    // 120m-coreml` legt seine Gewichte in Unterordner je Blockdauer (160ms/320ms/1280ms),
    // waehrend der Lader eine flache Ablage erwartet. Und das aufklarer-EOU-Modell hat
    // ohnehin eine ausgewiesene Lizenz -- es gibt hier gar nichts zu heilen.
    //
    // Pruefsummen wurden NICHT angepasst, weil es auf diesem Weg keine gibt: der
    // Download laeuft ueber `HuggingFaceDownloader` in speech-swift, und geprueft wird
    // nur, ob die erwarteten Dateien da sind (`filesReady()` in lib.swift). Das ist
    // schwaecher als der CRC32-Weg der gguf-Modelle in `crates/local-model` und waere
    // ein eigenes Thema.
    pub const fn repo(self) -> &'static str {
        match self {
            Self::ParakeetStreaming => "aufklarer/Parakeet-EOU-120M-CoreML-INT8",
            Self::ParakeetBatch => "aufklarer/Parakeet-TDT-v3-CoreML-INT8-30s",
            Self::Omnilingual => "aufklarer/Omnilingual-ASR-CTC-300M-CoreML-INT8-10s",
            Self::Qwen3Small => "aufklarer/Qwen3-ASR-0.6B-MLX-4bit",
            Self::Qwen3Large => "aufklarer/Qwen3-ASR-1.7B-MLX-8bit",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::ParakeetStreaming => "Parakeet Streaming",
            Self::ParakeetBatch => "Parakeet Batch",
            Self::Omnilingual => "Omnilingual ASR",
            Self::Qwen3Small => "Qwen3 ASR 0.6B",
            Self::Qwen3Large => "Qwen3 ASR 1.7B",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::ParakeetStreaming => "Realtime transcription for 25 European languages.",
            Self::ParakeetBatch => {
                "Batch transcription with on-device speaker labels for 25 European languages."
            }
            Self::Omnilingual => "Multilingual batch transcription.",
            Self::Qwen3Small => "Multilingual batch transcription.",
            Self::Qwen3Large => "Multilingual batch transcription.",
        }
    }

    pub const fn size_bytes(self) -> u64 {
        match self {
            Self::ParakeetStreaming => 120 * 1024 * 1024,
            Self::ParakeetBatch => 632 * 1024 * 1024,
            Self::Omnilingual => 300 * 1024 * 1024,
            Self::Qwen3Small => 600 * 1024 * 1024,
            Self::Qwen3Large => 1_700 * 1024 * 1024,
        }
    }

    pub const fn supports_live(self) -> bool {
        matches!(self, Self::ParakeetStreaming)
    }

    /// Runtime availability. Deliberately NOT `const`: the macOS version can only be
    /// read at runtime, and the previous `const fn` hard-coded `false` for every model
    /// that requires macOS 15 — it never asked the OS. Mirrors the runtime shape of
    /// `AppleSpeechModel::is_available_on_current_platform` in `transcribe-speechanalyzer`.
    pub fn is_available_on_current_platform(self) -> bool {
        cfg!(all(target_os = "macos", target_arch = "aarch64"))
            && (!self.requires_macos_15() || macos_version_at_least(15))
    }

    /// Pure model property, so it stays `const`.
    pub(crate) const fn requires_macos_15(self) -> bool {
        matches!(self, Self::Qwen3Small | Self::Qwen3Large)
    }

    pub fn supports_live_on_current_platform(self) -> bool {
        self.supports_live() && self.is_available_on_current_platform()
    }

    pub const fn batch_model(self) -> Self {
        match self {
            Self::ParakeetStreaming => Self::ParakeetBatch,
            model => model,
        }
    }

    pub fn supports_language(self, language: &anlg_language::Language) -> bool {
        match self {
            Self::ParakeetStreaming | Self::ParakeetBatch => {
                anlg_language::is_parakeet_tdt_v3_language(language)
            }
            Self::Omnilingual | Self::Qwen3Small | Self::Qwen3Large => true,
        }
    }

    pub fn supports_languages(self, languages: &[anlg_language::Language]) -> bool {
        languages
            .iter()
            .all(|language| self.supports_language(language))
    }

    fn matches_identifier(self, value: &str) -> bool {
        value == self.as_str()
            || value == self.repo()
            || matches!(
                (self, value),
                (Self::ParakeetBatch, "aufklarer/Parakeet-TDT-v3-CoreML-INT8")
            )
    }
}

impl std::fmt::Display for SoniqoModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SoniqoModel {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::KNOWN
            .iter()
            .copied()
            .find(|model| model.matches_identifier(value))
            .ok_or_else(|| Error::UnsupportedModel(value.to_string()))
    }
}
