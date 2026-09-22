use crate::common_derives;
use crate::stream;

// https://github.com/deepgram/deepgram-rust-sdk/blob/0.7.0/src/common/batch_response.rs
// https://developers.deepgram.com/reference/speech-to-text/listen-pre-recorded

common_derives! {
    #[specta(rename = "BatchWord")]
    #[cfg_attr(feature = "openapi", schema(as = BatchWord))]
    pub struct Word {
        pub word: String,
        pub start: f64,
        pub end: f64,
        pub confidence: f64,
        /// Fork (03.09.2026): die GEMESSENE Wortsicherheit -- oder gar nichts.
        ///
        /// `confidence` daneben kann das nicht ausdruecken: es ist ein nackter
        /// `f64` (Deepgram-Erbe), und wer keine Messung hat, schreibt dort
        /// `1,0`. Nach dem Speichern ist "sicher gemessen" von "nichts
        /// gewusst" nicht mehr zu unterscheiden -- und ein spaeterer Filter
        /// auf NIEDRIGE Sicherheit greift dann genau invers: er ueberspringt
        /// die ungemessenen Woerter, weil sie maximal sicher aussehen.
        ///
        /// `metadata.timing.source` beantwortet das NICHT. Es wird einmal je
        /// Antwort aus einem `any()` ueber alle Kanaele gestempelt
        /// (`transcribe-soniqo/src/responses.rs`, `has_word_timings`); eine
        /// gemischte Antwort etikettiert auch synthetische Woerter als
        /// gemessen. Die Aussage gehoert an das einzelne Wort.
        ///
        /// `None` heisst ausdruecklich "keine Messung", nicht "schlecht".
        ///
        /// Heute gefuellt von den beiden lokalen Wegen (Parakeet ueber
        /// `transcribe-soniqo`, Apple SpeechAnalyzer) und von den zwei
        /// Cloud-Adaptern, die selbst zwischen "gemessen" und "keine Angabe"
        /// unterscheiden (`revai`, `smallestai` -- dort steht im Quelltyp ein
        /// `Option`). Die uebrigen 19 Batch-Adapter lassen es leer, auch wo ihr
        /// Anbieter eine Zahl liefert: deren Bedeutung ist nicht geprueft, und
        /// nichts zu behaupten ist die sichere Richtung. Das ist eine offene
        /// Strecke, kein Endzustand.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub measured_confidence: Option<f64>,
        #[serde(default)]
        pub channel: i32,
        pub speaker: Option<usize>,
        pub punctuated_word: Option<String>,
    }
}

common_derives! {
    #[specta(rename = "BatchAlternatives")]
    #[cfg_attr(feature = "openapi", schema(as = BatchAlternatives))]
    pub struct Alternatives {
        pub transcript: String,
        pub confidence: f64,
        #[serde(default)]
        pub words: Vec<Word>,
    }
}

common_derives! {
    #[specta(rename = "BatchChannel")]
    #[cfg_attr(feature = "openapi", schema(as = BatchChannel))]
    pub struct Channel {
        pub alternatives: Vec<Alternatives>,
    }
}

common_derives! {
    #[specta(rename = "BatchResults")]
    #[cfg_attr(feature = "openapi", schema(as = BatchResults))]
    pub struct Results {
        pub channels: Vec<Channel>,
    }
}

common_derives! {
    #[specta(rename = "BatchResponse")]
    #[cfg_attr(feature = "openapi", schema(as = BatchResponse))]
    pub struct Response {
        #[cfg_attr(feature = "openapi", schema(value_type = Object))]
        pub metadata: serde_json::Value,
        pub results: Results,
    }
}

impl From<stream::Word> for Word {
    fn from(word: stream::Word) -> Self {
        Self {
            word: word.word,
            start: word.start,
            end: word.end,
            confidence: word.confidence,
            // Der Live-Weg kann keine gemessene Sicherheit tragen: `stream::Word`
            // hat das Feld nicht, es gibt hier also nichts zu uebernehmen.
            //
            // Ausdruecklich NICHT die Begruendung: "im Live-Betrieb misst ohnehin
            // niemand". Das waere falsch -- `soniox/live.rs`, `smallestai/live.rs`
            // und `transcribe-speechanalyzer` schreiben dort echte
            // Anbieterwerte, und die gehen an dieser Stelle verloren. Sie in
            // `confidence` zu suchen hilft nicht: dasselbe Feld traegt bei jedem
            // Anbieter ohne Messung eine 1,0 (in `transcribe-soniqo` hart, siehe
            // `stream_words_from_text`), also ist es hier nicht unterscheidbar.
            //
            // Bekannte Luecke. Sie zu schliessen heisst, das Feld auch in
            // `stream::Word` aufzunehmen -- eine eigene Strecke.
            measured_confidence: None,
            channel: 0,
            speaker: word
                .speaker
                .and_then(|speaker| (speaker >= 0).then_some(speaker as usize)),
            punctuated_word: word.punctuated_word,
        }
    }
}

impl From<stream::Alternatives> for Alternatives {
    fn from(alternatives: stream::Alternatives) -> Self {
        let transcript = alternatives.transcript.trim().to_string();
        let words = alternatives
            .words
            .into_iter()
            .map(Word::from)
            .collect::<Vec<_>>();

        Self {
            transcript,
            confidence: alternatives.confidence,
            words,
        }
    }
}

impl From<stream::Channel> for Channel {
    fn from(channel: stream::Channel) -> Self {
        let alternatives = channel
            .alternatives
            .into_iter()
            .map(Alternatives::from)
            .collect::<Vec<_>>();

        Self { alternatives }
    }
}

impl From<stream::Metadata> for serde_json::Value {
    fn from(metadata: stream::Metadata) -> Self {
        serde_json::to_value(metadata).unwrap_or_else(|_| serde_json::json!({}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_batch_word_defaults_missing_channel_to_zero() {
        let response: Response = serde_json::from_value(serde_json::json!({
            "metadata": {},
            "results": {
                "channels": [
                    {
                        "alternatives": [
                            {
                                "transcript": "hello",
                                "confidence": 0.9,
                                "words": [
                                    {
                                        "word": "hello",
                                        "start": 0.0,
                                        "end": 1.0,
                                        "confidence": 0.9
                                    }
                                ]
                            }
                        ]
                    }
                ]
            }
        }))
        .unwrap();

        let word = &response.results.channels[0].alternatives[0].words[0];
        assert_eq!(word.channel, 0);
        assert_eq!(word.word, "hello");
    }
}
