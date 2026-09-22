use std::path::PathBuf;

use super::*;

/// Die echte Wortliste des Betreibers, so wie sie im Auftrag steht -- nicht
/// ausgedacht: genau diese Begriffe soll der Erkenner spaeter treffen.
///
/// Drei Begriffe stehen nicht auf seiner Liste und sind trotzdem hier: bis zum
/// 03.09.2026 trug **kein einziger** der zwanzig einen Umlaut oder ein `ß`.
/// Das Vokabular hat 194 Stuecke mit Umlaut, es sprach also nichts dagegen --
/// bewiesen war es nur nicht, und der Rundlauf ueber die Stuecke ist genau die
/// Stelle, an der eine Normalisierung Zeichen still veraendern wuerde.
const TERMS: [&str; 23] = [
    "Sedacz",
    "Nordwerk",
    "Phonowerk",
    "Glinck",
    "Noormann",
    "NOR Drucktechnik",
    "Vurden",
    "Verlin",
    "ClickUp",
    "LLMs",
    "Peak AI",
    "Malte Landweg",
    "Webflow",
    "Lexware",
    "Retainer",
    "Lieferanto",
    "Sales-Viewer",
    "Calendly",
    "HubSpot",
    "Brevo",
    "Müller",
    "Straße",
    "Köln",
];

/// Der Ordner, in dem das CoreML-Modell liegt -- und damit auch `vocab.json`
/// und (nach dem Bezug) `tokenizer.json`.
fn model_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let snapshots = PathBuf::from(home)
        .join(".cache/huggingface/hub")
        .join("models--aufklarer--Parakeet-TDT-v3-CoreML-INT8-30s")
        .join("snapshots");

    std::fs::read_dir(snapshots)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.join("vocab.json").exists())
}

/// Laedt den Zerleger aus der echten Datei -- und BRICHT, wenn sie fehlt.
///
/// Diese Tests tragen `#[ignore]`, weil sie das geladene Modell voraussetzen:
/// auf einer frischen Kiste gibt es weder `vocab.json` noch `tokenizer.json`,
/// und ein roter Gesamtlauf waere dort eine Falschmeldung. Aufruf:
/// `cargo test -p vocabulary -- --ignored`.
///
/// Bewusst KEIN stilles Ueberspringen im Rumpf: ein Test, der sich bei
/// fehlender Datei selbst gruen meldet, kann nicht scheitern -- und ein Test,
/// der nicht scheitern kann, ist kein Beweis.
fn real_tokenizer() -> (TermTokenizer, PathBuf) {
    let dir = model_dir().expect(
        "Das Parakeet-Modell liegt nicht im Hugging-Face-Zwischenspeicher. \
         Diese Tests messen gegen die echte Datei.",
    );
    // Bewusst der PRODUKTIVE Ladeweg und nicht `from_verified_file`: so faehrt
    // jeder dieser Tests den Vokabular-Waechter mit, statt ihn nur an einer
    // Stelle eigens aufzurufen.
    let tokenizer =
        TermTokenizer::load_next_to_model(&dir).unwrap_or_else(|error| panic!("{error}"));
    (tokenizer, dir)
}

#[test]
#[cfg_attr(
    not(feature = "real-model"),
    ignore = "braucht das geladene Parakeet-Modell"
)]
fn every_term_splits_into_the_model_vocabulary() {
    let (tokenizer, _) = real_tokenizer();

    for term in TERMS {
        let ids = tokenizer
            .split_term(term)
            .unwrap_or_else(|error| panic!("{error}"));

        // Falsifikator 1: eine ID ausserhalb 0..8191 -- `split_term` faengt sie
        // schon ab, hier steht sie noch einmal als Aussage.
        assert!(
            ids.iter().all(|&id| id < VOCAB_SIZE),
            "{term}: {ids:?} verlaesst das Vokabular"
        );
        // Falsifikator 3: eine leere Zerlegung.
        assert!(!ids.is_empty(), "{term}: leer zerlegt");

        // Falsifikator 2: der Rundlauf ueber die Stuecke.
        let joined = tokenizer
            .join_pieces(&ids)
            .unwrap_or_else(|| panic!("{term}: eine ID hat kein Stueck"));
        assert_eq!(
            joined, term,
            "{term}: Rundlauf ergibt {joined:?} (IDs {ids:?})"
        );
    }
}

#[test]
#[cfg_attr(
    not(feature = "real-model"),
    ignore = "braucht das geladene Parakeet-Modell"
)]
fn the_vocabulary_of_the_tokenizer_is_the_vocabulary_of_the_model() {
    // Der Waechter. Ohne ihn kann die Zerlegung sauber aussehen und trotzdem
    // auf ein anderes Vokabular zeigen als der Erkenner -- dann schiebt der
    // Schubs Wahrscheinlichkeit auf fremde Silben, und niemand sieht es.
    let (tokenizer, dir) = real_tokenizer();
    let vocab_path = dir.join("vocab.json");
    let vocab_json = std::fs::read_to_string(&vocab_path).expect("vocab.json lesbar");

    assert_eq!(tokenizer.core_vocab_size(), VOCAB_SIZE as usize);
    tokenizer
        .check_against_model_vocabulary(&vocab_json, &vocab_path)
        .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
#[cfg_attr(
    not(feature = "real-model"),
    ignore = "braucht das geladene Parakeet-Modell"
)]
fn the_guard_notices_a_single_swapped_piece() {
    // Die Gegenprobe zum Waechter: er muss auch ANSCHLAGEN. Ein Waechter, der
    // nur an der heilen Wirklichkeit gruen wird, hat nichts bewiesen.
    let (tokenizer, dir) = real_tokenizer();
    let vocab_json = std::fs::read_to_string(dir.join("vocab.json")).expect("vocab.json lesbar");
    let mut map: std::collections::BTreeMap<String, String> =
        serde_json::from_str(&vocab_json).expect("vocab.json ist eine ID->Stueck-Karte");
    map.insert("681".to_string(), "\u{2581}Nicht".to_string());
    let tampered = serde_json::to_string(&map).expect("wieder serialisierbar");

    let error = tokenizer
        .check_against_model_vocabulary(&tampered, dir.join("vocab.json"))
        .expect_err("ein vertauschtes Stueck muss auffallen");
    assert!(format!("{error}").contains("ID 681"), "{error}");
}

#[test]
#[cfg_attr(
    not(feature = "real-model"),
    ignore = "braucht das geladene Parakeet-Modell"
)]
fn the_load_path_itself_refuses_a_model_whose_vocabulary_disagrees() {
    // Der Falsifikator zu `load_next_to_model`. Der Waechter im Ladeweg zu
    // haben ist nur dann etwas wert, wenn das LADEN scheitert -- ein Test, der
    // `check_against_model_vocabulary` eigens aufruft, beweist nur, dass die
    // Funktion existiert.
    //
    // Aufbau: echte `tokenizer.json`, danebengelegte `vocab.json` mit EINEM
    // vertauschten Stueck.
    let (_, real_dir) = real_tokenizer();
    let dir = tempfile::tempdir().expect("Arbeitsordner");
    std::fs::copy(
        tokenizer_path_next_to_model(&real_dir),
        tokenizer_path_next_to_model(dir.path()),
    )
    .expect("tokenizer.json kopierbar");

    let mut map: std::collections::BTreeMap<String, String> = serde_json::from_str(
        &std::fs::read_to_string(real_dir.join("vocab.json")).expect("vocab.json lesbar"),
    )
    .expect("vocab.json ist eine ID->Stueck-Karte");
    map.insert("681".to_string(), "\u{2581}Nicht".to_string());
    std::fs::write(
        dir.path().join("vocab.json"),
        serde_json::to_string(&map).expect("wieder serialisierbar"),
    )
    .expect("schreibbar");

    let Err(error) = TermTokenizer::load_next_to_model(dir.path()) else {
        panic!("ein vertauschtes Stueck muss den LADEVORGANG abbrechen");
    };
    assert!(matches!(error, Error::VocabularyMismatch { .. }), "{error}");
    assert!(format!("{error}").contains("ID 681"), "{error}");
}

#[test]
#[cfg_attr(
    not(feature = "real-model"),
    ignore = "braucht das geladene Parakeet-Modell"
)]
fn a_term_of_pure_whitespace_is_refused_instead_of_split_into_nothing() {
    // Leere und Leerraum sind der einzige Weg, auf dem eine leere ID-Folge
    // entsteht. Sie darf nicht als "erfolgreich zerlegt" durchgehen.
    let (tokenizer, _) = real_tokenizer();

    assert!(matches!(tokenizer.split_term(""), Err(Error::Empty { .. })));
}

#[test]
fn the_pinned_source_is_a_manufacturer_address_and_not_a_foreign_bucket() {
    // ZICK-263: der Fork zog seine Modelle ueber den Speicher des
    // Ursprungsprojekts, bis der mit 403 antwortete. Diese Zeile haelt fest,
    // wohin `tokenizer.json` zeigt -- eine spaetere Umstellung auf einen
    // Spiegel muss diesen Test brechen, nicht still passieren.
    assert!(
        TOKENIZER_URL.starts_with("https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3/"),
        "{TOKENIZER_URL}"
    );
    assert_eq!(TOKENIZER_BYTES, 1_159_960);
    assert_eq!(TOKENIZER_CRC32, 1_601_548_114);

    // Und die Adresse muss dieselbe Datei meinen wie die Pruefsumme darueber.
    // `/resolve/main/` ist ein beweglicher Zweig: tauscht NVIDIA die Datei,
    // scheitert ein voellig legitimer Neubezug an der Pruefsumme, und der
    // Fehler liest sich wie eine beschaedigte Datei.
    assert!(
        TOKENIZER_URL.contains("/resolve/541d1f99c6b0c3cd0b11a95167540bb8edefd82b/"),
        "Adresse zeigt nicht auf die gepinnte Revision: {TOKENIZER_URL}"
    );
    assert!(
        !TOKENIZER_URL.contains("/resolve/main/"),
        "Adresse zeigt auf einen beweglichen Zweig: {TOKENIZER_URL}"
    );
}

#[test]
fn a_file_that_is_not_the_pinned_one_is_refused() {
    // Die Pruefung muss scheitern koennen, und zwar an genau dem, was sie
    // behauptet zu pruefen.
    let dir = tempfile::tempdir().expect("Arbeitsordner");
    let path = tokenizer_path_next_to_model(dir.path());
    std::fs::write(&path, b"{}").expect("schreibbar");

    let error = verify_tokenizer_file(&path).expect_err("zwei Bytes sind nicht die Datei");
    assert!(matches!(error, Error::Checksum { .. }), "{error}");
}

#[test]
fn a_missing_tokenizer_names_the_way_to_get_it() {
    // Der stille Ausfall, den D4 benennt: die Datei haengt an keinem Bezug.
    // Fehlt sie, darf der Fehler nicht nur "nicht lesbar" sagen -- er muss den
    // Befehl mitliefern, sonst sucht der naechste Mensch im Code danach.
    let dir = tempfile::tempdir().expect("Arbeitsordner");
    let Err(error) = TermTokenizer::load_next_to_model(dir.path()) else {
        panic!("in einem leeren Ordner liegt keine tokenizer.json");
    };

    assert!(matches!(error, Error::Missing { .. }), "{error}");
    let text = format!("{error}");
    assert!(text.contains("curl -L -o"), "{text}");
    assert!(text.contains(TOKENIZER_URL), "{text}");
}

#[test]
fn a_missing_file_says_so_instead_of_pretending_it_matched() {
    let dir = tempfile::tempdir().expect("Arbeitsordner");
    let error = verify_tokenizer_file(tokenizer_path_next_to_model(dir.path()))
        .expect_err("es gibt keine Datei");

    assert!(matches!(error, Error::Read { .. }), "{error}");
}

#[test]
fn an_id_beyond_the_core_vocabulary_is_refused() {
    // 8192 ist `<blank>`: er steht in `added_tokens`, nicht in `model.vocab`.
    // Der Dekoder gibt ihn nie als Wort aus, und ein Schubs darauf waere ein
    // Schubs auf die Stille zwischen den Woertern.
    let error = guard_ids("Sedacz", vec![681, VOCAB_SIZE], ordinary_pieces)
        .expect_err("8192 gehoert nicht zum Vokabular");

    assert!(matches!(
        error,
        Error::OutOfVocabulary {
            id: 8192,
            max: 8191,
            ..
        }
    ));
}

#[test]
fn an_empty_split_is_refused() {
    assert!(matches!(
        guard_ids("Nordwerk", Vec::new(), ordinary_pieces),
        Err(Error::Empty { .. })
    ));
}

/// Ein erfundenes Vokabular ohne Steuertokens -- fuer die Faelle, in denen die
/// Steuertoken-Pruefung nicht die Sache ist.
fn ordinary_pieces(id: u32) -> Option<String> {
    Some(format!("\u{2581}silbe{id}"))
}

#[test]
fn a_control_token_inside_the_vocabulary_is_refused() {
    // Die Luecke bis zum 03.09.2026. `VOCAB_SIZE` faengt genau EINEN der 264
    // Steuertokens ab -- `<blank>` mit 8192 --, weil nur der ausserhalb liegt.
    // `<pad>` steht bei 2, `<|startoftranscript|>` bei 4, die 30
    // `<|spltoken*|>` bei 244..273: alle mitten im gueltigen Bereich, alle
    // waeren stillschweigend durchgelaufen.
    let error = guard_ids("Sedacz", vec![681, 4], |id| match id {
        4 => Some("<|startoftranscript|>".to_string()),
        _ => ordinary_pieces(id),
    })
    .expect_err("ein Steuertoken ist keine Silbe");

    assert!(
        matches!(&error, Error::ControlToken { id: 4, .. }),
        "{error}"
    );
}

#[test]
fn a_digit_is_not_mistaken_for_a_control_token() {
    // Die Gegenprobe, und der Grund fuer die Pruefung nach FORM statt nach
    // Mitgliedschaft in `added_tokens`: die Ziffern `0`..`9` stehen bei
    // 234..243 ebenfalls in `added_tokens`, sind aber gewoehnliche Stuecke.
    // Wer sie mitverbietet, kann `NOR 24` nicht mehr zerlegen.
    let ids = guard_ids("NOR 24", vec![681, 236, 238], |id| match id {
        236 => Some("2".to_string()),
        238 => Some("4".to_string()),
        _ => ordinary_pieces(id),
    })
    .expect("Ziffern sind Silben");

    assert_eq!(ids, vec![681, 236, 238]);
}

#[test]
#[ignore = "Ausgabe fuer den Bericht, kein Beweis"]
fn print_the_split_of_every_term() {
    let (tokenizer, _) = real_tokenizer();
    for term in TERMS {
        let ids = tokenizer.split_term(term).expect("zerlegbar");
        let pieces: Vec<String> = ids
            .iter()
            .map(|&id| tokenizer.piece(id).unwrap_or_default())
            .collect();
        println!("{term} -> {ids:?} {pieces:?}");
    }
}
