//! The settings-bundle envelope: how a Mitschnitt setup travels to another machine.
//!
//! Mitschnitt-Fork (F14). Upstream anarlog has no settings export at all, so
//! nothing here overlaps with upstream code and an upstream release can be
//! merged without looking at this crate.
//!
//! Two shapes, one file format:
//!
//! * **without credentials** -- readable JSON, meant to be handed around freely.
//! * **with credentials** -- the payload is encrypted with a key derived from a
//!   password (Argon2id) and sealed with XChaCha20-Poly1305. The header stays
//!   readable so the importing side can say "this file needs a password"
//!   *before* asking for one, but the header is bound into the ciphertext as
//!   associated data, so editing it breaks authentication instead of silently
//!   changing what gets imported.
//!
//! The whole payload is one opaque JSON value here. Deciding *what* belongs in
//! it is the frontend's job (an explicit allowlist in
//! `apps/desktop/src/settings-transfer/bundle.ts`); this crate only decides how
//! it is wrapped, so the two concerns can be reviewed apart from each other.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const FORMAT: &str = "mitschnitt.settings";
pub const VERSION: u32 = 1;

pub const KDF_ARGON2ID: &str = "argon2id";
pub const CIPHER_XCHACHA20POLY1305: &str = "xchacha20poly1305";

/// OWASP's second recommended Argon2id profile (19 MiB, 2 passes, 1 lane).
/// Stored in the file as well, so a future hardening can still open old files.
const ARGON2_M_COST: u32 = 19_456;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;

const KEY_BYTES: usize = 32;
const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 24;

/// Argon2 refuses salts under 8 bytes; anything we would accept below the
/// length we write ourselves is a downgrade attempt, not an old file.
const MIN_SALT_BYTES: usize = 16;

/// The shortest password a NEW bundle may be written with (Entscheid 01.09.2026).
///
/// Counted in characters rather than bytes, so a password of six umlauts is six
/// and not twelve. Deliberately nothing else -- no character classes, no
/// entropy score: a rule people work around is worse than a short one they
/// keep.
///
/// **Writing only.** Opening is never held to it: bundles written before this
/// existed have to keep opening, and a length rule applied on the way in would
/// lock people out of their own files without making anything safer.
pub const MIN_PASSWORD_CHARS: usize = 6;

/// Ceilings on the work factors a FILE may ask for.
///
/// The parameters travel with the bundle so a future hardening can still open
/// old files -- but that same freedom lets a hand-edited file demand arbitrary
/// work BEFORE the password is even checked. Measured on this machine:
/// `m_cost = 4_000_000` (about 4 GiB) burns roughly 21 seconds and the memory
/// to match, per attempt, on a file the user merely opened.
///
/// 256 MiB is thirteen times the profile we write ourselves, so a real future
/// hardening has room; ten passes and four lanes likewise. Beyond that a
/// bundle is not hardened, it is hostile.
const MAX_M_COST: u32 = 262_144;
const MAX_T_COST: u32 = 10;
const MAX_P_COST: u32 = 4;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("this file is not a Mitschnitt settings bundle")]
    NotABundle,
    #[error("this settings bundle was written by a newer version of Mitschnitt")]
    UnsupportedVersion,
    #[error("the settings bundle is damaged")]
    Malformed,
    #[error("this settings bundle is protected by a password")]
    PasswordRequired,
    #[error("the password does not fit this settings bundle")]
    WrongPassword,
    #[error("the password must not be empty")]
    EmptyPassword,
    #[error("the password must be at least {MIN_PASSWORD_CHARS} characters long")]
    PasswordTooShort,
    #[error("cryptographic randomness is unavailable")]
    RandomnessUnavailable,
    #[error("a bundle without credentials must not contain a credential")]
    SecretInPlainBundle,
    #[error("this settings bundle asks for more work than any honest bundle needs")]
    UnreasonableKdfCost,
    #[error("this settings bundle contradicts itself about its credentials")]
    InconsistentBundle,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// What the importing side may know before it has the password.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleInfo {
    pub version: u32,
    pub created_at: String,
    pub app_version: String,
    pub includes_secrets: bool,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Encryption {
    kdf: String,
    salt: String,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
    cipher: String,
    nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    format: String,
    version: u32,
    created_at: String,
    app_version: String,
    includes_secrets: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    encryption: Option<Encryption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ciphertext: Option<String>,
}

/// The bytes the ciphertext is authenticated against. Field order is the
/// declaration order of this struct, so both sides build the same bytes without
/// depending on how a JSON map happens to be ordered.
#[derive(Serialize)]
struct AssociatedData<'a> {
    format: &'a str,
    version: u32,
    created_at: &'a str,
    app_version: &'a str,
    includes_secrets: bool,
    kdf: &'a str,
    salt: &'a str,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
    cipher: &'a str,
    nonce: &'a str,
}

fn associated_data(envelope: &Envelope, encryption: &Encryption) -> Vec<u8> {
    let aad = AssociatedData {
        format: &envelope.format,
        version: envelope.version,
        created_at: &envelope.created_at,
        app_version: &envelope.app_version,
        includes_secrets: envelope.includes_secrets,
        kdf: &encryption.kdf,
        salt: &encryption.salt,
        m_cost: encryption.m_cost,
        t_cost: encryption.t_cost,
        p_cost: encryption.p_cost,
        cipher: &encryption.cipher,
        nonce: &encryption.nonce,
    };
    // Serializing a plain struct of strings and numbers cannot fail.
    serde_json::to_vec(&aad).expect("associated data is serializable")
}

/// Field names that carry a credential in this payload, and in any payload a
/// future version is likely to grow.
///
/// Compared against a NORMALISED name (lowercased, `-` folded to `_`), so
/// `API-Key`, `apiKey` and `api_key` are one entry here rather than three.
const CREDENTIAL_FIELDS: [&str; 16] = [
    "api_key",
    "apikey",
    "key",
    "token",
    "secret",
    "client_secret",
    "access_token",
    "refresh_token",
    "bearer",
    "credential",
    "auth",
    "password",
    "authorization",
    "access_key",
    "private_key",
    "session_cookie",
];

/// Suffixes that make a name a credential whatever its prefix -- `openai_token`,
/// `stt_api_key`, `mollie_client_secret`.
///
/// Deliberately NOT `_key` on its own: template content and future settings may
/// legitimately end in it, and a gate that blocks an honest export is worse
/// than one that misses an exotic name -- the frontend allowlist is the first
/// line, this walk is the net under it.
const CREDENTIAL_SUFFIXES: [&str; 6] = [
    "_api_key",
    "_apikey",
    "_token",
    "_secret",
    "_password",
    "_credential",
];

/// `API-Key`, `apiKey`, `clientSecret` and `client_secret` all become the one
/// snake_case name the lists below are written in: a case boundary counts as a
/// separator, and so does `-`.
///
/// Lowercasing alone is not enough, and that is not a guess -- the test for
/// this walk failed on `clientSecret` while the first version of this function
/// only lowercased.
fn normalise_field_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut previous_was_lower_or_digit = false;
    for character in name.chars() {
        if character == '-' || character == ' ' {
            out.push('_');
            previous_was_lower_or_digit = false;
            continue;
        }
        if character.is_ascii_uppercase() && previous_was_lower_or_digit {
            out.push('_');
        }
        previous_was_lower_or_digit = character.is_ascii_lowercase() || character.is_ascii_digit();
        out.push(character.to_ascii_lowercase());
    }
    out
}

/// Whether a field NAME is one that would hold a credential.
///
/// Deliberately matched on whole names and explicit suffixes rather than as a
/// substring: `key` as a substring turns `monkey`, `keyboard` and `hotkey` into
/// findings, and a gate that fires on innocent settings is a gate people learn
/// to route around.
fn is_credential_field(name: &str) -> bool {
    let name = normalise_field_name(name);
    CREDENTIAL_FIELDS.contains(&name.as_str())
        || CREDENTIAL_SUFFIXES
            .iter()
            .any(|suffix| name.len() > suffix.len() && name.ends_with(suffix))
}

/// Whether a VALUE under a credential name actually carries something.
///
/// A number or an array under `api_key` is not a plausible setting -- it is a
/// credential the writer failed to stringify, and the earlier version of this
/// walk waved it through because it only ever looked at strings.
fn carries_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::String(text) => !text.is_empty(),
        serde_json::Value::Array(items) => !items.is_empty(),
        serde_json::Value::Object(fields) => !fields.is_empty(),
        _ => true,
    }
}

/// The last gate before an unencrypted file is written.
///
/// Deciding what goes into a bundle happens in the frontend allowlist, and that
/// is where the credentials are supposed to be dropped. This walk exists because
/// the layer that WRITES THE FILE should not depend on a caller three layers up
/// having got it right: a plain bundle is made to be handed around, so a
/// credential in one is the most expensive mistake this feature can make.
///
/// Found during the file probe: `seal` used to write exactly such a file
/// without a word.
fn find_credential(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(fields) => fields.iter().any(|(name, nested)| {
            let carries = is_credential_field(name) && carries_value(nested);
            carries || find_credential(nested)
        }),
        serde_json::Value::Array(items) => items.iter().any(find_credential),
        _ => false,
    }
}

fn random_bytes(len: usize) -> Result<Vec<u8>> {
    let mut buffer = vec![0u8; len];
    getrandom::fill(&mut buffer).map_err(|_| Error::RandomnessUnavailable)?;
    Ok(buffer)
}

fn derive_key(
    password: &str,
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<Zeroizing<[u8; KEY_BYTES]>> {
    if password.is_empty() {
        return Err(Error::EmptyPassword);
    }
    if salt.len() < MIN_SALT_BYTES {
        return Err(Error::Malformed);
    }
    // Before any work is done, not after: the whole point is that a hostile
    // file must not be able to spend this machine's time and memory on the
    // strength of nothing but having been opened.
    if m_cost > MAX_M_COST || t_cost > MAX_T_COST || p_cost > MAX_P_COST {
        return Err(Error::UnreasonableKdfCost);
    }

    let params =
        Params::new(m_cost, t_cost, p_cost, Some(KEY_BYTES)).map_err(|_| Error::Malformed)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0u8; KEY_BYTES]);
    argon2
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| Error::Malformed)?;
    Ok(key)
}

/// Wraps a payload. `password` is required exactly when the payload carries
/// credentials -- the caller decides that, and `seal` refuses the combination
/// that would leak them.
pub fn seal(
    payload: &serde_json::Value,
    includes_secrets: bool,
    password: Option<&str>,
    created_at: &str,
    app_version: &str,
) -> Result<String> {
    seal_inner(
        payload,
        includes_secrets,
        password,
        created_at,
        app_version,
        true,
    )
}

/// The body of [`seal`], with the minimum password length as a switch.
///
/// Off only in the tests, and only to write the shape of file that existed
/// before the rule did -- those have to keep opening, so there has to be a way
/// to produce one.
fn seal_inner(
    payload: &serde_json::Value,
    includes_secrets: bool,
    password: Option<&str>,
    created_at: &str,
    app_version: &str,
    enforce_minimum_length: bool,
) -> Result<String> {
    let password = password.filter(|value| !value.is_empty());

    if includes_secrets && password.is_none() {
        return Err(Error::EmptyPassword);
    }

    // On the way out only. `open` deliberately does not check length: a bundle
    // written before this rule existed is still that person's settings, and
    // refusing to read it would take something away without protecting anyone.
    if enforce_minimum_length
        && let Some(password) = password
        && password.chars().count() < MIN_PASSWORD_CHARS
    {
        return Err(Error::PasswordTooShort);
    }

    // Tied to what the header claims, not to whether a password was typed.
    // Bound to `password.is_none()` this walk skipped the one combination that
    // produces a lying file: `includes_secrets: false` with a password
    // encrypted the payload -- so nothing leaked to a reader -- while the
    // readable header still announced "no credentials in here", and `inspect`
    // repeats that claim to the importing side before anyone is asked for a
    // password. A bundle that misdescribes itself is exactly what this gate is
    // for.
    if !includes_secrets && find_credential(payload) {
        return Err(Error::SecretInPlainBundle);
    }

    let mut envelope = Envelope {
        format: FORMAT.to_string(),
        version: VERSION,
        created_at: created_at.to_string(),
        app_version: app_version.to_string(),
        includes_secrets,
        encryption: None,
        payload: None,
        ciphertext: None,
    };

    let Some(password) = password else {
        envelope.payload = Some(payload.clone());
        return Ok(serde_json::to_string_pretty(&envelope)? + "\n");
    };

    let salt = random_bytes(SALT_BYTES)?;
    let nonce: [u8; NONCE_BYTES] = random_bytes(NONCE_BYTES)?
        .try_into()
        .map_err(|_| Error::RandomnessUnavailable)?;
    let encryption = Encryption {
        kdf: KDF_ARGON2ID.to_string(),
        salt: BASE64.encode(&salt),
        m_cost: ARGON2_M_COST,
        t_cost: ARGON2_T_COST,
        p_cost: ARGON2_P_COST,
        cipher: CIPHER_XCHACHA20POLY1305.to_string(),
        nonce: BASE64.encode(&nonce),
    };

    let key = derive_key(password, &salt, ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST)?;
    let cipher = XChaCha20Poly1305::new(key.as_ref().into());
    let aad = associated_data(&envelope, &encryption);
    let plaintext = Zeroizing::new(serde_json::to_vec(payload)?);

    let ciphertext = cipher
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: plaintext.as_ref(),
                aad: &aad,
            },
        )
        .map_err(|_| Error::Malformed)?;

    envelope.ciphertext = Some(BASE64.encode(&ciphertext));
    envelope.encryption = Some(encryption);
    Ok(serde_json::to_string_pretty(&envelope)? + "\n")
}

fn parse(contents: &str) -> Result<Envelope> {
    let envelope: Envelope = serde_json::from_str(contents).map_err(|_| Error::NotABundle)?;
    if envelope.format != FORMAT {
        return Err(Error::NotABundle);
    }
    if envelope.version > VERSION {
        return Err(Error::UnsupportedVersion);
    }
    Ok(envelope)
}

/// The invariants an UNENCRYPTED envelope has to satisfy before it counts as a
/// bundle at all.
///
/// `seal` refuses to write such a file, but `open` used to accept one that
/// arrived anyway: a hand-written envelope with `includes_secrets: true` and no
/// encryption, or a readable payload with a key in it, was reported as a valid
/// plain bundle and its contents handed straight to the importer. Reading is
/// where a foreign file actually meets this machine, so the same gate belongs
/// on both sides of the door.
fn check_plain_envelope(envelope: &Envelope) -> Result<()> {
    if envelope.encryption.is_some() {
        return Ok(());
    }
    if envelope.includes_secrets {
        return Err(Error::InconsistentBundle);
    }
    if let Some(payload) = envelope.payload.as_ref() {
        if find_credential(payload) {
            return Err(Error::SecretInPlainBundle);
        }
    }
    Ok(())
}

/// What can be told about a bundle without the password.
pub fn inspect(contents: &str) -> Result<BundleInfo> {
    let envelope = parse(contents)?;
    check_plain_envelope(&envelope)?;
    Ok(BundleInfo {
        version: envelope.version,
        created_at: envelope.created_at.clone(),
        app_version: envelope.app_version.clone(),
        includes_secrets: envelope.includes_secrets,
        encrypted: envelope.encryption.is_some(),
    })
}

/// Unwraps a payload. Either the whole payload comes back or an error does --
/// there is no half-open bundle, which is what keeps a wrong password from
/// producing a partial import.
pub fn open(contents: &str, password: Option<&str>) -> Result<(BundleInfo, serde_json::Value)> {
    let envelope = parse(contents)?;
    check_plain_envelope(&envelope)?;
    let info = BundleInfo {
        version: envelope.version,
        created_at: envelope.created_at.clone(),
        app_version: envelope.app_version.clone(),
        includes_secrets: envelope.includes_secrets,
        encrypted: envelope.encryption.is_some(),
    };

    let Some(encryption) = envelope.encryption.as_ref() else {
        let payload = envelope.payload.clone().ok_or(Error::Malformed)?;
        return Ok((info, payload));
    };

    let password = password.filter(|value| !value.is_empty());
    let Some(password) = password else {
        return Err(Error::PasswordRequired);
    };

    if encryption.kdf != KDF_ARGON2ID || encryption.cipher != CIPHER_XCHACHA20POLY1305 {
        return Err(Error::Malformed);
    }

    let salt = BASE64
        .decode(&encryption.salt)
        .map_err(|_| Error::Malformed)?;
    let nonce: [u8; NONCE_BYTES] = BASE64
        .decode(&encryption.nonce)
        .map_err(|_| Error::Malformed)?
        .try_into()
        .map_err(|_| Error::Malformed)?;
    let ciphertext = BASE64
        .decode(envelope.ciphertext.as_deref().ok_or(Error::Malformed)?)
        .map_err(|_| Error::Malformed)?;

    let key = derive_key(
        password,
        &salt,
        encryption.m_cost,
        encryption.t_cost,
        encryption.p_cost,
    )?;
    let cipher = XChaCha20Poly1305::new(key.as_ref().into());
    let aad = associated_data(&envelope, encryption);

    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &ciphertext,
                    aad: &aad,
                },
            )
            // A tampered header and a wrong password fail the same way here.
            // Reporting the password is the honest guess: the header is ours to
            // write, the password is the user's to get wrong.
            .map_err(|_| Error::WrongPassword)?,
    );

    let payload: serde_json::Value =
        serde_json::from_slice(plaintext.as_ref()).map_err(|_| Error::Malformed)?;
    Ok((info, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> serde_json::Value {
        json!({
            "settings": { "current_stt_provider": "whispercpp" },
            "providers": { "stt:openai": { "base_url": "", "api_key": "sk-geheim-4711" } },
        })
    }

    fn payload_without_credentials() -> serde_json::Value {
        let mut value = payload();
        value["providers"]["stt:openai"]["api_key"] = json!("");
        value
    }

    #[test]
    fn a_plain_bundle_refuses_to_carry_a_credential() {
        // The frontend allowlist drops keys for a shareable bundle. This is the
        // gate at the file itself, so a caller that gets it wrong writes no
        // file at all instead of a readable one with a key in it.
        assert!(matches!(
            seal(&payload(), false, None, "t", "v"),
            Err(Error::SecretInPlainBundle)
        ));

        // Nested and array-shaped payloads are walked too.
        let nested = json!({ "a": [{ "b": { "token": "t-4711" } }] });
        assert!(matches!(
            seal(&nested, false, None, "t", "v"),
            Err(Error::SecretInPlainBundle)
        ));

        // An empty credential field is not a credential.
        let empty = json!({ "providers": { "stt:openai": { "api_key": "" } } });
        assert!(seal(&empty, false, None, "t", "v").is_ok());
    }

    // The gate belongs to what the header CLAIMS, not to whether a password was
    // typed. `includes_secrets: false` plus a password used to skip the walk
    // entirely: the file was encrypted, so nothing leaked to a reader, but its
    // readable header announced "no credentials in here" over a payload that had
    // one -- and `inspect` repeats that claim to the importing side before
    // anyone types a password. A bundle that lies about itself is the thing this
    // gate exists to prevent.
    #[test]
    fn a_bundle_that_says_it_has_no_credentials_must_not_have_one() {
        assert!(matches!(
            seal(&payload(), false, Some("kennwort"), "t", "v"),
            Err(Error::SecretInPlainBundle)
        ));

        // The honest combinations are untouched: declared and encrypted goes
        // through, and so does a password over a payload that carries none.
        assert!(seal(&payload(), true, Some("kennwort"), "t", "v").is_ok());
        assert!(
            seal(
                &payload_without_credentials(),
                false,
                Some("kennwort"),
                "t",
                "v"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_plain_bundle_round_trips_and_stays_readable() {
        let sealed = seal(
            &payload_without_credentials(),
            false,
            None,
            "2026-08-31T10:00:00Z",
            "1.4.14",
        )
        .unwrap();
        assert!(sealed.contains("whispercpp"));

        let (info, opened) = open(&sealed, None).unwrap();
        assert!(!info.encrypted);
        assert!(!info.includes_secrets);
        assert_eq!(opened, payload_without_credentials());
    }

    #[test]
    fn a_sealed_bundle_round_trips_with_the_right_password() {
        let sealed = seal(
            &payload(),
            true,
            Some("richtig"),
            "2026-08-31T10:00:00Z",
            "1.4.14",
        )
        .unwrap();

        let (info, opened) = open(&sealed, Some("richtig")).unwrap();
        assert!(info.encrypted);
        assert!(info.includes_secrets);
        assert_eq!(opened, payload());
    }

    #[test]
    fn a_sealed_bundle_hides_every_secret_from_the_file() {
        let sealed = seal(
            &payload(),
            true,
            Some("richtig"),
            "2026-08-31T10:00:00Z",
            "1.4.14",
        )
        .unwrap();

        // The header is meant to be readable, the payload is not.
        assert!(sealed.contains("mitschnitt.settings"));
        for secret in ["sk-geheim-4711", "whispercpp", "openai", "base_url"] {
            assert!(
                !sealed.contains(secret),
                "the sealed bundle leaks {secret}: {sealed}"
            );
        }
    }

    #[test]
    fn the_wrong_password_opens_nothing() {
        let sealed = seal(
            &payload(),
            true,
            Some("richtig"),
            "2026-08-31T10:00:00Z",
            "1.4.14",
        )
        .unwrap();

        assert!(matches!(
            open(&sealed, Some("falsch")),
            Err(Error::WrongPassword)
        ));
        assert!(matches!(open(&sealed, None), Err(Error::PasswordRequired)));
        assert!(matches!(
            open(&sealed, Some("")),
            Err(Error::PasswordRequired)
        ));
    }

    #[test]
    fn an_edited_header_breaks_authentication() {
        let sealed = seal(
            &payload(),
            true,
            Some("richtig"),
            "2026-08-31T10:00:00Z",
            "1.4.14",
        )
        .unwrap();

        // Flipping the credentials flag would otherwise let someone hand out a
        // bundle that claims to be harmless while still carrying keys.
        let tampered = sealed.replace("\"includes_secrets\": true", "\"includes_secrets\": false");
        assert_ne!(tampered, sealed);
        assert!(matches!(
            open(&tampered, Some("richtig")),
            Err(Error::WrongPassword)
        ));
    }

    #[test]
    fn an_edited_ciphertext_breaks_authentication() {
        let sealed = seal(
            &payload(),
            true,
            Some("richtig"),
            "2026-08-31T10:00:00Z",
            "1.4.14",
        )
        .unwrap();
        let envelope: Envelope = serde_json::from_str(&sealed).unwrap();
        let mut bytes = BASE64.decode(envelope.ciphertext.unwrap()).unwrap();
        bytes[0] ^= 0x01;

        let mut edited: serde_json::Value = serde_json::from_str(&sealed).unwrap();
        edited["ciphertext"] = json!(BASE64.encode(&bytes));

        assert!(matches!(
            open(&edited.to_string(), Some("richtig")),
            Err(Error::WrongPassword)
        ));
    }

    #[test]
    fn credentials_never_leave_without_a_password() {
        assert!(matches!(
            seal(&payload(), true, None, "2026-08-31T10:00:00Z", "1.4.14"),
            Err(Error::EmptyPassword)
        ));
        assert!(matches!(
            seal(&payload(), true, Some(""), "2026-08-31T10:00:00Z", "1.4.14"),
            Err(Error::EmptyPassword)
        ));
    }

    #[test]
    fn two_seals_of_the_same_payload_differ() {
        let first = seal(&payload(), true, Some("gleich"), "t", "v").unwrap();
        let second = seal(&payload(), true, Some("gleich"), "t", "v").unwrap();
        assert_ne!(first, second, "salt and nonce must be fresh every time");
    }

    #[test]
    fn foreign_files_are_rejected_rather_than_guessed_at() {
        assert!(matches!(
            open("not json at all", None),
            Err(Error::NotABundle)
        ));
        assert!(matches!(
            open(r#"{"format":"something.else","version":1}"#, None),
            Err(Error::NotABundle)
        ));

        let future = json!({
            "format": FORMAT,
            "version": VERSION + 1,
            "created_at": "t",
            "app_version": "v",
            "includes_secrets": false,
            "payload": {},
        });
        assert!(matches!(
            open(&future.to_string(), None),
            Err(Error::UnsupportedVersion)
        ));
    }

    #[test]
    fn a_shortened_salt_is_refused_instead_of_weakening_the_key() {
        let sealed = seal(&payload(), true, Some("richtig"), "t", "v").unwrap();
        let mut edited: serde_json::Value = serde_json::from_str(&sealed).unwrap();
        edited["encryption"]["salt"] = json!(BASE64.encode(b"kurz"));

        assert!(matches!(
            open(&edited.to_string(), Some("richtig")),
            Err(Error::Malformed)
        ));
    }

    /// E3. Before the ceiling, a file could ask for gigabytes and seconds of
    /// work per attempt -- measured at `m_cost = 4_000_000`: about 21 seconds
    /// and 4 GiB, spent before the password was even checked. The time bound is
    /// the point of the test, not decoration: an error that arrives after the
    /// work was done is not a fix.
    #[test]
    fn an_absurd_work_factor_is_refused_before_any_work_happens() {
        let sealed = seal(&payload(), true, Some("richtig"), "t", "v").unwrap();

        for (field, value) in [
            ("m_cost", json!(4_000_000u32)),
            ("t_cost", json!(64u32)),
            ("p_cost", json!(255u32)),
        ] {
            let mut edited: serde_json::Value = serde_json::from_str(&sealed).unwrap();
            edited["encryption"][field] = value;
            let contents = edited.to_string();

            let started = std::time::Instant::now();
            let outcome = open(&contents, Some("richtig"));
            let elapsed = started.elapsed();

            assert!(
                matches!(outcome, Err(Error::UnreasonableKdfCost)),
                "{field} was accepted"
            );
            assert!(
                elapsed < std::time::Duration::from_millis(100),
                "{field} still cost {elapsed:?} before the refusal"
            );
        }
    }

    /// The ceiling must leave a real hardening room: the profile the sibling
    /// test seals with (8192/3/2) and the maximum we allow both have to open.
    #[test]
    fn a_stronger_but_sane_profile_still_opens() {
        assert!(
            derive_key(
                "richtig",
                &[7u8; SALT_BYTES],
                MAX_M_COST,
                MAX_T_COST,
                MAX_P_COST
            )
            .is_ok()
        );
        assert!(derive_key("richtig", &[7u8; SALT_BYTES], 8192, 3, 2).is_ok());
    }

    /// E4a. Every name here slipped through the old five-entry exact-match
    /// list. They are the reason the list is now normalised plus suffixed.
    #[test]
    fn credential_names_the_old_list_walked_past() {
        let slipped = [
            "API_KEY",
            "Api-Key",
            "openai_api_key",
            "client_secret",
            "clientSecret",
            "secret",
            "refresh_token",
            "auth",
            "credential",
            "bearer",
            "stt_token",
            "key",
        ];
        for name in slipped {
            let value = json!({ "providers": { "x": { name: "sk-geheim-4711" } } });
            assert!(
                matches!(
                    seal(&value, false, None, "t", "v"),
                    Err(Error::SecretInPlainBundle)
                ),
                "a credential under `{name}` was written into a plain bundle"
            );
        }
    }

    /// E4c (found during the audit that widened E4a). Five more plausible
    /// names -- four exact and one suffix (`*_credential`, so a compound name
    /// like `google_credential` is caught even though `credential` alone is
    /// already an exact match). Collected rather than asserted one by one, so
    /// a run against the unpatched lists names every miss at once instead of
    /// stopping at the first.
    #[test]
    fn credential_names_still_missing_after_the_e4a_widening() {
        let should_be_caught = [
            ("authorization", "Bearer sk-geheim-4711"),
            ("access_key", "AKIAGEHEIM4711"),
            ("private_key", "-----BEGIN PRIVATE KEY-----"),
            ("session_cookie", "sk-geheim-4711"),
            ("google_credential", "sk-geheim-4711"),
        ];
        let missed: Vec<&str> = should_be_caught
            .iter()
            .filter(|(name, secret)| {
                let value = json!({ "providers": { "x": { *name: *secret } } });
                !matches!(
                    seal(&value, false, None, "t", "v"),
                    Err(Error::SecretInPlainBundle)
                )
            })
            .map(|(name, _)| *name)
            .collect();
        assert!(
            missed.is_empty(),
            "these credential names slipped through into a plain bundle: {missed:?}"
        );
    }

    /// A non-string under a credential name is a credential someone failed to
    /// stringify -- the old walk only ever looked at `as_str()`.
    #[test]
    fn a_credential_that_is_not_a_string_still_counts() {
        for value in [json!(4711), json!(["sk-geheim"]), json!({ "v": "sk" })] {
            let payload = json!({ "providers": { "x": { "api_key": value } } });
            assert!(
                matches!(
                    seal(&payload, false, None, "t", "v"),
                    Err(Error::SecretInPlainBundle)
                ),
                "a non-string credential was written into a plain bundle"
            );
        }
    }

    /// The other direction: the wider net must not start blocking honest
    /// exports. `monkey` is the reason `key` is matched as a whole name.
    #[test]
    fn ordinary_settings_are_not_mistaken_for_credentials() {
        let innocent = json!({
            "settings": {
                "monkey": "yes",
                "keyboard": "qwertz",
                "week_start": "monday",
                "app_icon": "default",
                "hotkey": "cmd+k",
                "authors": ["Mads"],
                "token_budget": 4096,
                // Near-misses for the E4c widening: none of these is the
                // exact name or ends in the suffix that was added.
                "authorization_mode": "strict",
                "cookie_banner_dismissed": true,
                "private_key_hint": "ends in .pem",
                "access_key_visible": false,
            },
            "templates": [{ "id": "t", "sections_json": [{ "title": "Keynote" }] }],
        });
        assert!(seal(&innocent, false, None, "t", "v").is_ok());
    }

    /// E4b. `seal` refuses to write these; `open` used to hand them straight to
    /// the importer anyway.
    #[test]
    fn a_plain_bundle_that_contradicts_itself_is_refused_on_reading() {
        let claims_secrets = json!({
            "format": FORMAT,
            "version": VERSION,
            "created_at": "t",
            "app_version": "v",
            "includes_secrets": true,
            "payload": { "settings": {} },
        })
        .to_string();
        assert!(matches!(
            open(&claims_secrets, None),
            Err(Error::InconsistentBundle)
        ));
        assert!(matches!(
            inspect(&claims_secrets),
            Err(Error::InconsistentBundle)
        ));

        let carries_a_key = json!({
            "format": FORMAT,
            "version": VERSION,
            "created_at": "t",
            "app_version": "v",
            "includes_secrets": false,
            "payload": { "providers": { "stt:openai": { "api_key": "sk-geheim-4711" } } },
        })
        .to_string();
        assert!(matches!(
            open(&carries_a_key, None),
            Err(Error::SecretInPlainBundle)
        ));
        assert!(matches!(
            inspect(&carries_a_key),
            Err(Error::SecretInPlainBundle)
        ));
    }

    #[test]
    fn inspect_answers_before_a_password_is_asked_for() {
        let sealed = seal(&payload(), true, Some("richtig"), "t", "v").unwrap();
        let info = inspect(&sealed).unwrap();
        assert!(info.encrypted);
        assert!(info.includes_secrets);
        assert_eq!(info.app_version, "v");

        let plain = seal(&payload_without_credentials(), false, None, "t", "v").unwrap();
        assert!(!inspect(&plain).unwrap().encrypted);
    }

    #[test]
    fn the_stored_kdf_parameters_are_the_ones_used() {
        let sealed = seal(&payload(), true, Some("richtig"), "t", "v").unwrap();
        let envelope: Envelope = serde_json::from_str(&sealed).unwrap();
        let encryption = envelope.encryption.unwrap();
        assert_eq!(encryption.kdf, KDF_ARGON2ID);
        assert_eq!(encryption.cipher, CIPHER_XCHACHA20POLY1305);
        assert_eq!(encryption.m_cost, ARGON2_M_COST);
        assert_eq!(encryption.t_cost, ARGON2_T_COST);
        assert_eq!(encryption.p_cost, ARGON2_P_COST);
        assert_eq!(BASE64.decode(&encryption.salt).unwrap().len(), SALT_BYTES);
        assert_eq!(BASE64.decode(&encryption.nonce).unwrap().len(), NONCE_BYTES);
    }

    #[test]
    fn changed_kdf_parameters_still_open_with_the_parameters_in_the_file() {
        // Guards the promise that the parameters travel with the file: a bundle
        // written under a different profile must still open.
        let mut envelope = Envelope {
            format: FORMAT.to_string(),
            version: VERSION,
            created_at: "t".to_string(),
            app_version: "v".to_string(),
            includes_secrets: true,
            encryption: None,
            payload: None,
            ciphertext: None,
        };
        let salt = vec![7u8; SALT_BYTES];
        let nonce = [9u8; NONCE_BYTES];
        let encryption = Encryption {
            kdf: KDF_ARGON2ID.to_string(),
            salt: BASE64.encode(&salt),
            m_cost: 8192,
            t_cost: 3,
            p_cost: 2,
            cipher: CIPHER_XCHACHA20POLY1305.to_string(),
            nonce: BASE64.encode(&nonce),
        };
        let key = derive_key("richtig", &salt, 8192, 3, 2).unwrap();
        let cipher = XChaCha20Poly1305::new(key.as_ref().into());
        let aad = associated_data(&envelope, &encryption);
        let sealed_bytes = cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: serde_json::to_vec(&payload()).unwrap().as_ref(),
                    aad: &aad,
                },
            )
            .unwrap();
        envelope.ciphertext = Some(BASE64.encode(&sealed_bytes));
        envelope.encryption = Some(encryption);

        let contents = serde_json::to_string(&envelope).unwrap();
        let (_, opened) = open(&contents, Some("richtig")).unwrap();
        assert_eq!(opened, payload());
    }
}

#[cfg(test)]
mod password_length_tests {
    use super::*;
    use serde_json::json;

    fn plain() -> serde_json::Value {
        json!({ "settings": { "theme": "dark" } })
    }

    // Betreiber, 01.09.2026: "kennwort: mindestens 6 Zeichen." The core is where
    // the rule has to live -- the export form is the friendly half, but a
    // caller that skips it must not be able to write a weaker file.
    #[test]
    fn a_new_bundle_needs_a_password_of_at_least_six_characters() {
        assert!(matches!(
            seal(&plain(), true, Some("12345"), "t", "v"),
            Err(Error::PasswordTooShort)
        ));
        assert!(seal(&plain(), true, Some("123456"), "t", "v").is_ok());
    }

    // Counted in characters, not bytes: six umlauts are six characters and
    // twelve bytes, and refusing them would be a rule about encoding.
    #[test]
    fn six_characters_are_six_characters_whatever_they_weigh() {
        assert!(seal(&plain(), true, Some("äöüäöü"), "t", "v").is_ok());
    }

    // The rule is about writing. A file that already exists with a shorter
    // password has to keep opening, or the rule takes people's settings away
    // instead of protecting them.
    #[test]
    fn an_older_file_with_a_shorter_password_still_opens() {
        // Written the way it would have been before the rule existed.
        let sealed = seal_inner(&plain(), true, Some("kurz"), "t", "v", false).unwrap();

        let (_info, opened) = open(&sealed, Some("kurz")).unwrap();
        assert_eq!(opened["settings"]["theme"], "dark");
    }
}
