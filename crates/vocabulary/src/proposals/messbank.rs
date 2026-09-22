//! Die Messbank: beide Netze ueber den echten Korpus des Betreibers.
//!
//! Kein Beweis-Test, sondern das Werkzeug, mit dem die Vorgaben in
//! [`super::Options`] GEMESSEN und nicht geraten wurden. Laeuft nur, wenn eine
//! Korpus-Datei danebenliegt -- auf jeder anderen Kiste ist das kein roter
//! Lauf, sondern ein uebersprungener.
//!
//! ```text
//! MITSCHNITT_KORPUS=/pfad/korpus.json \
//!   cargo test -p vocabulary -- --ignored --nocapture messbank
//! ```
//!
//! Die Korpus-Datei enthaelt echte Gespraeche und gehoert NICHT ins
//! Repo. Erzeugt wird sie aus einer KOPIE der laufenden Datenbank (mit `-wal`
//! und `-shm`), nie aus dem Original.

use std::collections::BTreeMap;

use super::*;

#[derive(serde::Deserialize)]
struct KorpusDatei {
    sessions: Vec<KorpusSitzung>,
    /// Die Woerterbuchliste, wie sie in `app_settings` steht.
    #[serde(default)]
    dictionary_terms: Vec<String>,
}

#[derive(serde::Deserialize)]
struct KorpusSitzung {
    session_id: String,
    title: String,
    words: Vec<KorpusWort>,
    context_names: Vec<String>,
    /// Die Mailadressen -- eigenes Feld, weil sie NUR Immunitaet liefern.
    /// `default`, damit eine aeltere Korpus-Datei weiter lesbar bleibt.
    #[serde(default)]
    context_emails: Vec<String>,
    context_text: String,
    #[serde(default)]
    generated_title: String,
}

#[derive(serde::Deserialize)]
struct KorpusWort {
    text: String,
    measured_confidence: Option<f64>,
}

fn korpus() -> Option<(Vec<ScanSession>, BTreeMap<String, String>, Vec<String>)> {
    let pfad = std::env::var("MITSCHNITT_KORPUS").ok()?;
    let roh = std::fs::read_to_string(&pfad).ok()?;
    let datei: KorpusDatei = serde_json::from_str(&roh).ok()?;
    let begriffe = liste_aus_korpus(&datei);
    let mut titel = BTreeMap::new();
    let sessions = datei
        .sessions
        .into_iter()
        .map(|sitzung| {
            titel.insert(sitzung.session_id.clone(), sitzung.title);
            ScanSession {
                session_id: sitzung.session_id,
                words: sitzung
                    .words
                    .into_iter()
                    .map(|wort| ScanWord {
                        text: wort.text,
                        measured_confidence: wort.measured_confidence,
                    })
                    .collect(),
                context_names: sitzung.context_names,
                context_emails: sitzung.context_emails,
                generated_title: String::new(),
                context_text: sitzung.context_text,
            }
        })
        .collect();
    Some((sessions, titel, begriffe))
}

/// Die Woerterbuchliste kommt AUS DER KORPUS-DATEI, nicht aus dem Quelltext.
///
/// Bis zum 03.09.2026 standen hier zwanzig echte Begriffe: Kundennamen, der
/// Arbeitgeber, der Wohnort. Ein Fork, der eines Tages oeffentlich wird
/// (ZICK-252), traegt so etwas fuer immer im Verlauf mit sich -- und ein
/// Rueckbau nach dem Push ist keiner. Die Liste gehoert zur Messung, nicht
/// ins Repo.
fn liste_aus_korpus(datei: &KorpusDatei) -> Vec<String> {
    datei.dictionary_terms.clone()
}

#[test]
#[ignore = "braucht MITSCHNITT_KORPUS mit einer Kopie der Datenbank"]
fn messbank_am_echten_korpus() {
    let Some((sessions, titel, liste)) = korpus() else {
        eprintln!("MITSCHNITT_KORPUS nicht gesetzt oder nicht lesbar -- uebersprungen.");
        return;
    };

    let woerter: usize = sessions.iter().map(|s| s.words.len()).sum();
    let gemessen: usize = sessions
        .iter()
        .flat_map(|s| &s.words)
        .filter(|w| w.measured_confidence.is_some())
        .count();
    println!(
        "\nKorpus: {} Gespraeche, {woerter} Woerter, davon {gemessen} mit gemessener Sicherheit ({:.1} %)",
        sessions.len(),
        100.0 * gemessen as f64 / woerter as f64
    );

    let options = Options::default();
    let kandidaten: usize = sessions
        .iter()
        .map(|s| collect_candidates(s, &options).len())
        .sum();
    println!("Kandidaten (grossgeschrieben, kein Satzanfang, kein Allerweltswort): {kandidaten}");

    let gefunden = scan(&sessions, &liste, &[], &options);

    let kurz = |id: &String| -> String {
        let name = titel.get(id).cloned().unwrap_or_default();
        let name = if name.is_empty() {
            "(ohne Titel)".to_string()
        } else {
            name
        };
        format!("{}…{}", &id[..8], name.chars().take(22).collect::<String>())
    };

    for netz in [ProposalKind::Recurrence, ProposalKind::Context] {
        let teil: Vec<&Proposal> = gefunden.iter().filter(|p| p.kind == netz).collect();
        println!("\n=== {netz:?}: {} Vorschlaege ===", teil.len());
        for vorschlag in teil {
            println!(
                "  {:>3}x  {:22} <- {:22}  [{}]  {}",
                vorschlag.occurrences,
                vorschlag.canonical,
                vorschlag.alias,
                vorschlag.source,
                vorschlag
                    .session_ids
                    .iter()
                    .map(kurz)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!("        …{}", vorschlag.evidence);
        }
    }

    println!("\nGESAMT: {} Vorschlaege", gefunden.len());
}

/// Die bekannten Paare aus dem Auftrag, einzeln nachgesehen -- steht ihr
/// Klangvergleich ueberhaupt?
#[test]
#[ignore = "Diagnose, kein Beweis"]
fn klangcodes_der_bekannten_paare() {
    let options = Options::default();
    for (links, rechts) in [
        // Die ERFUNDENEN Paare der Tests, mit denselben Eigenschaften wie die
        // echten. Diese Zeile ist der Beleg, dass der Ersatz traegt.
        ("Grandpfeil", "Grandfall"),
        ("Tofmann", "Tochmann"),
        ("Mads", "Madss"),
        ("Verlin", "Merlin"),
        ("Ohlandez", "Sarnec"),
        ("Sarnec", "Sarnek"),
        ("Meiners", "Meinerz"),
        ("Aufgabe", "Aufgaben"),
        ("Sodas", "Suedetauss"),
        ("Sarkan", "Serken"),
        ("Serken", "Serkins"),
        ("Sarkan", "Serkins"),
    ] {
        println!(
            "{links:12} {:10}   {rechts:12} {:10}   Code-Abstand {}   Buchstaben-Abstand {} (erlaubt {})   trifft {}",
            encode(links),
            encode(rechts),
            levenshtein(&encode(links), &encode(rechts)),
            levenshtein(&links.to_lowercase(), &rechts.to_lowercase()),
            max_edits_for(links, rechts, &options),
            sounds_alike(links, rechts, &options),
        );
    }
}
