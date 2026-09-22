use anlg_db_core::Db;
use anlg_db_execute::{DbExecutor, ProxyQueryMethod, ProxyQueryResult};
use anlg_db_reactive::{LiveQueryRuntime, QueryEventSink, SubscriptionRegistration};
use tauri::ipc::Channel;

use crate::{QueryEvent, Result, TransactionStatement};

mod open;

pub use open::{open_app_db, open_app_db_unmigrated};

#[derive(Clone)]
pub struct QueryEventChannel(Channel<QueryEvent>);

impl QueryEventChannel {
    pub fn new(channel: Channel<QueryEvent>) -> Self {
        Self(channel)
    }
}

impl QueryEventSink for QueryEventChannel {
    fn send_result(&self, rows: Vec<serde_json::Value>) -> std::result::Result<(), String> {
        self.0
            .send(QueryEvent::Result(rows))
            .map_err(|error| error.to_string())
    }

    fn send_error(&self, error: String) -> std::result::Result<(), String> {
        self.0
            .send(QueryEvent::Error(error))
            .map_err(|error| error.to_string())
    }
}

struct ExplicitRollbackTransaction {
    transaction: Option<sqlx::Transaction<'static, sqlx::Sqlite>>,
}

impl ExplicitRollbackTransaction {
    fn new(transaction: sqlx::Transaction<'static, sqlx::Sqlite>) -> Self {
        Self {
            transaction: Some(transaction),
        }
    }

    fn connection(&mut self) -> &mut sqlx::SqliteConnection {
        &mut *self
            .transaction
            .as_mut()
            .expect("transaction should be present")
    }

    async fn commit(mut self) -> std::result::Result<(), sqlx::Error> {
        self.transaction
            .take()
            .expect("transaction should be present")
            .commit()
            .await
    }

    async fn rollback(mut self) -> std::result::Result<(), sqlx::Error> {
        self.transaction
            .take()
            .expect("transaction should be present")
            .rollback()
            .await
    }
}

impl Drop for ExplicitRollbackTransaction {
    fn drop(&mut self) {
        let Some(transaction) = self.transaction.take() else {
            return;
        };

        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::error!("sqlite_transaction_cancelled_without_async_runtime");
            drop(transaction);
            return;
        };

        runtime.spawn(async move {
            if let Err(error) = transaction.rollback().await {
                tracing::error!(%error, "sqlite_cancelled_transaction_rollback_failed");
            }
        });
    }
}

pub struct PluginDbRuntime {
    db: std::sync::Arc<Db>,
    schema_ready: tokio::sync::OnceCell<()>,
    startup_tx: tokio::sync::watch::Sender<Option<std::result::Result<(), String>>>,
    startup_status: std::sync::RwLock<crate::StartupStatus>,
    synced_write_barrier: tokio::sync::RwLock<()>,
    executor: DbExecutor,
    live_query_runtime: LiveQueryRuntime<QueryEventChannel>,
    /// Der eigene Datenordner. Wird beim Plugin-Setup gesetzt, wo es einen
    /// AppHandle gibt -- Kommandos brauchen deshalb keinen eigenen
    /// Runtime-Parameter, den `collect_commands!` nicht aufloesen kann.
    vault_base: std::sync::OnceLock<std::path::PathBuf>,
    #[cfg(test)]
    pause_transaction_after_begin: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    transaction_started: tokio::sync::Notify,
}

impl PluginDbRuntime {
    pub fn new(db: std::sync::Arc<Db>) -> Self {
        // LiveQueryRuntime::new spawnt seinen Dispatcher per tokio::spawn und
        // braucht dafuer einen Runtime-Kontext auf DIESEM Thread. Der Plugin-
        // Setup laeuft aber auf dem Main-Thread, der die Tokio-Runtime vor
        // Builder::build() bewusst verlassen hat (siehe desktop main). Bis zum
        // Cloud-Ausbau (01.09.2026) lieferte den Kontext nebenbei der Enter-
        // Guard der cloudsync-Runtime; mit ihm verschwand er, und die App
        // panickte beim Start ("there is no reactor running"). Deshalb hier
        // ausdruecklich die App-Runtime betreten, sofern nicht ohnehin eine
        // aktiv ist (Tests laufen unter #[tokio::test]).
        let app_runtime = tokio::runtime::Handle::try_current()
            .is_err()
            .then(tauri::async_runtime::handle);
        let _enter = app_runtime.as_ref().map(|handle| handle.inner().enter());
        let (startup_tx, _) = tokio::sync::watch::channel(None);
        Self {
            db: std::sync::Arc::clone(&db),
            schema_ready: tokio::sync::OnceCell::new(),
            startup_tx,
            startup_status: std::sync::RwLock::new(crate::StartupStatus::for_phase(
                crate::StartupPhase::PreparingDatabase,
            )),
            synced_write_barrier: tokio::sync::RwLock::new(()),
            executor: DbExecutor::new(std::sync::Arc::clone(&db)),
            live_query_runtime: LiveQueryRuntime::new(db),
            vault_base: std::sync::OnceLock::new(),
            #[cfg(test)]
            pause_transaction_after_begin: Default::default(),
            #[cfg(test)]
            transaction_started: Default::default(),
        }
    }

    pub fn pool(&self) -> &sqlx::SqlitePool {
        self.db.pool()
    }

    pub fn set_vault_base(&self, base: std::path::PathBuf) {
        let _ = self.vault_base.set(base);
    }

    pub fn vault_base(&self) -> crate::Result<&std::path::Path> {
        self.vault_base
            .get()
            .map(std::path::PathBuf::as_path)
            .ok_or_else(|| std::io::Error::other("Datenordner steht noch nicht fest").into())
    }

    #[cfg(test)]
    pub(crate) fn pause_next_transaction_after_begin(&self) {
        self.pause_transaction_after_begin
            .store(true, std::sync::atomic::Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) async fn wait_for_transaction_after_begin(&self) {
        self.transaction_started.notified().await;
    }

    pub async fn synced_write_guard(&self) -> tokio::sync::RwLockReadGuard<'_, ()> {
        self.synced_write_barrier.read().await
    }

    pub(crate) async fn ensure_app_schema(&self) -> Result<()> {
        self.schema_ready
            .get_or_try_init(|| async {
                anlg_db_app::prepare_schema_with_progress(self.db.as_ref(), |progress| {
                    if progress.completed < progress.total {
                        self.set_startup_status_if_running(crate::StartupStatus {
                            phase: crate::StartupPhase::MigratingDatabase,
                            migration_current: Some(
                                u32::try_from(progress.completed + 1).unwrap_or(u32::MAX),
                            ),
                            migration_total: Some(
                                u32::try_from(progress.total).unwrap_or(u32::MAX),
                            ),
                        });
                    } else {
                        self.set_startup_status_if_running(crate::StartupStatus::for_phase(
                            crate::StartupPhase::PreparingDatabase,
                        ));
                    }
                })
                .await
            })
            .await?;
        Ok(())
    }

    pub(crate) fn set_startup_status_if_running(&self, status: crate::StartupStatus) {
        let mut current = self.startup_status.write().unwrap();
        if matches!(
            current.phase,
            crate::StartupPhase::Ready | crate::StartupPhase::Failed
        ) {
            return;
        }
        *current = status;
    }
    fn set_startup_status(&self, status: crate::StartupStatus) {
        *self.startup_status.write().unwrap() = status;
    }

    pub fn startup_status(&self) -> crate::StartupStatus {
        self.startup_status.read().unwrap().clone()
    }

    pub(crate) fn finish_startup(&self, result: std::result::Result<(), String>) {
        let phase = if result.is_ok() {
            crate::StartupPhase::Ready
        } else {
            crate::StartupPhase::Failed
        };
        self.set_startup_status(crate::StartupStatus::for_phase(phase));
        self.startup_tx.send_replace(Some(result));
    }

    pub async fn wait_until_ready(&self) -> Result<()> {
        let mut rx = self.startup_tx.subscribe();
        let outcome = rx
            .wait_for(|value| value.is_some())
            .await
            .map_err(|_| std::io::Error::other("database startup was interrupted"))?
            .clone();
        match outcome {
            Some(Ok(())) => Ok(()),
            Some(Err(message)) => Err(std::io::Error::other(message).into()),
            None => unreachable!("waited until startup reported a result"),
        }
    }

    pub async fn execute(
        &self,
        sql: String,
        params: Vec<serde_json::Value>,
    ) -> Result<Vec<serde_json::Value>> {
        let _write_guard = self.synced_write_barrier.read().await;
        self.ensure_app_schema().await?;
        Ok(self.executor.execute(sql, params).await?)
    }

    pub async fn execute_transaction(
        &self,
        statements: Vec<TransactionStatement>,
    ) -> Result<Vec<u64>> {
        let _write_guard = self.synced_write_barrier.read().await;
        self.ensure_app_schema().await?;
        let mut transaction =
            ExplicitRollbackTransaction::new(self.db.pool().begin_with("BEGIN IMMEDIATE").await?);
        #[cfg(test)]
        if self
            .pause_transaction_after_begin
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            self.transaction_started.notify_one();
            std::future::pending::<()>().await;
        }
        let mut rows_affected = Vec::with_capacity(statements.len());

        for (statement_index, statement) in statements.into_iter().enumerate() {
            let result = match bind_params(
                sqlx::query(sqlx::AssertSqlSafe(statement.sql.as_str())),
                &statement.params,
            )
            .execute(transaction.connection())
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    if let Err(rollback_error) = transaction.rollback().await {
                        tracing::error!(
                            %rollback_error,
                            "sqlite_failed_transaction_rollback_failed"
                        );
                    }
                    return Err(error.into());
                }
            };
            let actual = result.rows_affected();
            if let Some(expected) = statement.expected_rows_affected
                && actual != expected
            {
                let error = crate::Error::UnexpectedRowsAffected {
                    statement_index,
                    expected,
                    actual,
                };
                if let Err(rollback_error) = transaction.rollback().await {
                    tracing::error!(
                        %rollback_error,
                        "sqlite_mismatched_transaction_rollback_failed"
                    );
                }
                return Err(error);
            }
            rows_affected.push(actual);
        }

        transaction.commit().await?;
        Ok(rows_affected)
    }

    pub async fn execute_proxy(
        &self,
        sql: String,
        params: Vec<serde_json::Value>,
        method: ProxyQueryMethod,
    ) -> Result<ProxyQueryResult> {
        let _write_guard = self.synced_write_barrier.read().await;
        self.ensure_app_schema().await?;
        Ok(self.executor.execute_proxy(sql, params, method).await?)
    }

    pub async fn cleanup_legacy_files(&self) -> Result<crate::LegacyCleanupResult> {
        let _write_guard = self.synced_write_barrier.read().await;
        crate::import::cleanup_legacy_files(self.db.pool(), self.vault_base().ok()).await
    }

    pub async fn rerun_legacy_import(&self, dry_run: bool) -> Result<String> {
        let _write_guard = self.synced_write_barrier.read().await;
        crate::import::rerun_legacy_import(self.db.pool(), dry_run).await
    }

    pub async fn subscribe(
        &self,
        sql: String,
        params: Vec<serde_json::Value>,
        sink: QueryEventChannel,
    ) -> Result<SubscriptionRegistration> {
        self.ensure_app_schema().await?;
        Ok(self.live_query_runtime.subscribe(sql, params, sink).await?)
    }

    pub async fn unsubscribe(&self, subscription_id: &str) -> anlg_db_reactive::Result<()> {
        self.live_query_runtime.unsubscribe(subscription_id).await
    }
}

fn bind_params<'q>(
    mut query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
    params: &[serde_json::Value],
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments> {
    for param in params {
        query = match param {
            serde_json::Value::Null => query.bind(None::<String>),
            serde_json::Value::Bool(value) => query.bind(*value),
            serde_json::Value::Number(value) => {
                if let Some(integer) = value.as_i64() {
                    query.bind(integer)
                } else {
                    query.bind(value.as_f64().unwrap_or_default())
                }
            }
            serde_json::Value::String(value) => query.bind(value.clone()),
            other => query.bind(other.to_string()),
        };
    }

    query
}

#[cfg(test)]
mod tests;
