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
    /// Copies the vault items (`fs.rs`'s `VAULT_DIRECTORIES`/`VAULT_FILES`)
    /// to `new_path` without touching the old location or persisting
    /// anything -- used only by onboarding's storage-location picker
    /// (`folder-location.tsx`), where the caller still calls `set_vault_base`
    /// itself afterward.
    ///
    /// Like the general-settings `move_vault` this plugin used to expose
    /// (moved to `apps/desktop/src-tauri/src/vault_move.rs` -- see that
    /// module's doc comment), this does NOT copy `app.db`: at onboarding time
    /// the database already exists and is already open at the default
    /// location (`main()` opens it before any UI runs), so picking a custom
    /// vault folder here leaves the database behind exactly the way the
    /// general-settings button used to. Onboarding was out of scope for the
    /// audit that found and fixed that in the general-settings path; it is a
    /// known, disclosed gap here, not an oversight.
    pub async fn copy_vault(&self, new_path: Utf8PathBuf) -> Result<(), crate::Error> {
        let old_vault_base = self.vault_base()?;

        if new_path == old_vault_base {
            return Ok(());
        }

        anlg_storage::vault::validate_vault_base_change(
            old_vault_base.as_ref(),
            new_path.as_ref(),
        )?;
        anlg_storage::vault::ensure_vault_dir(new_path.as_ref())?;
        anlg_storage::vault::fs::copy_vault_items(old_vault_base.as_ref(), new_path.as_ref())
            .await?;

        Ok(())
    }

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
