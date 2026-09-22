use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use anlg_db_core::Db;
use sqlx::migrate::{
    AppliedMigration, Migrate, MigrateError as SqlxMigrateError, Migration, MigrationType,
};
use sqlx::{SqlSafeStr, Sqlite, SqliteConnection};

use crate::error::MigrateError;
use crate::schema::{DbSchema, MigrationScope, MigrationStep};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Copy)]
struct StepMeta {
    scope: MigrationScope,
    breaking: bool,
}

struct DbMigrateConnection {
    conn: sqlx::pool::PoolConnection<Sqlite>,
    meta_by_version: HashMap<i64, StepMeta>,
}

impl DbMigrateConnection {
    fn new(
        conn: sqlx::pool::PoolConnection<Sqlite>,
        meta_by_version: HashMap<i64, StepMeta>,
    ) -> Self {
        Self {
            conn,
            meta_by_version,
        }
    }
}

pub(crate) async fn run_migrations(
    db: &Db,
    schema: DbSchema,
    on_progress: impl FnMut(crate::MigrationProgress) + Send,
) -> Result<(), MigrateError> {
    let resolved = resolve_migrations(schema)?;
    let meta_by_version = resolved
        .iter()
        .map(|(step, migration)| {
            (
                migration.version,
                StepMeta {
                    scope: step.scope,
                    breaking: is_breaking_step(step.sql),
                },
            )
        })
        .collect();
    let migrations: Vec<_> = resolved
        .into_iter()
        .map(|(_, migration)| migration)
        .collect();

    let conn = db.pool().acquire().await?;
    let mut conn = DbMigrateConnection::new(conn, meta_by_version);
    run_direct(&migrations, &mut conn, on_progress).await?;
    Ok(())
}

const MIGRATIONS_TABLE: &str = "_sqlx_migrations";

async fn run_direct(
    migrations: &[Migration],
    conn: &mut DbMigrateConnection,
    mut on_progress: impl FnMut(crate::MigrationProgress) + Send,
) -> Result<(), MigrateError> {
    conn.lock().await?;
    conn.ensure_migrations_table(MIGRATIONS_TABLE).await?;
    ensure_schema_compat_table(&mut conn.conn).await?;

    if let Some(version) = conn.dirty_version(MIGRATIONS_TABLE).await? {
        return Err(SqlxMigrateError::Dirty(version).into());
    }

    let applied_migrations = conn.list_applied_migrations(MIGRATIONS_TABLE).await?;
    let max_known_version = migrations
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0);
    let max_applied_version = applied_migrations
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0);
    validate_applied_migrations(&applied_migrations, migrations, max_known_version)?;

    // A database written by a newer build is fine to keep using as long as
    // none of its extra migrations were marked "-- breaking"; the compat
    // floor records the newest breaking migration ever applied.
    let min_supported_version = read_min_supported_version(&mut conn.conn).await?;
    if min_supported_version > max_known_version {
        return Err(MigrateError::SchemaFromNewerApp {
            min_supported_version,
            max_known_version,
        });
    }

    let applied_migrations: HashMap<_, _> = applied_migrations
        .into_iter()
        .map(|migration| (migration.version, migration))
        .collect();

    if max_applied_version > max_known_version
        && let Some(migration) = migrations.iter().find(|migration| {
            !migration.migration_type.is_down_migration()
                && !applied_migrations.contains_key(&migration.version)
        })
    {
        return Err(MigrateError::DatabaseAhead {
            missing_version: migration.version,
            max_applied_version,
        });
    }

    let pending_total = migrations
        .iter()
        .filter(|migration| {
            !migration.migration_type.is_down_migration()
                && !applied_migrations.contains_key(&migration.version)
        })
        .count();
    let mut completed = 0;

    for migration in migrations {
        if migration.migration_type.is_down_migration() {
            continue;
        }

        match applied_migrations.get(&migration.version) {
            Some(applied_migration) => {
                if migration.checksum != applied_migration.checksum {
                    return Err(SqlxMigrateError::VersionMismatch(migration.version).into());
                }
            }
            None => {
                if completed == 0 {
                    on_progress(crate::MigrationProgress {
                        completed,
                        total: pending_total,
                    });
                }
                conn.apply(MIGRATIONS_TABLE, migration).await?;
                completed += 1;
                on_progress(crate::MigrationProgress {
                    completed,
                    total: pending_total,
                });
            }
        }
    }

    conn.unlock().await?;
    Ok(())
}

fn validate_applied_migrations(
    applied_migrations: &[AppliedMigration],
    migrations: &[Migration],
    max_known_version: i64,
) -> Result<(), SqlxMigrateError> {
    let versions: HashSet<_> = migrations
        .iter()
        .map(|migration| migration.version)
        .collect();

    for applied_migration in applied_migrations {
        if versions.contains(&applied_migration.version) {
            continue;
        }
        // Versions above everything this build knows come from a newer build
        // and are tolerated (subject to the compat floor); an unknown version
        // interleaved with known ones means a divergent history.
        if applied_migration.version <= max_known_version {
            return Err(SqlxMigrateError::VersionMissing(applied_migration.version));
        }
    }

    Ok(())
}

// A step whose leading comment block contains a "-- breaking" line makes the
// schema unreadable by builds that don't include it (e.g. dropped or renamed
// columns); applying it raises the compat floor so older builds refuse to
// open the database with a clear "update Mitschnitt" message instead of
// misbehaving on a schema they don't understand.
fn is_breaking_step(sql: &str) -> bool {
    sql.lines()
        .take_while(|line| {
            let line = line.trim();
            line.is_empty() || line.starts_with("--")
        })
        .any(|line| line.trim() == "-- breaking")
}

async fn ensure_schema_compat_table(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _anlg_schema_compat ( \
             id INTEGER PRIMARY KEY CHECK (id = 0), \
             min_supported_version INTEGER NOT NULL \
         )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO _anlg_schema_compat (id, min_supported_version) VALUES (0, 0)",
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

async fn read_min_supported_version(conn: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT min_supported_version FROM _anlg_schema_compat WHERE id = 0")
        .fetch_one(&mut *conn)
        .await
}

async fn raise_min_supported_version(
    conn: &mut SqliteConnection,
    version: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE _anlg_schema_compat \
         SET min_supported_version = MAX(min_supported_version, ?1) \
         WHERE id = 0",
    )
    .bind(version)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

fn resolve_migrations(
    schema: DbSchema,
) -> Result<Vec<(&'static MigrationStep, Migration)>, MigrateError> {
    let mut seen_versions = HashMap::new();
    let mut migrations = Vec::with_capacity(schema.steps.len());

    for step in schema.steps {
        let (version, description) = parse_step_id(step.id)?;

        if let Some(first_step_id) = seen_versions.insert(version, step.id) {
            return Err(MigrateError::DuplicateStepVersion {
                version,
                first_step_id,
                second_step_id: step.id,
            });
        }

        migrations.push((
            step,
            Migration::new(
                version,
                Cow::Borrowed(description),
                MigrationType::Simple,
                step.sql.into_sql_str(),
                step.sql.starts_with("-- no-transaction"),
            ),
        ));
    }

    migrations.sort_by_key(|(_, migration)| migration.version);
    Ok(migrations)
}

fn parse_step_id(step_id: &'static str) -> Result<(i64, &'static str), MigrateError> {
    let Some((version, description)) = step_id.split_once('_') else {
        return Err(MigrateError::InvalidStepId { step_id });
    };

    let version = version
        .parse::<i64>()
        .ok()
        .filter(|version| *version > 0)
        .ok_or(MigrateError::InvalidStepId { step_id })?;

    if description.is_empty() {
        return Err(MigrateError::InvalidStepId { step_id });
    }

    Ok((version, description))
}

impl Migrate for DbMigrateConnection {
    fn create_schema_if_not_exists<'e>(
        &'e mut self,
        schema_name: &'e str,
    ) -> BoxFuture<'e, Result<(), SqlxMigrateError>> {
        <SqliteConnection as Migrate>::create_schema_if_not_exists(&mut *self.conn, schema_name)
    }

    fn ensure_migrations_table<'e>(
        &'e mut self,
        table_name: &'e str,
    ) -> BoxFuture<'e, Result<(), SqlxMigrateError>> {
        <SqliteConnection as Migrate>::ensure_migrations_table(&mut *self.conn, table_name)
    }

    fn dirty_version<'e>(
        &'e mut self,
        table_name: &'e str,
    ) -> BoxFuture<'e, Result<Option<i64>, SqlxMigrateError>> {
        <SqliteConnection as Migrate>::dirty_version(&mut *self.conn, table_name)
    }

    fn list_applied_migrations<'e>(
        &'e mut self,
        table_name: &'e str,
    ) -> BoxFuture<'e, Result<Vec<AppliedMigration>, SqlxMigrateError>> {
        <SqliteConnection as Migrate>::list_applied_migrations(&mut *self.conn, table_name)
    }

    fn lock(&mut self) -> BoxFuture<'_, Result<(), SqlxMigrateError>> {
        <SqliteConnection as Migrate>::lock(&mut *self.conn)
    }

    fn unlock(&mut self) -> BoxFuture<'_, Result<(), SqlxMigrateError>> {
        <SqliteConnection as Migrate>::unlock(&mut *self.conn)
    }

    fn apply<'e>(
        &'e mut self,
        table_name: &'e str,
        migration: &'e Migration,
    ) -> BoxFuture<'e, Result<Duration, SqlxMigrateError>> {
        Box::pin(async move {
            let meta = self
                .meta_by_version
                .get(&migration.version)
                .copied()
                .unwrap_or(StepMeta {
                    scope: MigrationScope::Plain,
                    breaking: false,
                });

            // Raise the floor before the schema change lands: crashing in
            // between locks older builds out of a schema that is still
            // compatible, while the reverse order would let them open one
            // that is not.
            if meta.breaking {
                raise_min_supported_version(&mut self.conn, migration.version)
                    .await
                    .map_err(SqlxMigrateError::from)?;
            }

            match meta.scope {
                MigrationScope::Plain => {
                    <SqliteConnection as Migrate>::apply(&mut *self.conn, table_name, migration)
                        .await
                }
            }
        })
    }

    fn revert<'e>(
        &'e mut self,
        table_name: &'e str,
        migration: &'e Migration,
    ) -> BoxFuture<'e, Result<Duration, SqlxMigrateError>> {
        <SqliteConnection as Migrate>::revert(&mut *self.conn, table_name, migration)
    }
}
