// Redaktion fuer die Logdatei: Heimverzeichnis, Zugangsdaten, Mail-Adressen
// und IP-Adressen werden vor dem Schreiben ersetzt. Bis zum 01.09.2026 stand
// hier ausserdem `sanitize_sentry_event`, das dieselben Regeln auf
// Sentry-Ereignisse anwandte; mit dem Sentry-Sender (ZICK-257) ist es
// entfallen -- was nicht gesendet wird, muss nicht bereinigt werden. Die
// Logdatei bleibt der einzige Empfaenger.
//
// Die Zugangsdaten-Muster (C1, Opus/Forge/Grok/Kimi-Review 02.09.2026) sind
// dieselben wie in `redactSensitiveText()` (apps/desktop/src/error-reporting.ts):
// Anbieter-Antworten zitieren den abgelehnten Schluessel ("Incorrect API key
// provided: sk-…"), Anfrage-Fehler die URL samt Query. Die Webview-Seite
// redigiert zuerst; dieses Netz faengt, was nativ geloggt wird.
use std::io::{self, Write};
use std::sync::LazyLock;

use regex::Regex;

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").expect("Invalid regex")
});

static IP_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").expect("Invalid regex"));

// Reihenfolge wie in der TS-Fassung (D1, Fix-Runde 1d; G3, Fix-Runde 2):
// URL-Query vor Userinfo (die Authority muss das `@` noch sehen), beides vor
// den benannten Geheimnissen, damit eine Query als Query behandelt wird; die
// Header-Zeile vor dem nackten Schema, die Schemata vor `sk-`, damit
// "Bearer sk-…" eine Markierung hinterlaesst, nicht zwei. Alle Klassen sind
// ASCII -- `\w` des regex-Crates ist Unicode und haette z. B. `dbtoken=` und
// beliebige Buchstaben getroffen. Kein Wert beginnt mit `[`: das ist die
// Markierung eines frueheren Durchlaufs, und dieselbe Zeile kann zweimal
// durch die Redaktion laufen (Webview-Seite, dann Logdatei) -- ohne die Regel
// wuerde aus `key=[REDACTED]` beim zweiten Mal `key=[REDACTED]]`.
//
// Muster-Schwaenze stoppen an Leerraum und an `"`, `'`, `)`, `,`, `]`, `}`,
// damit ein JSON-Fragment wie {"url":"https://x/y?key=1","status":403} seine
// Struktur und seinen Status behaelt.
//
// Ein Query-Parameter faellt nach seinem NAMEN, auf jedem Host (G3): der
// Schluessel auf 127.0.0.1 ist derselbe wie auf googleapis.com, und
// `model=` eines lokalen Servers ist so nuetzlich wie das eines fernen.
// Bis Fix-Runde 2 behielt ein Loopback-Host seine ganze Query und jeder
// andere verlor jeden Wert. Ein Host in Klammern ([::1], [2001:db8::1]) ist
// eine Authority wie jede andere; Userinfo (`https://user:pass@host`) faellt
// ganz, mit und ohne Query.
static URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b((?:https?|wss?)://)((?:\[[^\]\s]*\]|[^\s?"'),\]}\[])*)\?([^\s"'),}]*)"#)
        .expect("Invalid regex")
});

static URL_USERINFO_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b((?:https?|wss?)://)[^\s"'),\]}\[@/]+@"#).expect("Invalid regex")
});

// Eine Authorization-Zeile verliert ihren Wert, wie er auch aussieht; ein
// nacktes Bearer/Token/Basic im Fliesstext nur, wenn das Wort danach nach
// einem Token aussieht (looks_like_secret) -- "a Bearer token expires" ist
// Prosa. Gross-/Kleinschreibung ist egal.
static AUTH_HEADER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b((?:proxy-)?authorization\s*:\s*(?:bearer|token|basic))\s+[^\s"'),\]}\[][^\s"'),\]}]*"#,
    )
    .expect("Invalid regex")
});

static AUTH_SCHEME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(bearer|token|basic)\s+([^\s"'),\]}\[][^\s"'),\]}]*)"#)
        .expect("Invalid regex")
});

// Ab 8 Zeichen, mit Stoppliste fuer die Woerter mit demselben Praefix
// (`sk-build`, `sk-project`, `sk-worker`); die alte Untergrenze 20 liess
// `sk-live-abc123` durch. Das regex-Crate kennt kein Lookahead, die
// Stoppliste sitzt im Closure.
static SK_KEY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bsk-[A-Za-z0-9_-]{8,}").expect("Invalid regex"));
const SK_STOP_WORDS: &[&str] = &["build", "project", "worker"];

// Benannte Geheimnisse in jeder Zuweisungsform: `key=…`, `key: …`,
// `"api_key":"…"`. Explizite Liste, kein `\w*token`-Joker. `=` und die
// Formen mit Anfuehrungszeichen immer; ein nacktes `name: wert` nur, wenn
// der Wert nach einem Token aussieht ("the key: press Enter" bleibt). Nackte
// Parameter stoppen auch an `&`, damit `sig=…&se=2026` seinen Nachbarn
// behaelt.
static NAMED_SECRET_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b(api[_-]?key|access_token|refresh_token|id_token|password|secret|token|sig|key)(\s*["']?\s*[:=]\s*["']?)([^\s"'),\]}&\[][^\s"'),\]}&]*)"#,
    )
    .expect("Invalid regex")
});

// Ein Parametername, der ein Geheimnis traegt, an seiner Endung: `key`,
// `api_key`, `x-api-key`, `token`, `id_token`, `client_secret`, `sig`,
// `jwt` … Ein Zuviel (`monkey`) kostet einen lesbaren Wert, ein Zuwenig
// einen Schluessel. Dieselbe Liste in error-reporting.ts und ws-client.
const SECRET_PARAM_NAME_ENDINGS: &[&str] = &[
    "key",
    "token",
    "secret",
    "sig",
    "signature",
    "password",
    "passwd",
    "pwd",
    "auth",
    "credential",
    "credentials",
    "jwt",
];

fn is_secret_param_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    SECRET_PARAM_NAME_ENDINGS
        .iter()
        .any(|ending| name.ends_with(ending))
}

// Ob ein nackter Wert ein Token ist und kein Wort: eine Ziffer, `._=/+`,
// ein Klein- gefolgt von einem Grossbuchstaben (Base64) oder 20+ Zeichen.
// Satzzeichen am Ende zaehlen nicht; ein Bindestrich allein auch nicht
// ("token-based"), `sk-…` hat sein eigenes Muster.
fn looks_like_secret(value: &str) -> bool {
    let core = value.trim_end_matches(['.', ':', ';', '!', '?']);
    core.chars()
        .any(|c| c.is_ascii_digit() || matches!(c, '.' | '_' | '=' | '/' | '+'))
        || core
            .as_bytes()
            .windows(2)
            .any(|pair| pair[0].is_ascii_lowercase() && pair[1].is_ascii_uppercase())
        || core.len() >= 20
}

fn is_sk_stop_word(matched: &str) -> bool {
    let rest = &matched[3..];
    SK_STOP_WORDS.iter().any(|word| {
        rest.len() >= word.len()
            && rest[..word.len()].eq_ignore_ascii_case(word)
            && rest[word.len()..]
                .chars()
                .next()
                .is_none_or(|next| !(next.is_ascii_alphanumeric() || next == '_'))
    })
}

fn redact_query_values(query: &str) -> String {
    query
        .split('&')
        .map(|pair| match pair.find('=') {
            Some(separator) if is_secret_param_name(&pair[..separator]) => {
                format!("{}=[REDACTED]", &pair[..separator])
            }
            _ => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn redact_sensitive_text(value: &str, home_dir: Option<&str>) -> String {
    let mut redacted = value.to_string();
    if let Some(home) = home_dir {
        redacted = redacted.replace(home, "[HOME]");
    }
    redacted = URL_REGEX
        .replace_all(&redacted, |caps: &regex::Captures| {
            format!("{}{}?{}", &caps[1], &caps[2], redact_query_values(&caps[3]))
        })
        .into_owned();
    redacted = URL_USERINFO_REGEX
        .replace_all(&redacted, "${1}[REDACTED]@")
        .into_owned();
    redacted = AUTH_HEADER_REGEX
        .replace_all(&redacted, "${1} [REDACTED]")
        .into_owned();
    redacted = AUTH_SCHEME_REGEX
        .replace_all(&redacted, |caps: &regex::Captures| {
            if looks_like_secret(&caps[2]) {
                format!("{} [REDACTED]", &caps[1])
            } else {
                caps[0].to_string()
            }
        })
        .into_owned();
    redacted = SK_KEY_REGEX
        .replace_all(&redacted, |caps: &regex::Captures| {
            if is_sk_stop_word(&caps[0]) {
                caps[0].to_string()
            } else {
                "[REDACTED]".to_string()
            }
        })
        .into_owned();
    redacted = NAMED_SECRET_REGEX
        .replace_all(&redacted, |caps: &regex::Captures| {
            let assignment = &caps[2];
            if assignment.contains('=')
                || assignment.contains('"')
                || assignment.contains('\'')
                || looks_like_secret(&caps[3])
            {
                format!("{}{}[REDACTED]", &caps[1], assignment)
            } else {
                caps[0].to_string()
            }
        })
        .into_owned();
    redacted = EMAIL_REGEX
        .replace_all(&redacted, "[EMAIL_REDACTED]")
        .into_owned();
    // Loopback bleibt im IP-Pass lesbar: 127.0.0.1 ist keine Personen-Adresse,
    // und ohne die Ausnahme stuende oben `[IP_REDACTED]:50060`. Nur die
    // eine Adresse -- 0.0.0.0 war bis Fix-Runde 2 in der URL-Ausnahme und
    // fiel im IP-Pass trotzdem; jetzt ist es ueberall dasselbe.
    IP_REGEX
        .replace_all(&redacted, |caps: &regex::Captures| {
            if &caps[0] == "127.0.0.1" {
                caps[0].to_string()
            } else {
                "[IP_REDACTED]".to_string()
            }
        })
        .into_owned()
}

pub struct RedactingWriter<W: Write> {
    inner: W,
    buffer: Vec<u8>,
    home_dir: Option<String>,
}

impl<W: Write> RedactingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            buffer: Vec::with_capacity(8192),
            home_dir: dirs::home_dir().map(|p| p.to_string_lossy().into_owned()),
        }
    }

    #[cfg(test)]
    fn with_home_dir(inner: W, home_dir: Option<String>) -> Self {
        Self {
            inner,
            buffer: Vec::with_capacity(8192),
            home_dir,
        }
    }

    fn redact_line(&self, line: &str) -> String {
        redact_sensitive_text(line, self.home_dir.as_deref())
    }

    fn flush_buffer(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let line = String::from_utf8_lossy(&self.buffer);
        let redacted = self.redact_line(&line);
        self.inner.write_all(redacted.as_bytes())?;

        self.buffer.clear();
        Ok(())
    }
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut last_newline_pos = 0;

        for (i, &byte) in buf.iter().enumerate() {
            if byte == b'\n' {
                self.buffer.extend_from_slice(&buf[last_newline_pos..=i]);
                self.flush_buffer()?;
                last_newline_pos = i + 1;
            }
        }

        if last_newline_pos < buf.len() {
            self.buffer.extend_from_slice(&buf[last_newline_pos..]);
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_buffer()?;
        self.inner.flush()
    }
}

impl<W: Write> Drop for RedactingWriter<W> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn assert_redaction(home: Option<&str>, input: &str, expected: &str) {
        let mut output = Vec::new();
        {
            let mut writer = RedactingWriter::with_home_dir(&mut output, home.map(String::from));
            writeln!(writer, "{}", input).unwrap();
            writer.flush().unwrap();
        }
        let result = String::from_utf8(output).unwrap();
        assert_eq!(result.trim(), expected, "input: {}", input);
    }

    macro_rules! redact_test {
        ($name:ident, home = $home:expr, $input:literal => $expected:literal) => {
            #[test]
            fn $name() {
                assert_redaction($home, $input, $expected);
            }
        };
    }

    redact_test!(redact_home_linux, home = Some("/home/johndoe"), "/home/johndoe/documents/file.txt" => "[HOME]/documents/file.txt");
    redact_test!(redact_home_linux_multiple, home = Some("/home/alice"), "/home/alice/file and /home/alice/other" => "[HOME]/file and [HOME]/other");
    redact_test!(redact_home_macos, home = Some("/Users/janedoe"), "/Users/janedoe/projects/app" => "[HOME]/projects/app");
    redact_test!(redact_home_windows, home = Some(r"C:\Users\johndoe"), r"C:\Users\johndoe\Desktop\file.txt" => r"[HOME]\Desktop\file.txt");
    redact_test!(redact_other_user_paths_preserved, home = Some("/home/alice"), "/home/bob/documents/file.txt" => "/home/bob/documents/file.txt");
    redact_test!(redact_email_single, home = None, "Contact: user@example.com for help" => "Contact: [EMAIL_REDACTED] for help");
    redact_test!(redact_email_multiple, home = None, "From alice@test.example to bob@example.com" => "From [EMAIL_REDACTED] to [EMAIL_REDACTED]");
    redact_test!(redact_email_complex, home = None, "Email: john.doe+tag@sub.example.com" => "Email: [EMAIL_REDACTED]");
    redact_test!(redact_ip_single, home = None, "Connected to 192.168.1.1 successfully" => "Connected to [IP_REDACTED] successfully");
    redact_test!(redact_ip_multiple, home = None, "From 10.0.0.1 to 192.168.0.100" => "From [IP_REDACTED] to [IP_REDACTED]");
    // D1: Loopback bleibt lesbar (bis 02.09.2026 stand hier [IP_REDACTED]:8080).
    redact_test!(redact_ip_localhost_stays_readable, home = None, "Listening on 127.0.0.1:8080" => "Listening on 127.0.0.1:8080");
    redact_test!(redact_mixed_content, home = Some("/home/alice"), "User alice@test.example at /home/alice connected from 192.168.1.50" => "User [EMAIL_REDACTED] at [HOME] connected from [IP_REDACTED]");
    redact_test!(redact_no_sensitive_data, home = None, "Application started successfully" => "Application started successfully");
    // C1 (Opus/Forge/Grok/Kimi-Review 02.09.2026): dieselben Muster wie
    // redactSensitiveText() in apps/desktop/src/error-reporting.ts.
    redact_test!(redact_openai_style_key, home = None, "Incorrect API key provided: sk-live-abcdefghijklmnopqrstuvwxyz" => "Incorrect API key provided: [REDACTED]");
    redact_test!(redact_bearer_token, home = None, "401 for Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig" => "401 for Authorization: Bearer [REDACTED]");
    redact_test!(redact_url_query_string, home = None, "GET https://x/y?key=abc&model=nova failed" => "GET https://x/y?key=[REDACTED]&model=nova failed");
    redact_test!(redact_api_key_param, home = None, "request api_key=secret123 rejected" => "request api_key=[REDACTED] rejected");
    redact_test!(redact_api_key_param_dash_and_case, home = None, "Api-Key=Secret123 rejected" => "Api-Key=[REDACTED] rejected");
    redact_test!(redact_token_param, home = None, "refresh_token=tok_987 expired" => "refresh_token=[REDACTED] expired");
    redact_test!(redact_ordinary_dashes_preserved, home = None, "risk-based desk-top task-1" => "risk-based desk-top task-1");
    redact_test!(redact_url_without_query_preserved, home = None, "GET https://x/y#frag ok" => "GET https://x/y#frag ok");
    // D1 (Fix-Runde 1d): dieselben Faelle wie error-reporting.test.ts.
    redact_test!(redact_wss_key, home = None, "connect wss://generativelanguage.googleapis.com/ws?key=AIzaSyD-1234567890abcdefghijklmnop failed" => "connect wss://generativelanguage.googleapis.com/ws?key=[REDACTED] failed");
    redact_test!(redact_token_scheme, home = None, "Authorization: Token abc" => "Authorization: Token [REDACTED]");
    redact_test!(redact_basic_scheme, home = None, "Authorization: Basic dXNlcjpwYXNz" => "Authorization: Basic [REDACTED]");
    redact_test!(redact_json_fragment_keeps_status, home = None, r#"{"url":"https://x/y?key=1","status":403}"# => r#"{"url":"https://x/y?key=[REDACTED]","status":403}"#);
    redact_test!(redact_quoted_json_secret, home = None, r#"body {"api_key":"secret-value-1"} rejected"# => r#"body {"api_key":"[REDACTED]"} rejected"#);
    redact_test!(redact_colon_assignment, home = None, "config key: hunter2 loaded" => "config key: [REDACTED] loaded");
    redact_test!(redact_sk_build_stays, home = None, "sk-build failed" => "sk-build failed");
    redact_test!(redact_long_sk_goes, home = None, "sk-live-abcdefghijklmnopqrstuvwxyz" => "[REDACTED]");
    redact_test!(redact_long_sk_uppercase_goes, home = None, "SK-LIVE-ABCDEFGHIJKLMNOPQRSTUVWXYZ" => "[REDACTED]");
    redact_test!(redact_loopback_url_stays, home = None, "http://127.0.0.1:50060/v1?model=x" => "http://127.0.0.1:50060/v1?model=x");
    redact_test!(redact_localhost_url_stays, home = None, "http://localhost:4040/v1?model=x" => "http://localhost:4040/v1?model=x");
    redact_test!(redact_ipv6_loopback_url_stays, home = None, "ws://[::1]:9000/listen?model=x" => "ws://[::1]:9000/listen?model=x");
    redact_test!(redact_password_param, home = None, "password=hunter2 rejected" => "password=[REDACTED] rejected");
    redact_test!(redact_sig_keeps_neighbour, home = None, "sig=abc123&se=2026" => "sig=[REDACTED]&se=2026");
    redact_test!(redact_no_token_wildcard, home = None, "dbtoken=abc stays" => "dbtoken=abc stays");
    redact_test!(redact_token_word_in_prose_stays, home = None, "the token expired" => "the token expired");
    redact_test!(redact_paren_param, home = None, "(key=abc) done" => "(key=[REDACTED]) done");
    redact_test!(redact_unicode_letters_not_a_secret, home = None, "schlüsseltoken=abc" => "schlüsseltoken=abc");
    // G3 (Grok 6/7, Kimi 2/4-7, Forge 6; Fix-Runde 2): dieselben Faelle wie
    // error-reporting.test.ts. Ein Query-Parameter faellt nach seinem NAMEN,
    // auf jedem Host; Diagnose-Parameter bleiben auf jedem Host lesbar.
    redact_test!(redact_loopback_secret_param, home = None, "ws://127.0.0.1:8080/stt?key=AIzaSyD-1234567890abcdefghijklmnop" => "ws://127.0.0.1:8080/stt?key=[REDACTED]");
    redact_test!(redact_localhost_secret_param_keeps_model, home = None, "http://localhost:4040/v1?api_key=abc&model=x" => "http://localhost:4040/v1?api_key=[REDACTED]&model=x");
    redact_test!(redact_ipv6_loopback_token, home = None, "ws://[::1]:9000/listen?token=abc" => "ws://[::1]:9000/listen?token=[REDACTED]");
    redact_test!(redact_deepgram_query_keeps_diagnostics, home = None, "wss://api.deepgram.com/v1/listen?model=nova-3&language=de&access_token=abc" => "wss://api.deepgram.com/v1/listen?model=nova-3&language=de&access_token=[REDACTED]");
    redact_test!(redact_ipv6_host_in_brackets, home = None, "wss://[2001:db8::1]/ws?key=abc&model=nova" => "wss://[2001:db8::1]/ws?key=[REDACTED]&model=nova");
    redact_test!(redact_ipv6_host_diagnostics_stay, home = None, "http://[2001:db8::1]:8080/v1?model=x" => "http://[2001:db8::1]:8080/v1?model=x");
    // 0.0.0.0 ist keine Ausnahme mehr -- weder in der URL noch im IP-Pass.
    redact_test!(redact_any_host_url, home = None, "http://0.0.0.0:50060/v1?key=abc&model=x" => "http://[IP_REDACTED]:50060/v1?key=[REDACTED]&model=x");
    redact_test!(redact_userinfo_without_query, home = None, "https://user:pass@x/y" => "https://[REDACTED]@x/y");
    redact_test!(redact_userinfo_with_query, home = None, "wss://user:pass@x/y?key=abc&model=nova" => "wss://[REDACTED]@x/y?key=[REDACTED]&model=nova");
    redact_test!(redact_bearer_lowercase, home = None, "authorization: bearer abc.def" => "authorization: bearer [REDACTED]");
    redact_test!(redact_bearer_uppercase, home = None, "BEARER eyJhbGciOiJIUzI1NiJ9.x.y" => "BEARER [REDACTED]");
    redact_test!(redact_short_sk_live, home = None, "sk-live-abc123" => "[REDACTED]");
    redact_test!(redact_sk_project_stays, home = None, "sk-project" => "sk-project");
    redact_test!(redact_sk_worker_stays, home = None, "sk-worker failed" => "sk-worker failed");
    redact_test!(redact_sk_too_short_stays, home = None, "sk-abc" => "sk-abc");
    // Prosa: ein Schema oder ein nacktes `name:` nur mit einem Wert, der
    // nach einem aussieht (Ziffer, Satzzeichen, gemischte Schreibung,
    // Laenge); eine Header-Zeile immer.
    redact_test!(redact_prose_bearer_token_stays, home = None, "a Bearer token expires" => "a Bearer token expires");
    redact_test!(redact_prose_key_colon_stays, home = None, "the key: press Enter" => "the key: press Enter");
    redact_test!(redact_prose_secret_colon_stays, home = None, "a secret: keep it simple" => "a secret: keep it simple");
    redact_test!(redact_bearer_with_digits, home = None, "Bearer abc123" => "Bearer [REDACTED]");
    redact_test!(redact_quoted_password_word, home = None, r#"{"password":"hunter"}"# => r#"{"password":"[REDACTED]"}"#);
    redact_test!(redact_bare_password_word_stays, home = None, "password: hunter" => "password: hunter");
    // Zweiter Durchlauf aendert nichts mehr.
    redact_test!(redact_idempotent_url, home = None, "wss://x/ws?key=[REDACTED]&model=nova" => "wss://x/ws?key=[REDACTED]&model=nova");
    redact_test!(redact_idempotent_userinfo, home = None, "wss://[REDACTED]@x/ws?key=[REDACTED]" => "wss://[REDACTED]@x/ws?key=[REDACTED]");
    redact_test!(redact_idempotent_bearer, home = None, "Bearer [REDACTED]" => "Bearer [REDACTED]");
    redact_test!(redact_idempotent_assignment, home = None, "key: [REDACTED] loaded" => "key: [REDACTED] loaded");

    #[test]
    fn writer_buffers_partial_lines() {
        let mut output = Vec::new();
        {
            let mut writer = RedactingWriter::with_home_dir(&mut output, Some("/home/user".into()));
            writer.write_all(b"test /home/").unwrap();
            writer.write_all(b"user/file\n").unwrap();
            writer.flush().unwrap();
        }
        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "test [HOME]/file\n");
    }

    #[test]
    fn writer_handles_empty_input() {
        let mut output = Vec::new();
        {
            let mut writer = RedactingWriter::new(&mut output);
            writer.write_all(b"").unwrap();
            writer.flush().unwrap();
        }
        assert_eq!(String::from_utf8(output).unwrap(), "");
    }

    #[test]
    fn writer_handles_only_newlines() {
        let mut output = Vec::new();
        {
            let mut writer = RedactingWriter::new(&mut output);
            writer.write_all(b"\n\n\n").unwrap();
            writer.flush().unwrap();
        }
        assert_eq!(String::from_utf8(output).unwrap(), "\n\n\n");
    }

    #[test]
    fn writer_handles_interleaved_writes() {
        let mut output = Vec::new();
        {
            let mut writer = RedactingWriter::new(&mut output);
            writer.write_all(b"line1 ").unwrap();
            writer.write_all(b"user@test.example").unwrap();
            writer.write_all(b" end\n").unwrap();
            writer.write_all(b"line2\n").unwrap();
            writer.flush().unwrap();
        }
        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "line1 [EMAIL_REDACTED] end\nline2\n");
    }

    #[test]
    fn multiline_redaction() {
        let mut output = Vec::new();
        {
            let mut writer =
                RedactingWriter::with_home_dir(&mut output, Some("/home/testuser".into()));
            writeln!(writer, "User logged in from /home/testuser/app").unwrap();
            writeln!(writer, "Email: user@example.com").unwrap();
            writeln!(writer, "Connection from 192.168.1.100").unwrap();
            writer.flush().unwrap();
        }
        let content = String::from_utf8(output).unwrap();
        assert!(content.contains("[HOME]/app"));
        assert!(content.contains("[EMAIL_REDACTED]"));
        assert!(content.contains("[IP_REDACTED]"));
        assert!(!content.contains("/home/testuser"));
        assert!(!content.contains("user@example.com"));
        assert!(!content.contains("192.168.1.100"));
    }
}
