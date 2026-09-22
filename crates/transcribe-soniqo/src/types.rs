use owhisper_interface::stream;

use crate::{SoniqoModel, stream_response_from_text};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadState {
    pub status: String,
    pub current_file: Option<String>,
    pub progress_percent: Option<u8>,
    pub local_path: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTranscript {
    pub text: String,
    pub duration_seconds: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunks: Vec<FileTranscriptChunk>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speaker_segments: Vec<DiarizationSegment>,
    /// Fork (02.09.2026): GEMESSENE Wortzeiten des Erkenners, relativ zum
    /// Anfang der uebergebenen Datei. Leer heisst: dieses Modell liefert keine
    /// (oder die Frame-Liste passte nicht zur Token-Liste) -- dann bleibt es
    /// bei der Gleichverteilung ueber den Abschnitt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<TranscriptWord>,
    /// Fork (07.09.2026): hier war eine Sprechertrennung geplant und wurde
    /// wegen der Kanallaenge gestrichen. Der Wert ist der DECKEL in Sekunden,
    /// nicht die Laenge der Aufnahme -- er reist mit, damit die Meldung an den
    /// Nutzer die Grenze nennen kann, ohne sie ein zweites Mal aufzuschreiben.
    ///
    /// Der Unterschied zu "es war nie eine geplant" ist der ganze Zweck des
    /// Feldes: nur dieser Fall ist eine Nachricht an den Nutzer. Vorher stand
    /// das Auslassen ausschliesslich im Protokoll, und im Transkript sah es
    /// aus wie ein schlechtes Ergebnis statt wie ein ausgelassener Schritt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diarization_skipped_over_seconds: Option<f64>,

    /// Fork (21.09.2026): hier war eine Sprechertrennung geplant, sie lief,
    /// und sie schlug fehl.
    ///
    /// Der Unterschied zu `diarization_skipped_over_seconds` ist die
    /// Nachricht: dort wurde ein Schritt bewusst ausgelassen, hier ist er
    /// gescheitert. Ohne dieses Feld waren beide Faelle im Transkript von
    /// einem guten Ergebnis ohne erkannte Sprecher nicht zu unterscheiden --
    /// der Grund stand nur im Protokoll, das niemand aufmacht.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub diarization_failed: bool,
}

impl FileTranscript {
    pub fn new(text: String, duration_seconds: f64) -> Self {
        Self {
            text,
            duration_seconds,
            chunks: Vec::new(),
            speaker_segments: Vec::new(),
            words: Vec::new(),
            diarization_skipped_over_seconds: None,
            diarization_failed: false,
        }
    }

    pub fn from_chunks(chunks: Vec<FileTranscriptChunk>, duration_seconds: f64) -> Self {
        let text = chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        Self {
            text,
            duration_seconds,
            chunks,
            speaker_segments: Vec::new(),
            words: Vec::new(),
            diarization_skipped_over_seconds: None,
            diarization_failed: false,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTranscriptChunk {
    pub text: String,
    pub start_seconds: f64,
    pub duration_seconds: f64,
    /// Fork: gemessene Wortzeiten dieses Abschnitts auf der ZEITACHSE DER
    /// AUFNAHME. Leer = wie bisher gleichverteilen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<TranscriptWord>,
}

/// Ein Wort mit gemessener Zeit. Die Zeitachse steht nicht im Typ -- sie ergibt
/// sich aus dem Ort: in [`FileTranscript`] relativ zur uebergebenen Datei, in
/// [`FileTranscriptChunk`] absolut zur Aufnahme.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptWord {
    pub text: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    /// Fork (03.09.2026): die GEMESSENE Sicherheit des Erkenners fuer dieses
    /// Wort, `exp(Mittel der Log-Wahrscheinlichkeiten seiner Tokens)` auf 0..1
    /// gedeckelt. `None` heisst: dieses Modell liefert keine -- dann bleibt es
    /// bei der bisherigen 1,0. Eine erfundene Eins ist schlimmer als ein leeres
    /// Feld, weil sie aussieht wie eine Messung; deshalb ist das Feld optional
    /// und nicht per Vorgabewert vollgeschrieben.
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiarizationSegment {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub speaker_index: usize,
}

/// Ein vollstaendiger Trennungslauf: Abschnitte UND die Stimmen dahinter.
///
/// Die Schwerpunkte (`speaker_embeddings`) sind der einzige Weg, zwei
/// getrennte Laeufe ueber dieselbe Person zu verbinden -- jeder Lauf
/// nummeriert seine Sprecher bei null beginnend neu, die Nummer allein sagt
/// also nichts. Ein Eintrag je Sprecher, in der Reihenfolge der
/// `speaker_index`-Werte.
///
/// Kann leer sein: eine aeltere Bruecke liefert den Schluessel nicht. Wer die
/// Schwerpunkte braucht, muss diesen Fall behandeln, statt eine leere Liste
/// fuer "keine Sprecher" zu halten.
#[derive(Debug, Clone, PartialEq)]
pub struct Diarization {
    pub segments: Vec<DiarizationSegment>,
    pub speaker_embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptSource {
    Microphone,
    System,
}

impl TranscriptSource {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::System => "system",
        }
    }

    pub const fn channel_index(self) -> i32 {
        match self {
            Self::Microphone => 0,
            Self::System => 1,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePartial {
    pub source: String,
    pub text: String,
    pub is_final: bool,
}

impl LivePartial {
    pub fn source(&self) -> TranscriptSource {
        match self.source.as_str() {
            "system" => TranscriptSource::System,
            _ => TranscriptSource::Microphone,
        }
    }

    pub fn into_stream_response(
        self,
        model: SoniqoModel,
        start: f64,
        duration: f64,
    ) -> stream::StreamResponse {
        let source = self.source();
        stream_response_from_text(
            model,
            self.text,
            start,
            duration,
            self.is_final,
            &[source.channel_index(), 2],
        )
    }
}
