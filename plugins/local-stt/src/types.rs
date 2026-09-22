#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CustomSttModelFormat {
    Ggml,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomSttModelInfo {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub format: CustomSttModelFormat,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressPayload {
    pub model: crate::LocalModel,
    pub status: anlg_model_downloader::DownloadStatus,
}

#[derive(Debug)]
pub struct Connection {
    pub model: Option<String>,
    pub base_url: String,
    pub api_key: Option<String>,
}

/// Eine eingetragene Verhoerung, die der Nachlauf nicht anwendet, weil sie
/// selbst gewoehnliches Deutsch ist.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RiskyAlias {
    pub canonical: String,
    pub alias: String,
}

/// Ein Gespraech, wie es in den Vorschlags-Lauf geht.
///
/// Die Oberflaeche liest die Woerter und den Kontext aus der Datenbank und
/// reicht sie herein -- das Plugin spricht bewusst nicht selbst mit der
/// Datenbank, genauso wie `vocabulary_risky_aliases` seine Liste bekommt statt
/// sie zu holen.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposalScanSession {
    pub session_id: String,
    pub words: Vec<ProposalScanWord>,
    /// Teilnehmernamen, ganz ("Mads Verlin | nordwerk") -- aus der Sitzung
    /// UND aus dem Kalendertermin. NUR Namen.
    pub context_names: Vec<String>,
    /// Die Mailadressen der Teilnehmer, getrennt gefuehrt: sie liefern
    /// ausschliesslich Immunitaet und nie ein Ersetzungsziel. Siehe
    /// `anlg_vocabulary::ScanSession::context_emails`.
    pub context_emails: Vec<String>,
    /// Der von Menschen geschriebene Kontext: Kalendertitel,
    /// Kalenderbeschreibung, Notiz.
    pub context_text: String,
    /// Der vom Modell erzeugte Sitzungstitel. Wird ausdruecklich NICHT als
    /// Beleg genommen -- siehe `anlg_vocabulary::ScanSession`.
    pub generated_title: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposalScanWord {
    pub text: String,
    /// Die GEMESSENE Wortsicherheit -- `null` heisst "nicht gemessen", nicht
    /// "sicher".
    pub measured_confidence: Option<f64>,
}

/// Ein Paar, das ein Mensch schon einmal verworfen hat.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DismissedProposal {
    pub canonical: String,
    pub alias: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VocabularyProposalKind {
    /// Wiederkehr mit Schwankung ueber mehrere Gespraeche.
    Recurrence,
    /// Klingt wie ein Name aus dem Kontext des Gespraechs.
    Context,
}

/// Ein Vorschlag -- KEINE Ersetzung. Erst wenn ein Mensch ihn annimmt, wird
/// daraus ein Woerterbuch-Alias.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyProposal {
    pub kind: VocabularyProposalKind,
    pub canonical: String,
    pub alias: String,
    pub occurrences: usize,
    pub session_ids: Vec<String>,
    pub evidence: String,
    pub source: String,
}
