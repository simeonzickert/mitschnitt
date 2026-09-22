#[cfg(feature = "whisper-cpp")]
pub mod internal;
pub mod supervisor;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type,
)]
// Nur noch ein Wert. `External` war der Argmax-Sidecar und ist am 01.09.2026
// ersatzlos entfallen; der Typ bleibt, weil er in den TS-Bindings und im
// `get_servers`-Ergebnis steht.
pub enum ServerType {
    #[serde(rename = "internal")]
    Internal,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type,
)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Unreachable,
    Loading,
    Ready,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ServerInfo {
    pub url: Option<String>,
    pub status: ServerStatus,
    pub model: Option<crate::LocalModel>,
    pub custom_model_path: Option<String>,
}
