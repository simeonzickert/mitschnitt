//! Die eingetragene Stichwortliste, angewandt auf ein fertiges Transkript.
//!
//! Zweck: Eigennamen, die das Modell verhoert hat, wieder herstellen -- ohne
//! dabei ein gewoehnliches deutsches Wort kaputtzumachen. Die zweite Haelfte
//! des Satzes ist die wichtigere. Am 16.07.2026 wurde die akustische
//! Namenskorrektur (ZICK-139) genau deshalb abgeschaltet: sie haette 63
//! gewoehnliche deutsche Woerter ersetzt.
//!
//! Aufbau in drei Stufen, absteigend nach Sicherheit:
//!
//! 1. **Alias** -- ein Mensch hat gesagt, dass "Sedatsch" ein verhoertes
//!    "Sedacz" ist. Wortgenauer Vergleich, keine Vermutung. Fehlalarme sind
//!    hier nur moeglich, wenn die Liste selbst falsch ist.
//! 2. **Alias-Klang** -- der Koelner-Phonetik-Code der bekannten Verhoerer
//!    faengt weitere Varianten derselben Verhoerung ("Zedatsch"). Die
//!    Suchflaeche bleibt auf das beschraenkt, was schon als Muell erkannt ist.
//! 3. **Namens-Klang** -- der Code des kanonischen Namens selbst. Das ist die
//!    Stufe, die 2026 explodiert ist, und sie laeuft deshalb nur unter vier
//!    Waechtern (siehe [`matcher`]).
//!
//! Was hier bewusst NICHT passiert: kein Sprachmodell. Dieselbe Studie, die den
//! phonetischen Gewinn misst (arXiv 2506.10779), misst Sprachmodell-Korrektur
//! OHNE phonetischen Vorfilter bei 43,4 % Namensfehler -- schlechter als gar
//! keine Korrektur. Ein Modell, das raten darf, fabuliert.

pub mod koelner;
mod matcher;
pub mod proposals;
mod stopwords;
pub mod tokens;

pub use koelner::{encode, encode_phrase, levenshtein};
pub use matcher::{MatchKind, Options, Replacement, Token, apply};
pub use proposals::{Proposal, ProposalKind, ScanSession, ScanWord, scan};
pub use stopwords::{is_all_common_german_phrase, is_common_german_word};

/// Eine eingetragene Verhoerung, die der Nachlauf NICHT anwendet, weil sie
/// selbst gewoehnliche deutsche Sprache ist.
///
/// Kein stiller Ausgang: der Mensch, der sie eingetragen hat, soll erfahren,
/// dass sein Eintrag nicht wirkt -- eine Warnung ist ein besseres Ergebnis als
/// eine Ersetzung, die ihm mitten im Satz ein Wort wegnimmt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskyAlias {
    /// Die richtige Schreibweise, auf die der Eintrag zeigen wuerde.
    pub canonical: String,
    /// Die abgelehnte Verhoerung, so wie sie in der Liste steht.
    pub alias: String,
}

/// Ein Eintrag der Stichwortliste: die richtige Schreibweise und die
/// Verhoerungen, die auf sie zeigen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Die Schreibweise, die im Transkript stehen soll.
    pub canonical: String,
    /// Bekannte Verhoerungen, die auf `canonical` zurueckgeschrieben werden.
    pub aliases: Vec<String>,
}

/// Die geparste Stichwortliste.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vocabulary {
    entries: Vec<Entry>,
}

/// Vergleicht zwei Schreibweisen so, wie der Nachlauf sie spaeter vergleicht.
///
/// `eq_ignore_ascii_case` waere das Naheliegende und ist falsch: der Index in
/// [`matcher`] schlaegt ueber `to_lowercase` nach, und die beiden gehen bei
/// allem auseinander, was nicht ASCII ist. Bei `MUELLERMANN` gegen
/// `Muellermann` mit Umlaut wuerden die Eintraege hier NICHT zusammengelegt,
/// der Index saehe danach zwei Ziele fuer dieselbe Verhoerung -- und
/// `unique_target` gibt bei zwei Zielen nichts zurueck. Der Alias hoerte
/// stumm auf zu wirken.
fn gleich(links: &str, rechts: &str) -> bool {
    links.to_lowercase() == rechts.to_lowercase()
}

/// Trennt die richtige Schreibweise von ihren Verhoerungen.
const ALIAS_ARROW: &str = "=>";
/// Trennt die Verhoerungen untereinander.
const ALIAS_SEPARATOR: char = ';';

impl Vocabulary {
    /// Liest die Liste, wie sie in den Einstellungen steht.
    ///
    /// Zwei Zeilenformen, beide aus des Betreibers vorhandenen Dateien:
    ///
    /// ```text
    /// Sedacz => Sedatsch; Sedatz; Seda das
    /// Webflow
    /// ```
    ///
    /// Eine Zeile ohne Pfeil ist ein Eintrag ohne bekannte Verhoerungen. Leere
    /// Zeilen, Zeilen ohne kanonischen Teil und leere Aliasstuecke fallen weg.
    pub fn parse<S: AsRef<str>>(lines: impl IntoIterator<Item = S>) -> Self {
        let mut entries: Vec<Entry> = Vec::new();

        for line in lines {
            let line = line.as_ref().trim();
            if line.is_empty() {
                continue;
            }

            let (canonical, alias_part) = match line.split_once(ALIAS_ARROW) {
                Some((left, right)) => (left.trim(), right),
                None => (line, ""),
            };
            if canonical.is_empty() {
                continue;
            }

            let aliases: Vec<String> = alias_part
                .split(ALIAS_SEPARATOR)
                .map(str::trim)
                .filter(|alias| !alias.is_empty())
                // Ein Alias, der dem kanonischen Namen gleicht, ist ein
                // Schreibfehler in der Liste und waere eine Ersetzung durch
                // sich selbst.
                .filter(|alias| !gleich(alias, canonical))
                .map(str::to_string)
                .collect();

            // Zweimal derselbe Name in der Liste: Aliasse zusammenlegen statt
            // den zweiten Eintrag zu verlieren.
            match entries
                .iter_mut()
                .find(|entry| gleich(&entry.canonical, canonical))
            {
                Some(existing) => {
                    for alias in aliases {
                        if !existing.aliases.iter().any(|known| gleich(known, &alias)) {
                            existing.aliases.push(alias);
                        }
                    }
                }
                None => entries.push(Entry {
                    canonical: canonical.to_string(),
                    aliases,
                }),
            }
        }

        Self { entries }
    }

    /// Nur die richtigen Schreibweisen -- das, was ein Anbieter als Hinweis
    /// bekommt und was in Whispers Kontextvorgabe geht.
    ///
    /// Die Verhoerungen duerfen dort NICHT hin: einem Modell "Sedatsch" als
    /// erwuenschtes Wort vorzulegen, waere das Gegenteil der Absicht.
    pub fn canonical_terms(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.canonical.clone())
            .collect()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Die Verhoerungen, die der Nachlauf ablehnt, weil sie selbst
    /// gewoehnliche deutsche Sprache sind.
    ///
    /// Der Aufrufer meldet sie -- ins Protokoll und, sobald die Oberflaeche
    /// Verhoerungen ueberhaupt kennt, an den Menschen. Der Nachlauf selbst
    /// wendet sie nicht an; siehe [`matcher`].
    pub fn risky_aliases(&self) -> Vec<RiskyAlias> {
        self.entries
            .iter()
            .flat_map(|entry| {
                entry
                    .aliases
                    .iter()
                    .filter(|alias| stopwords::is_all_common_german_phrase(alias))
                    .map(|alias| RiskyAlias {
                        canonical: entry.canonical.clone(),
                        alias: alias.clone(),
                    })
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeile_ohne_pfeil_ist_ein_eintrag_ohne_verhoerungen() {
        let vocabulary = Vocabulary::parse(["Webflow"]);
        assert_eq!(
            vocabulary.entries(),
            [Entry {
                canonical: "Webflow".to_string(),
                aliases: vec![],
            }]
        );
    }

    #[test]
    fn das_aliasformat_wird_gelesen() {
        let vocabulary =
            Vocabulary::parse(["Sedacz => Sedatsch; Sedatz; Seda das; Cedatsch"]);
        let entry = &vocabulary.entries()[0];
        assert_eq!(entry.canonical, "Sedacz");
        assert_eq!(
            entry.aliases,
            ["Sedatsch", "Sedatz", "Seda das", "Cedatsch"]
        );
    }

    #[test]
    fn leerzeilen_und_leere_aliasstuecke_fallen_weg() {
        let vocabulary =
            Vocabulary::parse(["", "   ", "Glinck => Glink; ; Kling;", "=> nur Alias"]);
        assert_eq!(vocabulary.len(), 1);
        assert_eq!(vocabulary.entries()[0].aliases, ["Glink", "Kling"]);
    }

    #[test]
    fn ein_alias_das_dem_namen_gleicht_wird_verworfen() {
        // Sonst stuende eine Ersetzung durch sich selbst im Index.
        let vocabulary = Vocabulary::parse(["Noormann => Nohrmann; Noormann"]);
        assert_eq!(vocabulary.entries()[0].aliases, ["Nohrmann"]);
    }

    #[test]
    fn derselbe_name_zweimal_legt_die_aliasse_zusammen() {
        let vocabulary = Vocabulary::parse(["Glinck => Glink", "glinck => Kling"]);
        assert_eq!(vocabulary.len(), 1);
        assert_eq!(vocabulary.entries()[0].aliases, ["Glink", "Kling"]);
    }

    /// Dasselbe mit Umlaut. `eq_ignore_ascii_case` haette hier zwei Eintraege
    /// stehen lassen; der Index in [`matcher`] schlaegt danach ueber
    /// `to_lowercase` nach, saehe zwei Ziele fuer dieselbe Verhoerung und
    /// liefert bei Uneindeutigkeit gar nichts -- der Alias haette stumm
    /// aufgehoert zu wirken.
    #[test]
    fn die_zusammenlegung_haelt_auch_ausserhalb_von_ascii() {
        let vocabulary =
            Vocabulary::parse(["MÜLLERMANN => Myllermann", "Müllermann => Muellermann"]);
        assert_eq!(vocabulary.len(), 1);
        assert_eq!(
            vocabulary.entries()[0].aliases,
            ["Myllermann", "Muellermann"]
        );

        // Und der Selbst-Alias-Filter ebenso.
        let selbst = Vocabulary::parse(["Müllermann => MÜLLERMANN; Myllermann"]);
        assert_eq!(selbst.entries()[0].aliases, ["Myllermann"]);
    }

    /// Die Gegenprobe zur Schreibseite in TypeScript.
    ///
    /// Diese drei Zeilen sind nicht von Hand getippt: sie stammen woertlich aus
    /// `formatDictionaryEntry` und `addDictionaryAlias`
    /// (`apps/desktop/src/stt/dictionary-entry.ts`) und stehen hier, damit der
    /// Bau bricht, wenn eine der beiden Seiten das Format aendert. Ohne diesen
    /// Test wuerde ein verschobenes Trennzeichen niemandem auffallen -- der
    /// Nachlauf wuerde einfach nichts mehr ersetzen und dabei "erfolgreich"
    /// melden.
    ///
    /// Neu erzeugen: die drei Aufrufe in einem vitest-Fall ausfuehren und ihr
    /// Ergebnis hier eintragen.
    #[test]
    fn was_die_oberflaeche_schreibt_liest_dieser_parser() {
        let vocabulary = Vocabulary::parse([
            "Sedacz => Sarec; Seda das",
            "Nordwerk => Nordfall",
            "NOR Drucktechnik => NOR Druck Technik",
        ]);

        assert_eq!(
            vocabulary.entries(),
            [
                Entry {
                    canonical: "Sedacz".to_string(),
                    aliases: vec!["Sarec".to_string(), "Seda das".to_string()],
                },
                Entry {
                    canonical: "Nordwerk".to_string(),
                    aliases: vec!["Nordfall".to_string()],
                },
                Entry {
                    canonical: "NOR Drucktechnik".to_string(),
                    aliases: vec!["NOR Druck Technik".to_string()],
                },
            ]
        );

        // Und der Weg zum Modell fuehrt keine Verhoerung mit.
        assert_eq!(
            vocabulary.canonical_terms(),
            ["Sedacz", "Nordwerk", "NOR Drucktechnik"]
        );

        // Der Punkt der Uebung: die geschriebene Zeile ersetzt auch wirklich.
        // Ein Parser-Test allein bewiese nur, dass die Zeile lesbar ist -- und
        // genau das war am 02.09.2026 der Zustand, in dem 20 Eintraege null
        // Ersetzungen ergaben.
        let tokens: Vec<Token> = "Termin mit Sarec bei Nordfall"
            .split_whitespace()
            .map(Token::new)
            .collect();
        let plan = apply(&vocabulary, &tokens, Options::default());
        let ersetzt: Vec<&str> = plan
            .iter()
            .map(|replacement| replacement.after.as_str())
            .collect();
        assert_eq!(ersetzt, ["Sedacz", "Nordwerk"]);
    }

    #[test]
    fn anbieter_bekommen_nur_die_richtigen_schreibweisen() {
        let vocabulary = Vocabulary::parse(["Sedacz => Sedatsch", "Webflow"]);
        assert_eq!(vocabulary.canonical_terms(), ["Sedacz", "Webflow"]);
    }
}
