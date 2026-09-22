use crate::Store2PluginExt;

const SECURE_STORE_SUFFIX: &str = "secure-store";
// Schluesselbund-Konten, die nur nativ gelesen und geschrieben werden und dem
// Webview verschlossen bleiben. `store2:default` (capabilities/default.json)
// gibt get/set/delete_secret jedem Fenster frei; ohne diese Liste kann die
// Oberflaeche Stimmprofile lesen, ueberschreiben und loeschen (Opus-Review
// 02.09.2026, F3). Der Waechter stand bis b92ad801c0 hier -- mit "e2ee:" als
// einzigem Eintrag -- und ging, weil die Liste leer schien. Sie war es nicht.
//
// - voiceprint_exemplars:<uuid> / voiceprint_candidates:<uuid>: die
//   Stimmprofile (plugins/transcription/src/voiceprint.rs, Scopes in
//   crates/db-app/src/voiceprint_types.rs).
// - e2ee: Altbestand der Cloud-Schicht. Der Fork schreibt ihn nicht mehr; ein
//   Eintrag aus einer frueheren Installation bleibt dem Renderer trotzdem
//   verschlossen.
const NATIVE_SECRET_ACCOUNT_PREFIXES: &[&str] =
    &["voiceprint_exemplars:", "voiceprint_candidates:", "e2ee:"];
#[cfg(target_os = "macos")]
const MACOS_KEYCHAIN_ACCESS_ERROR_PREFIX: &str = "macOS couldn't access your login Keychain.";
#[cfg(target_os = "linux")]
const LINUX_SECRET_SERVICE_ACCESS_ERROR: &str =
    "Linux couldn't access Secret Service. Unlock your login keyring, then try again.";
#[cfg(target_os = "linux")]
const LINUX_SECRET_SERVICE_UNAVAILABLE_ERROR: &str =
    "Linux Secret Service is unavailable. Start your desktop keyring service, then try again.";

#[cfg(target_os = "macos")]
const ERR_SEC_AUTH_FAILED: i32 = -25293;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SecretCaller {
    Native,
    Renderer,
}

// Der Kontoname kommt aus demselben Helfer wie in `secret_entry`, damit
// Waechter und Schluesselbund nie zwei verschiedene Konten sehen. Verglichen
// wird getrimmt und ohne Gross-/Kleinschreibung: der Windows Credential
// Manager unterscheidet "Voiceprint_Exemplars" nicht von
// "voiceprint_exemplars", und ein fuehrendes Leerzeichen ist kein anderes
// Konto (C4, Review 02.09.2026).
fn validate_secret_coordinate(
    caller: SecretCaller,
    identifier: &str,
    scope: &str,
    key: &str,
) -> Result<(), String> {
    let account = secure_store_account(identifier, scope, key)
        .trim()
        .to_ascii_lowercase();
    if caller == SecretCaller::Renderer
        && NATIVE_SECRET_ACCOUNT_PREFIXES
            .iter()
            .any(|prefix| account.starts_with(prefix))
    {
        return Err("secure-store account is reserved for native use".to_string());
    }

    Ok(())
}

// Hier stand eine Umschreibung von com.hyprnote.* auf com.anarlog.*, mit der
// das Original seinen eigenen Umbenennungs-Sprung ueberbrueckt hat. Dieser
// Fork hat nie unter einer dieser Kennungen ausgeliefert -- unsere Bauten
// heissen durchweg media.zickert.mitschnitt* und liefen ohnehin durch den
// Durchreiche-Zweig. Entfernt statt umbenannt: eine Umschreibung auf unsere
// eigenen Namen haette Schluesselbund-Eintraege einer FREMDEN Installation
// adressiert.
fn secure_store_service(identifier: &str) -> String {
    format!("{identifier}.{SECURE_STORE_SUFFIX}")
}

// Die Sonderbehandlung fuer "com.hyprnote.dev" (Praefix "v2:") ist mit der
// Umschreibung oben entfallen: sie galt Entwickler-Eintraegen des Originals.
fn secure_store_account(_identifier: &str, scope: &str, key: &str) -> String {
    format!("{scope}:{key}")
}

fn secure_store_error(error: keyring::Error) -> String {
    #[cfg(target_os = "macos")]
    if keychain_error_code(&error) == Some(ERR_SEC_AUTH_FAILED) {
        return format!(
            "{MACOS_KEYCHAIN_ACCESS_ERROR_PREFIX} Use “Repair Keychain Access” below, then try again."
        );
    }

    #[cfg(target_os = "linux")]
    match error {
        keyring::Error::NoStorageAccess(_) => {
            return LINUX_SECRET_SERVICE_ACCESS_ERROR.to_string();
        }
        keyring::Error::PlatformFailure(_) => {
            return LINUX_SECRET_SERVICE_UNAVAILABLE_ERROR.to_string();
        }
        _ => {}
    }

    error.to_string()
}

#[cfg(target_os = "macos")]
fn keychain_error_code(error: &keyring::Error) -> Option<i32> {
    let source = match error {
        keyring::Error::PlatformFailure(source) | keyring::Error::NoStorageAccess(source) => source,
        _ => return None,
    };

    source
        .downcast_ref::<security_framework::base::Error>()
        .map(|error| error.code())
}

#[cfg(target_os = "macos")]
fn repair_macos_keychain_access() -> Result<(), String> {
    use objc2_security::SecKeychain;

    const ERR_SEC_SUCCESS: i32 = 0;
    const ERR_SEC_USER_CANCELED: i32 = -128;

    #[allow(deprecated)]
    let lock_status = unsafe { SecKeychain::lock(None) };
    if lock_status != ERR_SEC_SUCCESS {
        return Err(format!(
            "macOS couldn't lock your login Keychain (OSStatus {lock_status})."
        ));
    }

    #[allow(deprecated)]
    let unlock_status = unsafe { SecKeychain::unlock(None, 0, std::ptr::null(), false) };
    if unlock_status == ERR_SEC_USER_CANCELED {
        return Err(
            "Keychain unlock was cancelled. Your login Keychain is still locked; run the repair again to unlock it."
                .to_string(),
        );
    }
    if unlock_status != ERR_SEC_SUCCESS {
        return Err(format!(
            "macOS couldn't unlock your login Keychain (OSStatus {unlock_status}). Your login Keychain is still locked."
        ));
    }

    Ok(())
}

fn legacy_secret_locations(identifier: &str, scope: &str, key: &str) -> Vec<(String, String)> {
    let service = secure_store_service(identifier);
    let account = format!("{scope}:{key}");
    let current_account = secure_store_account(identifier, scope, key);
    let legacy_service = format!("{identifier}.{SECURE_STORE_SUFFIX}");
    let mut locations = Vec::new();

    if account != current_account {
        locations.push((service.clone(), account.clone()));
    }
    if legacy_service != service {
        locations.push((legacy_service, account));
    }

    locations
}

fn legacy_secret_entries<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    scope: &str,
    key: &str,
) -> Result<Vec<keyring::Entry>, String> {
    legacy_secret_locations(&app.config().identifier, scope, key)
        .into_iter()
        .map(|(service, account)| {
            keyring::Entry::new(&service, &account).map_err(secure_store_error)
        })
        .collect()
}

fn secret_entry<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    scope: &str,
    key: &str,
) -> Result<keyring::Entry, String> {
    if scope.trim().is_empty() || key.trim().is_empty() {
        return Err("secure-store scope and key must not be empty".to_string());
    }

    let identifier = &app.config().identifier;
    let service = secure_store_service(identifier);
    let account = secure_store_account(identifier, scope, key);
    keyring::Entry::new(&service, &account).map_err(secure_store_error)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    app.store2().save().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn repair_keychain_access() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return tauri::async_runtime::spawn_blocking(repair_macos_keychain_access)
            .await
            .map_err(|error| error.to_string())?;
    }

    #[cfg(not(target_os = "macos"))]
    Err("Keychain repair is only available on macOS.".to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_str<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
) -> Result<Option<String>, String> {
    let store = app
        .store2()
        .scoped_store::<String>(scope)
        .map_err(|e| e.to_string())?;

    store.get::<String>(key).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_str<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
    value: String,
) -> Result<(), String> {
    let store = app
        .store2()
        .scoped_store::<String>(scope)
        .map_err(|e| e.to_string())?;

    store.set(key, value).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_bool<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
) -> Result<Option<bool>, String> {
    let store = app
        .store2()
        .scoped_store::<String>(scope)
        .map_err(|e| e.to_string())?;

    store.get::<bool>(key).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_bool<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
    value: bool,
) -> Result<(), String> {
    let store = app
        .store2()
        .scoped_store::<String>(scope)
        .map_err(|e| e.to_string())?;

    store.set(key, value).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_number<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
) -> Result<Option<f64>, String> {
    let store = app
        .store2()
        .scoped_store::<String>(scope)
        .map_err(|e| e.to_string())?;

    store.get::<f64>(key).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_number<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
    value: f64,
) -> Result<(), String> {
    let store = app
        .store2()
        .scoped_store::<String>(scope)
        .map_err(|e| e.to_string())?;

    store.set(key, value).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_secret<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
) -> Result<Option<String>, String> {
    read_secret_for(SecretCaller::Renderer, app, scope, key).await
}

pub async fn read_secret<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
) -> Result<Option<String>, String> {
    read_secret_for(SecretCaller::Native, app, scope, key).await
}

pub fn read_secret_blocking<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    scope: &str,
    key: &str,
) -> Result<Option<String>, String> {
    read_secret_blocking_for(SecretCaller::Native, app, scope, key)
}

async fn read_secret_for<R: tauri::Runtime>(
    caller: SecretCaller,
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
) -> Result<Option<String>, String> {
    validate_secret_coordinate(caller, &app.config().identifier, &scope, &key)?;
    tauri::async_runtime::spawn_blocking(move || {
        read_secret_blocking_for(caller, &app, &scope, &key)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn read_secret_blocking_for<R: tauri::Runtime>(
    caller: SecretCaller,
    app: &tauri::AppHandle<R>,
    scope: &str,
    key: &str,
) -> Result<Option<String>, String> {
    validate_secret_coordinate(caller, &app.config().identifier, scope, key)?;
    let entry = secret_entry(app, scope, key)?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => {
            for legacy_entry in legacy_secret_entries(app, scope, key)? {
                match legacy_entry.get_password() {
                    Ok(secret) => {
                        if entry.set_password(&secret).is_ok() {
                            let _ = legacy_entry.delete_credential();
                        }
                        return Ok(Some(secret));
                    }
                    Err(keyring::Error::NoEntry | keyring::Error::PlatformFailure(_)) => {}
                    Err(error) => return Err(secure_store_error(error)),
                }
            }
            Ok(None)
        }
        Err(error) => Err(secure_store_error(error)),
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_secret<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
    value: String,
) -> Result<(), String> {
    write_secret_for(SecretCaller::Renderer, app, scope, key, value).await
}

pub async fn write_secret<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
    value: String,
) -> Result<(), String> {
    write_secret_for(SecretCaller::Native, app, scope, key, value).await
}

pub fn write_secret_blocking<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    scope: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    write_secret_blocking_for(SecretCaller::Native, app, scope, key, value)
}

async fn write_secret_for<R: tauri::Runtime>(
    caller: SecretCaller,
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
    value: String,
) -> Result<(), String> {
    validate_secret_coordinate(caller, &app.config().identifier, &scope, &key)?;
    tauri::async_runtime::spawn_blocking(move || {
        write_secret_blocking_for(caller, &app, &scope, &key, &value)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn write_secret_blocking_for<R: tauri::Runtime>(
    caller: SecretCaller,
    app: &tauri::AppHandle<R>,
    scope: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    validate_secret_coordinate(caller, &app.config().identifier, scope, key)?;
    let entry = secret_entry(app, scope, key)?;
    entry.set_password(value).map_err(secure_store_error)?;
    for legacy_entry in legacy_secret_entries(app, scope, key)? {
        let _ = legacy_entry.delete_credential();
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn delete_secret<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
) -> Result<(), String> {
    delete_secret_for(SecretCaller::Renderer, app, scope, key).await
}

pub fn delete_secret_blocking<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    scope: &str,
    key: &str,
) -> Result<(), String> {
    delete_secret_blocking_for(SecretCaller::Native, app, scope, key)
}

async fn delete_secret_for<R: tauri::Runtime>(
    caller: SecretCaller,
    app: tauri::AppHandle<R>,
    scope: String,
    key: String,
) -> Result<(), String> {
    validate_secret_coordinate(caller, &app.config().identifier, &scope, &key)?;
    tauri::async_runtime::spawn_blocking(move || {
        delete_secret_blocking_for(caller, &app, &scope, &key)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn delete_secret_blocking_for<R: tauri::Runtime>(
    caller: SecretCaller,
    app: &tauri::AppHandle<R>,
    scope: &str,
    key: &str,
) -> Result<(), String> {
    validate_secret_coordinate(caller, &app.config().identifier, scope, key)?;
    for legacy_entry in legacy_secret_entries(app, scope, key)? {
        match legacy_entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry | keyring::Error::PlatformFailure(_)) => {}
            Err(error) => return Err(secure_store_error(error)),
        }
    }
    let entry = secret_entry(app, scope, key)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(error) => return Err(secure_store_error(error)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN_ID: &str = "media.zickert.mitschnitt";

    // Fremde Kennungen werden NICHT mehr umgeschrieben. Der Test haelt die
    // Richtung fest: wer hier wieder eine Zuordnung einbaut, laesst diesen
    // Fork im Schluesselbund einer fremden Installation nachschlagen.
    #[test]
    fn never_rewrites_a_foreign_bundle_identifier() {
        for identifier in [
            "com.hyprnote.dev",
            "com.hyprnote.staging",
            "com.hyprnote.stable",
            "com.hyprnote.Hyprnote",
            "com.anarlog.stable",
        ] {
            assert_eq!(
                secure_store_service(identifier),
                format!("{identifier}.secure-store"),
                "{identifier} wird auf einen fremden Dienstnamen umgeschrieben"
            );
        }
    }

    #[test]
    fn uses_our_own_identifier_verbatim() {
        assert_eq!(
            secure_store_service("media.zickert.mitschnitt"),
            "media.zickert.mitschnitt.secure-store"
        );
        assert_eq!(
            secure_store_service("media.zickert.mitschnitt.stable"),
            "media.zickert.mitschnitt.stable.secure-store"
        );
    }

    #[test]
    fn preserves_unknown_service_identifiers() {
        assert_eq!(
            secure_store_service("com.example.app"),
            "com.example.app.secure-store"
        );
    }

    #[test]
    fn account_no_longer_depends_on_the_identifier() {
        for identifier in [
            "com.hyprnote.dev",
            "com.hyprnote.stable",
            "media.zickert.mitschnitt",
        ] {
            assert_eq!(
                secure_store_account(identifier, "provider", "deepgram"),
                "provider:deepgram"
            );
        }
    }

    // Ohne Umschreibung faellt Dienst- und Kontoname fuer JEDE Kennung
    // zusammen, die Liste der Altorte ist also immer leer. Der Test haelt
    // das fest, damit die Leere als Absicht lesbar bleibt und nicht als
    // uebersehener Fehler -- inklusive unserer eigenen Kennung.
    #[test]
    fn there_are_no_legacy_secret_locations_left() {
        for identifier in [
            "com.hyprnote.dev",
            "com.example.app",
            "media.zickert.mitschnitt",
        ] {
            assert!(
                legacy_secret_locations(identifier, "provider", "deepgram").is_empty(),
                "{identifier} meldet wieder Altorte im Schluesselbund"
            );
        }
    }

    #[test]
    fn skips_duplicate_legacy_secret_locations() {
        assert!(legacy_secret_locations("com.example.app", "provider", "deepgram").is_empty());
    }

    #[test]
    fn isolates_native_secret_accounts_from_renderer_commands() {
        assert!(
            validate_secret_coordinate(SecretCaller::Renderer, OWN_ID, "provider", "deepgram")
                .is_ok()
        );
        for (scope, key) in [
            (
                "voiceprint_exemplars",
                "3f1c0d2e-0000-4000-8000-000000000001",
            ),
            (
                "voiceprint_candidates",
                "3f1c0d2e-0000-4000-8000-000000000002",
            ),
            ("e2ee", "account:user-a:recovery-v1"),
            ("e2ee:account", "user-a:recovery-v1"),
            // C4 (Review 02.09.2026): der Windows Credential Manager vergleicht
            // Kontonamen ohne Gross-/Kleinschreibung, und ein fuehrendes
            // Leerzeichen ist kein anderes Konto -- der Waechter muss das
            // genauso sehen wie der Schluesselbund.
            ("Voiceprint_Exemplars", "x"),
            (" voiceprint_exemplars", "x"),
            ("VOICEPRINT_CANDIDATES", "x"),
        ] {
            assert!(
                validate_secret_coordinate(SecretCaller::Renderer, OWN_ID, scope, key).is_err(),
                "{scope}:{key} ist vom Renderer erreichbar"
            );
            assert!(
                validate_secret_coordinate(SecretCaller::Native, OWN_ID, scope, key).is_ok(),
                "{scope}:{key} ist nativ gesperrt"
            );
        }
        // Ein Konto, das den Praefix nur enthaelt, ist nicht reserviert.
        assert!(
            validate_secret_coordinate(
                SecretCaller::Renderer,
                OWN_ID,
                "notes",
                "voiceprint_exemplars:x"
            )
            .is_ok()
        );
    }

    // Falsifikator (Opus-Review 02.09.2026, F3): ein Renderer-Aufruf auf ein
    // Stimmprofil-Konto liefert einen Wert (oder Ok(None)) statt eines Fehlers.
    // Die Konten werden ausschliesslich nativ geschrieben und gelesen
    // (plugins/transcription/src/voiceprint.rs), stehen aber ueber
    // `store2:default` jedem Fenster offen.
    #[tokio::test]
    async fn renderer_secret_commands_reject_native_accounts_before_keychain_access() {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let app = app.handle().clone();
        let expected = "secure-store account is reserved for native use";

        for (scope, key) in [
            (
                "voiceprint_exemplars",
                "3f1c0d2e-0000-4000-8000-000000000001",
            ),
            (
                "voiceprint_candidates",
                "3f1c0d2e-0000-4000-8000-000000000002",
            ),
            ("e2ee", "account:user-a:recovery-v1"),
            ("Voiceprint_Exemplars", "x"),
            (" voiceprint_exemplars", "x"),
        ] {
            assert_eq!(
                get_secret(app.clone(), scope.to_string(), key.to_string())
                    .await
                    .unwrap_err(),
                expected,
                "{scope}:{key} ist vom Renderer lesbar"
            );
            assert_eq!(
                set_secret(
                    app.clone(),
                    scope.to_string(),
                    key.to_string(),
                    "replacement".to_string()
                )
                .await
                .unwrap_err(),
                expected,
                "{scope}:{key} ist vom Renderer ueberschreibbar"
            );
            assert_eq!(
                delete_secret(app.clone(), scope.to_string(), key.to_string())
                    .await
                    .unwrap_err(),
                expected,
                "{scope}:{key} ist vom Renderer loeschbar"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn explains_macos_keychain_access_failures() {
        let error = keyring::Error::PlatformFailure(Box::new(
            security_framework::base::Error::from_code(ERR_SEC_AUTH_FAILED),
        ));

        assert_eq!(
            secure_store_error(error),
            "macOS couldn't access your login Keychain. Use “Repair Keychain Access” below, then try again."
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn preserves_unrelated_macos_keychain_failures() {
        let platform_error = security_framework::base::Error::from_code(-34018);
        let expected = format!("Platform failure: {platform_error}");
        let error = keyring::Error::PlatformFailure(Box::new(platform_error));

        assert_eq!(secure_store_error(error), expected);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn explains_locked_linux_secret_service() {
        let error = keyring::Error::NoStorageAccess(Box::new(std::io::Error::other("locked")));

        assert_eq!(secure_store_error(error), LINUX_SECRET_SERVICE_ACCESS_ERROR);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn explains_unavailable_linux_secret_service() {
        let error = keyring::Error::PlatformFailure(Box::new(std::io::Error::other("unavailable")));

        assert_eq!(
            secure_store_error(error),
            LINUX_SECRET_SERVICE_UNAVAILABLE_ERROR
        );
    }
}
