//! Der Nachlauf ueber das fertige Transkript.
//!
//! Arbeitet auf einer flachen Wortfolge und gibt einen PLAN zurueck, keine
//! veraenderte Folge: welche Woerter durch welchen Text ersetzt werden. Der
//! Aufrufer fuehrt ihn aus, weil nur er weiss, was ausser dem Text noch am Wort
//! haengt (Zeitmarken, Kanal, Sprecher).
//!
//! Die Reihenfolge ist absichtlich gierig von lang nach kurz: "NOR Druck
//! Technik" muss als Dreiergruppe gefunden werden, bevor "Technik" allein
//! irgendetwas ausloest.

use crate::{Vocabulary, koelner, stopwords};

/// Ein Wort des Transkripts, so wie ein Mensch es liest -- mit Satzzeichen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
}

impl Token {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Auf welchem Weg ein Treffer zustande kam. Steht im Protokoll, damit sich
/// hinterher messen laesst, welche Stufe wie viel bringt -- und welche Schaden
/// anrichtet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Wortgenau gegen eine vom Menschen eingetragene Verhoerung.
    Alias,
    /// Gleicher Klang wie eine eingetragene Verhoerung.
    AliasPhonetic,
    /// Gleicher Klang wie der richtige Name selbst.
    CanonicalPhonetic,
}

/// Eine geplante Ersetzung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    /// Erster betroffener Wortindex.
    pub start: usize,
    /// Anzahl der Woerter, die ersetzt werden.
    pub len: usize,
    /// Der Text, der dort stand (mit Satzzeichen, zum Nachlesen im Protokoll).
    pub before: String,
    /// Der Text, der dort stehen soll (Satzzeichen der Vorlage uebernommen).
    ///
    /// Kann Leerzeichen enthalten -- der Eintrag `NOR Drucktechnik` ist
    /// zweiwortig. Der Aufrufer macht daraus ZWEI Woerter mit eigenen Zeiten,
    /// nicht ein Wort mit einem Leerzeichen darin.
    pub after: String,
    pub kind: MatchKind,
}

/// Welche Stufen laufen duerfen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Stufe 2: Klangvergleich gegen die eingetragenen Verhoerungen.
    pub alias_phonetics: bool,
    /// Stufe 3: Klangvergleich gegen den richtigen Namen selbst.
    ///
    /// Die Stufe mit dem groessten Risiko und deshalb in der Vorgabe AUS:
    /// gemessen am 02.09.2026 waren 20 ihrer 31 Ersetzungen Fehlgriffe. Die
    /// Begruendung im Ganzen steht an [`Options::default`].
    pub canonical_phonetics: bool,
    /// Groesste Wortfolge, die als ein Begriff geprueft wird.
    pub max_phrase_tokens: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            // BEIDE Klangstufen sind AUS. Das ist keine Vorsicht auf Verdacht,
            // sondern das Ergebnis der Messung vom 02.09.2026: `scan` ueber
            // 1.886.759 Woerter aus 390 echten Transkripten des Betreibers,
            // mit seiner echten Liste (20 Eintraege).
            //
            //   Stufe            Ersetzungen   davon Fehlgriffe
            //   alias                     19   0 (jede ist eine von einem
            //                                  Menschen eingetragene Zuordnung)
            //   alias_klang               59   mindestens 13 (~22 %)
            //   namens_klang              31   20 (~65 %)
            //
            // Die 20 Fehlgriffe der Namensstufe im Wortlaut: "Werten" ->
            // "Vurden" (10x), "Lieferant" -> "Lieferanto" (7x), "Clinch" ->
            // "Glinck" (3x). Die der Alias-Klangstufe unter anderem: "klinik",
            // "Klinke", "killing" -> "Glinck"; "lines", "lands", "Linus",
            // "Lenas" -> "LLMs".
            //
            // Warum sich das nicht durch schaerfere Schwellen loesen laesst:
            // "Klinik" und "Glink" liegen zwei Buchstaben auseinander und
            // klingen identisch. Keine Abstandsschranke kann ein echtes Wort
            // von einer Verhoerung trennen, wenn beide gleich klingen und
            // gleich geschrieben sind -- das kann nur ein Woerterbuch, und die
            // Waechterliste in [`crate::stopwords`] ist keines (1.048 Woerter
            // von Hand; ihr fehlten alle oben genannten).
            //
            // NACHGETRAGEN 04.09.2026, damit niemand hier das Falsche
            // schliesst: seit diesem Tag LIEGT ein deutsches Woerterbuch im
            // Baum (`wortliste-de.txt`, 38.424 Wortformen). Es haengt
            // ABSICHTLICH nicht an dieser Stelle, sondern nur am
            // Vorschlags-Lauf -- dort raet eine Maschine, hier wird eine von
            // einem Menschen getippte Entscheidung ausgefuehrt, und die
            // Fehlerrichtungen sind entgegengesetzt (siehe
            // `stopwords::is_ordinary_german_word`).
            //
            // Wer diese Schalter einschalten will, hat jetzt zwar das
            // Woerterbuch, aber noch nicht die Messung: von den zehn oben
            // namentlich genannten Fehlgriffen deckt die neue Liste GEMESSEN
            // drei ab ("werten", "lieferant", "klinik"), sieben nicht --
            // darunter "Klinke" und "lines". Einschalten bleibt also eine
            // eigene Strecke mit eigenem Vorher/Nachher am echten Korpus.
            //
            // Ein zerstoertes Alltagswort ist teurer als zehn gefundene Namen:
            // Ein Mensch liest das Transkript, er zaehlt keine Trefferquoten. Die
            // Stufen bleiben im Code und sind ueber diese Schalter messbar --
            // eingeschaltet werden sie, wenn ein echtes deutsches Woerterbuch
            // hinter dem Waechter steht. Genau daran ist die
            // Vorgaengerfunktion am 16.07.2026 gestorben (ZICK-139, 63
            // gewoehnliche Woerter).
            alias_phonetics: false,
            canonical_phonetics: false,
            // der laengste Eintrag ist zweiwortig ("NOR Drucktechnik"),
            // seine laengste Verhoerung dreiwortig ("NOR Druck Technik").
            max_phrase_tokens: 3,
        }
    }
}

impl Options {
    /// Alle Stufen an -- fuer die Messbank, nicht fuer den Betrieb.
    pub fn all_stages() -> Self {
        Self {
            alias_phonetics: true,
            canonical_phonetics: true,
            ..Self::default()
        }
    }
}

/// Kuerzester Koelner-Code, den die Alias-Klangstufe akzeptiert. Zwei Ziffern
/// treffen zu viele Woerter, um eine Aussage zu sein.
const MIN_CODE_LEN_ALIAS: usize = 3;
/// Fuer die Namens-Klangstufe strenger.
const MIN_CODE_LEN_CANONICAL: usize = 4;
/// Groesster erlaubter Buchstabenabstand bei der Namens-Klangstufe.
const MAX_EDITS_CANONICAL: usize = 2;

/// Untergrenze des erlaubten Buchstabenabstands bei der Alias-Klangstufe. Ein
/// Verhoerer weicht typisch um ein bis zwei Buchstaben ab ("Zedatsch" gegen
/// "Sedatsch": 1).
const MIN_EDITS_ALIAS: usize = 2;
/// Anteil der Alias-Laenge, den der Abstand hoechstens ausmachen darf. Bei
/// einem zehn Buchstaben langen Alias sind das vier.
const MAX_EDIT_SHARE_ALIAS: f64 = 0.4;

/// Groesster erlaubter Abstand zwischen gefundenem Text und eingetragener
/// Verhoerung.
///
/// GEMESSEN, nicht geschaetzt (02.09.2026, `scan` ueber 114.517 Woerter aus 28
/// vorhandenen Transkripten mit der echten Liste des Betreibers): ohne diese Schranke
/// wurde `"so.Das"` zu `"Sedacz"`. Beide haben den Koelner-Code `828` --
/// die Koelner Phonetik dampft "Sedatsch" auf drei Ziffern ein, und auf drei
/// Ziffern passt irgendwann alles. Der Klang allein ist zu wenig Beweis; die
/// Schreibweise muss in derselben Gegend liegen.
///
/// Die Namens-Klangstufe hatte diese Schranke von Anfang an
/// ([`MAX_EDITS_CANONICAL`]), die Alias-Stufe nicht -- das war die Luecke.
fn max_edits_alias(alias_len: usize) -> usize {
    MIN_EDITS_ALIAS.max((alias_len as f64 * MAX_EDIT_SHARE_ALIAS) as usize)
}

/// Baut den Ersetzungsplan.
pub fn apply(vocabulary: &Vocabulary, tokens: &[Token], options: Options) -> Vec<Replacement> {
    if vocabulary.is_empty() || tokens.is_empty() {
        return Vec::new();
    }

    let index = Index::build(vocabulary);
    let mut plan = Vec::new();
    let mut position = 0usize;

    while position < tokens.len() {
        let mut matched = None;
        let longest = options.max_phrase_tokens.min(tokens.len() - position);

        // Gierig von lang nach kurz.
        for len in (1..=longest).rev() {
            let slice = &tokens[position..position + len];
            if !is_joinable(slice) {
                continue;
            }
            let cores: Vec<&str> = slice.iter().map(|token| core_of(&token.text)).collect();
            if cores.iter().any(|core| core.is_empty()) {
                continue;
            }
            let phrase = cores.join(" ");

            // Steht der richtige Name schon da: Woerter verbrauchen, damit
            // keine kuerzere Stufe daran herumfummelt, aber nichts aendern.
            if index.is_canonical(&phrase) {
                matched = Some((len, None));
                break;
            }

            if let Some((canonical, kind)) = index.lookup(&phrase, &cores, options) {
                let after = format!(
                    "{}{}{}",
                    prefix_of(&slice[0].text),
                    canonical,
                    suffix_of(&slice[len - 1].text)
                );
                let before = slice
                    .iter()
                    .map(|token| token.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                matched = Some((
                    len,
                    Some(Replacement {
                        start: position,
                        len,
                        before,
                        after,
                        kind,
                    }),
                ));
                break;
            }
        }

        match matched {
            Some((len, replacement)) => {
                if let Some(replacement) = replacement {
                    plan.push(replacement);
                }
                position += len;
            }
            None => position += 1,
        }
    }

    plan
}

/// Ein Name laeuft nicht ueber eine Satzgrenze. Endet ein Wort INNERHALB der
/// Gruppe mit einem Satzzeichen, ist die Gruppe keine Einheit.
fn is_joinable(slice: &[Token]) -> bool {
    slice[..slice.len().saturating_sub(1)]
        .iter()
        .all(|token| !ends_sentence(&token.text))
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end()
        .chars()
        .next_back()
        .is_some_and(|character| matches!(character, '.' | '!' | '?' | ',' | ';' | ':'))
}

/// Der Wortkern ohne umgebende Satzzeichen. Der Bindestrich bleibt drin, weil
/// er in "Sales-Viewer" zum Namen gehoert.
fn core_of(text: &str) -> &str {
    text.trim_matches(|character: char| {
        !character.is_alphanumeric() && character != '-' && character != '\''
    })
}

fn prefix_of(text: &str) -> &str {
    let core = core_of(text);
    match core.is_empty() {
        true => "",
        false => {
            let offset = text.find(core).unwrap_or(0);
            &text[..offset]
        }
    }
}

fn suffix_of(text: &str) -> &str {
    let core = core_of(text);
    match core.is_empty() {
        true => "",
        false => {
            let offset = text.find(core).unwrap_or(0) + core.len();
            &text[offset..]
        }
    }
}

/// Die vorbereiteten Nachschlagetabellen.
struct Index {
    /// Kleingeschriebene richtige Schreibweisen -- reine Erkennung, keine
    /// Ersetzung.
    canonical: Vec<String>,
    /// Kleingeschriebene Verhoerung -> richtige Schreibweise.
    aliases: Vec<(String, String)>,
    /// Koelner-Code einer Verhoerung -> die Verhoerung selbst und die richtige
    /// Schreibweise. Die Verhoerung steht mit dabei, weil der Abstand zu IHR
    /// gemessen wird, nicht zum kanonischen Namen.
    alias_codes: Vec<AliasCode>,
    /// Koelner-Code eines richtigen Namens -> der Name selbst.
    canonical_codes: Vec<(String, String)>,
}

/// Eine eingetragene Verhoerung, vorbereitet fuer den Klangvergleich.
struct AliasCode {
    code: String,
    /// Kleingeschrieben -- der Abstand wird gegen diese Schreibweise gerechnet.
    alias: String,
    canonical: String,
}

impl AsTarget for AliasCode {
    fn target(&self) -> &str {
        &self.canonical
    }
}

impl Index {
    fn build(vocabulary: &Vocabulary) -> Self {
        let mut canonical = Vec::new();
        let mut aliases = Vec::new();
        let mut alias_codes = Vec::new();
        let mut canonical_codes = Vec::new();

        for entry in vocabulary.entries() {
            canonical.push(entry.canonical.to_lowercase());
            let code = code_of_phrase(&entry.canonical);
            if code_length(&code) >= MIN_CODE_LEN_CANONICAL {
                canonical_codes.push((code, entry.canonical.clone()));
            }

            for alias in &entry.aliases {
                // Der Waechter der Alias-Stufe. Bis zum 02.09.2026 hatte sie
                // gar keinen: sie verglich wortgenau und ohne Ruecksicht auf
                // Gross-/Kleinschreibung, und die Schranken weiter unten
                // gelten ausschliesslich fuer die beiden Klangstufen, die in
                // der Vorgabe AUS sind. Damit stand die einzige eingeschaltete
                // Stufe voellig ungesichert da.
                //
                // Zwei Saetze aus der eigenen Liste des Betreibers haben das gezeigt:
                // `Glinck => Kling` machte aus "Es macht Kling und die Tuer
                // geht auf." ein "Es macht Glinck ..."; `ClickUp => Klick ab`
                // machte aus "Klick ab und schliess das Fenster." ein "ClickUp
                // und ...". Beides genau die Klasse, an der am 16.07.2026 die
                // Vorgaengerfunktion gestorben ist (ZICK-139) -- nur ueber die
                // Aliasliste statt ueber den Klang wieder hereingekommen.
                //
                // Was die Schreibweise NICHT leisten kann: Der Betreiber hat "klick
                // ab" und "Klick ab" beide eingetragen. Gross/klein ist also
                // kein Signal.
                if stopwords::is_all_common_german_phrase(alias) {
                    continue;
                }
                aliases.push((alias.to_lowercase(), entry.canonical.clone()));
                let code = code_of_phrase(alias);
                if code_length(&code) >= MIN_CODE_LEN_ALIAS {
                    alias_codes.push(AliasCode {
                        code,
                        alias: alias.to_lowercase(),
                        canonical: entry.canonical.clone(),
                    });
                }
            }
        }

        Self {
            canonical,
            aliases,
            alias_codes,
            canonical_codes,
        }
    }

    fn is_canonical(&self, phrase: &str) -> bool {
        let lowered = phrase.to_lowercase();
        self.canonical.contains(&lowered)
    }

    fn lookup(
        &self,
        phrase: &str,
        cores: &[&str],
        options: Options,
    ) -> Option<(String, MatchKind)> {
        let lowered = phrase.to_lowercase();

        // Stufe 1 -- ein Mensch hat diese Verhoerung eingetragen.
        if let Some(canonical) = unique_target(&self.aliases, |(alias, _)| *alias == lowered) {
            return Some((canonical, MatchKind::Alias));
        }

        // Ab hier arbeiten nur noch die Klangstufen. Sind beide aus -- und das
        // ist die Vorgabe --, ist alles Weitere Rechenzeit ohne Wirkung: der
        // Koelner-Code wird bis zu dreimal je Wort gebildet und nie benutzt.
        if !options.alias_phonetics && !options.canonical_phonetics {
            return None;
        }

        let code = code_of_phrase(phrase);
        if code_length(&code) == 0 {
            return None;
        }

        // Stufe 2 -- gleicher Klang wie eine eingetragene Verhoerung.
        if options.alias_phonetics && code_length(&code) >= MIN_CODE_LEN_ALIAS {
            // Eine Verhoerung darf kein gewoehnliches deutsches Wort sein --
            // sonst faengt die Stufe die Sprache statt des Fehlers.
            let common = cores
                .iter()
                .any(|core| stopwords::is_common_german_word(core));
            if !common {
                let candidate = unique_target(&self.alias_codes, |known| {
                    known.code == code
                        && koelner::levenshtein(&lowered, &known.alias)
                            <= max_edits_alias(known.alias.chars().count())
                });
                if let Some(canonical) = candidate {
                    return Some((canonical, MatchKind::AliasPhonetic));
                }
            }
        }

        // Stufe 3 -- gleicher Klang wie der richtige Name. Vier Waechter.
        if options.canonical_phonetics && code_length(&code) >= MIN_CODE_LEN_CANONICAL {
            let candidate = unique_target(&self.canonical_codes, |(known, _)| *known == code);
            if let Some(canonical) = candidate {
                let common = cores
                    .iter()
                    .any(|core| stopwords::is_common_german_word(core));
                let capitalized = cores[0]
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_uppercase());
                let edits = koelner::levenshtein(&lowered, &canonical.to_lowercase());
                let close =
                    edits <= MAX_EDITS_CANONICAL && edits * 5 <= canonical.chars().count() * 2;

                if !common && capitalized && close {
                    return Some((canonical, MatchKind::CanonicalPhonetic));
                }
            }
        }

        None
    }
}

/// Gibt das Ziel nur zurueck, wenn es EINDEUTIG ist. Passen zwei Eintraege auf
/// dieselbe Stelle, weiss niemand welcher gemeint war -- dann bleibt der Text,
/// wie er ist. Nichtstun ist die sichere Fehlerrichtung.
fn unique_target<T>(table: &[T], mut matches: impl FnMut(&T) -> bool) -> Option<String>
where
    T: AsTarget,
{
    let mut found: Option<&str> = None;
    for row in table.iter().filter(|row| matches(row)) {
        let target = row.target();
        match found {
            None => found = Some(target),
            Some(existing) if existing == target => {}
            Some(_) => return None,
        }
    }
    found.map(str::to_string)
}

trait AsTarget {
    fn target(&self) -> &str;
}

impl AsTarget for (String, String) {
    fn target(&self) -> &str {
        &self.1
    }
}

fn code_of_phrase(phrase: &str) -> String {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    koelner::encode_phrase(&words)
}

/// Laenge ohne die Wortgrenzen-Striche -- die zaehlen nicht als Information.
fn code_length(code: &str) -> usize {
    code.chars().filter(char::is_ascii_digit).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> Vec<Token> {
        text.split_whitespace().map(Token::new).collect()
    }

    fn liste_des_betreibers() -> Vocabulary {
        Vocabulary::parse([
            "Sedacz => Sedatsch; Sedatz; Seda das; Cedatsch",
            "Nordwerk => Nordwerker; Nord Werk",
            "Phonowerk => Phono Werk; Phono-Werk",
            "Glinck => Glink; Kling",
            "Noormann => Nohrmann",
            "NOR Drucktechnik => NOR Druck Technik; Nor Drucktechnik",
            "ClickUp => klick ab; Klick ab; Click ab; klickab",
            "Vurden",
            "Webflow",
            "Lexware",
        ])
    }

    fn apply_text(vocabulary: &Vocabulary, text: &str, options: Options) -> String {
        let tokens = tokens(text);
        let plan = apply(vocabulary, &tokens, options);
        let mut out: Vec<String> = Vec::new();
        let mut position = 0usize;
        for replacement in &plan {
            while position < replacement.start {
                out.push(tokens[position].text.clone());
                position += 1;
            }
            out.push(replacement.after.clone());
            position += replacement.len;
        }
        while position < tokens.len() {
            out.push(tokens[position].text.clone());
            position += 1;
        }
        out.join(" ")
    }

    #[test]
    fn eingetragene_verhoerung_wird_ersetzt() {
        let vocabulary = liste_des_betreibers();
        assert_eq!(
            apply_text(
                &vocabulary,
                "Wir haben mit Sedatsch gesprochen",
                Options::default()
            ),
            "Wir haben mit Sedacz gesprochen"
        );
    }

    #[test]
    fn satzzeichen_bleiben_stehen() {
        let vocabulary = liste_des_betreibers();
        assert_eq!(
            apply_text(
                &vocabulary,
                "Das war Nordwerker, oder?",
                Options::default()
            ),
            "Das war Nordwerk, oder?"
        );
    }

    #[test]
    fn mehrwortige_verhoerung_wird_zu_einem_wort() {
        let vocabulary = liste_des_betreibers();
        assert_eq!(
            apply_text(
                &vocabulary,
                "die Phono Werk in Hamburg",
                Options::default()
            ),
            "die Phonowerk in Hamburg"
        );
        // Gierig: die Dreiergruppe gewinnt gegen die Zweiergruppe.
        assert_eq!(
            apply_text(&vocabulary, "bei NOR Druck Technik.", Options::default()),
            "bei NOR Drucktechnik."
        );
    }

    #[test]
    fn klang_faengt_eine_variante_die_nicht_in_der_liste_steht() {
        // "Zedatsch" steht nirgends -- klingt aber wie "Sedatsch".
        let vocabulary = liste_des_betreibers();
        assert_eq!(
            koelner::encode("Zedatsch"),
            koelner::encode("Sedatsch"),
            "Voraussetzung des Tests"
        );
        assert_eq!(
            apply_text(&vocabulary, "Herr Zedatsch war da", Options::all_stages()),
            "Herr Sedacz war da"
        );
    }

    #[test]
    fn der_ortsname_frisst_das_hilfsverb_nicht() {
        // DER Test, an dem alles haengt: "Vurden" und "werden" haben denselben
        // Koelner-Code. Am 16.07.2026 hat genau diese Klasse von Fehlern die
        // Vorgaengerfunktion (ZICK-139) das Leben gekostet.
        let vocabulary = liste_des_betreibers();
        assert_eq!(koelner::encode("werden"), koelner::encode("Vurden"));
        for satz in [
            "Das wird noch werden",
            "Werden wir das schaffen",
            "Wir werden sehen",
        ] {
            assert_eq!(apply_text(&vocabulary, satz, Options::all_stages()), satz);
        }
    }

    #[test]
    fn gewoehnliche_woerter_bleiben_auch_wenn_sie_wie_ein_name_klingen() {
        let vocabulary = liste_des_betreibers();
        for satz in [
            "Das klingt gut",
            "Klingt gut",
            "Er kann kommen",
            "Wie hat das geklungen",
        ] {
            assert_eq!(apply_text(&vocabulary, satz, Options::all_stages()), satz);
        }
    }

    #[test]
    fn kleingeschriebenes_wird_von_der_namensstufe_nie_angefasst() {
        // Deutsche Eigennamen sind gross. Ein kleingeschriebenes Wort kann
        // keiner sein -- diese Regel allein faengt den grossen Teil.
        let vocabulary = Vocabulary::parse(["Wehblau"]);
        let options = Options::all_stages();
        assert_eq!(
            apply_text(&vocabulary, "das ist wehblau", options),
            "das ist wehblau"
        );
    }

    #[test]
    fn namensstufe_repariert_eine_kleine_verhoerung() {
        let vocabulary = liste_des_betreibers();
        // "Lexwahre" klingt wie "Lexware", ist gross geschrieben, ist kein
        // deutsches Wort und liegt einen Buchstaben daneben.
        assert_eq!(koelner::encode("Lexwahre"), koelner::encode("Lexware"));
        assert_eq!(koelner::levenshtein("lexwahre", "lexware"), 1);
        assert_eq!(
            apply_text(&vocabulary, "Das liegt in Lexwahre", Options::all_stages()),
            "Das liegt in Lexware"
        );
    }

    #[test]
    fn namensstufe_haelt_bei_zu_grossem_abstand_still() {
        let vocabulary = Vocabulary::parse(["Noormann"]);
        let options = Options::all_stages();
        // Gleicher Klang, aber drei Buchstaben Abstand -- zu weit, um zu raten.
        let satz = "Nuhrmahn";
        assert_eq!(koelner::encode(satz), koelner::encode("Noormann"));
        assert!(koelner::levenshtein(&satz.to_lowercase(), "noormann") > MAX_EDITS_CANONICAL);
        assert_eq!(apply_text(&vocabulary, satz, options), satz);
    }

    #[test]
    fn ein_richtig_geschriebener_name_bleibt_unangetastet() {
        let vocabulary = liste_des_betreibers();
        let satz = "Sedacz und Nordwerk und Phonowerk";
        assert_eq!(apply_text(&vocabulary, satz, Options::default()), satz);
        assert!(apply(&vocabulary, &tokens(satz), Options::default()).is_empty());
    }

    /// Der Fehlgriff, den die Messung am 02.09.2026 gefunden hat, als
    /// Regressionsfall: `scan` ueber 114.517 Woerter aus 28 vorhandenen
    /// Transkripten machte aus `"so.Das"` ein `"Sedacz"`.
    ///
    /// Beide haben den Koelner-Code `828` -- die Koelner Phonetik dampft
    /// "Sedatsch" auf drei Ziffern ein, und auf drei Ziffern passt
    /// irgendwann alles. Aufgehalten wird es allein von der
    /// Abstandsschranke: sieben Buchstaben Abstand bei erlaubten vier.
    #[test]
    fn der_gemessene_fehlgriff_passiert_nicht_mehr() {
        let vocabulary = liste_des_betreibers();
        // Voraussetzungen, damit der Test nicht aus einem anderen Grund gruen
        // ist: der Klang stimmt wirklich ueberein, und kein anderer Waechter
        // greift (kein gewoehnliches Wort).
        assert_eq!(koelner::encode("so.Das"), koelner::encode("Sedatsch"));
        assert!(!crate::stopwords::is_common_german_word("so.Das"));
        assert!(koelner::levenshtein("so.das", "sedatsch") > max_edits_alias(8));

        assert_eq!(
            apply_text(&vocabulary, "war so.Das Thema", Options::all_stages()),
            "war so.Das Thema"
        );
    }

    /// Der zweite Teil desselben Fundes: die Koelner Phonetik dampft
    /// "Sedatsch" auf drei Ziffern ein. Auf drei Ziffern passt irgendwann
    /// alles, also braucht auch die Alias-Klangstufe eine Abstandsschranke.
    #[test]
    fn die_alias_klangstufe_haelt_bei_zu_grossem_abstand_still() {
        let vocabulary = liste_des_betreibers();
        // Kein Kleberwort, gross geschrieben, kein deutsches Wort, gleicher
        // Klang -- allein die Schranke haelt es auf.
        // "Sodass" waere ungeeignet: das faengt schon die Stopwortliste.
        assert_eq!(koelner::encode("Sodas"), koelner::encode("Sedatsch"));
        assert!(!crate::stopwords::is_common_german_word("Sodas"));
        assert!(koelner::levenshtein("sodas", "sedatsch") > max_edits_alias(8));
        assert_eq!(
            apply_text(&vocabulary, "Sodas kam", Options::all_stages()),
            "Sodas kam"
        );
    }

    /// Und die Gegenprobe: die Schranke darf den Zweck der Stufe nicht toeten.
    /// Eine echte Variante liegt einen Buchstaben daneben und muss durch.
    #[test]
    fn die_schranke_laesst_echte_varianten_durch() {
        let vocabulary = liste_des_betreibers();
        assert_eq!(koelner::levenshtein("zedatsch", "sedatsch"), 1);
        assert_eq!(
            apply_text(&vocabulary, "Herr Zedatsch war da", Options::all_stages()),
            "Herr Sedacz war da"
        );
    }

    #[test]
    fn zwei_moegliche_ziele_heissen_nichts_tun() {
        // Beide Eintraege klingen gleich; welcher gemeint war, weiss niemand.
        let vocabulary = Vocabulary::parse(["Maier => Mayr", "Meyer => Mayr"]);
        assert_eq!(
            apply_text(&vocabulary, "Herr Mayr", Options::all_stages()),
            "Herr Mayr"
        );
    }

    #[test]
    fn ein_name_laeuft_nicht_ueber_eine_satzgrenze() {
        let vocabulary = liste_des_betreibers();
        // "Phono." beendet den Satz -- "Phono. Werk" ist keine Einheit.
        let satz = "Wir waren im Phono. Werk heisst der Rest";
        assert_eq!(apply_text(&vocabulary, satz, Options::default()), satz);
    }

    #[test]
    fn stufen_lassen_sich_einzeln_abschalten() {
        let vocabulary = liste_des_betreibers();
        let nur_alias = Options {
            alias_phonetics: false,
            canonical_phonetics: false,
            ..Options::default()
        };
        // Stufe 1 laeuft weiter.
        assert_eq!(
            apply_text(&vocabulary, "mit Sedatsch", nur_alias),
            "mit Sedacz"
        );
        // Stufe 2 nicht mehr.
        assert_eq!(
            apply_text(&vocabulary, "mit Zedatsch", nur_alias),
            "mit Zedatsch"
        );
        // Stufe 3 auch nicht.
        assert_eq!(
            apply_text(&vocabulary, "in Lexwahre", nur_alias),
            "in Lexwahre"
        );
    }

    /// Die Zusicherung, auf der alles andere steht: in der Vorgabe RAET der
    /// Nachlauf nicht. Er ersetzt nur, was ein Mensch als Verhoerung
    /// eingetragen hat.
    #[test]
    fn die_vorgabe_raet_nicht() {
        let vocabulary = liste_des_betreibers();

        // Stufe 1 arbeitet.
        assert_eq!(
            apply_text(&vocabulary, "mit Sedatsch", Options::default()),
            "mit Sedacz"
        );

        // Beide Klangstufen schweigen -- und zwar nachweislich an Woertern,
        // die sie mit eingeschalteten Stufen anfassen WUERDEN.
        for satz in ["Herr Zedatsch war da", "Das liegt in Lexwahre"] {
            assert_ne!(
                apply_text(&vocabulary, satz, Options::all_stages()),
                satz,
                "Voraussetzung: mit allen Stufen wird hier ersetzt"
            );
            assert_eq!(apply_text(&vocabulary, satz, Options::default()), satz);
        }

        // Ein Plan aus Rate-Woertern UND einer echten Verhoerung. Der Plan
        // muss die Verhoerung enthalten und sonst nichts.
        //
        // Frueher stand hier `plan.iter().all(...)` ueber einen Plan, der
        // leer sein soll -- `all` auf einer leeren Folge ist immer wahr, der
        // Test konnte also nicht scheitern. Jetzt traegt er eine Stueckzahl.
        let plan = apply(
            &vocabulary,
            &tokens("Sedatsch Zedatsch Lexwahre Werten Lieferant"),
            Options::default(),
        );
        assert_eq!(plan.len(), 1, "erwartet genau die eingetragene Verhoerung");
        assert_eq!(plan[0].kind, MatchKind::Alias);
        assert_eq!(plan[0].after, "Sedacz");
    }

    /// Der kritische Befund vom 02.09.2026, wortgetreu: zwei Saetze aus
    /// gewoehnlichem Deutsch, die des Betreibers eigene Liste zerstoert haette.
    #[test]
    fn ein_alias_das_gewoehnliche_sprache_ist_fasst_nichts_an() {
        let vocabulary = liste_des_betreibers();
        for satz in [
            "Es macht Kling und die Tuer geht auf.",
            "Klick ab und schliess das Fenster.",
        ] {
            assert_eq!(
                apply_text(&vocabulary, satz, Options::default()),
                satz,
                "gewoehnliche Sprache darf die Alias-Stufe nicht anfassen"
            );
            // Und auch nicht ueber die Klangstufen, wenn die jemand einschaltet.
            assert_eq!(apply_text(&vocabulary, satz, Options::all_stages()), satz);
        }
    }

    /// Die Gegenprobe: der Waechter darf die Stufe nicht toeten. Beide
    /// Eintraege, aus denen die gefaehrlichen Aliasse stammen, arbeiten mit
    /// ihren uebrigen Verhoerungen weiter.
    #[test]
    fn der_waechter_laesst_die_ungefaehrlichen_verhoerungen_durch() {
        let vocabulary = liste_des_betreibers();
        assert_eq!(
            apply_text(&vocabulary, "Herr Glink rief an", Options::default()),
            "Herr Glinck rief an"
        );
        assert_eq!(
            apply_text(&vocabulary, "steht in klickab", Options::default()),
            "steht in ClickUp"
        );
        // Und der Fall, an dem sich "alle Woerter" gegen "eines genuegt"
        // entscheidet: "das" ist gewoehnlich, "Seda das" als Folge nicht.
        assert_eq!(
            apply_text(&vocabulary, "mit Seda das gesprochen", Options::default()),
            "mit Sedacz gesprochen"
        );
    }

    #[test]
    fn eine_abgelehnte_verhoerung_wird_als_solche_gemeldet() {
        let risky = liste_des_betreibers().risky_aliases();
        let aliases: Vec<&str> = risky.iter().map(|item| item.alias.as_str()).collect();
        assert_eq!(aliases, ["Kling", "klick ab", "Klick ab"]);
        assert_eq!(risky[0].canonical, "Glinck");
        assert_eq!(risky[2].canonical, "ClickUp");
    }

    #[test]
    fn leere_liste_und_leeres_transkript_tun_nichts() {
        assert!(
            apply(
                &Vocabulary::default(),
                &tokens("irgendwas"),
                Options::default()
            )
            .is_empty()
        );
        assert!(apply(&liste_des_betreibers(), &[], Options::default()).is_empty());
    }

    #[test]
    fn der_plan_zeigt_woher_ein_treffer_kam() {
        let vocabulary = liste_des_betreibers();
        let plan = apply(
            &vocabulary,
            &tokens("Sedatsch Zedatsch Lexwahre"),
            Options::all_stages(),
        );
        let kinds: Vec<MatchKind> = plan.iter().map(|item| item.kind).collect();
        assert_eq!(
            kinds,
            [
                MatchKind::Alias,
                MatchKind::AliasPhonetic,
                MatchKind::CanonicalPhonetic
            ]
        );
    }
}
