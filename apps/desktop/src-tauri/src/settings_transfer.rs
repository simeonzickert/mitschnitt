//! Writing and reading a settings bundle (F14).
//!
//! Mitschnitt-Fork. Deliberately thin: the envelope and its cryptography live in
//! `crates/settings-transfer`, deciding what goes into a bundle lives in
//! `apps/desktop/src/settings-transfer/bundle.ts`. This file only does the file
//! IO and hands the frontend a stable error code, so the message the user reads
//! stays in the translation catalogs rather than in Rust.

use anlg_settings_transfer as transfer;

/// Stable codes rather than prose. The frontend turns them into translated
/// sentences; anything else would put user-facing German into Rust and out of
/// reach of `lingui extract`.
fn code(error: &transfer::Error) -> &'static str {
    match error {
        transfer::Error::NotABundle => "not_a_bundle",
        transfer::Error::UnsupportedVersion => "unsupported_version",
        transfer::Error::Malformed => "malformed",
        transfer::Error::PasswordRequired => "password_required",
        transfer::Error::WrongPassword => "wrong_password",
        transfer::Error::EmptyPassword => "empty_password",
        transfer::Error::PasswordTooShort => "password_too_short",
        transfer::Error::RandomnessUnavailable => "randomness_unavailable",
        transfer::Error::SecretInPlainBundle => "secret_in_plain_bundle",
        transfer::Error::UnreasonableKdfCost => "unreasonable_kdf_cost",
        transfer::Error::InconsistentBundle => "inconsistent_bundle",
        transfer::Error::Json(_) => "malformed",
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct BundleInfo {
    pub version: u32,
    pub created_at: String,
    pub app_version: String,
    pub includes_secrets: bool,
    pub encrypted: bool,
}

impl From<transfer::BundleInfo> for BundleInfo {
    fn from(info: transfer::BundleInfo) -> Self {
        Self {
            version: info.version,
            created_at: info.created_at,
            app_version: info.app_version,
            includes_secrets: info.includes_secrets,
            encrypted: info.encrypted,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct OpenedBundle {
    pub info: BundleInfo,
    /// The payload as a JSON string. Kept as text so the shape stays the
    /// frontend's business and a payload change needs no Rust change.
    pub payload: String,
}

#[tauri::command]
#[specta::specta]
pub async fn export_settings_bundle<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    path: String,
    payload: String,
    includes_secrets: bool,
    password: Option<String>,
) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(&payload).map_err(|_| "malformed")?;
    let created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let app_version = tauri::Manager::package_info(&app).version.to_string();

    let contents = transfer::seal(
        &value,
        includes_secrets,
        password.as_deref(),
        &created_at,
        &app_version,
    )
    .map_err(|error| code(&error).to_string())?;

    write_private(&path, &contents).map_err(|error| format!("io:{error}"))
}

/// Writes the bundle so only its owner can read it (H9).
///
/// An encrypted bundle still holds the ciphertext of every API key, and a plain
/// one holds the whole setup; `std::fs::write` would leave both at whatever the
/// umask allows, which on a shared machine is commonly world-readable. The mode
/// is set at CREATION, not afterwards, so there is no window in which the file
/// exists with its contents and the wrong permissions.
fn write_private(path: &str, contents: &str) -> std::io::Result<()> {
    use std::io::Write;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    // `create` only applies the mode to a NEW file; overwriting an existing
    // one would keep whatever permissions it already had.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(contents.as_bytes())?;
    file.flush()
}

#[tauri::command]
#[specta::specta]
pub async fn inspect_settings_bundle(path: String) -> Result<BundleInfo, String> {
    let contents = std::fs::read_to_string(&path).map_err(|error| format!("io:{error}"))?;
    transfer::inspect(&contents)
        .map(Into::into)
        .map_err(|error| code(&error).to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn read_settings_bundle(
    path: String,
    password: Option<String>,
) -> Result<OpenedBundle, String> {
    let contents = std::fs::read_to_string(&path).map_err(|error| format!("io:{error}"))?;

    // Opening a sealed bundle runs Argon2id: seconds of CPU and tens of MiB by
    // design. On the async worker that would stall every other command for the
    // duration, so it goes to the blocking pool. The crate caps what a file may
    // ask for (`UnreasonableKdfCost`); this keeps even the honest cost off the
    // path everything else shares.
    let opened = tauri::async_runtime::spawn_blocking(move || {
        transfer::open(&contents, password.as_deref()).map_err(|error| code(&error).to_string())
    })
    .await
    .map_err(|_| "malformed".to_string())?;
    let (info, payload) = opened?;

    Ok(OpenedBundle {
        info: info.into(),
        payload: serde_json::to_string(&payload).map_err(|_| "malformed")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_envelope_error_has_its_own_code() {
        // A code that collapses two causes into one leaves the UI unable to
        // tell "you need a password" from "that password was wrong".
        assert_eq!(
            code(&transfer::Error::PasswordRequired),
            "password_required"
        );
        assert_eq!(code(&transfer::Error::WrongPassword), "wrong_password");
        assert_eq!(code(&transfer::Error::NotABundle), "not_a_bundle");
        assert_eq!(
            code(&transfer::Error::UnsupportedVersion),
            "unsupported_version"
        );
        assert_eq!(code(&transfer::Error::EmptyPassword), "empty_password");
        assert_eq!(
            code(&transfer::Error::PasswordTooShort),
            "password_too_short"
        );
    }

    /// The minimum password length exists twice: once in the Rust core, where
    /// it is the rule, and once in the export form, where it is the message the
    /// person reads before the file is written. Two copies of a number drift,
    /// and the drift here is silent in the worse direction -- a form that
    /// accepts what the core refuses looks like a bug in the core.
    #[test]
    fn the_export_form_uses_the_same_minimum_as_the_core() {
        const KEYS_TS: &str = "../src/settings-transfer/keys.ts";
        let source = std::fs::read_to_string(KEYS_TS).unwrap();
        let expected = format!(
            "export const MIN_PASSWORD_CHARS = {};",
            transfer::MIN_PASSWORD_CHARS
        );
        assert!(
            source.contains(&expected),
            "{KEYS_TS} does not say `{expected}`"
        );
        assert_eq!(
            code(&transfer::Error::SecretInPlainBundle),
            "secret_in_plain_bundle"
        );
        assert_eq!(
            code(&transfer::Error::UnreasonableKdfCost),
            "unreasonable_kdf_cost"
        );
        assert_eq!(
            code(&transfer::Error::InconsistentBundle),
            "inconsistent_bundle"
        );
    }

    /// H9. A bundle carries the whole setup and, sealed, the ciphertext of
    /// every key. Before this it was written at whatever the umask allowed.
    #[cfg(unix)]
    #[test]
    fn an_exported_bundle_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bundle.json");

        // Also covers the overwrite case: a pre-existing world-readable file
        // must not keep its permissions.
        std::fs::write(&path, "alt").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_private(path.to_str().unwrap(), "{}").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the bundle was written as {mode:o}");
    }
}
