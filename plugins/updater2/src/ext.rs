use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use tauri::Manager;
use tauri_plugin_store2::Store2PluginExt;
use tauri_plugin_updater::UpdaterExt;
use tauri_specta::Event;

use crate::events::{
    UpdateAvailableEvent, UpdateDownloadFailedEvent, UpdateDownloadProgressEvent,
    UpdateDownloadingEvent, UpdateReadyEvent, UpdatedEvent,
};

static DOWNLOAD_MUTEX: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

static MEETING_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub struct Updater2<'a, R: tauri::Runtime, M: tauri::Manager<R>> {
    manager: &'a M,
    _runtime: std::marker::PhantomData<fn() -> R>,
}

impl<'a, R: tauri::Runtime, M: tauri::Manager<R>> Updater2<'a, R, M> {
    pub fn automatic_updates_enabled(&self) -> Result<bool, crate::Error> {
        let store = self.manager.store2().scoped_store(crate::PLUGIN_NAME)?;
        let enabled = store
            .get(crate::StoreKey::AutomaticUpdatesEnabled)?
            .unwrap_or(true);
        Ok(enabled)
    }

    pub fn set_automatic_updates_enabled(&self, enabled: bool) -> Result<(), crate::Error> {
        let store = self.manager.store2().scoped_store(crate::PLUGIN_NAME)?;
        store.set(crate::StoreKey::AutomaticUpdatesEnabled, enabled)?;
        Ok(())
    }

    pub fn meeting_active(&self) -> bool {
        MEETING_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_meeting_active(&self, active: bool) {
        MEETING_ACTIVE.store(active, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_last_seen_version(&self) -> Result<Option<String>, crate::Error> {
        let store = self.manager.store2().scoped_store(crate::PLUGIN_NAME)?;
        let v = store.get(crate::StoreKey::LastSeenVersion)?;
        Ok(v)
    }

    pub fn set_last_seen_version(&self, version: String) -> Result<(), crate::Error> {
        let store = self.manager.store2().scoped_store(crate::PLUGIN_NAME)?;
        store.set(crate::StoreKey::LastSeenVersion, version)?;
        Ok(())
    }

    pub fn maybe_emit_updated(&self) {
        let current_version = match self.manager.config().version.as_ref() {
            Some(v) => v.clone(),
            None => {
                tracing::warn!("no_version_in_config");
                return;
            }
        };

        let (should_emit, previous) = match self.get_last_seen_version() {
            Ok(Some(last_version)) if !last_version.is_empty() => {
                (last_version != current_version, Some(last_version))
            }
            Ok(_) => (false, None),
            Err(e) => {
                tracing::error!("failed_to_get_last_seen_version: {}", e);
                (false, None)
            }
        };

        if should_emit {
            let payload = UpdatedEvent {
                previous,
                current: current_version.clone(),
            };

            if let Err(e) = payload.emit(self.manager.app_handle()) {
                tracing::error!("failed_to_emit_updated_event: {}", e);
            }
        }

        if let Err(e) = self.set_last_seen_version(current_version) {
            tracing::error!("failed_to_update_version: {}", e);
        }
    }

    // Caches the downloaded bytes together with the signature `update.signature`
    // that already verified them in Update::download(). Without the sidecar
    // signature, install_and_relaunch() would trust whatever sits in the cache
    // file at install time -- and that file lives in app_cache_dir(), writable
    // by anything running as the same user. Pairing bytes with the signature
    // that vouches for them lets get_cached_update_bytes() re-verify before
    // handing them to update.install() (TOCTOU fix, gemessen 22.09.2026).
    fn cache_update_bytes(
        &self,
        version: &str,
        bytes: &[u8],
        signature: &str,
    ) -> Result<(), crate::Error> {
        let cache_path =
            get_cache_path(self.manager, version).ok_or(crate::Error::CachePathUnavailable)?;
        let sig_path =
            get_sig_path(self.manager, version).ok_or(crate::Error::CachePathUnavailable)?;

        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&cache_path, bytes)?;
        std::fs::write(&sig_path, signature)?;
        tracing::debug!("cached_update_bytes: {:?}", cache_path);
        Ok(())
    }

    fn get_updater_pubkey(&self) -> Result<String, crate::Error> {
        self.manager
            .config()
            .plugins
            .0
            .get("updater")
            .and_then(|v| v.get("pubkey"))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or(crate::Error::UpdaterPubkeyUnavailable)
    }

    // Re-verifies bytes read back from the cache against the signature cached
    // alongside them. This is the same check Update::download() already ran
    // (verify_signature() in tauri_plugin_updater::updater, not public --
    // the actual decode+verify is mirrored in the free function below so it
    // can be unit-tested without a mocked tauri::Manager), repeated here
    // because the bytes crossed a disk round-trip since then -- a cache file
    // is not a place we trust just because we wrote it once.
    fn verify_cached_bytes(&self, version: &str, bytes: &[u8]) -> Result<(), crate::Error> {
        let sig_path =
            get_sig_path(self.manager, version).ok_or(crate::Error::CachePathUnavailable)?;
        let signature_b64 = std::fs::read_to_string(&sig_path).map_err(|_| {
            crate::Error::CachedUpdateSignatureInvalid {
                version: version.to_string(),
            }
        })?;
        let pubkey_b64 = self.get_updater_pubkey()?;

        if verify_bytes_against_signature(bytes, &pubkey_b64, signature_b64.trim()) {
            Ok(())
        } else {
            Err(crate::Error::CachedUpdateSignatureInvalid {
                version: version.to_string(),
            })
        }
    }

    fn get_cached_update_bytes(&self, version: &str) -> Result<Vec<u8>, crate::Error> {
        let cache_path =
            get_cache_path(self.manager, version).ok_or(crate::Error::CachePathUnavailable)?;

        if !cache_path.exists() {
            return Err(crate::Error::CachedUpdateNotFound);
        }

        let bytes = std::fs::read(&cache_path)?;

        if let Err(e) = self.verify_cached_bytes(version, &bytes) {
            tracing::error!("cached_update_signature_invalid: {:?}: {}", cache_path, e);
            // Discard the untrustworthy files instead of leaving them for a
            // retry to trip over again; the next check()/download() rebuilds
            // both from scratch and re-verifies at download time.
            let _ = std::fs::remove_file(&cache_path);
            if let Some(sig_path) = get_sig_path(self.manager, version) {
                let _ = std::fs::remove_file(&sig_path);
            }
            return Err(e);
        }

        Ok(bytes)
    }

    pub async fn check(&self) -> Result<Option<String>, crate::Error> {
        let updater = self.manager.updater()?;
        let update = updater.check().await?;
        let version = update.map(|u| u.version);

        // install_and_relaunch never returns, so this periodic check is the
        // only place stale cached bins can be cleaned up.
        prune_cached_updates(self.manager, version.as_deref());

        if let Some(version) = &version {
            if self.has_cached_update(version) {
                let _ = UpdateReadyEvent {
                    version: version.clone(),
                }
                .emit(self.manager.app_handle());
            } else {
                let _ = UpdateAvailableEvent {
                    version: version.clone(),
                }
                .emit(self.manager.app_handle());
            }
        }

        Ok(version)
    }

    pub fn has_cached_update(&self, version: &str) -> bool {
        get_cache_path(self.manager, version)
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    pub async fn download(&self, version: &str) -> Result<(), crate::Error> {
        let _download_guard = DOWNLOAD_MUTEX.lock().await;

        if self.has_cached_update(version) {
            let _ = UpdateReadyEvent {
                version: version.to_string(),
            }
            .emit(self.manager.app_handle());
            return Ok(());
        }

        let updater = self.manager.updater()?;
        let update = updater
            .check()
            .await?
            .ok_or(crate::Error::UpdateNotAvailable)?;

        if update.version != version {
            return Err(crate::Error::VersionMismatch {
                expected: version.to_string(),
                actual: update.version,
            });
        }

        let _ = UpdateDownloadingEvent {
            version: version.to_string(),
        }
        .emit(self.manager.app_handle());

        let app_handle = self.manager.app_handle().clone();
        let version_string = version.to_string();
        let result: Result<(), crate::Error> = async {
            let bytes = update
                .download(
                    |chunk_length, content_length| {
                        let _ = UpdateDownloadProgressEvent {
                            version: version_string.clone(),
                            chunk_length: chunk_length as u64,
                            content_length,
                        }
                        .emit(&app_handle);
                    },
                    || {},
                )
                .await?;
            self.cache_update_bytes(version, &bytes, &update.signature)?;
            Ok(())
        }
        .await;

        if let Err(e) = &result {
            tracing::error!("download_failed: {}", e);
            let _ = UpdateDownloadFailedEvent {
                version: version.to_string(),
            }
            .emit(self.manager.app_handle());
            return Err(result.unwrap_err());
        }

        let _ = UpdateReadyEvent {
            version: version.to_string(),
        }
        .emit(self.manager.app_handle());

        Ok(())
    }

    // Install and relaunch are one operation so no caller can install without
    // completing the required restart. The restart only happens after a
    // successful install; any earlier failure returns without restarting.
    //
    // The cached artifact may be older than the latest release (daily releases
    // outpace app opens); installing it still moves the app forward, and the
    // relaunched process downloads the newer one. The newer-than-current guard
    // is what prevents an install/relaunch loop, since the installed artifact
    // stays cached until check() prunes it.
    pub async fn install_and_relaunch(&self, version: &str) -> Result<(), crate::Error> {
        let current = self.manager.config().version.clone().unwrap_or_default();
        let is_newer = match (
            semver::Version::parse(version),
            semver::Version::parse(&current),
        ) {
            (Ok(cached), Ok(current)) => cached > current,
            _ => false,
        };
        if !is_newer {
            return Err(crate::Error::UpdateNotNewer {
                version: version.to_string(),
                current,
            });
        }

        let bytes = self.get_cached_update_bytes(version)?;

        // The check only supplies the platform install plumbing; signature
        // verification already happened when the cached bytes were downloaded.
        let updater = self.manager.updater()?;
        let mut update = updater
            .check()
            .await?
            .ok_or(crate::Error::UpdateNotAvailable)?;
        update.version = version.to_string();

        let _ = self.manager.store2().save();

        update.install(&bytes)?;
        self.manager.app_handle().restart();
    }
}

pub trait Updater2PluginExt<R: tauri::Runtime> {
    fn updater2(&self) -> Updater2<'_, R, Self>
    where
        Self: tauri::Manager<R> + Sized;
}

impl<R: tauri::Runtime, T: tauri::Manager<R>> Updater2PluginExt<R> for T {
    fn updater2(&self) -> Updater2<'_, R, Self>
    where
        Self: Sized,
    {
        Updater2 {
            manager: self,
            _runtime: std::marker::PhantomData,
        }
    }
}

fn get_updates_dir<R: tauri::Runtime, M: tauri::Manager<R>>(manager: &M) -> Option<PathBuf> {
    manager
        .app_handle()
        .path()
        .app_cache_dir()
        .ok()
        .map(|p: PathBuf| p.join("updates"))
}

fn get_cache_path<R: tauri::Runtime, M: tauri::Manager<R>>(
    manager: &M,
    version: &str,
) -> Option<PathBuf> {
    Some(get_updates_dir(manager)?.join(format!("{}.bin", version)))
}

fn get_sig_path<R: tauri::Runtime, M: tauri::Manager<R>>(
    manager: &M,
    version: &str,
) -> Option<PathBuf> {
    Some(get_updates_dir(manager)?.join(format!("{}.sig", version)))
}

// Decodes a base64 blob into the UTF-8 text it wraps. Both the embedded
// updater pubkey (tauri.conf.json) and every signature `tauri signer sign`
// produces are base64 of a multi-line minisign text file (comment line +
// key/signature line), not the raw key/signature bytes -- confirmed against
// a real `tauri signer generate`/`sign` run (22.09.2026). PublicKey::decode()
// and Signature::decode() both expect that decoded text, matching the
// private verify_signature() in tauri_plugin_updater::updater.
fn decode_base64_to_string(value: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .ok()?;
    String::from_utf8(bytes).ok()
}

// The actual signature check: pubkey and signature are both a base64 blob of
// a minisign text file (comment line + key/signature line) -- decode once to
// get that text, then hand it to PublicKey::decode()/Signature::decode(),
// exactly as tauri_plugin_updater::updater::verify_signature() does.
fn verify_bytes_against_signature(bytes: &[u8], pubkey_b64: &str, signature_b64: &str) -> bool {
    (|| -> Option<()> {
        let pubkey_text = decode_base64_to_string(pubkey_b64)?;
        let public_key = PublicKey::decode(&pubkey_text).ok()?;

        let signature_text = decode_base64_to_string(signature_b64)?;
        let signature = Signature::decode(&signature_text).ok()?;

        public_key.verify(bytes, &signature, true).ok()
    })()
    .is_some()
}

fn prune_cached_updates<R: tauri::Runtime, M: tauri::Manager<R>>(manager: &M, keep: Option<&str>) {
    if let Some(dir) = get_updates_dir(manager) {
        prune_updates_dir(&dir, keep);
    }
}

fn prune_updates_dir(dir: &Path, keep: Option<&str>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        if ext != Some("bin") && ext != Some("sig") {
            continue;
        }
        if keep.is_some() && path.file_stem().and_then(|s| s.to_str()) == keep {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => tracing::info!("pruned_cached_update: {:?}", path),
            Err(e) => tracing::warn!("failed_to_prune_cached_update: {:?}: {}", path, e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{prune_updates_dir, verify_bytes_against_signature};

    fn write_bins(dir: &std::path::Path, versions: &[&str]) {
        for v in versions {
            std::fs::write(dir.join(format!("{v}.bin")), b"update").unwrap();
        }
    }

    // Real fixture from a genuine `tauri signer generate` + `tauri signer sign`
    // run (22.09.2026, scratchpad key, discarded after) -- proves the decode
    // chain (base64 -> minisign text -> PublicKey/Signature::decode) matches
    // what the actual CLI writes, not just what we assume it writes.
    const PUBKEY_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IENBQzJDOUMyNEEzREI2NjkKUldScHRqMUt3c25DeWlBZ1cvK1FuTjVQdmc2cGhmN0Y1aGI1U2FUdVMyK3A5Y1hWcXZyRThJMWgK";
    const SIGNATURE_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVScHRqMUt3c25DeXJhTTdDN3czTkJsYXFZU0NGcDNORWpkNXBDeGtFNzVIdzZwcEdhcUlHS1RaYW9tWVBRbzZScUhRK0lmQ00xSHJnZldOaFkxSE52bk5hRkxhQWdxUlFnPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkwMDgxMjA0CWZpbGU6dGVzdC50eHQKV3FrL3dnMlJ3VnN5eVpFTWR0UWs0bkZsMGN1V3AvVkxLQ0oyMUl2T2xNeDlYYzdxMXRYa2tRUFZTdVAxUitnK0NkU0pZSHMwYTFtWEhXMnZoc3pKQUE9PQo=";
    const PAYLOAD: &[u8] = b"hallo welt test payload\n";

    #[test]
    fn verify_accepts_bytes_matching_their_own_signature() {
        assert!(verify_bytes_against_signature(
            PAYLOAD,
            PUBKEY_B64,
            SIGNATURE_B64
        ));
    }

    // The red line for F2: bytes tampered with after signing must NOT verify,
    // even against an otherwise-valid signature file for the same version --
    // this is exactly the TOCTOU an attacker would exploit by swapping the
    // cached .bin between download() and install_and_relaunch().
    #[test]
    fn verify_rejects_bytes_that_were_swapped_after_signing() {
        let mut tampered = PAYLOAD.to_vec();
        tampered[0] ^= 0x01;
        assert!(!verify_bytes_against_signature(
            &tampered,
            PUBKEY_B64,
            SIGNATURE_B64
        ));
    }

    #[test]
    fn verify_rejects_a_signature_for_different_bytes() {
        assert!(!verify_bytes_against_signature(
            b"not the signed payload",
            PUBKEY_B64,
            SIGNATURE_B64
        ));
    }

    #[test]
    fn verify_rejects_garbage_signature_file() {
        assert!(!verify_bytes_against_signature(
            PAYLOAD,
            PUBKEY_B64,
            "not a real signature"
        ));
    }

    #[test]
    fn verify_rejects_garbage_pubkey() {
        assert!(!verify_bytes_against_signature(
            PAYLOAD,
            "not a real pubkey",
            SIGNATURE_B64
        ));
    }

    #[test]
    fn prune_keeps_only_the_kept_version() {
        let dir = tempfile::tempdir().unwrap();
        write_bins(dir.path(), &["1.4.1", "1.4.2", "1.4.8"]);

        prune_updates_dir(dir.path(), Some("1.4.8"));

        assert!(dir.path().join("1.4.8.bin").exists());
        assert!(!dir.path().join("1.4.1.bin").exists());
        assert!(!dir.path().join("1.4.2.bin").exists());
    }

    #[test]
    fn prune_without_keep_removes_all_bins() {
        let dir = tempfile::tempdir().unwrap();
        write_bins(dir.path(), &["1.4.1", "1.4.2"]);

        prune_updates_dir(dir.path(), None);

        assert!(!dir.path().join("1.4.1.bin").exists());
        assert!(!dir.path().join("1.4.2.bin").exists());
    }

    #[test]
    fn prune_ignores_non_bin_files() {
        let dir = tempfile::tempdir().unwrap();
        write_bins(dir.path(), &["1.4.1"]);
        std::fs::write(dir.path().join("notes.txt"), b"keep").unwrap();

        prune_updates_dir(dir.path(), None);

        assert!(dir.path().join("notes.txt").exists());
        assert!(!dir.path().join("1.4.1.bin").exists());
    }

    // The .sig sidecar has to leave with its .bin -- an orphaned .sig for a
    // pruned version would just sit there doing nothing, but a leftover .sig
    // for a version whose .bin gets re-downloaded later would be read back
    // by verify_cached_bytes() against bytes it never actually signed.
    #[test]
    fn prune_removes_sig_sidecars_alongside_their_bin() {
        let dir = tempfile::tempdir().unwrap();
        write_bins(dir.path(), &["1.4.1", "1.4.8"]);
        std::fs::write(dir.path().join("1.4.1.sig"), b"sig").unwrap();
        std::fs::write(dir.path().join("1.4.8.sig"), b"sig").unwrap();

        prune_updates_dir(dir.path(), Some("1.4.8"));

        assert!(dir.path().join("1.4.8.sig").exists());
        assert!(!dir.path().join("1.4.1.sig").exists());
    }

    #[test]
    fn prune_missing_dir_is_noop() {
        let dir = tempfile::tempdir().unwrap();

        prune_updates_dir(&dir.path().join("does-not-exist"), Some("1.4.8"));
    }
}
