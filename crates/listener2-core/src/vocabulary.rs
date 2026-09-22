//! Die eingetragene Stichwortliste auf das fertige Transkript anwenden.
//!
//! Sitzt bewusst am Ausgang von [`crate::run_batch`] und nicht in einem
//! einzelnen Anbieter: dort laeuft JEDER Weg vorbei -- Parakeet, whisper.cpp,
//! Apple Speech und die Wolke. Ein Nachlauf, den man an drei Stellen einbauen
//! muss, ist an zweien irgendwann nicht mehr eingebaut.
//!
//! Warum nicht schon vor dem Modell: Parakeet nimmt gar keine Vorgabe an
//! (`transcribe_file` kennt nur Modell, Datei und Sprache), und
//! `transcribe-whisper-local` liest zwar `keywords` aus der Anfrage, benutzt
//! sie aber nirgends. Nach dem Modell ist der einzige Ort, der fuer alle
//! lokalen Wege gleich funktioniert.

use std::sync::RwLock;

use anlg_vocabulary::{MatchKind, Options, Replacement, Token, Vocabulary};
use owhisper_interface::batch;

static OPTIONS: RwLock<Option<Options>> = RwLock::new(None);

/// Setzt die Stufen des Nachlaufs prozessweit. Nur die Messbank ruft das auf --
/// im Betrieb gelten [`Options::default`].
pub fn set_vocabulary_options(options: Options) {
    let mut guard = OPTIONS.write().unwrap_or_else(|error| error.into_inner());
    *guard = Some(options);
}

/// Liest die Stufen. Ohne vorheriges Setzen: [`Options::default`].
pub(crate) fn vocabulary_options() -> Options {
    OPTIONS
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .unwrap_or_default()
}

/// Wendet die Liste auf alle Kanaele der Antwort an und protokolliert, was
/// passiert ist. Gibt zurueck, wie viele Woerter ersetzt wurden.
pub(crate) fn apply_to_response(
    response: &mut batch::Response,
    vocabulary: &Vocabulary,
    options: Options,
) -> usize {
    if vocabulary.is_empty() {
        // Eine EIGENE Meldung, nicht die Erfolgsmeldung von unten. Ohne sie
        // sieht der Regelfall eines neuen Nutzers -- die Liste ist leer -- im
        // Protokoll wieder genauso aus wie "der Nachlauf lief gar nicht", und
        // das ist wortgleich die Ununterscheidbarkeit, die `ce311bb02c`
        // beseitigt hat. Mit `vocabulary_pass_completed` darf sie trotzdem
        // nicht zusammenfallen: gelaufen ist hier nichts.
        tracing::debug!("vocabulary_pass_skipped_empty_list");
        return 0;
    }

    // Was der Nachlauf ablehnt, sagt er. Eine Verhoerung, die selbst
    // gewoehnliche deutsche Sprache ist, wird nicht angewandt -- und der
    // Mensch, der sie eingetragen hat, soll erfahren, dass sein Eintrag nicht
    // wirkt. Eine Warnung ist ein besseres Ergebnis als eine Ersetzung, die
    // ihm mitten im Satz ein Wort wegnimmt.
    for risky in vocabulary.risky_aliases() {
        tracing::warn!(
            anarlog.vocabulary.alias = %risky.alias,
            anarlog.vocabulary.canonical = %risky.canonical,
            "vocabulary_alias_rejected_common_word"
        );
    }

    let mut total = 0usize;
    for channel in response.results.channels.iter_mut() {
        for alternative in channel.alternatives.iter_mut() {
            total += apply_to_alternative(alternative, vocabulary, options);
        }
    }

    // Auch bei NULL Ersetzungen. Ein Nachlauf, der nur meldet, wenn er etwas
    // gefunden hat, ist von einem Nachlauf, der gar nicht lief, nicht zu
    // unterscheiden -- und dann ist die Null keine Messung, sondern eine
    // Hoffnung. Genau daran ist die Auswertung vom 02.09.2026 haengen
    // geblieben: `tom.mp3` ergab 0 Ersetzungen, und im Protokoll stand nichts,
    // was belegt haette, dass der Nachlauf ueberhaupt gelaufen war.
    tracing::info!(
        anarlog.vocabulary.entries = vocabulary.len(),
        anarlog.vocabulary.replacements = total,
        "vocabulary_pass_completed"
    );
    total
}

/// Groesster Abstand zwischen zwei Woertern, die noch zu EINEM Begriff gehoeren
/// duerfen.
///
/// GEMESSEN, nicht geschaetzt (02.09.2026, `tom.mp3`, 3355 Woerter, Abstaende
/// innerhalb desselben Kanals): der Median ist 0,0 s, 86 % liegen unter 0,3 s,
/// 90 % unter 1,0 s. Die Woerter eines gesprochenen Namens folgen unmittelbar
/// aufeinander; was mehr als eine Sekunde auseinanderliegt, ist eine Pause und
/// keine Wortfolge. Eine Sekunde ist damit reichlich bemessen -- die Schranke
/// soll den Absatz trennen, nicht den Namen zerschneiden.
const MAX_WORD_GAP_SECONDS: f64 = 1.0;

fn apply_to_alternative(
    alternative: &mut batch::Alternatives,
    vocabulary: &Vocabulary,
    options: Options,
) -> usize {
    if alternative.words.is_empty() {
        return 0;
    }

    let tokens: Vec<Token> = alternative
        .words
        .iter()
        .map(|word| Token::new(display_text(word)))
        .collect();

    // Ein Begriff endet an der Sprecher-, Kanal- und Zeitgrenze. Der Nachlauf
    // sieht nur Text und kann das nicht wissen -- also bekommt er die Folge
    // stueckweise, statt sie am Stueck zu bekommen und darin Grenzen zu
    // ueberspringen.
    let mut plan: Vec<Replacement> = Vec::new();
    for segment in segments(&alternative.words) {
        for mut replacement in anlg_vocabulary::apply(vocabulary, &tokens[segment.clone()], options)
        {
            replacement.start += segment.start;
            plan.push(replacement);
        }
    }
    if plan.is_empty() {
        return 0;
    }

    // Woerter und Fliesstext duerfen nicht auseinanderlaufen -- entweder
    // beide oder keins. Zwei Wahrheiten in derselben Zeile sind teurer als
    // eine unkorrigierte.
    //
    // Weg 1: Der Text setzt sich aus den Woertern zusammen (bei Parakeet per
    // Konstruktion so) -- dann wird er aus den neuen Woertern neu gebaut.
    // Weg 2: Ein Wolken-Anbieter formatiert anders -- dann wird jede
    // Ersetzung im Text selbst vorgenommen, aber nur, wenn die Vorlage dort
    // GENAU EINMAL an einer Wortgrenze steht. Sonst waere es geraten.
    let transcript = match join_tokens(&tokens) == alternative.transcript {
        true => None,
        false => match apply_plan_to_text(&alternative.transcript, &plan) {
            Some(text) => Some(text),
            None => {
                // Kein stiller Ausgang und keine halbe Korrektur: es bleibt
                // alles, wie es war.
                tracing::warn!(
                    anarlog.vocabulary.replacements = plan.len(),
                    "vocabulary_skipped_transcript_mismatch"
                );
                return 0;
            }
        },
    };

    for replacement in &plan {
        tracing::info!(
            anarlog.vocabulary.kind = kind_label(replacement.kind),
            anarlog.vocabulary.before = %replacement.before,
            anarlog.vocabulary.after = %replacement.after,
            "vocabulary_replacement"
        );
    }

    alternative.words = rewrite_words(&alternative.words, &plan);
    alternative.transcript = match transcript {
        Some(text) => text,
        None => alternative
            .words
            .iter()
            .map(display_text)
            .collect::<Vec<_>>()
            .join(" "),
    };

    plan.len()
}

/// Zerlegt die Wortfolge in Stuecke, innerhalb derer ein Begriff liegen darf:
/// gleicher Kanal, gleicher Sprecher, kein Loch groesser als
/// [`MAX_WORD_GAP_SECONDS`].
///
/// Ohne das entstand der Fehlgriff, den der Zweitblick am 02.09.2026 gefunden
/// hat: Sprecher 1 sagt "Klick", Sprecher 2 antwortet "ab" -- und die Regel
/// `ClickUp => Klick ab` machte daraus EIN Wort, vollstaendig Sprecher 1
/// zugeschrieben, mit einer Zeitspanne, die die Pause dazwischen verschluckt.
fn segments(words: &[batch::Word]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for index in 1..words.len() {
        let previous = &words[index - 1];
        let current = &words[index];
        let broken = current.channel != previous.channel
            || current.speaker != previous.speaker
            || current.start - previous.end > MAX_WORD_GAP_SECONDS;
        if broken {
            out.push(start..index);
            start = index;
        }
    }
    out.push(start..words.len());
    out
}

/// Setzt den Plan um.
///
/// Ein mehrwortiger richtiger Name ergibt MEHRERE Woerter mit eigenen Zeiten,
/// nicht ein Wort mit einem Leerzeichen darin. Die Alternative waere gewesen,
/// solche Eintraege gar nicht zu ersetzen -- das haette aber genau des Betreibers
/// eigenen Eintrag `NOR Drucktechnik` getroffen, also die Haelfte des Zwecks.
/// Ein Wort namens `"NOR Drucktechnik"` dagegen bricht jeden Verbraucher, der
/// je Element ein eigenstaendig getaktetes Anzeigewort erwartet: im Abspieler
/// laesst es sich nicht mehr einzeln anspringen.
///
/// Die Zeitspanne der Gruppe wird nach Zeichenzahl auf die neuen Woerter
/// verteilt. Das ist eine Naeherung und soll keine sein wollen -- gemessen ist
/// nur der Anfang des ersten und das Ende des letzten Wortes, und genau die
/// beiden Werte bleiben erhalten.
fn rewrite_words(words: &[batch::Word], plan: &[Replacement]) -> Vec<batch::Word> {
    let mut out: Vec<batch::Word> = Vec::with_capacity(words.len());
    let mut position = 0usize;

    for replacement in plan {
        while position < replacement.start {
            out.push(words[position].clone());
            position += 1;
        }

        let group = &words[replacement.start..replacement.start + replacement.len];
        out.extend(split_replacement(group, &replacement.after));
        position = replacement.start + replacement.len;
    }

    while position < words.len() {
        out.push(words[position].clone());
        position += 1;
    }
    out
}

fn split_replacement(group: &[batch::Word], after: &str) -> Vec<batch::Word> {
    let first = &group[0];
    let last = &group[group.len() - 1];
    // Die vorsichtigste Zahl der Gruppe -- eine Ersetzung macht ein Wort nicht
    // sicherer, als sein schwaechstes Teilstueck war.
    let confidence = group
        .iter()
        .map(|word| word.confidence)
        .fold(f64::INFINITY, f64::min);
    // Fork (03.09.2026): dieselbe Rechnung fuer die GEMESSENE Sicherheit --
    // aber nur, wenn jedes Wort der Gruppe eine hat. Fehlt einer die Messung,
    // waere das Minimum der uebrigen eine Aussage ueber eine Teilmenge, die
    // sich als Aussage ueber das ganze ersetzte Wort liest. Dann lieber nichts.
    let measured_confidence = group
        .iter()
        .map(|word| word.measured_confidence)
        .try_fold(f64::INFINITY, |acc, value| Some(acc.min(value?)));

    let parts: Vec<&str> = after.split_whitespace().collect();
    let total: usize = parts.iter().map(|part| part.chars().count()).sum();
    let span = (last.end - first.start).max(0.0);

    let mut out = Vec::with_capacity(parts.len());
    let mut cursor = first.start;
    for (index, part) in parts.iter().enumerate() {
        let is_last = index + 1 == parts.len();
        let end = match is_last || total == 0 {
            true => last.end,
            false => cursor + span * part.chars().count() as f64 / total as f64,
        };
        out.push(batch::Word {
            word: strip_punctuation(part).to_string(),
            start: cursor,
            end,
            confidence,
            measured_confidence,
            channel: first.channel,
            speaker: first.speaker,
            punctuated_word: Some((*part).to_string()),
        });
        cursor = end;
    }
    out
}

/// Nimmt dieselben Ersetzungen am Fliesstext vor -- aber nur, wenn dort GENAU
/// SO VIELE Vorkommen einer Vorlage stehen, wie der Plan vorsieht.
///
/// Die Zaehlung ist der Kern. Der Plan enthaelt EINE Zeile JE VORKOMMEN: ein
/// Transkript mit "mit Sedatsch gesprochen, und Sedatsch meinte" ergibt
/// zwei Zeilen mit derselben Vorlage. Eine Pruefung, die je Zeile Eindeutigkeit
/// verlangt, faellt genau dort um -- und das trifft die wertvollsten Eintraege
/// zuerst, naemlich die Namen, die das Modell WIEDERHOLT verhoert.
///
/// Zwei Vorkommen bei zwei Planzeilen sind keine Mehrdeutigkeit, sondern eine
/// saubere Eins-zu-eins-Zuordnung. Mehrdeutig ist erst, wenn die Zahlen
/// auseinandergehen: dann steht im Text ein Vorkommen, das der Plan nicht
/// vorgesehen hat (oder umgekehrt), und dann wird nichts angefasst.
///
/// Gesucht wird in EINEM Durchlauf ueber den urspruenglichen Text, laengste
/// Vorlage zuerst. Eine Schleife, die den Text zwischen den Durchlaeufen
/// austauscht, wuerde die naechste Pruefung gegen bereits veraenderten Text
/// laufen lassen.
fn apply_plan_to_text(text: &str, plan: &[Replacement]) -> Option<String> {
    // Je Vorlage: das Ziel und die Zahl der geplanten Vorkommen.
    let mut expected: Vec<(&str, &str, usize)> = Vec::new();
    for replacement in plan {
        match expected
            .iter_mut()
            .find(|(before, _, _)| *before == replacement.before)
        {
            // Dieselbe Vorlage mit zwei verschiedenen Zielen: im Text ist
            // nicht zu unterscheiden, welches Vorkommen welches meint.
            Some((_, after, _)) if *after != replacement.after => return None,
            Some((_, _, count)) => *count += 1,
            None => expected.push((&replacement.before, &replacement.after, 1)),
        }
    }
    // Laengste zuerst, damit "NOR Druck Technik" gewinnt und nicht "Technik".
    expected.sort_by_key(|(before, _, _)| std::cmp::Reverse(before.len()));

    let mut out = String::with_capacity(text.len());
    let mut found: Vec<usize> = vec![0; expected.len()];
    let mut cursor = 0usize;
    while cursor < text.len() {
        let hit = expected.iter().enumerate().find(|(_, (before, _, _))| {
            text[cursor..].starts_with(before) && at_word_boundary(text, cursor, before.len())
        });
        match hit {
            Some((index, (before, after, _))) => {
                out.push_str(after);
                found[index] += 1;
                cursor += before.len();
            }
            None => {
                let character = text[cursor..].chars().next()?;
                out.push(character);
                cursor += character.len_utf8();
            }
        }
    }

    let stimmt = expected
        .iter()
        .zip(&found)
        .all(|((_, _, count), seen)| count == seen);
    match stimmt {
        true => Some(out),
        false => None,
    }
}

/// Wahr, wenn der Fund an beiden Seiten von etwas anderem als einem Buchstaben
/// begrenzt ist. Ohne das wuerde die Vorlage "Kling" auch in "Klingel" treffen.
fn at_word_boundary(text: &str, offset: usize, length: usize) -> bool {
    let before = text[..offset].chars().next_back();
    let after = text[offset + length..].chars().next();
    !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
}

fn display_text(word: &batch::Word) -> String {
    word.punctuated_word
        .as_deref()
        .unwrap_or(&word.word)
        .to_string()
}

fn join_tokens(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_punctuation(text: &str) -> &str {
    text.trim_matches(|character: char| {
        !character.is_alphanumeric() && character != '-' && character != '\''
    })
}

fn kind_label(kind: MatchKind) -> &'static str {
    match kind {
        MatchKind::Alias => "alias",
        MatchKind::AliasPhonetic => "alias_klang",
        MatchKind::CanonicalPhonetic => "namens_klang",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start: f64, end: f64) -> batch::Word {
        word_von(text, start, end, 0, Some(1))
    }

    fn word_von(
        text: &str,
        start: f64,
        end: f64,
        channel: i32,
        speaker: Option<usize>,
    ) -> batch::Word {
        batch::Word {
            word: strip_punctuation(text).to_string(),
            start,
            end,
            confidence: 0.9,
            measured_confidence: Some(0.9),
            channel,
            speaker,
            punctuated_word: Some(text.to_string()),
        }
    }

    fn response(words: Vec<batch::Word>) -> batch::Response {
        let transcript = words.iter().map(display_text).collect::<Vec<_>>().join(" ");
        batch::Response {
            metadata: serde_json::json!({}),
            results: batch::Results {
                channels: vec![batch::Channel {
                    alternatives: vec![batch::Alternatives {
                        words,
                        transcript,
                        confidence: 1.0,
                    }],
                }],
            },
        }
    }

    fn liste() -> Vocabulary {
        Vocabulary::parse([
            "Sedacz => Sedatsch",
            "Phonowerk => Phono Werk",
            "NOR Drucktechnik => NOR Druck Technik",
            "ClickUp => Click ab",
            "Vurden",
        ])
    }

    #[test]
    fn ein_wort_wird_ersetzt_und_behaelt_seine_zeitmarke() {
        let mut response = response(vec![
            word("Herr", 1.0, 1.4),
            word("Sedatsch", 1.4, 2.0),
            word("kam.", 2.0, 2.3),
        ]);
        let count = apply_to_response(&mut response, &liste(), Options::default());
        assert_eq!(count, 1);

        let words = &response.results.channels[0].alternatives[0].words;
        assert_eq!(words.len(), 3);
        assert_eq!(words[1].punctuated_word.as_deref(), Some("Sedacz"));
        assert_eq!(words[1].word, "Sedacz");
        assert_eq!((words[1].start, words[1].end), (1.4, 2.0));
        assert_eq!(words[1].speaker, Some(1));
    }

    #[test]
    fn eine_wortgruppe_wird_zu_einem_wort_mit_der_ganzen_spanne() {
        let mut response = response(vec![
            word("die", 0.0, 0.2),
            word("Phono", 0.2, 0.7),
            word("Werk,", 0.7, 1.3),
            word("Hamburg", 1.3, 1.9),
        ]);
        apply_to_response(&mut response, &liste(), Options::default());

        let words = &response.results.channels[0].alternatives[0].words;
        assert_eq!(words.len(), 3);
        assert_eq!(words[1].punctuated_word.as_deref(), Some("Phonowerk,"));
        // Der Wortstamm traegt kein Satzzeichen, die Anzeigefassung schon.
        assert_eq!(words[1].word, "Phonowerk");
        // Die Spanne umfasst beide urspruenglichen Woerter.
        assert_eq!((words[1].start, words[1].end), (0.2, 1.3));
    }

    #[test]
    fn der_fliesstext_wird_mitgezogen() {
        let mut response = response(vec![word("mit", 0.0, 0.3), word("Sedatsch", 0.3, 0.9)]);
        apply_to_response(&mut response, &liste(), Options::default());
        assert_eq!(
            response.results.channels[0].alternatives[0].transcript,
            "mit Sedacz"
        );
    }

    #[test]
    fn ohne_treffer_bleibt_alles_bitgleich() {
        let vorher = response(vec![
            word("Wir", 0.0, 0.3),
            word("werden", 0.3, 0.8),
            word("sehen.", 0.8, 1.2),
        ]);
        let mut nachher = vorher.clone();
        let count = apply_to_response(&mut nachher, &liste(), Options::default());
        assert_eq!(count, 0);
        assert_eq!(
            serde_json::to_string(&vorher).unwrap(),
            serde_json::to_string(&nachher).unwrap()
        );
    }

    #[test]
    fn leere_liste_fasst_nichts_an() {
        let mut response = response(vec![word("Sedatsch", 0.0, 0.5)]);
        assert_eq!(
            apply_to_response(&mut response, &Vocabulary::default(), Options::default()),
            0
        );
        assert_eq!(
            response.results.channels[0].alternatives[0].words[0]
                .punctuated_word
                .as_deref(),
            Some("Sedatsch")
        );
    }

    /// Ein Wolken-Anbieter darf einen anders formatierten Fliesstext liefern.
    /// Dann wird die Ersetzung dort direkt vorgenommen -- Woerter und Text
    /// bleiben dieselbe Wahrheit.
    ///
    /// Bis zum 02.09.2026 wurden hier NUR die Woerter korrigiert und der Text
    /// stehen gelassen: im Fliesstext stand dann "Mit Sedatsch!", im
    /// getakteten Wort "Sedacz". Zwei Wahrheiten in derselben Zeile.
    #[test]
    fn ein_anders_formatierter_fliesstext_wird_mitkorrigiert() {
        let mut response = response(vec![word("mit", 0.0, 0.3), word("Sedatsch", 0.3, 0.9)]);
        response.results.channels[0].alternatives[0].transcript = "Mit Sedatsch!".to_string();
        assert_eq!(
            apply_to_response(&mut response, &liste(), Options::default()),
            1
        );
        assert_eq!(
            response.results.channels[0].alternatives[0].transcript,
            "Mit Sedacz!"
        );
        assert_eq!(
            response.results.channels[0].alternatives[0].words[1]
                .punctuated_word
                .as_deref(),
            Some("Sedacz")
        );
    }

    /// Und wenn die Vorlage im Fliesstext gar nicht auffindbar ist, bleibt
    /// ALLES stehen -- auch die Woerter. Eine halbe Korrektur waere die
    /// Divergenz, die dieser Zweig verhindern soll.
    #[test]
    fn ein_unauffindbarer_fliesstext_blockt_auch_die_woerter() {
        let vorher = {
            let mut response = response(vec![word("mit", 0.0, 0.3), word("Sedatsch", 0.3, 0.9)]);
            response.results.channels[0].alternatives[0].transcript =
                "voellig anderer Text".to_string();
            response
        };
        let mut nachher = vorher.clone();
        assert_eq!(
            apply_to_response(&mut nachher, &liste(), Options::default()),
            0
        );
        assert_eq!(
            serde_json::to_string(&vorher).unwrap(),
            serde_json::to_string(&nachher).unwrap()
        );
    }

    /// Ein mehrdeutiger Fund zaehlt als unauffindbar: steht die Vorlage
    /// zweimal im Text, weiss niemand, welches Vorkommen gemeint war.
    #[test]
    fn ein_mehrdeutiger_fliesstext_blockt_ebenfalls() {
        let mut response = response(vec![word("mit", 0.0, 0.3), word("Sedatsch", 0.3, 0.9)]);
        response.results.channels[0].alternatives[0].transcript =
            "Sedatsch und Sedatsch.".to_string();
        assert_eq!(
            apply_to_response(&mut response, &liste(), Options::default()),
            0
        );
    }

    /// Der Fehlgriff, den der Zweitblick am 02.09.2026 gefunden hat: ein
    /// mehrwortiger Treffer darf nicht ueber einen Sprecherwechsel laufen.
    /// Sprecher 1 sagt "Click", Sprecher 2 antwortet "ab" -- daraus wurde EIN
    /// Wort "ClickUp", vollstaendig Sprecher 1 zugeschrieben.
    #[test]
    fn ein_treffer_laeuft_nicht_ueber_einen_sprecherwechsel() {
        // Gleicher Kanal, anderer Sprecher -- damit haengt dieser Test an der
        // Sprecher-Bedingung allein.
        pruefe_keine_ersetzung(vec![
            word_von("Click", 1.0, 1.4, 0, Some(1)),
            word_von("ab", 1.6, 1.9, 0, Some(2)),
        ]);
    }

    /// Dasselbe eine Ebene tiefer: getrennte Tonspuren. Am Kanal haengt bei
    /// dem Menschen die Zuordnung Mikrofon gegen Gegenueber.
    #[test]
    fn ein_treffer_laeuft_nicht_ueber_einen_kanalwechsel() {
        // Gleicher Sprecher, anderer Kanal -- haengt an der Kanal-Bedingung
        // allein.
        pruefe_keine_ersetzung(vec![
            word_von("Click", 1.0, 1.4, 0, Some(1)),
            word_von("ab", 1.6, 1.9, 1, Some(1)),
        ]);
    }

    /// Die Gegenprobe zu beiden: derselbe Wortlaut auf EINEM Kanal bei EINEM
    /// Sprecher wird sehr wohl ersetzt. Ohne sie waeren die beiden Tests auch
    /// dann gruen, wenn der Nachlauf gar nichts mehr faende.
    #[test]
    fn derselbe_wortlaut_bei_einem_sprecher_wird_ersetzt() {
        let mut response = response(vec![
            word_von("Click", 1.0, 1.4, 0, Some(1)),
            word_von("ab", 1.6, 1.9, 0, Some(1)),
        ]);
        assert_eq!(
            apply_to_response(&mut response, &liste(), Options::default()),
            1
        );
        assert_eq!(
            response.results.channels[0].alternatives[0].words[0].word,
            "ClickUp"
        );
    }

    /// Laesst die Liste ueber die Woerter laufen und verlangt, dass nichts
    /// passiert -- bitgleich, nicht nur "null gemeldet".
    fn pruefe_keine_ersetzung(words: Vec<batch::Word>) {
        let vorher = response(words);
        let mut nachher = vorher.clone();
        assert_eq!(
            apply_to_response(&mut nachher, &liste(), Options::default()),
            0
        );
        assert_eq!(
            serde_json::to_string(&vorher).unwrap(),
            serde_json::to_string(&nachher).unwrap()
        );
    }

    /// Dasselbe an der Zeitgrenze: gleicher Sprecher, gleicher Kanal, aber
    /// eine Pause dazwischen. Was mehr als eine Sekunde auseinanderliegt, ist
    /// keine Wortfolge.
    #[test]
    fn ein_treffer_laeuft_nicht_ueber_eine_lange_pause() {
        pruefe_keine_ersetzung(vec![
            word_von("Click", 1.0, 1.4, 0, Some(1)),
            word_von("ab", 8.0, 8.3, 0, Some(1)),
        ]);

        // Und knapp unterhalb der Schranke greift es weiterhin -- die Schranke
        // soll die Pause trennen, nicht den Namen zerschneiden.
        let mut knapp = response(vec![
            word_von("Click", 1.0, 1.4, 0, Some(1)),
            word_von("ab", 2.3, 2.6, 0, Some(1)),
        ]);
        assert_eq!(
            apply_to_response(&mut knapp, &liste(), Options::default()),
            1
        );
    }

    /// Ein mehrwortiger richtiger Name ergibt MEHRERE Woerter mit eigenen
    /// Zeiten. Bis zum 02.09.2026 entstand daraus ein einziges Wort namens
    /// "NOR Drucktechnik" -- mit einem Leerzeichen darin, einer gemeinsamen
    /// Zeitmarke und ohne Moeglichkeit, die Haelften einzeln anzuspringen.
    #[test]
    fn ein_mehrwortiger_name_wird_zu_mehreren_woertern() {
        let mut response = response(vec![
            word("bei", 0.0, 0.2),
            word("NOR", 0.2, 0.6),
            word("Druck", 0.6, 1.0),
            word("Technik.", 1.0, 1.6),
        ]);
        apply_to_response(&mut response, &liste(), Options::default());

        let words = &response.results.channels[0].alternatives[0].words;
        assert_eq!(words.len(), 3, "bei + NOR + Drucktechnik.");
        assert_eq!(words[1].word, "NOR");
        assert_eq!(words[2].word, "Drucktechnik");
        assert_eq!(words[2].punctuated_word.as_deref(), Some("Drucktechnik."));

        for entry in words {
            assert!(
                !entry.word.contains(' ')
                    && !entry.punctuated_word.as_deref().unwrap().contains(' '),
                "kein Wort traegt ein Leerzeichen: {entry:?}"
            );
        }

        // Die gemessenen Raender der Gruppe bleiben erhalten, und die Zeiten
        // laufen ohne Ueberschneidung durch.
        assert_eq!(words[1].start, 0.2);
        assert_eq!(words[2].end, 1.6);
        assert!(words[1].end <= words[2].start);
        assert!(words[1].end > words[1].start);

        assert_eq!(
            response.results.channels[0].alternatives[0].transcript,
            "bei NOR Drucktechnik."
        );
    }

    /// Eine Verhoerung, die selbst gewoehnliche deutsche Sprache ist, wird
    /// nicht angewandt -- und sie wird gemeldet, statt still zu verschwinden.
    #[test]
    fn eine_abgelehnte_verhoerung_wird_gemeldet() {
        let vocabulary = Vocabulary::parse(["Glinck => Glink; Kling"]);
        let ereignisse = protokoll_von(|| {
            let mut response = response(vec![
                word("Es", 0.0, 0.2),
                word("macht", 0.2, 0.5),
                word("Kling", 0.5, 0.9),
                word("und", 0.9, 1.1),
            ]);
            assert_eq!(
                apply_to_response(&mut response, &vocabulary, Options::default()),
                0
            );
            assert_eq!(
                response.results.channels[0].alternatives[0].transcript,
                "Es macht Kling und"
            );
        });
        assert!(
            ereignisse
                .iter()
                .any(|(message, _)| message == "vocabulary_alias_rejected_common_word"),
            "die Ablehnung muss im Protokoll stehen: {ereignisse:?}"
        );
    }

    /// Faengt die Meldungen samt der Zahl `anarlog.vocabulary.replacements`.
    #[derive(Default)]
    struct MeldungsFaenger {
        message: String,
        replacements: Option<u64>,
    }

    impl tracing::field::Visit for MeldungsFaenger {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.message = format!("{value:?}").trim_matches('"').to_string();
            }
        }

        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            if field.name() == "message" {
                self.message = value.to_string();
            }
        }

        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            if field.name() == "anarlog.vocabulary.replacements" {
                self.replacements = Some(value);
            }
        }
    }

    /// Meldung und, falls vorhanden, die Zahl der Ersetzungen.
    type Ereignis = (String, Option<u64>);

    thread_local! {
        /// Nur gefuellt, solange auf DIESEM Faden ein `protokoll_von` laeuft.
        static GESAMMELT: std::cell::RefCell<Option<Vec<Ereignis>>> =
            const { std::cell::RefCell::new(None) };
    }

    struct Protokoll;

    impl<S: tracing::Subscriber> tracing_subscriber::layer::Layer<S> for Protokoll {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            GESAMMELT.with(|slot| {
                let mut slot = slot.borrow_mut();
                let Some(liste) = slot.as_mut() else {
                    return;
                };
                let mut faenger = MeldungsFaenger::default();
                event.record(&mut faenger);
                liste.push((faenger.message, faenger.replacements));
            });
        }
    }

    /// Laesst `arbeit` laufen und gibt die dabei gemeldeten Ereignisse zurueck.
    ///
    /// Der Empfaenger wird EINMAL global gesetzt, nicht je Aufruf als
    /// Faden-Empfaenger. Der Grund ist gemessen, nicht vermutet: `tracing`
    /// merkt sich je Meldestelle, ob ueberhaupt jemand zuhoert. Laeuft ein
    /// anderer Test derselben Datei durch dieselbe Meldestelle, waehrend kein
    /// globaler Empfaenger gesetzt ist -- und die Tests laufen nebenlaeufig --,
    /// steht dort "niemand hoert zu", und ein Faden-Empfaenger bekommt danach
    /// nichts mehr. Genau so ist `ein_nachlauf_ohne_treffer_meldet_sich_trotzdem`
    /// am 02.09.2026 rot geworden: allein gruen, im Rudel leer, und zwar
    /// unzuverlaessig. Ein Test, dessen Ergebnis von der Laufreihenfolge
    /// abhaengt, ist keine Messung.
    ///
    /// `set_global_default` baut den Merker beim Setzen neu auf und wird nie
    /// wieder abgeraeumt -- ab dem ersten Aufruf ist die Sache entschieden.
    /// Welcher Faden gerade sammelt, entscheidet dann `GESAMMELT`.
    fn protokoll_von(arbeit: impl FnOnce()) -> Vec<Ereignis> {
        use tracing_subscriber::layer::SubscriberExt;

        static EINMAL: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        EINMAL.get_or_init(|| {
            tracing::subscriber::set_global_default(tracing_subscriber::registry().with(Protokoll))
                .expect("kein anderer Test in dieser Kiste darf den Empfaenger setzen");
        });

        GESAMMELT.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
        arbeit();
        GESAMMELT.with(|slot| slot.borrow_mut().take().unwrap_or_default())
    }

    #[test]
    fn ein_nachlauf_ohne_treffer_meldet_sich_trotzdem() {
        // Die wichtigste Zeile dieser Datei fuer die Auswertung: sonst ist
        // "null Ersetzungen" nicht von "der Nachlauf lief gar nicht" zu
        // unterscheiden, und die Null waere keine Messung.
        let ereignisse = protokoll_von(|| {
            let mut response = response(vec![word("Wir", 0.0, 0.3), word("werden", 0.3, 0.8)]);
            assert_eq!(
                apply_to_response(&mut response, &liste(), Options::default()),
                0
            );
        });

        assert_eq!(
            ereignisse
                .iter()
                .find(|(message, _)| message == "vocabulary_pass_completed")
                .map(|(_, replacements)| *replacements),
            Some(Some(0)),
            "erwartet wird eine Meldung mit 0 Ersetzungen, gesehen: {ereignisse:?}"
        );
    }

    #[test]
    fn ohne_liste_meldet_sich_niemand() {
        // Die Gegenprobe: wo gar keine Liste da ist, gibt es auch nichts zu
        // melden -- sonst stuende in jedem Lauf ohne Stichwortliste eine Zeile
        // ueber einen Nachlauf, den es nicht gab.
        let ereignisse = protokoll_von(|| {
            let mut response = response(vec![word("Sedatsch", 0.0, 0.5)]);
            apply_to_response(&mut response, &Vocabulary::default(), Options::default());
        });
        assert!(
            !ereignisse
                .iter()
                .any(|(message, _)| message == "vocabulary_pass_completed"),
            "unerwartete Meldung: {ereignisse:?}"
        );
        // Aber die leere Liste bleibt vom "nie gelaufen" unterscheidbar --
        // unter eigenem Namen, denn gelaufen ist hier nichts.
        assert!(
            ereignisse
                .iter()
                .any(|(message, _)| message == "vocabulary_pass_skipped_empty_list"),
            "die leere Liste muss sich melden: {ereignisse:?}"
        );
    }

    /// Der Fall, an dem eine je Planzeile gedachte Eindeutigkeitspruefung
    /// umfaellt: dieselbe Verhoerung kommt ZWEIMAL vor. Der Plan traegt dann
    /// zwei Zeilen mit derselben Vorlage, und beide Vorkommen muessen
    /// korrigiert werden -- gerade die Namen, die das Modell wiederholt
    /// verhoert, sind die wertvollsten Eintraege der Liste.
    #[test]
    fn eine_mehrfach_vorkommende_verhoerung_wird_ueberall_korrigiert() {
        let mut response = response(vec![
            word("Wir", 0.0, 0.2),
            word("haben", 0.2, 0.5),
            word("mit", 0.5, 0.7),
            word("Sedatsch", 0.7, 1.3),
            word("gesprochen,", 1.3, 1.9),
            word("und", 1.9, 2.1),
            word("Sedatsch", 2.1, 2.7),
            word("meinte", 2.7, 3.1),
        ]);
        // Ein anders formatierter Fliesstext, damit der Weg ueber
        // `apply_plan_to_text` genommen wird und nicht das Neubauen.
        response.results.channels[0].alternatives[0].transcript =
            "Wir haben mit Sedatsch gesprochen, und Sedatsch meinte...".to_string();

        assert_eq!(
            apply_to_response(&mut response, &liste(), Options::default()),
            2
        );
        assert_eq!(
            response.results.channels[0].alternatives[0].transcript,
            "Wir haben mit Sedacz gesprochen, und Sedacz meinte..."
        );
        let words = &response.results.channels[0].alternatives[0].words;
        assert_eq!(words[3].word, "Sedacz");
        assert_eq!(words[6].word, "Sedacz");
    }

    /// Und die Vorsicht bleibt: ein Vorkommen, das der Plan NICHT vorgesehen
    /// hat, laesst alles stehen. Hier steht die Verhoerung dreimal im Text,
    /// aber nur zweimal in der Wortliste.
    #[test]
    fn ein_vorkommen_das_der_plan_nicht_kennt_blockt_alles() {
        let vorher = {
            let mut response = response(vec![
                word("mit", 0.0, 0.3),
                word("Sedatsch", 0.3, 0.9),
                word("und", 0.9, 1.1),
                word("Sedatsch", 1.1, 1.7),
            ]);
            response.results.channels[0].alternatives[0].transcript =
                "Sedatsch, mit Sedatsch und Sedatsch".to_string();
            response
        };
        let mut nachher = vorher.clone();
        assert_eq!(
            apply_to_response(&mut nachher, &liste(), Options::default()),
            0
        );
        assert_eq!(
            serde_json::to_string(&vorher).unwrap(),
            serde_json::to_string(&nachher).unwrap()
        );
    }

    #[test]
    fn alle_kanaele_werden_bearbeitet() {
        let mut response = response(vec![word("Sedatsch", 0.0, 0.5)]);
        let second = response.results.channels[0].clone();
        response.results.channels.push(second);
        assert_eq!(
            apply_to_response(&mut response, &liste(), Options::default()),
            2
        );
    }
}
