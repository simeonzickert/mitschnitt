use std::path::PathBuf;

use camino::Utf8PathBuf;

use anlg_storage::ObsidianVault;

pub struct Settings<'a, R: tauri::Runtime, M: tauri::Manager<R>> {
    manager: &'a M,
    _runtime: std::marker::PhantomData<fn() -> R>,
}

impl<'a, R: tauri::Runtime, M: tauri::Manager<R>> Settings<'a, R, M> {
    fn settings_base_path(&self) -> Result<PathBuf, crate::Error> {
        let bundle_id: &str = self.manager.config().identifier.as_ref();
        let path = anlg_storage::global::compute_default_base(bundle_id)
            .ok_or(anlg_storage::Error::DataDirUnavailable)?;
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub fn settings_base(&self) -> Result<Utf8PathBuf, crate::Error> {
        let path = self.settings_base_path()?;
        Utf8PathBuf::from_path_buf(path).map_err(|_| anlg_storage::Error::PathNotValidUtf8.into())
    }

    pub fn global_base(&self) -> Result<Utf8PathBuf, crate::Error> {
        self.settings_base()
    }

    pub fn settings_path(&self) -> Result<Utf8PathBuf, crate::Error> {
        let base = self.vault_base()?;
        Ok(base.join(anlg_storage::vault::SETTINGS_FILENAME))
    }

    pub fn vault_base(&self) -> Result<Utf8PathBuf, crate::Error> {
        let snapshot = self.manager.state::<crate::state::StartupSnapshot>();
        Utf8PathBuf::from_path_buf(snapshot.startup_vault_base().clone())
            .map_err(|_| anlg_storage::Error::PathNotValidUtf8.into())
    }

    pub fn resolve_startup_vault_base(&self) -> Result<PathBuf, crate::Error> {
        let settings_base = self.settings_base_path()?;
        Ok(anlg_storage::vault::resolve_base(
            &settings_base,
            &settings_base,
        ))
    }

    pub fn obsidian_vaults(&self) -> Result<Vec<ObsidianVault>, crate::Error> {
        anlg_storage::obsidian::list_vaults().map_err(Into::into)
    }

    pub fn is_empty_or_missing_dir(&self, path: Utf8PathBuf) -> Result<bool, crate::Error> {
        anlg_storage::vault::fs::is_empty_or_missing_dir(path.as_ref()).map_err(Into::into)
    }

    pub async fn load(&self) -> crate::Result<serde_json::Value> {
        let snapshot = self.manager.state::<crate::state::StartupSnapshot>();
        let legacy_base = self.settings_base_path()?;
        snapshot.load_with_legacy_fallback(&legacy_base).await
    }

    pub async fn save(&self, settings: serde_json::Value) -> crate::Result<()> {
        let snapshot = self.manager.state::<crate::state::StartupSnapshot>();
        snapshot.save(settings).await
    }

    pub fn reset(&self) -> crate::Result<()> {
        let snapshot = self.manager.state::<crate::state::StartupSnapshot>();
        snapshot.reset()
    }
}

impl<'a, R: tauri::Runtime, M: tauri::Manager<R>> Settings<'a, R, M> {
    /// Persists `new_path` as the vault base. Rust-internal only: the one
    /// caller is `apps/desktop/src-tauri/src/vault_move.rs`, which copies the
    /// vault items AND `app.db` first. Not exposed to the webview any more
    /// (ZICK-310, 26.09.2026): since `db::desktop_db_dir` follows this path,
    /// setting it without moving the database would make the next start open
    /// a fresh, empty `app.db` at the new location. The former onboarding
    /// picker (`copy_vault` + this command) was removed for that reason.
    pub async fn set_vault_base(&self, new_path: Utf8PathBuf) -> Result<(), crate::Error> {
        let settings_base = self.settings_base_path()?;
        anlg_storage::vault::persist_vault_path(&settings_base, &settings_base, new_path.as_ref())?;
        Ok(())
    }
}

pub trait SettingsPluginExt<R: tauri::Runtime> {
    fn settings(&self) -> Settings<'_, R, Self>
    where
        Self: tauri::Manager<R> + Sized;
}

impl<R: tauri::Runtime, T: tauri::Manager<R>> SettingsPluginExt<R> for T {
    fn settings(&self) -> Settings<'_, R, Self>
    where
        Self: Sized,
    {
        Settings {
            manager: self,
            _runtime: std::marker::PhantomData,
        }
    }
}
