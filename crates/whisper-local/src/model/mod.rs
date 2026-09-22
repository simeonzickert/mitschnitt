#[cfg(feature = "actual")]
mod actual;
#[cfg(feature = "actual")]
pub use actual::*;

#[cfg(not(feature = "actual"))]
mod mock;
#[cfg(not(feature = "actual"))]
pub use mock::*;

/// Fork-Abweichung (31.08.2026): Whisper wird als Batch-Anbieter nutzbar gemacht.
///
/// Der Dekoder laeuft mit ZWEI Parametersaetzen, nicht mit einem gemeinsamen.
/// `Live` ist der Bestand und bleibt unangetastet: Zeitmarken aus, ein Segment
/// je Aufruf, `[_BEG_]` unterdrueckt. Das ist fuer den Live-Betrieb bewusst so
/// (weniger Latenz, kein Segmentieren mitten im Strom) und waere in einem
/// gemeinsamen Satz ein Rueckschritt.
///
/// `Batch` dreht genau DREI Schalter um: `no_timestamps` aus, `single_segment`
/// aus, und -- der, den man leicht uebersieht -- der Logits-Filter auf
/// `[_BEG_]` faellt weg. Solange dieses Token auf -inf liegt, kann das Modell
/// ueberhaupt kein Zeitmarken-Token erzeugen; `no_timestamps(false)` allein
/// bliebe wirkungslos.
///
/// `token_timestamps` bleibt in BEIDEN Modi aus, mit Begruendung an der
/// Fundstelle in `actual.rs`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TranscribeMode {
    #[default]
    Live,
    Batch,
}

impl TranscribeMode {
    pub fn is_batch(self) -> bool {
        matches!(self, Self::Batch)
    }
}

#[derive(Debug, Default)]
pub struct Segment {
    pub text: String,
    pub language: Option<String>,
    pub start: f64,
    pub end: f64,
    pub confidence: f32,
    pub meta: Option<serde_json::Value>,
}

impl Segment {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    pub fn start(&self) -> f64 {
        self.start
    }

    pub fn end(&self) -> f64 {
        self.end
    }

    pub fn duration(&self) -> f64 {
        self.end - self.start
    }

    pub fn confidence(&self) -> f32 {
        self.confidence
    }

    pub fn meta(&self) -> Option<serde_json::Value> {
        self.meta.clone()
    }
}
