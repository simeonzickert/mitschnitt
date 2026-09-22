//! Prueft die Stichwortliste gegen fertige Texte -- ohne Transkription.
//!
//! Wozu: Die Frage, die ueber diesen Nachlauf entscheidet, ist NICHT "wie viele
//! Namen findet er", sondern "wie viele gewoehnliche Woerter macht er kaputt".
//! Ein einzelner Messlauf ueber eine Aufnahme liefert dafuer viel zu wenig
//! Sprache: in `tom.mp3` (23,6 min, 3355 Woerter) steht "werden" genau einmal.
//! Am 16.07.2026 hat die Vorgaengerfunktion (ZICK-139) an genau dieser Klasse
//! ihr Leben gelassen -- 63 gewoehnliche deutsche Woerter waeren ersetzt worden.
//!
//! Diese Bank laesst denselben Nachlauf ueber beliebig viel vorhandenen Text
//! laufen (alte Messlaeufe, Transkripte, Notizen) und listet JEDE Ersetzung
//! auf. Sie kostet keine Rechenzeit und skaliert mit dem Material.
//!
//! ```text
//! cargo run --release --example scan -p vocabulary -- \
//!     --vocabulary wortliste.txt --vocabulary aliasse.txt \
//!     --text a.md --text b.md
//! ```

use std::path::PathBuf;

use vocabulary::{MatchKind, Options, Token, Vocabulary, apply};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut vocabulary_files: Vec<PathBuf> = Vec::new();
    let mut text_files: Vec<PathBuf> = Vec::new();
    let mut options = Options::default();

    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].clone();
        let mut value = || -> String {
            index += 1;
            args.get(index).cloned().unwrap_or_else(|| {
                eprintln!("{flag} braucht einen Wert");
                std::process::exit(2);
            })
        };
        match flag.as_str() {
            "--vocabulary" => vocabulary_files.push(PathBuf::from(value())),
            "--text" => text_files.push(PathBuf::from(value())),
            // Ausdruecklich BEIDE Felder je Zweig setzen. Ein Zweig, der sich
            // auf die Vorgabe verlaesst, tut stillschweigend nichts mehr,
            // sobald sich die Vorgabe aendert -- genau das ist am 02.09.2026
            // passiert, als die Klangstufen abgeschaltet wurden.
            "--stages" => match value().as_str() {
                "alias" => {
                    options.alias_phonetics = false;
                    options.canonical_phonetics = false;
                }
                "alias+klang" => {
                    options.alias_phonetics = true;
                    options.canonical_phonetics = false;
                }
                "alle" => {
                    options.alias_phonetics = true;
                    options.canonical_phonetics = true;
                }
                other => {
                    eprintln!("--stages: alias|alias+klang|alle, nicht {other}");
                    std::process::exit(2);
                }
            },
            other => {
                eprintln!("unbekanntes Flag: {other}");
                std::process::exit(2);
            }
        }
        index += 1;
    }

    if vocabulary_files.is_empty() || text_files.is_empty() {
        eprintln!("--vocabulary <datei> (mehrfach) und --text <datei> (mehrfach) sind Pflicht");
        std::process::exit(2);
    }

    let mut lines: Vec<String> = Vec::new();
    for path in &vocabulary_files {
        match std::fs::read_to_string(path) {
            Ok(text) => lines.extend(text.lines().map(str::to_string)),
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                std::process::exit(2);
            }
        }
    }
    let vocabulary = Vocabulary::parse(&lines);

    let mut words_total = 0usize;
    let mut per_kind = [0usize; 3];
    let mut findings: Vec<(String, String, String, MatchKind)> = Vec::new();

    for path in &text_files {
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("{}: nicht lesbar, uebersprungen", path.display());
            continue;
        };
        let tokens: Vec<Token> = text.split_whitespace().map(Token::new).collect();
        words_total += tokens.len();

        for replacement in apply(&vocabulary, &tokens, options) {
            per_kind[kind_index(replacement.kind)] += 1;
            findings.push((
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                replacement.before,
                replacement.after,
                replacement.kind,
            ));
        }
    }

    println!("Eintraege in der Liste : {}", vocabulary.len());
    println!("Dateien                : {}", text_files.len());
    println!("Woerter geprueft       : {words_total}");
    println!("Ersetzungen            : {}", findings.len());
    println!("  alias                : {}", per_kind[0]);
    println!("  alias_klang          : {}", per_kind[1]);
    println!("  namens_klang         : {}", per_kind[2]);
    println!();
    for (file, before, after, kind) in &findings {
        println!("[{:<14}] {before:?} -> {after:?}   ({file})", label(*kind));
    }
}

fn kind_index(kind: MatchKind) -> usize {
    match kind {
        MatchKind::Alias => 0,
        MatchKind::AliasPhonetic => 1,
        MatchKind::CanonicalPhonetic => 2,
    }
}

fn label(kind: MatchKind) -> &'static str {
    match kind {
        MatchKind::Alias => "alias",
        MatchKind::AliasPhonetic => "alias_klang",
        MatchKind::CanonicalPhonetic => "namens_klang",
    }
}
