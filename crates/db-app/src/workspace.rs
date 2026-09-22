//! Lokale Arbeitsbereichs-Kennung.
//!
//! Der Fork hat keine Gegenstelle mehr, aber `workspace_id` ist auf den
//! Domaenentabellen NOT NULL und traegt jede lokal angelegte Sitzung. Die
//! Kennung lebt deshalb weiter -- als eine Zeile in `app_settings`, unter dem
//! unveraenderten Schluessel `cloudsync_workspace_binding`. Der Name bleibt,
//! weil bestehende Datenbanken ihn tragen; ein neuer Schluessel wuerde eine
//! zweite, leere Kennung anlegen und die vorhandenen Zeilen verwaisen lassen.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub const WORKSPACE_BINDING_ID: &str = "cloudsync_workspace_binding";

#[derive(Deserialize, Serialize)]
struct WorkspaceBinding {
    workspace_id: String,
    account_user_id: Option<String>,
}

/// Liest die lokale Arbeitsbereichs-Kennung oder legt sie einmalig an.
pub async fn ensure_workspace_binding(pool: &SqlitePool) -> Result<String, sqlx::Error> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;

    if let Some(value_json) =
        sqlx::query_scalar::<_, String>("SELECT value_json FROM app_settings WHERE id = ?")
            .bind(WORKSPACE_BINDING_ID)
            .fetch_optional(&mut *transaction)
            .await?
        && let Ok(binding) = serde_json::from_str::<WorkspaceBinding>(&value_json)
        && !binding.workspace_id.trim().is_empty()
    {
        transaction.commit().await?;
        return Ok(binding.workspace_id);
    }

    let binding = WorkspaceBinding {
        workspace_id: uuid::Uuid::new_v4().to_string(),
        account_user_id: None,
    };
    let value_json =
        serde_json::to_string(&binding).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    sqlx::query(
        "INSERT INTO app_settings (id, value_json) VALUES (?, ?)
         ON CONFLICT(id) DO UPDATE SET value_json = excluded.value_json",
    )
    .bind(WORKSPACE_BINDING_ID)
    .bind(value_json)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(binding.workspace_id)
}
