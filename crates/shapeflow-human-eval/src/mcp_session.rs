use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result as AnyhowResult};
use rmcp::{
    model::GetExtensions,
    model::{ClientJsonRpcMessage, ClientRequest, ServerJsonRpcMessage},
    serve_server,
    transport::{
        WorkerTransport,
        streamable_http_server::session::{
            ServerSseMessage, SessionId, SessionManager,
            local::{
                EventId, LocalSessionHandle, LocalSessionWorker, SessionConfig,
                create_local_session,
            },
        },
    },
};
use serde_json::{from_str, to_string};
use sqlx::{PgPool, Row, query};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use crate::{db::DbPool, mcp::HumanEvalMcpServer};

#[derive(Debug)]
pub struct McpSessionManagerError(anyhow::Error);

impl std::fmt::Display for McpSessionManagerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for McpSessionManagerError {}

type McpSessionManagerResult<T> = std::result::Result<T, McpSessionManagerError>;

const MCP_IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MCP_IDLE_REAPER_INTERVAL: Duration = Duration::from_secs(60);

pub struct McpSessionManager {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<LocalSessionHandle>>>>,
    session_activity: Arc<RwLock<HashMap<SessionId, Instant>>>,
    session_config: SessionConfig,
    pg_pool: Option<PgPool>,
    rehydrate_lock: Arc<Mutex<()>>,
    db_pool: DbPool,
}

impl McpSessionManager {
    pub fn new(pool: &DbPool) -> Self {
        let pg_pool = match pool {
            DbPool::Postgres(pg_pool) => Some(pg_pool.clone()),
            DbPool::Sqlite(_) => None,
        };
        let session_config = SessionConfig::default();

        let manager = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_activity: Arc::new(RwLock::new(HashMap::new())),
            session_config,
            pg_pool,
            rehydrate_lock: Arc::new(Mutex::new(())),
            db_pool: pool.clone(),
        };
        manager.spawn_idle_reaper();
        manager
    }

    fn spawn_idle_reaper(&self) {
        let manager = self.clone();
        tokio::spawn(async move {
            loop {
                sleep(MCP_IDLE_REAPER_INTERVAL).await;
                manager.reap_idle_sessions().await;
            }
        });
    }

    async fn touch_session_activity(&self, id: &SessionId) {
        self.session_activity
            .write()
            .await
            .insert(id.clone(), Instant::now());
    }

    async fn reap_idle_sessions(&self) {
        let now = Instant::now();
        let stale_session_ids: Vec<SessionId> = {
            let activity = self.session_activity.read().await;
            activity
                .iter()
                .filter_map(|(id, last_seen)| {
                    if now.duration_since(*last_seen) >= MCP_IDLE_TIMEOUT {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        for session_id in stale_session_ids {
            tracing::info!("closing idle MCP session {session_id} after inactivity timeout");
            if let Err(error) = self.close_session_internal(&session_id).await {
                tracing::warn!("failed to close idle MCP session {session_id}: {error}");
            }
        }
    }

    fn get_postgres_pool(&self) -> AnyhowResult<&PgPool> {
        self.pg_pool
            .as_ref()
            .context("postgres-backed MCP session persistence is unavailable")
    }

    async fn cache_session(
        &self,
        id: SessionId,
        handle: LocalSessionHandle,
    ) -> (Arc<LocalSessionHandle>, bool) {
        let handle = Arc::new(handle);
        let mut sessions = self.sessions.write().await;
        if let Some(existing) = sessions.get(&id) {
            return (existing.clone(), false);
        }

        sessions.insert(id.clone(), handle.clone());
        drop(sessions);
        self.touch_session_activity(&id).await;
        (handle, true)
    }

    async fn cached_session(&self, id: &SessionId) -> Option<Arc<LocalSessionHandle>> {
        self.sessions.read().await.get(id).cloned()
    }

    async fn remove_session(&self, id: &SessionId) -> Option<Arc<LocalSessionHandle>> {
        let removed = self.sessions.write().await.remove(id);
        if removed.is_some() {
            self.session_activity.write().await.remove(id);
        }
        removed
    }

    fn spawn_running_service(&self, id: SessionId, transport: WorkerTransport<LocalSessionWorker>) {
        let manager = self.clone();
        tokio::spawn(async move {
            match serve_server(
                HumanEvalMcpServer::new_with_pool(manager.db_pool.clone()),
                transport,
            )
            .await
            {
                Ok(service) => {
                    if let Err(error) = service.waiting().await {
                        tracing::error!(
                            "MCP service for session {id} exited with join error: {error}"
                        );
                    }
                }
                Err(error) => {
                    tracing::error!(
                        "failed to start MCP service for rehydrated session {id}: {error}"
                    );
                }
            }
            manager.remove_session(&id).await;
        });
    }

    async fn ensure_persisted_row_for_create(&self, session_id: &SessionId) -> AnyhowResult<()> {
        let pool = self.get_postgres_pool()?;
        let sql = r#"
            INSERT INTO mcp_http_sessions (
                session_id,
                initialized,
                closed,
                initialize_request_json,
                rehydrate_count,
                last_seen_at
            ) VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (session_id) DO UPDATE
                SET initialized = EXCLUDED.initialized,
                    closed = EXCLUDED.closed,
                    last_seen_at = NOW(),
                    updated_at = NOW()"#;
        query(sql)
            .bind(session_id.as_ref())
            .bind(false)
            .bind(false)
            .bind::<Option<String>>(None)
            .bind(0i32)
            .execute(pool)
            .await
            .context("failed to upsert MCP session metadata during session create")?;
        Ok(())
    }

    async fn persist_initialize_request(
        &self,
        session_id: &SessionId,
        initialize_request_json: &str,
    ) -> AnyhowResult<()> {
        let pool = self.get_postgres_pool()?;
        let sql = r#"
            UPDATE mcp_http_sessions
                SET initialize_request_json = $1,
                    initialized = TRUE,
                    closed = FALSE,
                    updated_at = NOW(),
                    last_seen_at = NOW()
              WHERE session_id = $2"#;
        let update = query(sql)
            .bind(initialize_request_json)
            .bind(session_id.as_ref())
            .execute(pool)
            .await
            .context("failed to persist MCP initialize request")?;
        if update.rows_affected() != 1 {
            anyhow::bail!("session {session_id} was not found during initialize persistence");
        }
        Ok(())
    }

    async fn session_has_persisted_state(&self, session_id: &SessionId) -> AnyhowResult<bool> {
        let pool = self.get_postgres_pool()?;
        let sql = r#"
            SELECT 1
            FROM mcp_http_sessions
            WHERE session_id = $1
              AND initialized = TRUE
              AND closed = FALSE
              AND initialize_request_json IS NOT NULL
            LIMIT 1"#;
        let row = query(sql)
            .bind(session_id.as_ref())
            .fetch_optional(pool)
            .await
            .context("failed to check persisted MCP session state")?;
        Ok(row.is_some())
    }

    async fn load_rehydrated_initialize_payload(
        &self,
        session_id: &SessionId,
    ) -> AnyhowResult<String> {
        let pool = self.get_postgres_pool()?;
        let sql = r#"
            SELECT initialize_request_json
            FROM mcp_http_sessions
            WHERE session_id = $1
              AND initialized = TRUE
              AND closed = FALSE
            LIMIT 1"#;
        let row = query(sql)
            .bind(session_id.as_ref())
            .fetch_optional(pool)
            .await
            .context("failed to load persisted MCP session")?;
        let row = row.context("session not found for MCP rehydrate")?;
        let initialize_request_json: Option<String> = row
            .try_get("initialize_request_json")
            .context("reading initialize payload from DB")?;
        initialize_request_json.ok_or_else(|| {
            anyhow::anyhow!("session {session_id} is missing persisted initialize payload")
        })
    }

    async fn increment_rehydrate_count(&self, session_id: &SessionId) -> AnyhowResult<()> {
        let pool = self.get_postgres_pool()?;
        let sql = r#"
            UPDATE mcp_http_sessions
            SET rehydrate_count = rehydrate_count + 1,
                last_seen_at = NOW(),
                updated_at = NOW()
            WHERE session_id = $1
              AND initialized = TRUE
              AND closed = FALSE
        "#;
        let update = query(sql)
            .bind(session_id.as_ref())
            .execute(pool)
            .await
            .context("failed to update MCP rehydrate metadata")?;
        if update.rows_affected() != 1 {
            anyhow::bail!("session {session_id} was not eligible for rehydrate");
        }
        Ok(())
    }

    async fn mark_session_closed(&self, session_id: &SessionId) -> AnyhowResult<()> {
        if self.pg_pool.is_none() {
            return Ok(());
        }
        let pool = self.get_postgres_pool()?;
        query(
            r#"
            UPDATE mcp_http_sessions
            SET closed = TRUE,
                updated_at = NOW(),
                last_seen_at = NOW()
            WHERE session_id = $1"#,
        )
        .bind(session_id.as_ref())
        .execute(pool)
        .await
        .context("failed to mark MCP session as closed")?;
        Ok(())
    }

    async fn update_last_seen(&self, session_id: &SessionId) -> AnyhowResult<()> {
        let pool = self.get_postgres_pool()?;
        query(
            r#"UPDATE mcp_http_sessions
                SET last_seen_at = NOW(),
                    updated_at = NOW()
              WHERE session_id = $1"#,
        )
        .bind(session_id.as_ref())
        .execute(pool)
        .await
        .context("failed to update MCP session last_seen_at")?;
        Ok(())
    }

    async fn get_or_rehydrate_session(
        &self,
        session_id: &SessionId,
    ) -> AnyhowResult<Arc<LocalSessionHandle>> {
        if let Some(handle) = self.cached_session(session_id).await {
            self.touch_session_activity(session_id).await;
            if self.pg_pool.is_some() {
                self.update_last_seen(session_id).await?;
            }
            return Ok(handle);
        }

        if self.pg_pool.is_none() {
            anyhow::bail!("session {session_id} not found");
        }

        let _guard = self.rehydrate_lock.lock().await;
        if let Some(handle) = self.cached_session(session_id).await {
            self.update_last_seen(session_id).await?;
            return Ok(handle);
        }

        let initialize_request_json = self.load_rehydrated_initialize_payload(session_id).await?;
        let initialize_request: ClientJsonRpcMessage = from_str(&initialize_request_json)
            .context("failed to deserialize persisted initialize request")?;

        let (handle, should_initialize) = {
            let (handle, worker) =
                create_local_session(session_id.clone(), self.session_config.clone());
            let transport = WorkerTransport::spawn(worker);
            let (handle, is_new_session) = self.cache_session(session_id.clone(), handle).await;
            if is_new_session {
                self.spawn_running_service(session_id.clone(), transport);
            } else {
                drop(transport);
            }
            (handle, is_new_session)
        };

        if should_initialize {
            if let Err(error) = handle.initialize(initialize_request).await {
                let _ = self.remove_session(session_id).await;
                let _ = handle.close().await;
                return Err(anyhow::Error::from(error)
                    .context("failed to replay persisted initialize request"));
            }
            self.increment_rehydrate_count(session_id).await?;
            tracing::debug!("rehydrated MCP session {session_id}");
        }
        self.touch_session_activity(session_id).await;

        Ok(handle)
    }

    async fn close_session_internal(&self, id: &SessionId) -> AnyhowResult<()> {
        if let Some(handle) = self.remove_session(id).await {
            if let Err(error) = handle.close().await {
                tracing::debug!(
                    "ignoring MCP handle close error for session {id} (likely already closed): {error}"
                );
            }
        }
        self.mark_session_closed(id).await?;
        Ok(())
    }
}

fn strip_initialize_extensions(
    mut message: ClientJsonRpcMessage,
) -> AnyhowResult<ClientJsonRpcMessage> {
    match &mut message {
        ClientJsonRpcMessage::Request(request) => {
            if !matches!(request.request, ClientRequest::InitializeRequest(_)) {
                anyhow::bail!("expected initialize request");
            }
            request.request.extensions_mut().clear();
            Ok(message)
        }
        _ => anyhow::bail!("expected initialize request"),
    }
}

fn mcp_error(error: impl Into<anyhow::Error>) -> McpSessionManagerError {
    McpSessionManagerError(error.into())
}

impl Clone for McpSessionManager {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
            session_activity: Arc::clone(&self.session_activity),
            session_config: self.session_config.clone(),
            pg_pool: self.pg_pool.clone(),
            rehydrate_lock: Arc::clone(&self.rehydrate_lock),
            db_pool: self.db_pool.clone(),
        }
    }
}

impl SessionManager for McpSessionManager {
    type Error = McpSessionManagerError;
    type Transport = WorkerTransport<LocalSessionWorker>;

    async fn create_session(&self) -> McpSessionManagerResult<(SessionId, Self::Transport)> {
        let session_id: SessionId = Uuid::now_v7().to_string().into();
        let (handle, worker) =
            create_local_session(session_id.clone(), self.session_config.clone());
        let (_handle, _) = self.cache_session(session_id.clone(), handle).await;
        self.touch_session_activity(&session_id).await;
        if self.pg_pool.is_some() {
            if let Err(error) = self.ensure_persisted_row_for_create(&session_id).await {
                drop(_handle);
                let _ = self.remove_session(&session_id).await;
                return Err(mcp_error(error));
            }
        }

        Ok((session_id, WorkerTransport::spawn(worker)))
    }

    async fn initialize_session(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> McpSessionManagerResult<ServerJsonRpcMessage> {
        let handle = self
            .cached_session(id)
            .await
            .ok_or_else(|| anyhow::anyhow!("session {id} not found for initialize"))
            .map_err(mcp_error)?;
        let sanitized = strip_initialize_extensions(message.clone()).map_err(mcp_error)?;
        let response = handle
            .initialize(message)
            .await
            .context("failed to initialize MCP session")
            .map_err(mcp_error)?;
        if self.pg_pool.is_some() {
            let request_json = to_string(&sanitized)
                .context("failed to serialize initialize request")
                .map_err(mcp_error)?;
            self.persist_initialize_request(id, &request_json)
                .await
                .map_err(mcp_error)?;
        }
        self.touch_session_activity(id).await;
        Ok(response)
    }

    async fn has_session(&self, id: &SessionId) -> McpSessionManagerResult<bool> {
        if self.cached_session(id).await.is_some() {
            return Ok(true);
        }
        if self.pg_pool.is_none() {
            return Ok(false);
        }

        self.session_has_persisted_state(id)
            .await
            .map_err(mcp_error)
    }

    async fn close_session(&self, id: &SessionId) -> McpSessionManagerResult<()> {
        self.close_session_internal(id).await.map_err(mcp_error)
    }

    async fn create_stream(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> McpSessionManagerResult<impl Stream<Item = ServerSseMessage> + Send + Sync + 'static> {
        let handle = self.get_or_rehydrate_session(id).await.map_err(mcp_error)?;
        let receiver = handle
            .establish_request_wise_channel()
            .await
            .context("failed to establish request-wise MCP stream")
            .map_err(mcp_error)?;
        handle
            .push_message(message, receiver.http_request_id)
            .await
            .context("failed to push MCP request into stream")
            .map_err(mcp_error)?;
        Ok(ReceiverStream::new(receiver.inner))
    }

    async fn create_standalone_stream(
        &self,
        id: &SessionId,
    ) -> McpSessionManagerResult<impl Stream<Item = ServerSseMessage> + Send + Sync + 'static> {
        let handle = self.get_or_rehydrate_session(id).await.map_err(mcp_error)?;
        let receiver = handle
            .establish_common_channel()
            .await
            .context("failed to establish standalone MCP stream")
            .map_err(mcp_error)?;
        Ok(ReceiverStream::new(receiver.inner))
    }

    async fn resume(
        &self,
        id: &SessionId,
        last_event_id: String,
    ) -> McpSessionManagerResult<impl Stream<Item = ServerSseMessage> + Send + Sync + 'static> {
        let handle = self.get_or_rehydrate_session(id).await.map_err(mcp_error)?;
        let last_event_id = last_event_id
            .parse::<EventId>()
            .context("invalid Last-Event-ID value")
            .map_err(mcp_error)?;
        let receiver = handle
            .resume(last_event_id)
            .await
            .context("failed to resume MCP stream")
            .map_err(mcp_error)?;
        Ok(ReceiverStream::new(receiver.inner))
    }

    async fn accept_message(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> McpSessionManagerResult<()> {
        let handle = self.get_or_rehydrate_session(id).await.map_err(mcp_error)?;
        handle
            .push_message(message, None)
            .await
            .context("failed to accept MCP message")
            .map_err(mcp_error)?;
        Ok(())
    }
}
