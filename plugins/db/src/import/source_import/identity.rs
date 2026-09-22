use sqlx::SqlitePool;

/// Die Kennung dieser Installation. Fremde Sitzungen tragen die Kennung ihres
/// Arbeitsbereichs; ohne Umschluesselung erscheinen sie in keiner Liste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalIdentity {
    pub workspace_id: String,
    pub owner_user_id: String,
}

/// Gemessen an der gemessenen Installation (10.09.2026): `workspace_id` und
/// `owner_user_id` tragen denselben Wert, und `app_settings` haelt ihn unter
/// `cloudsync_workspace_binding`. Eine frische Installation hat weder Zeilen
/// noch diesen Schluessel -- dann wird eine Kennung erzeugt.
pub async fn resolve_local_identity(pool: &SqlitePool) -> crate::Result<LocalIdentity> {
    if let Some(binding) = binding_workspace_id(pool).await? {
        let owner = most_common(pool, "sessions", "owner_user_id")
            .await?
            .unwrap_or_else(|| binding.clone());
        return Ok(LocalIdentity {
            workspace_id: binding,
            owner_user_id: owner,
        });
    }

    for (table, column) in [
        ("sessions", "workspace_id"),
        ("humans", "workspace_id"),
        ("session_documents", "workspace_id"),
    ] {
        if let Some(workspace_id) = most_common(pool, table, column).await? {
            let owner = most_common(pool, "sessions", "owner_user_id")
                .await?
                .unwrap_or_else(|| workspace_id.clone());
            return Ok(LocalIdentity {
                workspace_id,
                owner_user_id: owner,
            });
        }
    }

    let fresh = uuid::Uuid::new_v4().to_string();
    Ok(LocalIdentity {
        workspace_id: fresh.clone(),
        owner_user_id: fresh,
    })
}

async fn binding_workspace_id(pool: &SqlitePool) -> crate::Result<Option<String>> {
    let raw = sqlx::query_scalar::<_, String>(
        "SELECT value_json FROM app_settings WHERE id = 'cloudsync_workspace_binding'",
    )
    .fetch_optional(pool)
    .await?;

    let Some(raw) = raw else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<serde_json::Value>(&raw).unwrap_or(serde_json::Value::Null);
    Ok(parsed
        .get("workspace_id")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned))
}

/// Tabellen- und Spaltenname stammen ausschliesslich aus der festen Liste
/// oben, nie aus Eingaben -- deshalb ist das Zusammensetzen hier ungefaehrlich.
async fn most_common(
    pool: &SqlitePool,
    table: &str,
    column: &str,
) -> crate::Result<Option<String>> {
    let sql = format!(
        "SELECT {column} FROM {table} \
         WHERE {column} <> '' AND deleted_at IS NULL \
         GROUP BY {column} ORDER BY COUNT(*) DESC LIMIT 1"
    );
    Ok(sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .fetch_optional(pool)
        .await?)
}
