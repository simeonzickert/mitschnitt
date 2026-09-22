//! Die Kontext-Vorgabe fuer whisper.cpp (`initial_prompt`).
//!
//! whisper.cpp nimmt vor dem eigentlichen Text einen Prompt entgegen, der dem
//! Dekoder sagt, in welcher Welt er sich befindet. Das ist der dokumentierte
//! Weg, einem Whisper-Modell Eigennamen beizubringen -- Parakeet hat ihn nicht
//! (`transcribe_file` kennt nur Modell, Datei und Sprache), dort traegt allein
//! der Nachlauf ueber das fertige Transkript.
//!
//! Zwei Eigenheiten bestimmen den Bau, und beide sind still:
//!
//! 1. **Die Vorgabe ist auf 224 Token begrenzt** (`n_text_ctx / 2` in
//!    whisper.cpp). Was darueber liegt, faellt weg -- ohne Fehler und ohne
//!    Warnung.
//! 2. **Abgeschnitten wird VORNE.** Es zaehlen die LETZTEN Token. Wer die
//!    wichtigsten Begriffe an den Anfang stellt, verliert genau sie.
//!
//! Deshalb steht hier der wichtigste Begriff am ENDE, und gekuerzt wird von
//! vorne. Die Reihenfolge der Liste ist die Rangfolge: was ein Mensch zuletzt
//! eingetragen hat, ist ihm am naechsten und wird zuletzt geopfert.

/// Harte Grenze von whisper.cpp: `n_text_ctx / 2` = 224 Token.
const MAX_PROMPT_TOKENS: usize = 224;

/// Zeichenbudget in BYTES, und zwar als obere Schranke statt als Schaetzung.
///
/// Bis zum 02.09.2026 stand hier `224 * 2` ZEICHEN, mit der Begruendung, zwei
/// Zeichen je Token seien "pessimistisch". Das ist keine Schranke, sondern eine
/// Annahme: sie haelt nur, solange kein Begriff dichter als ein Token je zwei
/// Zeichen zerfaellt. Wo er das tut, faellt vorne wieder still etwas ab -- also
/// genau der Fehler, den diese Datei verhindern soll.
///
/// Der Tokenizer selbst ist hier nicht zu fragen: whisper.cpp tokenisiert im
/// geladenen Modellkontext, und `build_prompt` hat keinen. Was aber ohne
/// Modell gilt: Whisper benutzt einen byteweisen BPE (GPT-2-Bauart). Ein
/// solcher Tokenizer kann im schlimmsten Fall jedes EINZELNE Byte als eigenes
/// Token ausgeben und niemals mehr Token als Bytes erzeugen. Damit ist
/// "hoechstens 224 Bytes" eine echte obere Schranke fuer "hoechstens 224
/// Token" -- unabhaengig davon, was das Modell mit dem Text anstellt.
///
/// Der Preis ist die halbe Kapazitaet gegenueber der alten Schaetzung (rund 20
/// bis 25 Begriffe statt rund 45). Das ist die sichere Richtung: zu kurz
/// verliert den letzten Begriff sichtbar, zu lang verliert die ersten still.
const MAX_PROMPT_BYTES: usize = MAX_PROMPT_TOKENS;

/// Trennzeichen zwischen den Begriffen. Ein Komma mit Leerzeichen liest sich
/// fuer das Modell wie eine Aufzaehlung, nicht wie ein Satz.
const SEPARATOR: &str = ", ";

/// Baut die Kontext-Vorgabe aus den richtigen Schreibweisen.
///
/// Erwartet AUSSCHLIESSLICH kanonische Begriffe. Eine eingetragene Verhoerung
/// ("Sedatsch") darf hier nie hinein -- sie dem Modell als erwuenschtes Wort
/// vorzulegen waere das Gegenteil der Absicht.
///
/// Gibt einen leeren String zurueck, wenn nichts uebrig bleibt; der Aufrufer
/// setzt dann keine Vorgabe.
pub fn build_prompt(terms: &[String]) -> String {
    let cleaned: Vec<&str> = terms
        .iter()
        .map(|term| term.trim())
        .filter(|term| !term.is_empty())
        .collect();
    if cleaned.is_empty() {
        return String::new();
    }

    // Von hinten aufsammeln, solange das Budget reicht -- das Ende ist das,
    // was whisper.cpp garantiert sieht.
    let mut kept: Vec<&str> = Vec::new();
    let mut length = 0usize;
    for term in cleaned.iter().rev() {
        let additional = term.len()
            + match kept.is_empty() {
                true => 0,
                false => SEPARATOR.len(),
            };
        if length + additional > MAX_PROMPT_BYTES {
            // `continue`, nicht `break`. Bis zum 02.09.2026 stand hier ein
            // `break`: EIN einzelner zu langer Begriff am Ende der Liste
            // liess damit ALLE davorstehenden fallen. Mit `["Tom", <449
            // Zeichen>]` kam ein leerer Prompt heraus, und whisper.cpp bekam
            // wieder den Vorblock-Text statt des brauchbaren "Tom". Ein zu
            // langer Begriff kostet ab jetzt nur sich selbst.
            continue;
        }
        length += additional;
        kept.push(term);
    }

    kept.reverse();
    kept.join(SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn eine_leere_liste_gibt_keine_vorgabe() {
        assert_eq!(build_prompt(&[]), "");
        assert_eq!(build_prompt(&terms(&["", "   "])), "");
    }

    #[test]
    fn die_begriffe_stehen_als_aufzaehlung_in_ihrer_reihenfolge() {
        assert_eq!(
            build_prompt(&terms(&["Sedacz", "Nordwerk", "Phonowerk"])),
            "Sedacz, Nordwerk, Phonowerk"
        );
    }

    #[test]
    fn leerraum_wird_abgeraeumt_ohne_die_reihenfolge_zu_stoeren() {
        assert_eq!(
            build_prompt(&terms(&["  Sedacz ", "", "Webflow"])),
            "Sedacz, Webflow"
        );
    }

    /// Der Punkt, an dem sich diese Datei rechtfertigt: whisper.cpp schneidet
    /// VORNE ab, also muss der letzte Begriff ueberleben und der erste fallen.
    #[test]
    fn beim_kuerzen_faellt_der_erste_begriff_und_der_letzte_bleibt() {
        let viele: Vec<String> = (0..400).map(|index| format!("Begriff{index:03}")).collect();
        let prompt = build_prompt(&viele);

        // Gemessen wird gegen die ZUSICHERUNG (224 Token, und ein
        // byteweiser BPE erzeugt nie mehr Token als Bytes), nicht gegen
        // `MAX_PROMPT_BYTES`. Gegen die eigene Schranke zu pruefen hiesse,
        // dass der Test jede Aenderung dieser Schranke mitmacht statt sie
        // festzuhalten -- er koennte dann gar nicht scheitern.
        assert!(prompt.len() <= MAX_PROMPT_TOKENS);
        assert!(
            prompt.ends_with("Begriff399"),
            "der letzte Begriff muss ueberleben: {prompt}"
        );
        assert!(
            !prompt.contains("Begriff000"),
            "der erste Begriff muss gefallen sein: {prompt}"
        );
    }

    #[test]
    fn was_hineinpasst_bleibt_vollstaendig() {
        let liste = terms(&["Sedacz", "Nordwerk", "Phonowerk", "Glinck", "Vurden"]);
        let prompt = build_prompt(&liste);
        for term in &liste {
            assert!(prompt.contains(term.as_str()), "{term} fehlt in {prompt}");
        }
    }

    /// Ein einzelner Begriff, der allein schon zu lang ist, ergibt keine halbe
    /// Vorgabe -- lieber gar keine als eine abgeschnittene.
    #[test]
    fn ein_zu_langer_einzelbegriff_wird_nicht_zerschnitten() {
        let riese = "A".repeat(MAX_PROMPT_BYTES + 1);
        assert_eq!(build_prompt(&terms(&[riese.as_str()])), "");
    }

    /// Und er kostet nur sich selbst. Bis zum 02.09.2026 riss er alles mit:
    /// `break` statt `continue` liess bei `["Tom", <zu lang>]` einen leeren
    /// Prompt entstehen, und whisper.cpp bekam wieder den Vorblock-Text statt
    /// des brauchbaren "Tom".
    #[test]
    fn ein_zu_langer_begriff_reisst_die_anderen_nicht_mit() {
        let riese = "A".repeat(MAX_PROMPT_BYTES + 1);
        assert_eq!(build_prompt(&terms(&["Tom", riese.as_str()])), "Tom");
        assert_eq!(
            build_prompt(&terms(&["Tom", riese.as_str(), "Tofmann"])),
            "Tom, Tofmann"
        );
    }

    /// Die Grenze ist eine Schranke, keine Schaetzung: gemessen wird in Bytes,
    /// und ein byteweiser BPE kann nie mehr Token als Bytes erzeugen. Deshalb
    /// haelt sie auch fuer Begriffe, die dichter zerfallen als zwei Zeichen je
    /// Token -- Umlaute belegen in UTF-8 zwei Bytes je Zeichen.
    #[test]
    fn die_grenze_haelt_auch_bei_mehrbyte_zeichen() {
        let dicht: Vec<String> = (0..200)
            .map(|index| format!("Müllerstraße{index}"))
            .collect();
        let prompt = build_prompt(&dicht);
        assert!(
            prompt.len() <= MAX_PROMPT_TOKENS,
            "{} Bytes bei hoechstens {MAX_PROMPT_TOKENS} Token",
            prompt.len()
        );
        assert!(prompt.ends_with("Müllerstraße199"));
    }
}
