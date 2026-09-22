use aspasia::{Subtitle as SubtitleTrait, TimedSubtitleFile, WebVttSubtitle};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Token {
    text: String,
    start_time: u64,
    end_time: u64,
    speaker: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct VttWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Subtitle {
    tokens: Vec<Token>,
}

impl From<TimedSubtitleFile> for Subtitle {
    fn from(sub: TimedSubtitleFile) -> Self {
        let vtt: WebVttSubtitle = sub.into();

        let tokens = vtt
            .events()
            .iter()
            .map(|cue| Token {
                text: cue.text.clone(),
                start_time: i64::from(cue.start) as u64,
                end_time: i64::from(cue.end) as u64,
                speaker: cue.identifier.as_ref().filter(|s| !s.is_empty()).cloned(),
            })
            .collect();

        Self { tokens }
    }
}

pub fn parse_subtitle_from_path<P: AsRef<std::path::Path>>(
    path: P,
) -> std::result::Result<Subtitle, String> {
    let sub = TimedSubtitleFile::new(path.as_ref()).map_err(|e| e.to_string())?;
    Ok(sub.into())
}

/// Fork (02.09.2026): seit die Wortzeiten GEMESSEN sind, kommen sie nicht mehr
/// lueckenlos und nicht mehr kanalweise sortiert an -- zwei Sprecher koennen
/// sich ueberlappen, und die Woerter des zweiten Kanals stehen hinter denen des
/// ersten statt dazwischen. WebVTT vertraegt Ueberlappung, aber keine
/// rueckwaerts laufende Zeitachse und keine Kachel mit Ende vor Beginn.
///
/// Deshalb hier, an genau EINER Stelle: nach Beginn sortieren (stabil, damit
/// gleichzeitige Woerter ihre Reihenfolge behalten) und jede Kachel auf
/// mindestens eine Millisekunde bringen.
fn normalize_vtt_words(mut words: Vec<VttWord>) -> Vec<VttWord> {
    words.sort_by_key(|word| word.start_ms);
    for word in &mut words {
        word.end_ms = word.end_ms.max(word.start_ms.saturating_add(1));
    }
    words
}

pub fn export_words_to_vtt_file<P: AsRef<std::path::Path>>(
    words: Vec<VttWord>,
    path: P,
) -> std::result::Result<(), String> {
    use aspasia::{Moment, webvtt::WebVttCue};

    let cues: Vec<WebVttCue> = normalize_vtt_words(words)
        .into_iter()
        .map(|word| {
            let start_i64 = i64::try_from(word.start_ms)
                .map_err(|_| format!("start_ms {} exceeds i64::MAX", word.start_ms))?;
            let end_i64 = i64::try_from(word.end_ms)
                .map_err(|_| format!("end_ms {} exceeds i64::MAX", word.end_ms))?;

            Ok(WebVttCue {
                identifier: word.speaker,
                text: word.text,
                settings: None,
                start: Moment::from(start_i64),
                end: Moment::from(end_i64),
            })
        })
        .collect::<Result<_, String>>()?;

    let vtt = WebVttSubtitle::builder().cues(cues).build();
    vtt.export(path.as_ref()).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{VttWord, normalize_vtt_words};

    fn word(text: &str, start_ms: u64, end_ms: u64) -> VttWord {
        VttWord {
            text: text.to_string(),
            start_ms,
            end_ms,
            speaker: None,
        }
    }

    #[test]
    fn overlapping_channels_are_ordered_by_start_time() {
        // So kommen zwei Kanaele mit gemessenen Zeiten an: erst der eine ganz,
        // dann der andere. Ohne Sortierung liefe die Zeitachse zurueck.
        let normalized = normalize_vtt_words(vec![
            word("ich", 1_000, 1_300),
            word("auch", 2_000, 2_300),
            word("ja", 1_100, 1_400),
        ]);

        assert_eq!(
            normalized
                .iter()
                .map(|word| (word.text.as_str(), word.start_ms))
                .collect::<Vec<_>>(),
            vec![("ich", 1_000), ("ja", 1_100), ("auch", 2_000)]
        );
    }

    #[test]
    fn a_cue_never_ends_before_it_starts() {
        let normalized = normalize_vtt_words(vec![word("hm", 5_000, 4_000)]);

        assert_eq!(normalized[0].start_ms, 5_000);
        assert_eq!(normalized[0].end_ms, 5_001);
    }
}
