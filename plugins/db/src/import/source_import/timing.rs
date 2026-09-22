/// Zensus ueber alle 404 Transkripte der Testquelle (10.09.2026): 453.141
/// Woerter tragen `"source":"synthetic_text"`, 49.613 `provider_word`, null
/// `decoder_frame`. Ein 400-ms-Raster je Wort ist also der Regelfall, nicht die
/// Ausnahme -- und die Oberflaeche muss das sagen koennen, ohne 337 MB JSON zu
/// durchsuchen. Deshalb wird die Einstufung EINMAL beim Import berechnet.
///
/// Bewusst ein Byte-Zensus statt eines JSON-Parsers: die Frage ist nur, ob
/// erfundene und echte Zeiten vorkommen, und ein Parser ueber 337 MB kostet
/// Minuten fuer dieselbe Antwort. Die Zeiten selbst werden NICHT angefasst.
/// Nur im Test: der echte Weg prueft die Woerter mit `instr` in SQLite, damit
/// sie die Datenbank nie verlassen. Diese Fassung haelt fest, WELCHE Marker
/// gemeint sind -- kippt einer davon, wird der Test rot, obwohl der SQL-Weg
/// ihn allein nicht sichtbar machen wuerde.
#[cfg(test)]
pub fn classify_words(words_json: &str) -> Option<&'static str> {
    classify(
        words_json.contains("synthetic_text"),
        words_json.contains("provider_word") || words_json.contains("decoder_frame"),
    )
}

/// Die Einstufung selbst -- ein Ort, damit der SQL-Weg (der die Woerter mit
/// `instr` in der Datenbank prueft, statt 337 MB nach Rust zu ziehen) und der
/// Byte-Weg nie auseinanderlaufen koennen.
pub fn classify(synthetic: bool, genuine: bool) -> Option<&'static str> {
    match (synthetic, genuine) {
        (true, true) => Some("mixed"),
        (true, false) => Some("synthetic"),
        (false, true) => Some("provider"),
        (false, false) => None,
    }
}

/// Schreibt `timing_quality` in ein `metadata_json`-Objekt, ohne bestehende
/// Felder zu verlieren. Ist die Einstufung unbekannt, bleibt das Feld weg --
/// ein erfundenes Etikett waere schlimmer als keins.
pub fn with_timing_quality(metadata_json: &str, quality: Option<&str>) -> String {
    let Some(quality) = quality else {
        return metadata_json.to_owned();
    };

    let mut value = serde_json::from_str::<serde_json::Value>(metadata_json)
        .ok()
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

    if let Some(object) = value.as_object_mut() {
        object.insert(
            "timing_quality".to_owned(),
            serde_json::Value::String(quality.to_owned()),
        );
    }

    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn einstufung_deckt_alle_vier_faelle() {
        assert_eq!(
            classify_words(r#"[{"timing":{"source":"synthetic_text"}}]"#),
            Some("synthetic")
        );
        assert_eq!(
            classify_words(r#"[{"timing":{"source":"provider_word"}}]"#),
            Some("provider")
        );
        assert_eq!(
            classify_words(r#"[{"timing":{"source":"decoder_frame"}}]"#),
            Some("provider")
        );
        assert_eq!(
            classify_words(
                r#"[{"timing":{"source":"synthetic_text"}},{"timing":{"source":"provider_word"}}]"#
            ),
            Some("mixed")
        );
        assert_eq!(classify_words("[]"), None);
    }

    #[test]
    fn bestehende_felder_ueberleben_das_etikett() {
        let vorher = r#"{"foo":1,"bar":"zwei"}"#;
        let nachher = with_timing_quality(vorher, Some("synthetic"));
        let parsed = serde_json::from_str::<serde_json::Value>(&nachher).unwrap();
        assert_eq!(parsed["foo"], 1);
        assert_eq!(parsed["bar"], "zwei");
        assert_eq!(parsed["timing_quality"], "synthetic");
    }

    #[test]
    fn kaputtes_metadata_json_wird_nicht_zum_datenverlust_am_etikett() {
        let nachher = with_timing_quality("nicht json", Some("provider"));
        let parsed = serde_json::from_str::<serde_json::Value>(&nachher).unwrap();
        assert_eq!(parsed["timing_quality"], "provider");
    }

    #[test]
    fn ohne_einstufung_bleibt_metadata_unveraendert() {
        assert_eq!(with_timing_quality(r#"{"foo":1}"#, None), r#"{"foo":1}"#);
    }
}
