use sqlx::SqlitePool;

use crate::{TemplateRow, UpsertTemplate};

pub async fn get_template(pool: &SqlitePool, id: &str) -> Result<Option<TemplateRow>, sqlx::Error> {
    sqlx::query_as::<_, TemplateRow>("SELECT * FROM templates WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn list_templates(pool: &SqlitePool) -> Result<Vec<TemplateRow>, sqlx::Error> {
    sqlx::query_as::<_, TemplateRow>("SELECT * FROM templates ORDER BY id")
        .fetch_all(pool)
        .await
}

pub async fn upsert_template(
    pool: &SqlitePool,
    input: UpsertTemplate<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO templates \
         (id, title, description, pinned, pin_order, category, targets_json, sections_json, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')) \
         ON CONFLICT(id) DO UPDATE SET \
           title = excluded.title, \
           description = excluded.description, \
           pinned = excluded.pinned, \
           pin_order = excluded.pin_order, \
           category = excluded.category, \
           targets_json = excluded.targets_json, \
           sections_json = excluded.sections_json, \
           updated_at = excluded.updated_at",
    )
    .bind(input.id)
    .bind(input.title)
    .bind(input.description)
    .bind(input.pinned)
    .bind(input.pin_order)
    .bind(input.category)
    .bind(input.targets_json)
    .bind(input.sections_json)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn insert_template_if_missing(
    pool: &SqlitePool,
    input: UpsertTemplate<'_>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO templates \
         (id, title, description, pinned, pin_order, category, targets_json, sections_json, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')) \
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(input.id)
    .bind(input.title)
    .bind(input.description)
    .bind(input.pinned)
    .bind(input.pin_order)
    .bind(input.category)
    .bind(input.targets_json)
    .bind(input.sections_json)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Loescht die Vorlage -- und setzt den Standard auf "Auto" zurueck, wenn er
/// auf genau diese Vorlage zeigte (C8, Review 02.09.2026). '""' ist dasselbe,
/// was das Abwaehlen im Formular schreibt (templates/template-form.tsx); der
/// React-Hook useDeleteTemplate tut es ebenso, hier gilt die Invariante auch
/// fuer jeden nativen Aufrufer. Bedingt (WHERE value_json = json_quote(?)),
/// damit ein gleichzeitiger Wechsel auf eine andere Vorlage nie ueberschrieben
/// wird; eine Loeschung, die die Wahl nicht trifft, fasst updated_at nicht an.
pub async fn delete_template(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("DELETE FROM templates WHERE id = ?")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "UPDATE app_settings
         SET value_json = '\"\"',
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE id = 'selected_template_id' AND value_json = json_quote(?)",
    )
    .bind(id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(())
}
