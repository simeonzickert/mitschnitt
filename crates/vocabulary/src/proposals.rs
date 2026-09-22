//! Zwei billige Netze, die fehlende Woerterbuch-Eintraege VORSCHLAGEN.
//!
//! Der Anlass, gemessen am 03.09.2026: ueber 11.562 Woerter aus zwei
//! Gespraechen kam GENAU EIN Woerterbuch-Begriff korrekt an. des Betreibers Liste
//! trug an dem Tag 20 Eintraege und NULL Verhoerungen -- der Nachlauf
//! (`matcher`) ersetzt aber nur, wo eine Verhoerung eingetragen ist. Seit dem
//! 03.09. lernt die Liste aus jeder Korrektur ein Paar; das greift aber erst ab
//! der ersten Korrektur und nur bei Woertern, die ein Mensch bemerkt. Diese
//! beiden Netze schliessen den Rest -- ohne Sprachmodell, auf des Betreibers Ansage
//! vom 03.09.2026 09:47 ("lass erstmal die 2 billigen netze implementieren
//! bitte. ohne die LLM Unterstuetzung.").
//!
//! # Netz 1 -- Wiederkehr mit Schwankung
//!
//! Ein Begriff, der ueber MEHRERE Gespraeche wiederkehrt und dabei
//! unterschiedlich geschrieben wird, ist ein fehlender Eintrag. Das Signal ist
//! nur im Stapel sichtbar: in einem einzelnen Transkript sieht "Grandfall" wie
//! ein Wort aus, ueber drei Gespraeche hinweg wie eine Verhoerung von
//! "Grandpfeil".
//!
//! # Netz 2 -- Klangvergleich gegen die Kontextquellen
//!
//! Die Wahrheit ueber Namen steht nicht im Transkript, sondern daneben:
//! Teilnehmer, Meetingtitel, Notiz, und die Woerterbuchliste selbst. Ein Wort,
//! das phonetisch nah an einem dieser Namen liegt, aber anders geschrieben ist,
//! ist eine Verhoerung.
//!
//! # Was hier bewusst NICHT passiert
//!
//! Nichts wird ersetzt und nichts wird eingetragen. Eine falsche Ersetzung im
//! Woerterbuch wirkt in JEDEM kuenftigen Gespraech -- der Preis eines
//! Fehlgriffs ist dauerhaft, der Preis einer Rueckfrage einmalig. Beide Netze
//! liefern nur Vorschlaege.

use std::collections::{BTreeMap, BTreeSet};

use crate::koelner::{encode, levenshtein};
use crate::stopwords::{
    is_all_common_german_phrase, is_common_german_word, is_ordinary_german_word,
};

/// Ein Wort aus einem Transkript, so wie die Oberflaeche es kennt.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanWord {
    /// Der Text mit fuehrendem Leerzeichen und Satzzeichen, unveraendert.
    pub text: String,
    /// Die GEMESSENE Wortsicherheit des Erkenners -- oder gar nichts.
    ///
    /// `None` heisst "nicht gemessen", NICHT "sicher". Gemessen am 03.09.2026
    /// an des Betreibers Datenbank: von neun lebenden Transkripten trug genau EINES
    /// diesen Wert (3.355 von 28.458 Woertern, 11,8 %). Ein Netz, das `None`
    /// wie "sicher" behandelt, waere auf acht von neun Gespraechen blind.
    pub measured_confidence: Option<f64>,
}

/// Ein Gespraech mit seinem Kontext.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanSession {
    pub session_id: String,
    pub words: Vec<ScanWord>,
    /// Teilnehmer dieses Gespraechs. Ganze Namen ("Mads Verlin |
    /// nordwerk") sind erlaubt, das Zerlegen passiert hier.
    ///
    /// Hier stehen NUR Namen. Mailadressen gehoeren in [`Self::context_emails`]
    /// -- und das ist keine Ordnungsfrage, sondern die Zusage selbst.
    pub context_names: Vec<String>,
    /// Die Mailadressen der Teilnehmer -- AUSSCHLIESSLICH fuer die Immunitaet.
    ///
    /// Warum ein eigenes Feld und nicht "die Eintraege mit @ in
    /// `context_names`": weil das die Herkunft an den WERT bindet statt an das
    /// FELD. Ein Kalendereintrag `{email: "Verlin"}` ohne "@" war bis zum
    /// 03.09.2026 von einem Namen nicht mehr zu unterscheiden und wurde zum
    /// ZIEL -- der Lauf haette vorgeschlagen, "Werlin" kuenftig durch "Verlin"
    /// zu ersetzen, auf Grundlage einer Quelle, die genau das nie tun darf.
    /// Eine Herkunft, die man aus dem Inhalt zurueckrechnen muss, geht
    /// irgendwann verloren; ein Feld nicht.
    ///
    /// Ein Anker sagt "diese Schreibweise ist in Ordnung" und kostet im
    /// schlimmsten Fall einen Vorschlag, der ausbleibt. Ein Ziel sagt "ersetze
    /// kuenftig X hierdurch" -- und aus "info@", "kontakt@" oder
    /// "buchhaltung@" wuerden damit Zielwoerter, die niemand als Namen gemeint
    /// hat.
    pub context_emails: Vec<String>,
    /// VON MENSCHEN GESCHRIEBENER Kontext: Kalendertitel,
    /// Kalenderbeschreibung, Notiz. Daraus werden die grossgeschriebenen
    /// Woerter zu Ankern.
    ///
    /// GEMESSEN am 03.09.2026, warum das noetig ist: der Name der
    /// Kundenfirma stand in KEINER Teilnehmerliste und in keinem
    /// Woerterbuch-Eintrag -- nur in der Kalenderbeschreibung des Termins.
    /// Ohne diese Quelle faellt sein Verhoerer aus dem Netz.
    pub context_text: String,
    /// Der VOM MODELL ERZEUGTE Sitzungstitel -- fuer Gespraeche ohne
    /// Kalendertermin schreibt ihn ein Sprachmodell aus dem Transkript.
    ///
    /// Er ist ausdruecklich KEIN Beleg. Zwei Gruende, beide gemessen:
    ///
    /// 1. Er ist ZIRKULAER. Er entsteht aus demselben Transkript, das hier
    ///    geprueft wird -- eine Verhoerung, die das Modell in den Titel
    ///    uebernimmt, belegt danach sich selbst.
    /// 2. Er dreht die Wahrheit um. Titel "Grandfall Strategie", im
    ///    Transkript steht richtig "Grandpfeil": als Anker gaebe das den
    ///    Vorschlag, kuenftig "Grandpfeil" durch "Grandfall" zu ersetzen.
    ///
    /// Er bleibt hier stehen, weil ein Aufrufer ihn liefern KANN und weil das
    /// Feld sagt, was mit ihm passiert -- naemlich nichts. Wer ihn eines
    /// Tages doch verwenden will, braucht erst eine Antwort auf (1).
    pub generated_title: String,
}

/// Welches Netz den Vorschlag gefunden hat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProposalKind {
    /// Netz 1: derselbe Begriff, mehrere Gespraeche, verschiedene
    /// Schreibweisen.
    Recurrence,
    /// Netz 2: klingt wie ein Name aus dem Kontext, ist aber anders
    /// geschrieben.
    Context,
}

/// Ein Vorschlag -- KEINE Ersetzung.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub kind: ProposalKind,
    /// Die Schreibweise, die im Transkript stehen sollte.
    pub canonical: String,
    /// Die Verhoerung, die auf sie zeigen wuerde.
    pub alias: String,
    /// Wie oft die Verhoerung im Korpus steht.
    pub occurrences: usize,
    /// In welchen Gespraechen -- aufsteigend, damit die Liste stabil ist.
    pub session_ids: Vec<String>,
    /// Ein Ausschnitt, an dem ein Mensch das Urteil faellen kann.
    pub evidence: String,
    /// Woher die richtige Schreibweise kommt. Fuer Netz 1 die Begruendung der
    /// Wahl, fuer Netz 2 die Kontextquelle.
    pub source: String,
}

/// Die Schrauben beider Netze. Jede Vorgabe ist gemessen, keine geraten --
/// siehe [`Options::default`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Kuerzestes Wort, das ueberhaupt Kandidat sein darf (in Zeichen).
    pub min_candidate_len: usize,
    /// Kuerzester Koelner-Code, der als Aussage zaehlt.
    pub min_code_len: usize,
    /// Groesster erlaubter Abstand zwischen den beiden Koelner-Codes.
    pub max_code_edits: usize,
    /// Anteil der Wortlaenge, den der Abstand hoechstens ausmachen darf.
    ///
    /// Es gibt bewusst KEINE Untergrenze -- siehe [`Options::default`].
    pub max_edit_share: f64,
    /// Netz 1: in wie vielen verschiedenen Gespraechen die Gruppe vorkommen
    /// muss.
    pub min_sessions: usize,
    /// Netz 2: oberhalb dieser gemessenen Wortsicherheit wird nicht mehr
    /// vorgeschlagen. Woerter OHNE gemessenen Wert laufen weiter mit.
    pub confidence_ceiling: f64,
}

impl Default for Options {
    /// Alle Werte gemessen am 03.09.2026 an des Betreibers neun lebenden
    /// Transkripten (28.458 Woerter); die Messung steht im Bericht zu ZICK-284
    /// und laesst sich mit `cargo test -p vocabulary -- --ignored
    /// messbank_am_echten_korpus` wiederholen.
    fn default() -> Self {
        Self {
            // Unter vier Zeichen tragen weder Koelner-Code noch
            // Levenshtein-Abstand genug Information; "Ja", "Aber", "Und"
            // sind zudem der Loewenanteil der grossgeschriebenen Woerter.
            min_candidate_len: 4,
            // Uebernommen aus `matcher::MIN_CODE_LEN_ALIAS`: zwei Ziffern
            // treffen zu viele Woerter, um eine Aussage zu sein. Drei ist die
            // Grenze, an der "Verlin" (Code 578) noch mitspielt -- ein Vierer
            // haette den Fall aus dem Auftrag stumm ausgeschlossen.
            min_code_len: 3,
            // GEMESSEN am 03.09.2026: Gleichheit des Codes waere zu streng --
            // sie verwirft JEDES bekannte Paar des Auftrags. "Grandpfeil"
            // (1762135) gegen "Grandfall" (176235) und "Tofmann" (0366) gegen
            // "Tochmann" (0466) liegen beide bei genau einer Ziffer
            // Unterschied; bei zwei Ziffern kommen "Grandwehr" und
            // "Grandfrei" dazu, aber auch spuerbar mehr Rauschen (Zahlen im
            // Bericht zu ZICK-284). Eins ist der gemessene Schnitt.
            max_code_edits: 1,
            // Der Abstand waechst mit der Wortlaenge, ohne Untergrenze.
            //
            // Ein fester Deckel waere an beiden Enden falsch: die bekannten
            // Verhoerer liegen drei bis vier Buchstaben auseinander, ein
            // Deckel von 2 haette sie stumm verworfen.
            //
            // Die Untergrenze 2, die hier bis zum 03.09.2026 stand, ist
            // GESTRICHEN. Was sie wirklich tat, ist nachgerechnet und schmaler
            // als zwei fruehere Begruendungen behaupteten: sie wirkte
            // AUSSCHLIESSLICH auf Paare mit vier Zeichen. Dort ergibt der
            // Anteil `floor(4 * 0,4)` genau 1, und `max(2, 1)` hob das auf 2 --
            // eine Verdopplung der Erlaubnis fuer die kuerzesten ueberhaupt
            // zugelassenen Kandidaten (`min_candidate_len` ist 4). Ab fuenf
            // Zeichen war sie wirkungslos.
            //
            // Ausdruecklich NICHT wahr, obwohl es hier bis zum 03.09.2026
            // stand: dass sie "Verlin" und "Merlin" zusammengebracht habe.
            // Beide sind sechs Zeichen lang, der Anteil ergibt 2, die
            // Untergrenze ergibt 2 -- sie hat an diesem Paar nie etwas
            // veraendert. Was den achtzehnfachen Fehlgriff wirklich zuhaelt,
            // ist die Immunitaet in `known_terms`, nicht diese Zahl.
            // `bei_vier_zeichen_ist_genau_ein_buchstabe_erlaubt` haelt die
            // Wirkung an der einzigen Laenge fest, an der es sie gibt.
            max_edit_share: 0.4,
            // Der Kern von Netz 1: ein Gespraech ist eine Beobachtung, zwei
            // sind ein Muster.
            min_sessions: 2,
            // Gemessen an den 3.355 Woertern mit Wert: Median 0,9944,
            // Mittel 0,9361, p25 0,9228, Minimum 0,2380. Bei 0,98 bleiben
            // rund 30 % der Woerter im Netz -- streng genug, um den Lauf
            // billig zu machen, weit genug, dass ein Verhoerer mit hoher
            // Selbstsicherheit nicht durchrutscht.
            confidence_ceiling: 0.98,
        }
    }
}

/// Ein Kandidat: ein grossgeschriebenes Wort, das kein Satzanfang und kein
/// gewoehnliches Deutsch ist.
#[derive(Debug, Clone)]
struct Candidate {
    core: String,
    session_id: String,
    /// Ein Ausschnitt um das Wort herum.
    evidence: String,
    measured_confidence: Option<f64>,
}

/// Der Wortkern ohne umgebende Satzzeichen -- dieselbe Regel wie
/// `matcher::core_of`, damit ein hier vorgeschlagener Alias spaeter auch
/// wirklich getroffen wird.
fn core_of(text: &str) -> &str {
    text.trim_matches(|character: char| {
        !character.is_alphanumeric() && character != '-' && character != '\''
    })
}

/// Beendet dieses Wort einen Satz? Dann ist das naechste ein Satzanfang und
/// seine Grossschreibung sagt nichts.
fn ends_sentence(text: &str) -> bool {
    text.trim_end()
        .chars()
        .next_back()
        .is_some_and(|character| matches!(character, '.' | '!' | '?' | ':' | '…'))
}

fn starts_uppercase(core: &str) -> bool {
    core.chars()
        .next()
        .is_some_and(|character| character.is_uppercase())
}

/// Nur Buchstaben, Bindestrich und Apostroph -- eine Zahl oder ein Kuerzel mit
/// Ziffern ist kein Name.
fn is_wordlike(core: &str) -> bool {
    core.chars()
        .all(|character| character.is_alphabetic() || character == '-' || character == '\'')
}

fn snippet(words: &[ScanWord], at: usize) -> String {
    let from = at.saturating_sub(6);
    let to = (at + 7).min(words.len());
    words[from..to]
        .iter()
        .map(|word| word.text.as_str())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn collect_candidates(session: &ScanSession, options: &Options) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (at, word) in session.words.iter().enumerate() {
        // Ein Satzanfang ist grossgeschrieben, weil er ein Satzanfang ist --
        // seine Grossschreibung ist kein Hinweis auf einen Namen.
        let sentence_start = at == 0 || ends_sentence(&session.words[at - 1].text);
        if sentence_start {
            continue;
        }
        let core = core_of(&word.text);
        if core.chars().count() < options.min_candidate_len
            || !starts_uppercase(core)
            || !is_wordlike(core)
            || is_ordinary_german_word(core)
        {
            continue;
        }
        let code = encode(core);
        if code.chars().count() < options.min_code_len {
            continue;
        }
        out.push(Candidate {
            core: core.to_string(),
            session_id: session.session_id.clone(),
            evidence: snippet(&session.words, at),
            measured_confidence: word.measured_confidence,
        });
    }
    out
}

/// Vergleichsschluessel -- MUSS `dictionaryEntryKey` in
/// `apps/desktop/src/stt/dictionary-entry.ts` entsprechen.
///
/// Beide Seiten teilen sich einen Schluessel: die Oberflaeche bildet ihn beim
/// Verwerfen eines Vorschlags, Rust vergleicht ihn beim naechsten Lauf. Laufen
/// die beiden Normalisierungen auseinander, kommt ein verworfener Vorschlag
/// zurueck -- und die Zusage "Verwerfen ist dauerhaft" bricht still.
///
/// Deshalb dieselben drei Schritte in derselben Reihenfolge: aussen kuerzen,
/// innen Leerraum zusammenziehen, kleinschreiben. `to_lowercase` ist dabei
/// die locale-unabhaengige Form, passend zu `toLowerCase` drueben -- nicht
/// `to_locale_lowercase`.
///
/// Die eine Stelle, an der die beiden Sprachen sich WIRKLICH unterschieden,
/// gemessen am 03.09.2026: JavaScripts `\s` deckt das Byte-Reihenfolge-Zeichen
/// `U+FEFF` mit ab, Rusts `char::is_whitespace` folgt der Unicode-Eigenschaft
/// `White_Space` und tut das NICHT. Alle uebrigen Zeichen aus JavaScripts `\s`
/// -- geschuetztes Leerzeichen, die Halbgeviert-Familie, `U+2028`, `U+3000` --
/// stehen in `White_Space` drin, die beiden Seiten sind dort einig. Ein
/// Begriff mit einem eingeschleppten `U+FEFF` (der Regelfall: aus einer
/// Tabelle oder einer Webseite kopiert) bekaeme also zwei verschiedene
/// Schluessel, und ein verworfenes Paar kaeme beim naechsten Lauf zurueck.
fn key(value: &str) -> String {
    value
        .split(|character: char| character.is_whitespace() || character == '\u{FEFF}')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Der groesste Buchstabenabstand, den zwei gleichklingende Schreibweisen
/// haben duerfen. Bemessen an der laengeren -- ein langes Wort traegt mehr
/// Beweis in sich als ein kurzes.
fn max_edits_for(left: &str, right: &str, options: &Options) -> usize {
    let longest = left.chars().count().max(right.chars().count());
    // Abgerundet, nicht gerundet: bei vier Zeichen ergibt das 1, und das ist
    // die Absicht. Wer hier rundet, oeffnet die kurzen Woerter wieder auf 2
    // und holt sich das "Verlin"/"Merlin"-Fenster zurueck.
    (longest as f64 * options.max_edit_share) as usize
}

/// Klingen die beiden gleich UND liegen sie in derselben Schreib-Gegend?
///
/// Der Klang allein ist zu wenig Beweis -- das steht seit dem 02.09.2026 als
/// gemessene Lehre in `matcher`: "so.Das" und "Sarnec" haben denselben
/// Koelner-Code. Deshalb immer beides.
fn sounds_alike(left: &str, right: &str, options: &Options) -> bool {
    let (left_key, right_key) = (key(left), key(right));
    if left_key == right_key {
        return false;
    }
    levenshtein(&encode(left), &encode(right)) <= options.max_code_edits
        && levenshtein(&left_key, &right_key) <= max_edits_for(left, right, options)
}

/// Sind die beiden nur zwei Beugungen desselben deutschen Wortes?
///
/// GEMESSEN am 03.09.2026 an des Betreibers Korpus: von 242 Vorschlaegen aus Netz 1
/// war der groesste einzelne Block Singular gegen Plural -- "Gespraech" gegen
/// "Gespraeche", "Aufgabe" gegen "Aufgaben", "Position" gegen "Positionen".
/// Das ist Grammatik, keine Verhoerung, und ein Woerterbuch-Eintrag daraus
/// wuerde in jedem kuenftigen Gespraech die Mehrzahl kaputtmachen.
///
/// Die Laengengrenze ist kein Beiwerk: ohne sie faellt "Verlin" gegen "Verlins"
/// mit unter die Regel -- die kuerzeren Woerter sind genau die, bei denen ein
/// angehaengter Buchstabe eher ein Verhoerer als eine Beugung ist.
fn is_inflection(left: &str, right: &str) -> bool {
    // Fuenf, nicht sechs. GEMESSEN am 04.09.2026 an des Betreibers Korpus: bei
    // sechs rutschte der Genitiv eines fuenfbuchstabigen Vornamens durch --
    // "von Marens Mail" wurde als Verhoerung des Vornamens vorgeschlagen, und
    // genau diese Sorte hat der Betreiber als erstes Beispiel genannt ("ganz
    // wilde Sachen, wie zum Beispiel zu Mehrzahlen").
    //
    // Bei VIER bleibt es ausdruecklich dabei, dass gar nicht geprueft wird:
    // dort ist ein angehaengter Buchstabe eher ein Verhoerer als eine Beugung,
    // und drei belegte Vorschlaege des Betreibers haengen daran (ein
    // vierbuchstabiger Vorname gegen drei verschiedene Verhoerungen).
    //
    // Der Schritt von sechs auf fuenf ist einzeln nachgerechnet: er beruehrt
    // NUR Paare, deren kuerzeres Wort genau fuenf Zeichen hat. Von den
    // Vorschlaegen des Korpus sind das vier, und drei davon ueberleben, weil
    // ihre Endungen keine deutschen Endungen sind.
    const MIN_LEN: usize = 5;
    /// Die deutschen Endungen, die eine Beugung anhaengt oder tauscht.
    const ENDINGS: &[&str] = &["", "n", "en", "e", "er", "r", "s", "es", "em", "ern"];

    let (left, right) = (left.to_lowercase(), right.to_lowercase());

    // Zusammensetzung statt Beugung: "Kampagnen" gegen "Kampagnenidee".
    //
    // Das ist keine Verhoerung, sondern ein Wort, an das ein zweites
    // angewachsen ist -- ein Woerterbuch-Eintrag daraus wuerde kuenftig jedes
    // Kompositum zerschlagen. Die Beugungsregel darunter faengt es nicht: der
    // Rest "idee" ist keine Endung.
    //
    // ENG geschnitten, und das ist der ganze Punkt: das kuerzere Wort muss ein
    // VOLLSTAENDIGER Anfang des laengeren sein UND der Rest muss selbst ein
    // gewoehnliches Wort sein. Ohne die zweite Bedingung faellt ein
    // vierbuchstabiger Vorname gegen seine Verhoerung mit darunter (der Rest
    // waere "on"); mit ihr nicht, weil "on" kein Wort der Liste ist. Ein
    // blosses "der Rest ist mindestens drei Zeichen lang" waere die Regel
    // gewesen, die genau diesen belegten Vorschlag getoetet haette.
    let (kurz, lang) = if left.chars().count() <= right.chars().count() {
        (&left, &right)
    } else {
        (&right, &left)
    };
    if let Some(rest) = lang.strip_prefix(kurz.as_str())
        && rest.chars().count() >= 3
        && is_ordinary_german_word(rest)
    {
        return true;
    }

    let shorter = left.chars().count().min(right.chars().count());
    if shorter < MIN_LEN {
        return false;
    }

    let common = left
        .chars()
        .zip(right.chars())
        .take_while(|(a, b)| a == b)
        .count();
    // Weniger als 60 % gemeinsamer Anfang ist keine Beugung mehr, sondern ein
    // anderes Wort. "Grandpfeil" und "Grandfall" teilen 50 % und bleiben
    // damit ausdruecklich im Netz.
    if (common as f64) < shorter as f64 * 0.6 {
        return false;
    }

    let tail = |word: &str| word.chars().skip(common).collect::<String>();
    ENDINGS.contains(&tail(&left).as_str()) && ENDINGS.contains(&tail(&right).as_str())
}

/// Die Striche, die zwei Namensteile zu einem Doppelnamen verbinden.
///
/// Bis zum 03.09.2026 kannte die Zerlegung nur den ASCII-Bindestrich. Die
/// anderen vier trennten deshalb schon vorher -- sie sind nicht alphabetisch
/// --, aber sie erzeugten NUR die Haelften und nie die ganze Form: aus einem
/// getippten "Zirk--Peter" mit Halbgeviertstrich wurde "Zirk" und "Peter",
/// nie "Zirk--Peter". Umgekehrt erzeugte der ASCII-Bindestrich vor dieser
/// Zeile in `context_text_terms` NUR die ganze Form und nie die Haelften.
/// Zwei Striche, zwei verschiedene Ergebnisse, beide unvollstaendig -- welche
/// Taste der Kalender getroffen hat, entschied ueber die Immunitaet eines
/// Namens. Jetzt liefern alle fuenf beides.
fn is_joiner(character: char) -> bool {
    matches!(
        character,
        '-' | '\u{2010}' | '\u{2011}' | '\u{2013}' | '\u{2014}'
    )
}

fn is_word_character(character: char) -> bool {
    character.is_alphabetic() || is_joiner(character) || character == '\''
}

/// Jedes Wort eines menschengeschriebenen Textes UND, bei einem Doppelnamen,
/// zusaetzlich seine Haelften.
///
/// Ohne die Haelften legt "Mads-Peter Verlin" nur "mads-peter" und "verlin" in
/// die Anker-Menge, nicht "mads" -- und genau "Mads" ist das Wort, das im
/// Transkript steht und geschuetzt werden muss. Die Schranke, die "Verlin" vor
/// "Merlin" rettet, haette an jedem Bindestrich ein Loch.
fn words_with_halves(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for part in text.split(|character: char| !is_word_character(character)) {
        if part.is_empty() {
            continue;
        }
        out.push(part);
        if part.chars().any(is_joiner) {
            for half in part.split(is_joiner) {
                if !half.is_empty() {
                    out.push(half);
                }
            }
        }
    }
    out
}

/// Zerlegt einen Kontextnamen in die Teile, gegen die verglichen wird.
///
/// "Mads Verlin | nordwerk" ist kein Wort, das je im Transkript steht --
/// verglichen wird gegen "Mads" und gegen "Verlin". Der Teil hinter dem
/// senkrechten Strich ist die Organisation, die der Kalender anhaengt, und
/// gehoert nicht zum Namen. Eine Mailadresse ist gar kein Name -- fuer die
/// Immunitaet zerlegt sie [`context_name_parts_for_known`] trotzdem.
pub fn context_name_parts(name: &str) -> Vec<String> {
    let without_org = name.split('|').next().unwrap_or(name);
    if without_org.contains('@') {
        return Vec::new();
    }
    collect_parts(without_org, 3)
}

/// Wie [`context_name_parts`], aber zusaetzlich die Namensteile aus einer
/// Mailadresse -- und AUSSCHLIESSLICH fuer die Immunitaet.
///
/// Wer im Kalender nur als "mads.verlin@nordwerk.example" steht, hatte bis zum
/// 03.09.2026 gar keinen Schutz: sein Name war fuer die Netze unbekannt, und
/// damit war "Verlin" wieder Freiwild fuer einen aehnlich klingenden
/// Woerterbuch-Eintrag. Genau der Fehlgriff, den die Immunitaet verhindern
/// soll, nur ueber eine Quelle, die niemand angesehen hatte.
///
/// Warum NUR fuer die Immunitaet und nie als Ziel: die beiden Rollen haben
/// voellig verschiedene Preise. Ein Anker, der sagt "diese Schreibweise ist in
/// Ordnung", kostet im schlimmsten Fall einen Vorschlag, der ausbleibt. Ein
/// Ziel sagt "ersetze kuenftig X hierdurch" -- und aus "info@", "kontakt@"
/// oder "buchhaltung@" wuerden damit Zielwoerter, die niemand als Namen
/// gemeint hat. Der Domainteil bleibt ganz draussen: dort stehen "gmail",
/// "outlook" und "web" so oft wie Firmennamen.
fn context_name_parts_for_known(name: &str) -> Vec<String> {
    let without_org = name.split('|').next().unwrap_or(name);
    if !without_org.contains('@') {
        return collect_parts(without_org, 3);
    }
    collect_parts(without_org.split('@').next().unwrap_or(""), 3)
}

fn collect_parts(text: &str, min_len: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in words_with_halves(text) {
        if part.chars().count() >= min_len && !out.iter().any(|bekannt| bekannt == part) {
            out.push(part.to_string());
        }
    }
    out
}

/// Die grossgeschriebenen Woerter eines Kontexttextes -- Titel, Beschreibung,
/// Notiz.
///
/// Bewusst grob: es geht nicht um eine saubere Namenserkennung, sondern um
/// eine Liste von Woertern, die belegt richtig geschrieben sind, weil ein
/// Mensch sie getippt hat. Gewoehnliches Deutsch faellt raus.
///
/// Die Haelften eines Doppelnamens gehoeren dazu, und das war bis zum
/// 03.09.2026 nicht so: der strukturierte Weg (`context_name_parts`) zerlegte
/// "Zirk-Peter", der menschengeschriebene Kalendertext nicht. Ein Kalendertitel
/// "Termin mit Zirk-Peter" liess "Zirk" damit ungeschuetzt -- obwohl ein Mensch
/// genau dieses Wort getippt hatte.
fn context_text_terms(text: &str) -> Vec<String> {
    context_text_fields(text).anker
}

/// Ein menschengeschriebener Kontexttext, aufgeteilt nach den beiden Rollen.
///
/// DIESELBE Zusage wie bei den strukturierten Feldern, nur ueber einen anderen
/// Weg -- und bis zum 04.09.2026 galt sie hier nicht. Eine Adresse steht nicht
/// nur im Teilnehmerfeld: sie steht auch mitten in einer Notiz oder einer
/// Kalenderbeschreibung ("Kontakt: Verlin@nordwerk.example"). `context_text_terms`
/// nahm daraus jedes grossgeschriebene Wort ab vier Zeichen und machte
/// "Verlin" zum ANKER -- also zu einer Schreibweise, gegen die der Lauf jedes
/// aehnlich klingende Wort zur Verhoerung erklaert. Der strukturierte Weg war
/// dagegen gehaertet, und weil saemtliche Adresstests ueber `context_emails`
/// einspeisten, hat es keiner gesehen.
///
/// Die Preise der beiden Rollen sind verschieden, und daran haengt der
/// Schnitt: ein Anker sagt "ersetze kuenftig X hierdurch" und wirkt dauerhaft,
/// eine Immunitaet kostet im schlimmsten Fall einen Vorschlag, der ausbleibt.
/// Aus "info@", "kontakt@" oder "buchhaltung@" duerfen deshalb keine Ziele
/// werden.
///
/// Der Schnitt laeuft am LEERRAUM-STUECK, nicht am Klammeraffen, und das war
/// bis zum 04.09.2026 anders. Die erste Fassung trennte an `split_once('@')`
/// und nahm nur den Lokalteil -- weggeworfen wurde damit aber nicht der
/// Domainteil, sondern der ganze Rest des Stuecks. Solange eine Adresse von
/// Leerzeichen umgeben ist, faellt das nicht auf; der Hauptpfad sieht genau
/// das nicht. Seit dem Ausbau des Titelschnitts geht die Notiz als rohes,
/// minifiziertes JSON in den Kontext, und darin steht fast kein Leerzeichen:
///
/// ```text
/// verlin@nordwerk.example"}]},{"type":"text","text":"Mads
/// ```
///
/// "Mads" verlor damit Anker UND Immunitaet und war wieder als Verhoerung
/// eines aehnlich klingenden Woerterbuch-Eintrags vorschlagbar -- vorher war
/// er beides. Dieselbe Klasse: komma-geklebte Adressen und ein fuehrendes "@"
/// ohne Lokalteil.
///
/// Deshalb gilt: ein Stueck mit Klammeraffen liefert NUR Immunitaet, und zwar
/// fuer ALLE seine Woerter ab drei Zeichen. Anker gibt es aus so einem Stueck
/// gar nicht. Die Anker-Sperre haelt damit vollstaendig, und der Preis ist ein
/// Anker, der ausbleibt, weil sein Wort im selben leerzeichenfreien Lauf wie
/// eine Adresse stand -- die billige Richtung. Der Domainteil ist darin
/// enthalten: "gmail" oder "outlook" werden immun, was hoechstens einen
/// Vorschlag kostet, aber nie ein Ziel begruendet.
///
/// Fuer die Immunitaet gilt die Untergrenze DREI wie ueberall auf dem
/// Adresspfad, und ohne die Grossschreibung -- Adressen sind klein getippt.
struct Kontextwoerter {
    /// Anker UND Immunitaet: ein Mensch hat das Wort als Wort getippt.
    anker: Vec<String>,
    /// Nur Immunitaet: das Wort stand in einer Adresse.
    nur_immun: Vec<String>,
}

fn context_text_fields(text: &str) -> Kontextwoerter {
    let mut anker = Vec::new();
    let mut nur_immun = Vec::new();
    // Erst an Leerraum trennen, damit eine Adresse als EIN Stueck erkennbar
    // bleibt. Fuer die Woerter darin aendert das nichts: `words_with_halves`
    // trennt an jedem Nicht-Wortzeichen, Leerraum eingeschlossen.
    for stueck in text.split_whitespace() {
        // Das GANZE Stueck, nicht nur der Lokalteil: was ohne Leerzeichen an
        // einer Adresse klebt, ist im minifizierten JSON der Regelfall.
        if stueck.contains('@') {
            for wort in words_with_halves(stueck) {
                if wort.chars().count() >= 3 {
                    nur_immun.push(wort.to_string());
                }
            }
        } else {
            for wort in words_with_halves(stueck) {
                if wort.chars().count() >= 4
                    && starts_uppercase(wort)
                    && !is_ordinary_german_word(wort)
                {
                    anker.push(wort.to_string());
                }
            }
        }
    }
    Kontextwoerter { anker, nur_immun }
}

/// Alles, was belegt richtig geschrieben ist -- in ZWEI Mengen, weil die
/// beiden Rollen verschiedene Quellen vertragen.
///
/// Bis zum 03.09.2026 war es EINE Menge, und das war der Fehler. Sie hatte
/// schon damals zwei Aufgaben:
///
/// 1. IMMUNITAET ([`Self::immun`]). Ein Wort, das hier drinsteht, wird nie als
///    Verhoerung vorgeschlagen. Ohne sie schlug der Lauf am 03.09.2026
///    achtzehn Mal vor, "Verlin" durch "Merlin" zu ersetzen -- der Teilnehmer
///    des Gespraechs, geopfert an einen Woerterbuch-Eintrag mit aehnlichem
///    Klang. Sie gilt ueber den GANZEN Korpus, nicht je Gespraech: dass Verlin
///    in einem anderen Termin nicht eingeladen war, macht seinen Namen nicht
///    falsch.
/// 2. ANKER ([`Self::anker`]). Daran entscheidet Netz 1, welche Schreibweise
///    einer Gruppe die RICHTIGE ist -- und erklaert damit jede andere
///    Schreibweise der Gruppe zu ihrer Verhoerung.
///
/// Die Adressen gehoeren in (1) und nie in (2). Die Trennung der FELDER
/// (`context_names` gegen `context_emails`, im SQL wie im Rust-Typ) war am
/// 03.09.2026 richtig, lief aber in dieser Funktion wieder zusammen: beide
/// Quellen landeten in derselben Menge, und Netz 1 zog seinen Anker daraus.
/// Gemessen am Fehlerfall: Sitzung A "hat Verlin geantwortet" mit
/// `context_emails = ["verlin@nordwerk.example"]`, Sitzung B "hat Werlin
/// geantwortet", Woerterbuch leer -- Vorschlag `Verlin <- Werlin`. Ohne die
/// Adresse gaebe es mangels Anker gar keinen Vorschlag. Die Adresse hat also
/// nicht nur geschuetzt, sie hat ein anderes Wort zur Verhoerung erklaert:
/// genau das, was der Kommentar an dieser Stelle ausschloss.
///
/// Der Preis der Trennung ist bewusst und billig: ein Vorschlag, der
/// ausbleibt, weil sein einziger Anker eine Adresse war. Ein Anker sagt
/// "ersetze kuenftig X hierdurch" und wirkt dauerhaft; er braucht eine Quelle,
/// die ein Mensch als Namen getippt hat.
struct Bekannt {
    /// Fuer die Sperren: alles, inklusive der Namensteile aus Mailadressen.
    immun: BTreeSet<String>,
    /// Fuer die Kanonisierung: dasselbe OHNE die Adressteile.
    anker: BTreeSet<String>,
}

fn known_terms(sessions: &[ScanSession], canonical_terms: &[String]) -> Bekannt {
    let mut immun = BTreeSet::new();
    let mut anker = BTreeSet::new();
    // Beleg fuer beide Rollen: ein Mensch hat das Wort als Wort getippt.
    let beides = |wert: &str, immun: &mut BTreeSet<String>, anker: &mut BTreeSet<String>| {
        if !wert.is_empty() {
            immun.insert(key(wert));
            anker.insert(key(wert));
        }
    };
    for term in canonical_terms {
        beides(term, &mut immun, &mut anker);
        for part in context_name_parts_for_known(term) {
            beides(&part, &mut immun, &mut anker);
        }
    }
    for session in sessions {
        for name in &session.context_names {
            for part in context_name_parts_for_known(name) {
                beides(&part, &mut immun, &mut anker);
            }
        }
        // Die Adressen liefern AUSSCHLIESSLICH Immunitaet ab. Sie tauchen
        // weder in `scan_context` als Ziel auf noch in der Anker-Menge, aus
        // der Netz 1 seine richtige Schreibweise waehlt -- und das ist die
        // ganze Zusage: eine Adresse schuetzt einen Namen, sie erklaert nie
        // ein anderes Wort zu ihrer Verhoerung.
        for address in &session.context_emails {
            for part in context_name_parts_for_known(address) {
                if !part.is_empty() {
                    immun.insert(key(&part));
                }
            }
        }
        // NUR der menschengeschriebene Kontext. `generated_title` ist
        // ausdruecklich nicht dabei -- siehe `ScanSession::generated_title`.
        //
        // Und auch hier trennen die beiden Rollen: eine Adresse MITTEN im
        // Text ("Kontakt: Verlin@nordwerk.example") schuetzt den Namen, sie ankert
        // ihn nicht. Dieselbe Zusage wie beim strukturierten Feld darueber,
        // nur ueber den Weg, den bis zum 04.09.2026 niemand angesehen hatte.
        let woerter = context_text_fields(&session.context_text);
        for term in woerter.anker {
            beides(&term, &mut immun, &mut anker);
        }
        for term in woerter.nur_immun {
            immun.insert(key(&term));
        }
    }
    Bekannt { immun, anker }
}

/// Netz 1: Wiederkehr mit Schwankung.
///
/// Gruppiert alle Kandidaten nach Koelner-Code, legt innerhalb einer Gruppe die
/// Schreibweisen zusammen, die hoechstens [`Options::max_edits`] auseinander
/// liegen, und schlaegt eine Gruppe vor, sobald sie mindestens zwei
/// Schreibweisen aus mindestens [`Options::min_sessions`] Gespraechen hat.
fn scan_recurrence(
    sessions: &[ScanSession],
    known: &Bekannt,
    options: &Options,
) -> Vec<Proposal> {
    #[derive(Default)]
    struct Spelling {
        count: usize,
        sessions: BTreeSet<String>,
        evidence: String,
    }

    // Schreibweise -> Vorkommen. BTreeMap, damit die Reihenfolge bei gleicher
    // Datenlage jedes Mal dieselbe ist.
    let mut spellings: BTreeMap<String, Spelling> = BTreeMap::new();
    for session in sessions {
        for candidate in collect_candidates(session, options) {
            let spelling = spellings.entry(candidate.core.clone()).or_default();
            spelling.count += 1;
            spelling.sessions.insert(candidate.session_id.clone());
            if spelling.evidence.is_empty() {
                spelling.evidence = candidate.evidence.clone();
            }
        }
    }

    let names: Vec<&String> = spellings.keys().collect();

    // Vorauswahl ueber die erste Ziffer des Codes. Zwei Schreibweisen
    // desselben Wortes koennen sich in einer Ziffer unterscheiden, aber der
    // ERSTE Laut ist der, den ein Erkenner am zuverlaessigsten trifft -- und
    // ohne diese Schranke waere der Vergleich quadratisch ueber den ganzen
    // Korpus. Die Grenze ist damit auch eine Aussage: eine Verhoerung, die
    // schon am ersten Laut danebenliegt, faengt dieses Netz nicht.
    let mut by_first: BTreeMap<char, Vec<usize>> = BTreeMap::new();
    for (index, name) in names.iter().enumerate() {
        if let Some(first) = encode(name).chars().next() {
            by_first.entry(first).or_default().push(index);
        }
    }

    // Einfachverkettung: was paarweise gleich klingt, gehoert in eine Gruppe.
    let mut parent: Vec<usize> = (0..names.len()).collect();
    fn find(parent: &mut Vec<usize>, mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    for bucket in by_first.values() {
        for (position, &left) in bucket.iter().enumerate() {
            for &right in &bucket[position + 1..] {
                if sounds_alike(names[left], names[right], options) {
                    let (a, b) = (find(&mut parent, left), find(&mut parent, right));
                    if a != b {
                        parent[a] = b;
                    }
                }
            }
        }
    }

    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for index in 0..names.len() {
        let root = find(&mut parent, index);
        groups.entry(root).or_default().push(index);
    }

    let mut out = Vec::new();
    for members in groups.into_values() {
        if members.len() < 2 {
            continue;
        }
        {
            let sessions_covered: BTreeSet<&String> = members
                .iter()
                .flat_map(|&index| spellings[names[index]].sessions.iter())
                .collect();
            if sessions_covered.len() < options.min_sessions {
                continue;
            }

            // Die richtige Schreibweise ist NICHT die haeufigste, sondern die
            // belegte. Gemessen am 03.09.2026: "Grandfall" steht oefter da
            // als "Grandpfeil" -- eine Mehrheitswahl haette die Verhoerung
            // zur Wahrheit gemacht.
            //
            // Und ohne Beleg wird gar nicht vorgeschlagen. Dieselbe Messung
            // zeigte, was der Mehrheits-Rueckfall produziert: von 242
            // Vorschlaegen aus Netz 1 hatten 240 keinen Anker, und darunter
            // war kein einziger fehlender Woerterbuch-Eintrag -- nur Beugungen
            // ("Aufgabe"/"Aufgaben") und verwandte Alltagswoerter
            // ("Sorge"/"Source", "Kachel"/"Kamera"). Ein Netz, das raet,
            // welches von zwei unbekannten Woertern das richtige ist, raet.
            //
            // Und der Anker kommt aus `known.anker`, nicht aus
            // `known.immun`: eine Mailadresse darf einen Namen schuetzen,
            // aber nie die richtige Schreibweise BEGRUENDEN. Siehe
            // [`Bekannt`].
            let Some(canonical_index) = members
                .iter()
                .copied()
                .find(|&index| known.anker.contains(&key(names[index])))
            else {
                continue;
            };

            let canonical = names[canonical_index].clone();
            // UNERREICHBAR, gemessen am 04.09.2026, und die Zeile bleibt
            // bewusst wie sie war: `canonical` stammt aus
            // `collect_candidates`, und die Stufe wirft Alltagswoerter schon
            // dort weg. Ein Mutant, der diese Zeile umdreht, ueberlebt jeden
            // Test -- sie gehoert in einem eigenen Durchgang entfernt, nicht
            // im selben Atemzug mit einer Verhaltensaenderung.
            if is_common_german_word(&canonical) {
                continue;
            }

            for &index in &members {
                if index == canonical_index {
                    continue;
                }
                let alias = names[index];
                // Derselbe Waechter, den der Nachlauf spaeter anlegt: eine
                // Verhoerung, die selbst gewoehnliches Deutsch ist, wuerde
                // ohnehin verworfen -- sie hier vorzuschlagen waere ein
                // Vorschlag ohne Wirkung.
                if is_all_common_german_phrase(alias) {
                    continue;
                }
                // Ein belegt richtiges Wort ist nie eine Verhoerung. Hier
                // gilt die WEITE Menge: eine Mailadresse schuetzt.
                if known.immun.contains(&key(alias)) || is_inflection(&canonical, alias) {
                    continue;
                }
                // Die Schwellen gelten fuer das Paar, das WIRKLICH
                // vorgeschlagen wird.
                //
                // Die Gruppe entsteht aus Einfachverkettung: A passt zu B, B
                // passt zu C, also liegen alle drei in einer Gruppe -- auch
                // wenn A und C weit auseinander sind. Ohne diese Zeile ginge
                // aus einer solchen Kette ein Vorschlag "C -> A" hervor,
                // dessen Klang- und Buchstabenabstand nie geprueft wurde.
                // Die Verkettung darf die Gruppe bilden, aber nicht das Paar
                // rechtfertigen.
                if !sounds_alike(&canonical, alias, options) {
                    continue;
                }
                let spelling = &spellings[alias];
                out.push(Proposal {
                    kind: ProposalKind::Recurrence,
                    canonical: canonical.clone(),
                    alias: alias.clone(),
                    occurrences: spelling.count,
                    session_ids: spelling.sessions.iter().cloned().collect(),
                    evidence: spelling.evidence.clone(),
                    source: "wiederkehrend, richtige Schreibweise belegt".to_string(),
                });
            }
        }
    }

    out
}

/// Netz 2: Klangvergleich gegen die Kontextquellen.
///
/// Laeuft nur auf Woertern, deren gemessene Sicherheit unter
/// [`Options::confidence_ceiling`] liegt -- oder gar nicht gemessen wurde.
fn scan_context(
    sessions: &[ScanSession],
    canonical_terms: &[String],
    known: &BTreeSet<String>,
    options: &Options,
) -> Vec<Proposal> {
    struct Hit {
        count: usize,
        sessions: BTreeSet<String>,
        evidence: String,
        source: String,
    }

    let mut hits: BTreeMap<(String, String), Hit> = BTreeMap::new();

    for session in sessions {
        // Die Namen dieses Gespraechs, dazu die Woerterbuchliste -- die ist
        // fuer JEDES Gespraech Kontext, und genau dort klafft die Luecke:
        // des Betreibers 20 Eintraege trugen am 03.09.2026 null Verhoerungen.
        // Ein gewoehnliches deutsches Wort ist nie eine RICHTIGE SCHREIBWEISE.
        //
        // Netz 1 haelt diesen Waechter seit dem 03.09.2026 (`if
        // is_common_german_word(&canonical)`), Netz 2 hatte ihn nicht -- und
        // genau diese Asymmetrie war das Loch. Sie kostet doppelt: das
        // Woerterbuch bekaeme einen Eintrag wie `Webseite => Website`, und der
        // Nachlauf ersetzt danach in JEDEM kuenftigen Gespraech jedes richtige
        // "Website". Ein Vorschlag, der ein Alltagswort zum Ziel erklaert, ist
        // in beiden Netzen falsch, nicht nur in einem.
        //
        // Der Waechter sitzt hier und nicht in der Wortliste, weil die Liste
        // VIER Verbraucher hat (`matcher` an drei Stellen,
        // `Vocabulary::risky_aliases`, beide Netze). Wer sie fuer einen
        // Verbraucher zurechtschneidet, aendert stumm die anderen drei -- am
        // 03.09.2026 genau so geschehen.
        let mut targets: Vec<(String, &'static str)> = Vec::new();
        let mut merke_ziel = |wort: String, quelle: &'static str| {
            if !is_ordinary_german_word(&wort) {
                targets.push((wort, quelle));
            }
        };
        for name in &session.context_names {
            for part in context_name_parts(name) {
                merke_ziel(part, "Teilnehmer");
            }
        }
        // NUR die richtigen Schreibweisen. Die rohe Zeile
        // "Grandpfeil => Grandfall" haette hier ihre VERHOERUNG als Ziel
        // eingeschleust -- und das Netz haette vorgeschlagen, kuenftig
        // "Grandpfeil" durch "Grandfall" zu ersetzen. Ein eigener Test haelt
        // diese Umkehrung fest.
        for term in canonical_terms {
            for part in context_name_parts(term) {
                merke_ziel(part, "Woerterbuch");
            }
        }
        for term in context_text_terms(&session.context_text) {
            merke_ziel(term, "Titel oder Notiz");
        }
        if targets.is_empty() {
            continue;
        }

        for candidate in collect_candidates(session, options) {
            // `None` ist "nicht gemessen", nicht "sicher" -- solche Woerter
            // laufen weiter mit.
            if candidate
                .measured_confidence
                .is_some_and(|value| value >= options.confidence_ceiling)
            {
                continue;
            }
            // Ein Wort, das selbst belegt richtig ist, wird nie zur
            // Verhoerung erklaert. Das ist der Waechter, der "Verlin" vor
            // "Merlin" rettet -- und er greift ueber den ganzen Korpus.
            if known.contains(&key(&candidate.core)) || is_all_common_german_phrase(&candidate.core)
            {
                continue;
            }
            for (target, source) in &targets {
                if !sounds_alike(&candidate.core, target, options)
                    || is_inflection(target, &candidate.core)
                {
                    continue;
                }
                let hit = hits
                    .entry((target.clone(), candidate.core.clone()))
                    .or_insert_with(|| Hit {
                        count: 0,
                        sessions: BTreeSet::new(),
                        evidence: candidate.evidence.clone(),
                        source: (*source).to_string(),
                    });
                hit.count += 1;
                hit.sessions.insert(candidate.session_id.clone());
                break;
            }
        }
    }

    hits.into_iter()
        .map(|((canonical, alias), hit)| Proposal {
            kind: ProposalKind::Context,
            canonical,
            alias,
            occurrences: hit.count,
            session_ids: hit.sessions.into_iter().collect(),
            evidence: hit.evidence,
            source: hit.source,
        })
        .collect()
}

/// Beide Netze, entdoppelt und um alles bereinigt, was schon bekannt oder
/// schon abgelehnt ist.
///
/// `dismissed` sind die Paare, die ein Mensch schon einmal verworfen hat.
/// Ohne sie kaeme jeder abgelehnte Vorschlag beim naechsten Lauf zurueck --
/// und ein Postfach, das Erledigtes wieder vorlegt, liest irgendwann niemand
/// mehr.
pub fn scan(
    sessions: &[ScanSession],
    dictionary_terms: &[String],
    dismissed: &[(String, String)],
    options: &Options,
) -> Vec<Proposal> {
    let known = crate::Vocabulary::parse(dictionary_terms);
    let canonical_terms = known.canonical_terms();
    // Was schon am Eintrag steht, ist erledigt.
    //
    // Die Gegenrichtung braucht hier NICHTS: ein Vorschlag
    // "Grandfall <- Grandpfeil" waere die Umkehrung einer Entscheidung, die
    // ein Mensch schon getroffen hat -- aber er kann gar nicht entstehen,
    // weil beide Netze eine belegte Schreibweise nie als Verhoerung
    // vorschlagen (`known.contains(alias)`). Eine Sperre fuer einen Zustand,
    // der nicht eintreten kann, ist keine Sperre, sondern eine Zeile, der ein
    // spaeterer Leser vertraut. GEMESSEN: ein Mutant, der die Gegenrichtung
    // entfernte, ueberlebte jeden Test -- deshalb steht sie nicht mehr da.
    // `ein_bekanntes_paar_bleibt_auch_umgedreht_gesperrt` haelt das Verhalten
    // fest, egal welcher Waechter es traegt.
    let mut known_pairs: BTreeSet<(String, String)> = BTreeSet::new();
    for entry in known.entries() {
        for alias in &entry.aliases {
            known_pairs.insert((key(&entry.canonical), key(alias)));
        }
    }
    let dismissed: BTreeSet<(String, String)> = dismissed
        .iter()
        .map(|(canonical, alias)| (key(canonical), key(alias)))
        .collect();

    let known = known_terms(sessions, &canonical_terms);
    let mut all = scan_recurrence(sessions, &known, options);
    // Netz 2 zieht seine Ziele selbst zusammen (Teilnehmer, Woerterbuch,
    // Titel und Notiz) und beruehrt `context_emails` an keiner Stelle -- es
    // braucht `known` deshalb NUR als Sperre und bekommt die weite Menge.
    all.extend(scan_context(sessions, &canonical_terms, &known.immun, options));

    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut out = Vec::new();
    for proposal in all {
        let pair = (key(&proposal.canonical), key(&proposal.alias));
        if pair.0 == pair.1
            || seen.contains(&pair)
            || dismissed.contains(&pair)
            || known_pairs.contains(&pair)
        {
            continue;
        }
        seen.insert(pair);
        out.push(proposal);
    }

    // Haeufigstes zuerst: was oft falsch stand, kostet am meisten.
    out.sort_by(|left, right| {
        right
            .occurrences
            .cmp(&left.occurrences)
            .then_with(|| right.session_ids.len().cmp(&left.session_ids.len()))
            .then_with(|| left.canonical.cmp(&right.canonical))
            .then_with(|| left.alias.cmp(&right.alias))
    });
    out
}

#[cfg(test)]
mod messbank;
#[cfg(test)]
mod tests;
