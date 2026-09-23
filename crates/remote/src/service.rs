use crate::{
    identity::{self, AgentConfig, Endpoint, PairingOffer},
    runner::Runner,
};
use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
};
use subtle::ConstantTimeEq;
use tailtask_core::remote::*;

#[derive(Clone)]
struct Service {
    runner: Runner,
    config: AgentConfig,
    endpoint: Endpoint,
    owner_hash: String,
    directory: PathBuf,
}
type ApiResult<T> = Result<Json<T>, ApiError>;
struct ApiError(String);
impl From<String> for ApiError {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = if self.0.contains("UNAUTHORIZED") {
            StatusCode::UNAUTHORIZED
        } else if self.0.contains("NOT_FOUND") {
            StatusCode::NOT_FOUND
        } else if self.0.contains("CONFLICT") {
            StatusCode::CONFLICT
        } else if self.0.contains("QUEUE_FULL") {
            StatusCode::TOO_MANY_REQUESTS
        } else {
            StatusCode::BAD_REQUEST
        };
        (status, Json(serde_json::json!({"error":self.0}))).into_response()
    }
}
fn bearer(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .filter(|s| s.len() == 64)
        .ok_or_else(|| ApiError("UNAUTHORIZED".into()))
}
impl Service {
    async fn controller(&self, headers: &HeaderMap) -> Result<String, ApiError> {
        Ok(self.runner.store.authenticate(bearer(headers)?).await?)
    }
    fn owner(&self, headers: &HeaderMap, peer: SocketAddr) -> Result<(), ApiError> {
        let hash = hex_digest(bearer(headers)?.as_bytes());
        if peer.ip().to_string() != self.endpoint.address
            || !bool::from(hash.as_bytes().ct_eq(self.owner_hash.as_bytes()))
        {
            return Err(ApiError("UNAUTHORIZED_OWNER".into()));
        }
        Ok(())
    }
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            protocol_version: PROTOCOL_VERSION,
            identity: self.endpoint.identity.clone(),
            os: std::env::consts::OS.into(),
            account: std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" })
                .unwrap_or_else(|_| "当前登录用户".into()),
            allowed_directories: self.config.allowed_directories.clone(),
            interpreters: available_interpreters(),
            concurrency: self.config.concurrency,
            accepting: self.runner.accepting.load(Ordering::SeqCst),
        }
    }
}

fn available_interpreters() -> Vec<Interpreter> {
    [
        Interpreter::Powershell,
        Interpreter::Pwsh,
        Interpreter::Sh,
        Interpreter::Bash,
        Interpreter::Zsh,
    ]
    .into_iter()
    .filter(|i| i.compatible(std::env::consts::OS))
    .filter(|i| {
        let program = i.program();
        if std::path::Path::new(program).is_absolute() {
            return std::path::Path::new(program).is_file();
        }
        std::env::var_os("PATH").is_some_and(|p| {
            std::env::split_paths(&p).any(|dir| {
                dir.join(program).is_file()
                    || cfg!(windows) && dir.join(format!("{program}.exe")).is_file()
            })
        })
    })
    .collect()
}

#[derive(Serialize, Deserialize)]
pub struct OwnerStatus {
    pub capabilities: AgentCapabilities,
    pub active_tasks: i64,
    pub waiting_tasks: i64,
    pub fault: Option<String>,
    pub controllers: Vec<AuthorizedController>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRequest {
    pub abort: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default)]
    after: i64,
}

async fn capabilities(
    State(s): State<Arc<Service>>,
    headers: HeaderMap,
) -> ApiResult<AgentCapabilities> {
    s.controller(&headers).await?;
    Ok(Json(s.capabilities()))
}
async fn pair(
    State(s): State<Arc<Service>>,
    Json(request): Json<PairRequest>,
) -> ApiResult<PairResponse> {
    if !s.runner.accepting.load(Ordering::SeqCst) {
        return Err(ApiError("AGENT_DRAINING".into()));
    }
    Ok(Json(s.runner.store.pair(&request).await?))
}
async fn submit(
    State(s): State<Arc<Service>>,
    headers: HeaderMap,
    Json(request): Json<RemoteRequest>,
) -> ApiResult<RemoteTask> {
    let controller = s.controller(&headers).await?;
    let _guard = s.runner.admission.lock().await;
    if !s.runner.accepting.load(Ordering::SeqCst) {
        return Err(ApiError("AGENT_DRAINING".into()));
    }
    request.validate(std::env::consts::OS)?;
    if request.target != s.endpoint.identity {
        return Err(ApiError("TARGET_CHANGED".into()));
    }
    let roots = s.config.validate().await?;
    identity::cwd_in_roots(&request.cwd, &roots).await?;
    if let Execution::Script { interpreter, .. } = &request.execution {
        if !available_interpreters().contains(interpreter) {
            return Err(ApiError("INTERPRETER_NOT_AVAILABLE".into()));
        }
    }
    let (task, _) = s.runner.store.accept(&controller, &request).await?;
    s.runner.notify.notify_one();
    Ok(Json(task))
}
async fn task(
    State(s): State<Arc<Service>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<RemoteTask> {
    let c = s.controller(&headers).await?;
    let mut value = s.runner.store.get(&id, Some(&c)).await?;
    if !value.state.terminal() {
        if let Some(fault) = s.runner.fault.lock().await.as_ref() {
            if fault.contains(&id) {
                value.state = RemoteState::RecoveryRequired;
                value.error = Some(fault.clone());
            }
        }
    }
    Ok(Json(value))
}
async fn cancel(
    State(s): State<Arc<Service>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<RemoteTask> {
    let c = s.controller(&headers).await?;
    Ok(Json(s.runner.cancel(&id, Some(&c)).await?))
}
async fn events(
    State(s): State<Arc<Service>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(page): Query<Page>,
) -> ApiResult<Vec<RemoteEvent>> {
    let c = s.controller(&headers).await?;
    Ok(Json(
        s.runner.store.events(&id, Some(&c), page.after).await?,
    ))
}

async fn artifacts(
    State(s): State<Arc<Service>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Vec<Artifact>> {
    let controller = s.controller(&headers).await?;
    Ok(Json(
        crate::artifacts::list(&s.runner.store, &id, &controller).await?,
    ))
}

async fn download(
    State(s): State<Arc<Service>>,
    headers: HeaderMap,
    Path((id, artifact_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let controller = s.controller(&headers).await?;
    let artifact = crate::artifacts::list(&s.runner.store, &id, &controller)
        .await?
        .into_iter()
        .find(|a| a.id == artifact_id)
        .ok_or_else(|| ApiError("ARTIFACT_NOT_FOUND".into()))?;
    uuid::Uuid::parse_str(&artifact_id).map_err(|_| ApiError("INVALID_ARTIFACT_ID".into()))?;
    let (start, len, partial) = crate::artifacts::byte_range(
        headers.get("range").and_then(|v| v.to_str().ok()),
        artifact.size,
    )
    .map_err(ApiError)?;
    let mut file = tokio::fs::File::open(s.directory.join("artifacts").join(&artifact_id))
        .await
        .map_err(|e| ApiError(e.to_string()))?;
    if file
        .metadata()
        .await
        .map_err(|e| ApiError(e.to_string()))?
        .len()
        != artifact.size
    {
        return Err(ApiError("ARTIFACT_CHANGED".into()));
    }
    file.seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(|e| ApiError(e.to_string()))?;
    let stream = tokio_util::io::ReaderStream::new(file.take(len));
    let mut response = axum::http::Response::builder()
        .status(if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header("content-type", "application/octet-stream")
        .header("content-length", len)
        .header("accept-ranges", "bytes")
        .header("etag", format!("\"{}\"", artifact.sha256));
    if partial {
        response = response.header(
            "content-range",
            format!("bytes {}-{}/{}", start, start + len - 1, artifact.size),
        );
    }
    response
        .body(axum::body::Body::from_stream(stream))
        .map_err(|e| ApiError(e.to_string()))
}
async fn owner_status(
    State(s): State<Arc<Service>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> ApiResult<OwnerStatus> {
    s.owner(&headers, peer)?;
    let active_tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_tasks WHERE state IN ('starting','running','cancelling')",
    )
    .fetch_one(s.runner.store.pool())
    .await
    .map_err(|e| ApiError(e.to_string()))?;
    let waiting_tasks: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM agent_tasks WHERE state='queued'")
            .fetch_one(s.runner.store.pool())
            .await
            .map_err(|e| ApiError(e.to_string()))?;
    Ok(Json(OwnerStatus {
        capabilities: s.capabilities(),
        active_tasks,
        waiting_tasks,
        fault: s.runner.fault.lock().await.clone(),
        controllers: s.runner.store.controllers().await?,
    }))
}
async fn owner_pair(
    State(s): State<Arc<Service>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> ApiResult<PairingOffer> {
    s.owner(&headers, peer)?;
    if !s.runner.accepting.load(Ordering::SeqCst) {
        return Err(ApiError("AGENT_DRAINING".into()));
    }
    let e = &s.endpoint;
    Ok(Json(PairingOffer {
        version: PROTOCOL_VERSION,
        identity: e.identity.clone(),
        address: e.address.clone(),
        port: e.port,
        certificate_pem: e.certificate_pem.clone(),
        fingerprint: e.fingerprint.clone(),
        session: s.runner.store.create_pairing().await?,
    }))
}
async fn owner_revoke(
    State(s): State<Arc<Service>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    s.owner(&headers, peer)?;
    s.runner.store.revoke(&id).await?;
    Ok(Json(serde_json::json!({"revoked":true})))
}
async fn owner_stop(
    State(s): State<Arc<Service>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<StopRequest>,
) -> ApiResult<serde_json::Value> {
    s.owner(&headers, peer)?;
    s.runner.request_stop(request.abort).await?;
    Ok(Json(serde_json::json!({"stopping":true})))
}

fn router(service: Arc<Service>) -> Router {
    Router::new()
        .route("/v1/pair", post(pair))
        .route("/v1/capabilities", get(capabilities))
        .route("/v1/tasks", post(submit))
        .route("/v1/tasks/{id}", get(task))
        .route("/v1/tasks/{id}/cancel", post(cancel))
        .route("/v1/tasks/{id}/events", get(events))
        .route("/v1/tasks/{id}/artifacts", get(artifacts))
        .route("/v1/tasks/{id}/artifacts/{artifact_id}", get(download))
        .route("/owner/status", get(owner_status))
        .route("/owner/pairing", post(owner_pair))
        .route("/owner/controllers/{id}/revoke", post(owner_revoke))
        .route("/owner/stop", post(owner_stop))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(service)
}

pub async fn serve(directory: PathBuf) -> Result<(), String> {
    let config: AgentConfig = identity::read_json(&directory.join("config.json")).await?;
    let roots = config.validate().await?;
    let store = AgentStore::open(&directory.join("agent.db")).await?;
    let endpoint = identity::initialize(&store, &config, &directory).await?;
    let key = crate::credentials::read(&endpoint.key_credential_ref)?;
    let owner_hash = store
        .setting("owner_token_hash")
        .await?
        .ok_or("OWNER_CREDENTIAL_MISSING")?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem(
        endpoint.certificate_pem.clone().into_bytes(),
        key.into_bytes(),
    )
    .await
    .map_err(|e| e.to_string())?;
    let address = SocketAddr::new(
        endpoint
            .address
            .parse()
            .map_err(|_| "INVALID_BIND_ADDRESS")?,
        endpoint.port,
    );
    // Bind before executing queued work: startup failures must not run tasks.
    let listener = std::net::TcpListener::bind(address).map_err(|e| format!("AGENT_BIND: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let runner = Runner::new(
        store,
        roots,
        directory.clone(),
        std::env::current_exe().map_err(|e| e.to_string())?,
        config.concurrency,
    );
    let service = Arc::new(Service {
        runner: runner.clone(),
        config,
        endpoint,
        owner_hash,
        directory,
    });
    let handle = axum_server::Handle::new();
    let runner_handle = handle.clone();
    let work = tokio::spawn(async move {
        let result = runner.run().await;
        runner_handle.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
        result
    });
    let result = axum_server::from_tcp_rustls(listener, tls)
        .map_err(|e| e.to_string())?
        .handle(handle)
        .serve(router(service.clone()).into_make_service_with_connect_info::<SocketAddr>())
        .await;
    if result.is_err() {
        service.runner.request_stop(true).await?;
    }
    work.await.map_err(|e| e.to_string())??;
    result.map_err(|e| e.to_string())
}
