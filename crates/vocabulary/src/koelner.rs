//! Koelner Phonetik (Hans Joachim Postel, 1969).
//!
//! Warum dieser Algorithmus und nicht Soundex oder Metaphone: die beiden
//! anderen sind auf englische Aussprache geeicht. Die Koelner Phonetik ist der
//! deutsche Standard und kennt genau die Faelle, die uns treffen -- "V" und "F"
//! klingen gleich, "C" haengt vom Folgebuchstaben ab, "PH" ist ein "F".
//!
//! WICHTIG, damit niemand die Zahl aus dem Auftrag falsch weitergibt: die
//! gemessenen 32,3 % -> 25,3 % Namensfehler (arXiv 2506.10779) stammen von
//! Double Metaphone auf ENGLISCHEN Vorlesungen. Fuer die Koelner Phonetik in
//! diesem Anwendungsfall gibt es keine veroeffentlichte Messung. Was hier
//! zaehlt, ist die Messung am eigenen Material.
//!
//! Der Code besteht aus Ziffern. Gleiche Ziffern hintereinander werden
//! zusammengezogen, danach faellt jede "0" weg -- ausser der ersten Stelle.

/// Reduziert ein Wort auf seinen Koelner-Phonetik-Code.
///
/// Gibt einen leeren String zurueck, wenn nichts Verwertbares uebrig bleibt
/// (Ziffern, Satzzeichen, leere Eingabe).
pub fn encode(word: &str) -> String {
    let letters = normalize(word);
    if letters.is_empty() {
        return String::new();
    }

    let mut digits: Vec<u8> = Vec::with_capacity(letters.len() + 1);
    for (index, &letter) in letters.iter().enumerate() {
        let previous = index.checked_sub(1).map(|i| letters[i]);
        let next = letters.get(index + 1).copied();
        push_code(&mut digits, letter, previous, next, index == 0);
    }

    // Gleiche Ziffern hintereinander gelten als eine.
    digits.dedup();

    // Die "0" traegt keine Information -- ausser als erste Stelle, wo sie sagt,
    // dass das Wort mit einem Vokal beginnt.
    let mut out = String::with_capacity(digits.len());
    for (index, digit) in digits.into_iter().enumerate() {
        if digit == b'0' && index > 0 {
            continue;
        }
        out.push(digit as char);
    }
    out
}

/// Grossbuchstaben, Umlaute aufgeloest, alles Nicht-Buchstabige verworfen.
fn normalize(word: &str) -> Vec<char> {
    let mut letters = Vec::with_capacity(word.len());
    for character in word.chars() {
        match character {
            'ä' | 'Ä' => letters.push('A'),
            'ö' | 'Ö' => letters.push('O'),
            'ü' | 'Ü' => letters.push('U'),
            'ß' => letters.extend(['S', 'S']),
            'é' | 'è' | 'ê' | 'É' | 'È' | 'Ê' => letters.push('E'),
            'á' | 'à' | 'â' | 'Á' | 'À' | 'Â' => letters.push('A'),
            'í' | 'ì' | 'î' | 'Í' | 'Ì' | 'Î' => letters.push('I'),
            'ó' | 'ò' | 'ô' | 'Ó' | 'Ò' | 'Ô' => letters.push('O'),
            'ú' | 'ù' | 'û' | 'Ú' | 'Ù' | 'Û' => letters.push('U'),
            'ç' | 'Ç' => letters.push('C'),
            'ñ' | 'Ñ' => letters.push('N'),
            _ if character.is_ascii_alphabetic() => letters.push(character.to_ascii_uppercase()),
            _ => {}
        }
    }
    letters
}

fn push_code(
    out: &mut Vec<u8>,
    letter: char,
    previous: Option<char>,
    next: Option<char>,
    first: bool,
) {
    // Die Reihenfolge folgt Postels Tabelle: die kontextabhaengigen Regeln
    // stehen vor den allgemeinen, sonst gewinnt die falsche.
    match letter {
        'A' | 'E' | 'I' | 'J' | 'O' | 'U' | 'Y' => out.push(b'0'),
        // H bekommt selbst keinen Code -- es faerbt nur das P davor.
        'H' => {}
        'B' => out.push(b'1'),
        'P' => out.push(if next == Some('H') { b'3' } else { b'1' }),
        'D' | 'T' => out.push(match next {
            Some('C' | 'S' | 'Z') => b'8',
            _ => b'2',
        }),
        'F' | 'V' | 'W' => out.push(b'3'),
        'G' | 'K' | 'Q' => out.push(b'4'),
        'C' => out.push(if first {
            match next {
                Some('A' | 'H' | 'K' | 'L' | 'O' | 'Q' | 'R' | 'U' | 'X') => b'4',
                _ => b'8',
            }
        } else if matches!(previous, Some('S' | 'Z')) {
            b'8'
        } else {
            match next {
                Some('A' | 'H' | 'K' | 'O' | 'Q' | 'U' | 'X') => b'4',
                _ => b'8',
            }
        }),
        'X' => {
            if matches!(previous, Some('C' | 'K' | 'Q')) {
                out.push(b'8');
            } else {
                out.push(b'4');
                out.push(b'8');
            }
        }
        'L' => out.push(b'5'),
        'M' | 'N' => out.push(b'6'),
        'R' => out.push(b'7'),
        'S' | 'Z' => out.push(b'8'),
        _ => {}
    }
}

/// Koelner-Code einer Wortfolge: jedes Wort einzeln kodiert, mit "-" verbunden.
///
/// Bewusst NICHT die Buchstaben zusammenziehen: "NOR Drucktechnik" und
/// "NORDrucktechnik" sind fuer uns dieselbe Sache, aber die Wortgrenze traegt
/// Information -- ohne sie wuerde eine Wortfolge zufaellig auf ein langes
/// Einzelwort passen.
pub fn encode_phrase(words: &[&str]) -> String {
    words
        .iter()
        .map(|word| encode(word))
        .collect::<Vec<_>>()
        .join("-")
}

/// Levenshtein-Abstand auf Zeichen (nicht Bytes -- Umlaute zaehlen als eins).
///
/// Eigenbau statt Abhaengigkeit: zwanzig Zeilen, und der Fork soll fuer eine
/// Schulbuchfunktion keine neue Lieferkette aufmachen.
pub fn levenshtein(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }

    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];

    for (i, &left_char) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, &right_char) in right.iter().enumerate() {
            let cost = usize::from(left_char != right_char);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postels_eigene_beispiele() {
        // Die kanonischen Beispiele aus der Literatur zur Koelner Phonetik.
        assert_eq!(encode("Müller-Lüdenscheidt"), "65752682");
        assert_eq!(encode("Wikipedia"), "3412");
        assert_eq!(encode("Breschnew"), "17863");
    }

    #[test]
    fn gleichklang_wird_gleich_kodiert() {
        assert_eq!(encode("Meier"), encode("Mayer"));
        assert_eq!(encode("Schmidt"), encode("Schmitt"));
        // PH und F sind derselbe Laut.
        assert_eq!(encode("Phonowerk"), encode("Fonowerk"));
    }

    #[test]
    fn h_bekommt_keinen_eigenen_code_aber_faerbt_das_p() {
        // PH ist ein F (3), P allein ist eine 1.
        assert_eq!(encode("Phon"), "36");
        assert_eq!(encode("Pon"), "16");
        // Ein H zwischen Vokalen verschwindet spurlos.
        assert_eq!(encode("Ahorn"), "076");
    }

    #[test]
    fn c_haengt_am_folgebuchstaben_und_am_wortanfang() {
        // C vor A ist ein K (4), C vor E ein Z (8). Die angehaengte Null faellt
        // weg, weil nur die erste Stelle eine behalten darf.
        assert_eq!(encode("Ca"), "4");
        assert_eq!(encode("Ce"), "8");
        // Nach S ist C immer 8 -- und faellt hier mit dem S zusammen.
        assert_eq!(encode("Sca"), "8");
    }

    #[test]
    fn x_zerfaellt_ausser_nach_c_k_q() {
        // X allein ist K+S, also zwei Ziffern.
        assert_eq!(encode("Xaver"), "4837");
        assert_eq!(encode("Axel"), "0485");
        // Direkt hinter dem K steckt der K-Anteil schon im K -- nur noch 8.
        assert_eq!(encode("Akx"), "048");
        // Dazwischen ein Vokal: X zerfaellt wieder in 4 und 8.
        assert_eq!(encode("Knox"), "4648");
    }

    #[test]
    fn fuehrende_null_bleibt_stehen_innere_nicht() {
        // Erste Stelle sagt: beginnt mit Vokal.
        assert_eq!(encode("Otto"), "02");
        assert_eq!(encode("Tor"), "27");
    }

    #[test]
    fn nichts_verwertbares_gibt_leer() {
        assert_eq!(encode(""), "");
        assert_eq!(encode("123"), "");
        assert_eq!(encode("--"), "");
    }

    #[test]
    fn umlaute_und_scharfes_s() {
        assert_eq!(encode("Müller"), "657");
        assert_eq!(encode("Müller"), encode("Mueller"));
        assert_eq!(encode("Straße"), encode("Strasse"));
    }

    #[test]
    fn wortfolgen_behalten_ihre_grenzen() {
        let zusammen = encode("NORDrucktechnik");
        let getrennt = encode_phrase(&["NOR", "Drucktechnik"]);
        assert_ne!(zusammen, getrennt);
        assert!(getrennt.contains('-'));
    }

    #[test]
    fn levenshtein_rechnet_auf_zeichen_nicht_auf_bytes() {
        assert_eq!(levenshtein("Muller", "Müller"), 1);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("Glinck", "Glingt"), 2);
    }

    /// Der Beleg fuer die Gefahr, wegen der dieses Modul einen Waechter
    /// braucht: zwei voellig verschiedene Woerter, ein identischer Code.
    #[test]
    fn ortsname_und_hilfsverb_klingen_fuer_den_algorithmus_gleich() {
        assert_eq!(encode("Vurden"), encode("werden"));
    }
}
