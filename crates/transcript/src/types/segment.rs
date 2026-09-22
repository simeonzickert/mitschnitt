#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type,
)]
#[repr(i32)]
pub enum ChannelProfile {
    DirectMic = 0,
    RemoteParty = 1,
    MixedCapture = 2,
}

impl From<i32> for ChannelProfile {
    fn from(value: i32) -> Self {
        match value {
            0 => ChannelProfile::DirectMic,
            1 => ChannelProfile::RemoteParty,
            2 => ChannelProfile::MixedCapture,
            _ => ChannelProfile::MixedCapture,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct SegmentKey {
    pub channel: ChannelProfile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_human_id: Option<String>,
}

impl SegmentKey {
    pub fn has_speaker_identity(&self) -> bool {
        self.speaker_index.is_some() || self.speaker_human_id.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct SegmentWord {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub channel: ChannelProfile,
    pub is_final: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct Segment {
    pub key: SegmentKey,
    pub words: Vec<SegmentWord>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct SegmentBuilderOptions {
    pub max_gap_ms: Option<i64>,
    pub complete_channels: Option<Vec<ChannelProfile>>,
    /// Kanaele, auf denen die Sprechertrennung MEHR ALS EINEN Sprecher gefunden
    /// hat (Fork, 08.09.2026).
    ///
    /// `None` heisst "rechne es dir selbst aus den uebergebenen Woertern aus".
    /// Das genuegt, solange alle Woerter EINES Aufrufs die ganze Sitzung sind --
    /// und genau das ist beim Rendern NICHT der Fall: `render_transcript_segments`
    /// ruft `build_segments` je Transkript-Zeile. Eine Sitzung aus zwei Zeilen,
    /// deren Zeilen je einen Sprecher tragen, saehe pro Aufruf "ein Sprecher"
    /// und bekaeme zweimal den Selbst-Menschen aufgedrueckt -- zwei
    /// verschiedene Menschen unter einem Namen, derselbe Fehler eine Ebene
    /// hoeher. Wer ueber mehrere Zeilen rendert, gibt die Menge deshalb vor.
    pub multi_speaker_channels: Option<Vec<ChannelProfile>>,
    /// Obergrenze fuer einen einzelnen Block. Seit die Block-Bildung an den
    /// letzten Block DESSELBEN Kanals anhaengt, endet ein Redebeitrag nicht
    /// mehr am Einwurf der Gegenseite -- ohne Deckel wuerde ein Monolog zu
    /// einem einzigen Block ueber die gesamte Aufnahme.
    ///
    /// Herleitung: gemessenes Maximum plus Reserve. An zwei echten
    /// Gespraechen (23,5 min / 63,8 min) liegt der laengste Block bei 445
    /// Woertern / 167 190 ms und bei 298 Woertern / 144 086 ms. 512 und
    /// 300 000 lassen darauf rund 15 % beziehungsweise 80 % Luft. Der Deckel
    /// schneidet damit heute keinen echten Redebeitrag; er faengt nur die
    /// Entgleisung.
    ///
    /// Der Bezug auf den Live-Pfad, der frueher hier stand (4096 Woerter,
    /// 30-Minuten-Fenster, "rund ein Achtel davon"), ist gestrichen: jene
    /// Grenzen kappen die GESAMTE gleitende Flaeche, nicht einen einzelnen
    /// Block. Aus einem Bruchteil einer anderen Groesse laesst sich diese
    /// hier nicht ableiten -- das war Schmuck, keine Begruendung.
    ///
    /// Offen und bewusst nicht angefasst: der Wort-Deckel schneidet hart am
    /// Zaehler, also notfalls mitten im Satz. Am letzten Wort mit Satzzeichen
    /// im Fenster zu schneiden waere schoener. Auf beiden Vorlagen greift der
    /// Deckel null Mal, und ihre Worttexte sind anonymisierte Platzhalter --
    /// es gibt also weder einen gemessenen Anlass noch Material, an dem sich
    /// die Verbesserung pruefen liesse. Erst messen, dann bauen.
    pub max_segment_words: Option<usize>,
    pub max_segment_ms: Option<i64>,
}

impl Default for SegmentBuilderOptions {
    fn default() -> Self {
        Self {
            max_gap_ms: None,
            complete_channels: Some(vec![ChannelProfile::DirectMic]),
            // `None`, nicht `Some(vec![])`: der Bauer soll es sich aus seinen
            // Woertern ausrechnen, wenn niemand es ihm sagt. Eine leere Liste
            // waere die Behauptung "kein Kanal traegt mehrere Sprecher" -- und
            // damit genau der Zustand, aus dem dieser Fehler kam.
            multi_speaker_channels: None,
            max_segment_words: Some(512),
            max_segment_ms: Some(300_000),
        }
    }
}
