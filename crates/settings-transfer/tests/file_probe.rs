//! Mitschnitt-Fork (F14). The falsifiers, run against a real file on disk.
//!
//! The unit tests in `lib.rs` work on strings. These write the file, then probe
//! it the way a suspicious person would -- with `strings(1)` and a text editor
//! -- because "no secret is in the file" is a claim about the file, not about a
//! return value.

use std::process::Command;

use anlg_settings_transfer::{Error, open, seal};

const PASSWORD: &str = "Kennwort-4711";
const API_KEY: &str = "sk-proj-PROBE-geheim-4711";
const DICTIONARY_TERM: &str = "Sedacz";
const TEMPLATE_TITLE: &str = "Kundentermin";

fn payload() -> serde_json::Value {
    serde_json::json!({
        "settings": {
            "current_stt_provider": "whispercpp",
            "current_stt_model": "QuantizedLargeV3Turbo",
            "ai_language": "de",
            "personalization_dictionary_terms": format!("[\"{DICTIONARY_TERM}\"]"),
        },
        "providers": {
            "llm": {},
            "stt": { "openai": { "base_url": "https://api.openai.com", "api_key": API_KEY } },
        },
        "templates": [{
            "id": "9f0f3a30-0000-4000-8000-000000000001",
            "title": TEMPLATE_TITLE,
            "description": "",
            "category": serde_json::Value::Null,
            "icon_json": serde_json::Value::Null,
            "targets_json": serde_json::Value::Null,
            "sections_json": [{ "title": "Entscheidungen", "description": "" }],
        }],
    })
}

/// The shape the app actually writes when credentials stay home: same setup,
/// no key. `seal` refuses anything else for a plain bundle.
fn payload_without_credentials() -> serde_json::Value {
    let mut value = payload();
    value["providers"]["stt"]["openai"]["api_key"] = serde_json::json!("");
    value
}

/// A fresh directory per call, removed when the returned handle drops.
///
/// The earlier fixed path `/tmp/mitschnitt-f14-probe` survived the run and was
/// shared by every test and every concurrent `cargo test`: leftovers from an
/// older revision could be read as this run's output, and nothing ever cleaned
/// up. Nothing here goes anywhere near a real vault either way.
fn write(name: &str, contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::Builder::new()
        .prefix("mitschnitt-f14-probe-")
        .tempdir()
        .unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, contents).unwrap();
    (dir, path)
}

fn strings_of(path: &std::path::Path) -> String {
    let output = Command::new("strings")
        .arg(path)
        .output()
        .expect("strings(1) must be available for this probe");
    assert!(output.status.success(), "strings(1) failed");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Falsifier 3: no meeting, no recording, no database stock in the file. The
/// bundle carries only what was put into it, so the probe lists the file's
/// entire content and checks the shape rather than trusting a sampling.
#[test]
fn a_bundle_contains_the_setup_and_nothing_from_the_meeting_stock() {
    let contents = seal(
        &payload_without_credentials(),
        false,
        None,
        "2026-08-31T16:00:00Z",
        "1.4.14",
    )
    .unwrap();
    let (_probe_dir_path, path) = write("ohne-zugaenge.json", &contents);
    let text = std::fs::read_to_string(&path).unwrap();
    let document: serde_json::Value = serde_json::from_str(&text).unwrap();

    // serde_json parses objects into a sorted map, so the lists are sorted.
    let top: Vec<&String> = document.as_object().unwrap().keys().collect();
    assert_eq!(
        top,
        vec![
            "app_version",
            "created_at",
            "format",
            "includes_secrets",
            "payload",
            "version"
        ]
    );

    let sections: Vec<&String> = document["payload"].as_object().unwrap().keys().collect();
    assert_eq!(sections, vec!["providers", "settings", "templates"]);

    // Words that would appear if session data had leaked in.
    for forbidden in [
        "session",
        "transcript",
        "recording",
        "audio",
        "participant",
        "speaker",
        "word_",
        "chunk",
        "diarization",
        "summary_html",
        "\"words\"",
        "started_at",
        "ended_at",
    ] {
        assert!(
            !text.contains(forbidden),
            "a bundle without credentials must not mention {forbidden}:\n{text}"
        );
    }

    // And a bundle with credentials is opaque anyway, but check the header too.
    let sealed = seal(
        &payload(),
        true,
        Some(PASSWORD),
        "2026-08-31T16:00:00Z",
        "1.4.14",
    )
    .unwrap();
    let (_probe_dir_sealed_path, sealed_path) = write("mit-zugaengen.json", &sealed);
    let sealed_text = std::fs::read_to_string(&sealed_path).unwrap();
    for forbidden in ["session", "transcript", "recording", "participant"] {
        assert!(!sealed_text.contains(forbidden));
    }
}

/// Falsifier 4, first half: a file with credentials must not be readable.
#[test]
fn strings_finds_no_credential_in_a_sealed_bundle() {
    let contents = seal(
        &payload(),
        true,
        Some(PASSWORD),
        "2026-08-31T16:00:00Z",
        "1.4.14",
    )
    .unwrap();
    let (_probe_dir_path, path) = write("strings-probe.json", &contents);
    let printable = strings_of(&path);

    for secret in [
        API_KEY,
        DICTIONARY_TERM,
        TEMPLATE_TITLE,
        "whispercpp",
        "QuantizedLargeV3Turbo",
        "api.openai.com",
        "personalization_dictionary_terms",
        PASSWORD,
    ] {
        assert!(
            !printable.contains(secret),
            "strings(1) found {secret} in the sealed bundle"
        );
    }

    // The counter-check: without the seal, the very same probe finds them --
    // otherwise this test would pass on an empty file.
    let plain = seal(
        &payload_without_credentials(),
        false,
        None,
        "2026-08-31T16:00:00Z",
        "1.4.14",
    )
    .unwrap();
    let (_probe_dir_plain_path, plain_path) = write("strings-gegenprobe.json", &plain);
    let plain_printable = strings_of(&plain_path);
    assert!(plain_printable.contains(DICTIONARY_TERM));
    assert!(plain_printable.contains("whispercpp"));
}

/// Falsifier 4, second half: the wrong password opens nothing at all -- not
/// part of the settings, not part of the templates, nothing.
#[test]
fn a_wrong_password_yields_no_fragment_of_the_payload() {
    let contents = seal(
        &payload(),
        true,
        Some(PASSWORD),
        "2026-08-31T16:00:00Z",
        "1.4.14",
    )
    .unwrap();
    let (_probe_dir_path, path) = write("falsches-kennwort.json", &contents);
    let text = std::fs::read_to_string(&path).unwrap();

    match open(&text, Some("Kennwort-4712")) {
        Err(Error::WrongPassword) => {}
        other => panic!("expected WrongPassword, got {other:?}"),
    }
    match open(&text, None) {
        Err(Error::PasswordRequired) => {}
        other => panic!("expected PasswordRequired, got {other:?}"),
    }

    // And the right one still works, so the test above is not passing because
    // the file is broken.
    let (info, opened) = open(&text, Some(PASSWORD)).unwrap();
    assert!(info.includes_secrets);
    assert_eq!(opened, payload());
}

/// The hole the probe above uncovered: `seal` used to write a readable file
/// with the key in it whenever the caller passed a secret-bearing payload with
/// `includes_secrets = false`. No file at all is the right answer.
#[test]
fn no_file_is_written_when_a_plain_bundle_would_carry_a_key() {
    match seal(&payload(), false, None, "2026-08-31T16:00:00Z", "1.4.14") {
        Err(Error::SecretInPlainBundle) => {}
        other => panic!("expected SecretInPlainBundle, got {other:?}"),
    }
}
