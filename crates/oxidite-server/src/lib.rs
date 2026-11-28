//! Oxidite session server (oxidited) library.

#![allow(clippy::collapsible_if, clippy::useless_conversion)]

pub mod auth;
pub mod complete_api;
pub mod device_connections;
pub mod devices;
pub mod git_api;
pub mod git_network;
pub mod metrics;
pub mod origin_guard;
pub mod project_scan;
pub mod routing;
pub mod security;
pub mod share;
pub mod tls;

use axum::{
    Json, Router,
    extract::{
        Path, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
};
use futures_util::{SinkExt, StreamExt};
use oxidite_proto::{
    AttachMode, ClientMsg, ErrorCode, PROTOCOL_VERSION, ServerMsg, SessionId, SessionState,
    pack_server_msg, unpack_client_msg,
};
use oxidite_session::{
    actor::{ActorCmd, AgentContext},
    manager::SessionManager,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

fn process_probe_cache() -> &'static Arc<oxidite_pty::agent_detect::ProcessProbeCache> {
    static PROBE: OnceLock<Arc<oxidite_pty::agent_detect::ProcessProbeCache>> = OnceLock::new();
    PROBE.get_or_init(|| Arc::new(oxidite_pty::agent_detect::ProcessProbeCache::default()))
}

fn origin_policy_for(state: &AppState) -> origin_guard::OriginPolicy {
    origin_guard::OriginPolicy::from_csv(&state.security.allowed_origins.join(","))
}

fn is_ws_origin_allowed(
    state: &AppState,
    peer: Option<std::net::SocketAddr>,
    headers: &HeaderMap,
) -> bool {
    let Some(origin_str) = headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) else {
        return true;
    };
    let policy = origin_policy_for(state);
    if policy.is_origin_str_allowed(origin_str) {
        return true;
    }

    let is_loopback_peer = peer.is_some_and(|p| p.ip().is_loopback());
    let host = if is_loopback_peer {
        headers
            .get("x-forwarded-host")
            .or_else(|| headers.get(header::HOST))
    } else {
        headers.get(header::HOST)
    }
    .and_then(|h| h.to_str().ok())
    .and_then(|raw| raw.split(',').next())
    .map(str::trim);

    // Block arbitrary *.ts.net Origins unless explicitly listed in OXIDITE_ALLOWED_ORIGINS.
    if origin_str.to_ascii_lowercase().contains(".ts.net") {
        return false;
    }
    state
        .security
        .is_origin_allowed_for_host(Some(origin_str), host)
}
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::auth::UserStore;
use crate::security::{
    AuditEvent, AuditLogger, RateLimiter, SecurityConfig, security_headers_middleware,
};
use crate::share::ShareStore;

#[derive(Clone)]
pub struct AppState {
    pub manager: SessionManager,
    pub token: String,
    pub static_dir: String,
    pub known_hosts: oxidite_ssh::KnownHostsStore,
    pub security: SecurityConfig,
    pub auth: UserStore,
    pub shares: ShareStore,
    pub rate_limiter: RateLimiter,
    pub metrics: metrics::MetricsCollector,
    pub node_id: String,
    pub directory: std::sync::Arc<dyn oxidite_directory::SessionDirectory>,
    pub devices: devices::DeviceStore,
    pub device_connections: device_connections::DeviceConnectionRegistry,
}

#[derive(Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub state: SessionState,
    pub label: Option<String>,
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub cwd_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_kind: Option<String>,
}

fn short_cwd_hash(path: &std::path::Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub expires_in_secs: u64,
}

#[derive(Deserialize)]
pub struct CreateShareRequest {
    pub mode: Option<String>,
    pub ttl_secs: Option<u64>,
}

#[derive(Serialize)]
pub struct CreateShareResponse {
    pub token: String,
    pub mode: String,
    pub expires_in_secs: u64,
}

pub fn create_app(state: AppState) -> Router {
    let cors_sec = state.security.clone();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, request_parts| {
            let origin_str = origin.to_str().ok();
            let host = request_parts
                .headers
                .get("x-forwarded-host")
                .or_else(|| request_parts.headers.get(header::HOST))
                .and_then(|h| h.to_str().ok())
                .and_then(|raw| raw.split(',').next())
                .map(str::trim);
            cors_sec.is_origin_allowed_for_host(origin_str, host)
        }))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(metrics::readiness_handler))
        .route("/metrics", get(metrics::prometheus_metrics_handler))
        .route("/api/login", post(login_handler))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/:id/share", post(create_share_handler))
        .route("/api/shares/:token", delete(revoke_share_handler))
        .route("/api/devices/enroll", post(enroll_device_handler))
        .route("/api/devices/grant", post(issue_device_grant_handler))
        .route("/api/devices", get(list_devices_handler))
        .route("/api/devices/:id", delete(revoke_device_handler))
        .route("/api/runtime/shells", get(runtime_shells_handler))
        .route("/api/fs/list", get(fs_list_handler))
        .route("/api/fs/read", get(fs_read_handler))
        .route("/api/project/scan", get(api_project_scan))
        .route("/api/git/changes", get(git_api::git_changes_handler))
        .route("/api/git/diff", get(git_api::git_diff_handler))
        .route("/api/git/log", get(git_api::git_log_handler))
        .route("/api/git/stage", post(git_api::git_stage_handler))
        .route("/api/git/commit", post(git_api::git_commit_handler))
        .route("/api/git/branches", get(git_api::git_branches_handler))
        .route("/api/git/checkout", post(git_api::git_checkout_handler))
        .route("/api/git/fetch", post(git_api::git_fetch_handler))
        .route("/api/git/pull", post(git_api::git_pull_handler))
        .route("/api/git/push", post(git_api::git_push_handler))
        .route("/api/complete", get(complete_api::complete_handler))
        .route("/ws", get(ws_handler))
        .fallback(static_handler)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            route_metrics_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security_headers_middleware,
        ))
        .layer(cors)
        .with_state(state)
}

async fn route_metrics_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let start = std::time::Instant::now();
    let path = request.uri().path().to_string();
    let route_class = routing::classify_request_path(&path);
    let mut response = next.run(request).await;
    let duration = start.elapsed();
    let status = response.status();
    let bytes = response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    state
        .metrics
        .observe_http(route_class, status, duration, bytes);

    if !response.headers().contains_key(header::CACHE_CONTROL) {
        if let Ok(val) = header::HeaderValue::from_str(routing::cache_control(route_class)) {
            response.headers_mut().insert(header::CACHE_CONTROL, val);
        }
    }

    response
}

async fn health_check() -> &'static str {
    "OK"
}

async fn login_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown");

    if !state.rate_limiter.check_limit(
        client_ip,
        state.security.rate_limit_per_minute,
        Duration::from_secs(60),
    ) {
        AuditLogger::record(AuditEvent::SecurityViolation {
            reason: "Rate limit exceeded on login",
            remote_ip: client_ip,
        });
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let ttl = Duration::from_secs(3600);
    if let Some(token) = state.auth.login(&payload.username, &payload.password, ttl) {
        AuditLogger::record(AuditEvent::LoginAttempt {
            username: &payload.username,
            success: true,
            remote_ip: client_ip,
        });
        Ok(Json(LoginResponse {
            token: token.token,
            user_id: token.user_id,
            expires_in_secs: ttl.as_secs(),
        }))
    } else {
        AuditLogger::record(AuditEvent::LoginAttempt {
            username: &payload.username,
            success: false,
            remote_ip: client_ip,
        });
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn create_share_handler(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<CreateShareRequest>,
) -> Result<Json<CreateShareResponse>, StatusCode> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let sid = match uuid::Uuid::parse_str(&session_id) {
        Ok(s) => s,
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };

    let user_id = if let Some(h) = auth_header {
        let token = h.strip_prefix("Bearer ").unwrap_or(h).trim();
        if let Ok(principal) = state.devices.authenticate(token).await {
            principal.user_id
        } else if let Some(uid) = state.auth.validate_token(token) {
            uid
        } else if token == state.token && !token.is_empty() {
            "admin".to_string()
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        }
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    if user_id != "admin" && !state.auth.is_session_owner(sid, &user_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    let mode = match payload.mode.as_deref() {
        Some("writer") | Some("Writer") => AttachMode::Writer,
        _ => AttachMode::Viewer,
    };
    let ttl_secs = payload.ttl_secs.unwrap_or(3600);
    let share = state
        .shares
        .create_share(sid, mode, Duration::from_secs(ttl_secs));

    Ok(Json(CreateShareResponse {
        token: share.token,
        mode: match mode {
            AttachMode::Writer => "writer".into(),
            AttachMode::Viewer => "viewer".into(),
        },
        expires_in_secs: ttl_secs,
    }))
}

async fn revoke_share_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> StatusCode {
    if state.shares.revoke_share(&token) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

#[derive(Deserialize)]
pub struct EnrollDeviceRequest {
    pub grant: String,
    pub label: Option<String>,
}

#[derive(Serialize)]
pub struct EnrollDeviceResponse {
    pub device_id: String,
    pub user_id: String,
    pub token: String,
    pub label: String,
}

#[derive(Deserialize)]
pub struct DeviceGrantRequest {
    pub ttl_secs: Option<u64>,
}

#[derive(Serialize)]
pub struct DeviceGrantResponse {
    pub grant: String,
    pub expires_in_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_url: Option<String>,
}

async fn enroll_device_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<EnrollDeviceRequest>,
) -> Result<Json<EnrollDeviceResponse>, StatusCode> {
    let rl_key = security::rate_limit_key(&headers, "enroll");
    if !state
        .rate_limiter
        .check_limit(&rl_key, 20, Duration::from_secs(60))
    {
        AuditLogger::record(AuditEvent::SecurityViolation {
            reason: "Rate limit exceeded on device enrollment",
            remote_ip: &rl_key,
        });
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let label = payload
        .label
        .unwrap_or_else(|| "Browser Device".to_string());
    match state.devices.enroll(&payload.grant, &label).await {
        Ok(res) => {
            let dev_id_str = res.device_id.to_string();
            AuditLogger::record(AuditEvent::DeviceEnrolled {
                device_id: &dev_id_str,
                user: &res.user_id,
            });
            Ok(Json(EnrollDeviceResponse {
                device_id: res.device_id.to_string(),
                user_id: res.user_id,
                token: res.token,
                label: res.label,
            }))
        }
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn issue_device_grant_handler(
    State(state): State<AppState>,
    peer: Option<axum::extract::ConnectInfo<std::net::SocketAddr>>,
    headers: HeaderMap,
    Json(payload): Json<DeviceGrantRequest>,
) -> Result<Json<DeviceGrantResponse>, StatusCode> {
    let rl_key = security::rate_limit_key(&headers, "grant");
    if !state
        .rate_limiter
        .check_limit(&rl_key, 30, Duration::from_secs(60))
    {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let user_id = if let Some(h) = auth_header {
        let token = h.strip_prefix("Bearer ").unwrap_or(h).trim();
        if let Ok(principal) = state.devices.authenticate(token).await {
            principal.user_id
        } else if let Some(uid) = state.auth.validate_token(token) {
            uid
        } else if token == state.token && !token.is_empty() {
            "admin".to_string()
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        }
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let ttl_secs = payload.ttl_secs.unwrap_or(300);
    match state
        .devices
        .issue_enrollment_grant(&user_id, Duration::from_secs(ttl_secs))
        .await
    {
        Ok(grant) => {
            let peer_addr = peer.map(|c| c.0).unwrap_or_else(|| {
                std::net::SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, 0))
            });
            let fallback = state
                .security
                .bind_addr
                .map(|a| a.to_string())
                .unwrap_or_else(|| "127.0.0.1:3030".to_string());
            let base = origin_policy_for(&state).external_base_url(peer_addr, &headers, &fallback);
            Ok(Json(DeviceGrantResponse {
                share_url: Some(format!("{base}/#pair={grant}")),
                grant,
                expires_in_secs: ttl_secs,
            }))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn list_devices_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<devices::DeviceInfo>>, StatusCode> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let user_id = if let Some(h) = auth_header {
        let token = h.strip_prefix("Bearer ").unwrap_or(h).trim();
        if let Ok(principal) = state.devices.authenticate(token).await {
            principal.user_id
        } else if let Some(uid) = state.auth.validate_token(token) {
            uid
        } else if token == state.token && !token.is_empty() {
            "admin".to_string()
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        }
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    match state.devices.list_for_user(&user_id).await {
        Ok(devices) => Ok(Json(devices)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn revoke_device_handler(
    State(state): State<AppState>,
    Path(device_id_str): Path<String>,
    headers: HeaderMap,
) -> StatusCode {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let user_id = if let Some(h) = auth_header {
        let token = h.strip_prefix("Bearer ").unwrap_or(h).trim();
        if let Ok(principal) = state.devices.authenticate(token).await {
            principal.user_id
        } else if let Some(uid) = state.auth.validate_token(token) {
            uid
        } else if token == state.token && !token.is_empty() {
            "admin".to_string()
        } else {
            return StatusCode::UNAUTHORIZED;
        }
    } else {
        return StatusCode::UNAUTHORIZED;
    };

    let device_id = match uuid::Uuid::parse_str(&device_id_str) {
        Ok(id) => id,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    match state.devices.revoke(&user_id, device_id).await {
        Ok(()) => {
            state.device_connections.close_local(device_id);
            StatusCode::NO_CONTENT
        }
        Err(devices::DeviceAuthError::NotFound) => StatusCode::NOT_FOUND,
        Err(devices::DeviceAuthError::Unauthorized) => StatusCode::UNAUTHORIZED,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Deserialize)]
pub struct FsListQuery {
    pub session: Option<String>,
    pub path: Option<String>,
}

#[derive(Serialize)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Serialize)]
pub struct FsListResponse {
    pub root: String,
    pub entries: Vec<FsEntry>,
}

#[derive(Serialize)]
pub struct FsReadResponse {
    pub path: String,
    pub name: String,
    pub content: String,
    pub truncated: bool,
}

async fn fs_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<FsListQuery>,
) -> Result<Json<FsListResponse>, StatusCode> {
    let user_id = git_api::check_auth(&state, &headers).await?;
    let cwd = match git_api::resolve_session_cwd(&state, &user_id, q.session.as_deref()).await? {
        Some(dir) => dir,
        None => {
            let sec = state.security.clone();
            tokio::task::spawn_blocking(move || sec.resolve_default_cwd())
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::BAD_REQUEST)?
        }
    };

    let target = match q.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => {
            let p = std::path::PathBuf::from(raw);
            let candidate = if p.is_absolute() { p } else { cwd.join(p) };
            let canon = security::canonical_clean_path(&candidate).ok_or(StatusCode::NOT_FOUND)?;
            if !state.security.is_workspace_path_allowed(&canon) {
                return Err(StatusCode::FORBIDDEN);
            }
            canon
        }
        None => cwd,
    };

    let root_str = target.to_string_lossy().into_owned();
    let entries = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&target) else {
            return out;
        };
        for entry in rd.flatten().take(300) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let path = entry.path().to_string_lossy().into_owned();
            out.push(FsEntry { name, path, is_dir });
        }
        out.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
        });
        out
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(FsListResponse {
        root: root_str,
        entries,
    }))
}

async fn fs_read_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<FsListQuery>,
) -> Result<Json<FsReadResponse>, StatusCode> {
    const MAX_PREVIEW_BYTES: usize = 256 * 1024;
    let user_id = git_api::check_auth(&state, &headers).await?;
    let cwd = match git_api::resolve_session_cwd(&state, &user_id, q.session.as_deref()).await? {
        Some(dir) => dir,
        None => {
            let sec = state.security.clone();
            tokio::task::spawn_blocking(move || sec.resolve_default_cwd())
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::BAD_REQUEST)?
        }
    };

    let raw = q
        .path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let p = std::path::PathBuf::from(raw);
    let candidate = if p.is_absolute() { p } else { cwd.join(p) };
    let canon = security::canonical_clean_path(&candidate).ok_or(StatusCode::NOT_FOUND)?;
    if !state.security.is_workspace_path_allowed(&canon) {
        return Err(StatusCode::FORBIDDEN);
    }

    let sec = state.security.clone();
    let res = tokio::task::spawn_blocking(move || -> Result<FsReadResponse, StatusCode> {
        use std::io::Read;
        if !sec.is_workspace_path_allowed(&canon) || !canon.is_file() {
            return Err(StatusCode::BAD_REQUEST);
        }
        let file = std::fs::File::open(&canon).map_err(|_| StatusCode::FORBIDDEN)?;
        let mut buf = Vec::new();
        file.take((MAX_PREVIEW_BYTES + 1) as u64)
            .read_to_end(&mut buf)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let truncated = buf.len() > MAX_PREVIEW_BYTES;
        if truncated {
            buf.truncate(MAX_PREVIEW_BYTES);
        }
        if buf.contains(&0) {
            return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        }
        let content = String::from_utf8_lossy(&buf).into_owned();
        let path_str = canon.to_string_lossy().into_owned();
        let name = canon
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&path_str)
            .to_string();
        Ok(FsReadResponse {
            path: path_str,
            name,
            content,
            truncated,
        })
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    Ok(Json(res))
}

#[derive(Deserialize)]
struct ProjectScanQuery {
    #[serde(alias = "session")]
    sid: Option<String>,
}

async fn api_project_scan(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<ProjectScanQuery>,
) -> Result<Json<project_scan::ProjectReport>, StatusCode> {
    let user_id = git_api::check_auth(&state, &headers).await?;
    let Some(cwd) = git_api::resolve_session_cwd(&state, &user_id, q.sid.as_deref()).await? else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let report = project_scan::scan_project_dir(cwd)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(report))
}

async fn runtime_shells_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<oxidite_proto::ShellCapability>>, StatusCode> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let token = match auth_header.and_then(|h| h.strip_prefix("Bearer ")) {
        Some(t) => t.trim(),
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let _user_id = if let Ok(principal) = state.devices.authenticate(token).await {
        principal.user_id
    } else if let Some(uid) = state.auth.validate_token(token) {
        uid
    } else if token == state.token && !token.is_empty() {
        "admin".to_string()
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let allow_local = state.security.is_local_pty_allowed();
    let pwsh_available = allow_local && oxidite_pty::is_powershell_available();
    let capabilities = vec![
        oxidite_proto::ShellCapability {
            id: "default".to_string(),
            available: allow_local,
        },
        oxidite_proto::ShellCapability {
            id: "SystemPowerShell".to_string(),
            available: pwsh_available,
        },
    ];

    Ok(Json(capabilities))
}

async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SessionInfo>>, StatusCode> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let user_id = if let Some(h) = auth_header {
        let token = h.strip_prefix("Bearer ").unwrap_or(h).trim();
        if let Ok(principal) = state.devices.authenticate(token).await {
            principal.user_id
        } else if let Some(uid) = state.auth.validate_token(token) {
            uid
        } else if token == state.token && !token.is_empty() {
            "admin".to_string()
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        }
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    state.auth.prune_expired_tokens();
    let home_dir = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .filter(|s| !s.trim().is_empty());

    let all_ids = state.manager.list();
    let mut list = Vec::new();
    for id in all_ids {
        if user_id != "admin" && !state.auth.is_session_owner(id, &user_id) {
            continue;
        }

        if let Some(handle) = state.manager.get(id) {
            let (tx, rx) = oneshot::channel();
            if handle
                .tx
                .send(ActorCmd::GetState { resp: tx })
                .await
                .is_ok()
                && let Ok(sess_state) = rx.await
            {
                let (actx_tx, actx_rx) = oneshot::channel();
                let AgentContext {
                    shell_pid,
                    title,
                    cwd: live_cwd,
                    shell_kind,
                } = if handle
                    .tx
                    .send(ActorCmd::GetAgentContext { resp: actx_tx })
                    .await
                    .is_ok()
                {
                    actx_rx.await.unwrap_or(AgentContext {
                        shell_pid: None,
                        title: String::new(),
                        cwd: None,
                        shell_kind: None,
                    })
                } else {
                    AgentContext {
                        shell_pid: None,
                        title: String::new(),
                        cwd: None,
                        shell_kind: None,
                    }
                };
                let effective_cwd = match live_cwd {
                    Some(p) if state.security.is_workspace_path_allowed(&p) => {
                        state.manager.set_cwd(id, p.clone());
                        Some(p)
                    }
                    _ => state
                        .manager
                        .get_cwd(id)
                        .filter(|p| state.security.is_workspace_path_allowed(p)),
                };
                let (workspace, cwd, cwd_id) = match effective_cwd {
                    Some(ref p) => {
                        let full = p.to_string_lossy().into_owned();
                        let name = p
                            .file_name()
                            .and_then(|n| n.to_str())
                            .filter(|s| !s.is_empty())
                            .map_or_else(|| full.clone(), String::from);
                        (Some(name), Some(full), Some(short_cwd_hash(p)))
                    }
                    None => (Some("Remote".to_string()), None, None),
                };

                let probe = process_probe_cache().clone();
                let agent = tokio::task::spawn_blocking(move || {
                    let osc = (!title.trim().is_empty()).then_some(title.as_str());
                    match shell_pid {
                        Some(pid) => probe.detect_for_shell(pid, osc),
                        None => oxidite_pty::agent_detect::detect_agent_from_argv_and_title(
                            None, None, osc,
                        ),
                    }
                })
                .await
                .ok()
                .flatten();

                list.push(SessionInfo {
                    id: id.to_string(),
                    state: sess_state,
                    label: Some(format!("session-{}", &id.to_string()[..8])),
                    workspace,
                    cwd,
                    cwd_id,
                    home_dir: home_dir.clone(),
                    active_agent: agent.map(|a| a.id().to_string()),
                    agent_label: agent.map(|a| a.label().to_string()),
                    shell_kind: shell_kind.map(String::from),
                });
            }
        }
    }
    Ok(Json(list))
}

async fn static_handler(State(state): State<AppState>, uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let static_dir = std::path::Path::new(&state.static_dir);
    let target = if path.is_empty() {
        static_dir.join("index.html")
    } else {
        match routing::resolve_static_path(static_dir, path) {
            Some(target) => target,
            None => return StatusCode::BAD_REQUEST.into_response(),
        }
    };

    if target.exists()
        && target.is_file()
        && let (Ok(canon_dir), Ok(canon_target)) = (
            tokio::fs::canonicalize(static_dir).await,
            tokio::fs::canonicalize(&target).await,
        )
    {
        if !canon_target.starts_with(&canon_dir) {
            return StatusCode::FORBIDDEN.into_response();
        }
        let mime = mime_guess::from_path(&canon_target).first_or_octet_stream();
        match tokio::fs::read(&canon_target).await {
            Ok(content) => {
                return (
                    [
                        (header::CONTENT_TYPE, mime.as_ref()),
                        (
                            header::CACHE_CONTROL,
                            routing::cache_control(routing::classify_request_path(uri.path())),
                        ),
                    ],
                    content,
                )
                    .into_response();
            }
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }

    let index_path = static_dir.join("index.html");
    if index_path.exists()
        && let (Ok(canon_dir), Ok(canon_index)) = (
            tokio::fs::canonicalize(static_dir).await,
            tokio::fs::canonicalize(&index_path).await,
        )
        && canon_index.starts_with(&canon_dir)
        && let Ok(content) = tokio::fs::read(&canon_index).await
    {
        return (
            [(
                header::CACHE_CONTROL,
                routing::cache_control(routing::RouteClass::Html),
            )],
            Html(content),
        )
            .into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    peer: Option<axum::extract::ConnectInfo<std::net::SocketAddr>>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    let rl_key = security::rate_limit_key(&headers, "ws");
    if !state.rate_limiter.check_limit(
        &rl_key,
        state.security.rate_limit_per_minute,
        Duration::from_secs(60),
    ) {
        AuditLogger::record(AuditEvent::SecurityViolation {
            reason: "Rate limit exceeded on WebSocket upgrade",
            remote_ip: &rl_key,
        });
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    let origin = headers.get(header::ORIGIN).and_then(|h| h.to_str().ok());
    if !is_ws_origin_allowed(&state, peer.map(|c| c.0), &headers) {
        AuditLogger::record(AuditEvent::SecurityViolation {
            reason: "Rejected WebSocket upgrade: unauthorized origin",
            remote_ip: origin.unwrap_or("unknown"),
        });
        return StatusCode::FORBIDDEN.into_response();
    }

    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    let ws_start = std::time::Instant::now();
    // Handshake: Expect ClientMsg::Hello first
    let (user_id, shared_access, device_id) = match socket.recv().await {
        Some(Ok(Message::Binary(bin))) => {
            match unpack_client_msg(&bin) {
                Ok(ClientMsg::Hello {
                    protocol_version,
                    token,
                }) => {
                    if protocol_version != PROTOCOL_VERSION {
                        let err = pack_server_msg(&ServerMsg::Error {
                            code: ErrorCode::ProtocolMismatch,
                            message: format!(
                                "Version mismatch: expected {PROTOCOL_VERSION}, got {protocol_version}"
                            ),
                        })
                        .unwrap_or_default();
                        let _ = socket.send(Message::Binary(err.into())).await;
                        let _ = socket.close().await;
                        return;
                    }

                    // Authenticate token
                    if let Ok(principal) = state.devices.authenticate(&token).await {
                        let dev_id_str = principal.device_id.to_string();
                        AuditLogger::record(AuditEvent::DeviceAuthenticated {
                            device_id: &dev_id_str,
                            user: &principal.user_id,
                        });
                        (principal.user_id, None, Some(principal.device_id))
                    } else if token == state.token && !token.is_empty() {
                        ("admin".to_string(), None, None)
                    } else if token.starts_with("share_") {
                        if let Some((sid, mode)) = state.shares.validate_share(&token) {
                            (format!("viewer_{}", &token[6..14]), Some((sid, mode)), None)
                        } else {
                            let err = pack_server_msg(&ServerMsg::Error {
                                code: ErrorCode::Unauthorized,
                                message: "Share link expired or revoked".into(),
                            })
                            .unwrap_or_default();
                            let _ = socket.send(Message::Binary(err.into())).await;
                            let _ = socket.close().await;
                            return;
                        }
                    } else if let Some(uid) = state.auth.validate_token(&token) {
                        (uid, None, None)
                    } else {
                        let err = pack_server_msg(&ServerMsg::Error {
                            code: ErrorCode::Unauthorized,
                            message: "Invalid access token".into(),
                        })
                        .unwrap_or_default();
                        let _ = socket.send(Message::Binary(err.into())).await;
                        let _ = socket.close().await;
                        return;
                    }
                }
                _ => {
                    let _ = socket.close().await;
                    return;
                }
            }
        }
        _ => {
            let _ = socket.close().await;
            return;
        }
    };

    // Send Welcome
    let welcome = pack_server_msg(&ServerMsg::Welcome {
        server_version: env!("CARGO_PKG_VERSION").to_string(),
    })
    .unwrap_or_default();
    if socket.send(Message::Binary(welcome.into())).await.is_err() {
        return;
    }
    state
        .metrics
        .observe_websocket_handshake(ws_start.elapsed());

    let (mut ws_sink, mut ws_stream) = socket.split();

    // Client connection state
    let client_id = rand::random::<u64>();
    let (close_tx, mut close_rx) = mpsc::channel::<()>(1);
    if let Some(dev_id) = device_id {
        if !state
            .device_connections
            .register(dev_id, client_id, close_tx)
        {
            let _ = ws_sink.close().await;
            return;
        }
    }

    let mut current_session: Option<SessionId> = None;
    let mut current_tx: Option<mpsc::Sender<ActorCmd>> = None;
    let mut pending_ssh_sid: Option<SessionId> = None;
    let mut ssh_hostkey_reply: Option<oneshot::Sender<bool>> = None;
    let mut ssh_auth_reply: Option<oneshot::Sender<Vec<String>>> = None;
    let (ssh_prompt_tx, mut ssh_prompt_rx) = mpsc::channel::<oxidite_ssh::SshPrompt>(8);
    let (ssh_done_tx, mut ssh_done_rx) = mpsc::channel::<(
        SessionId,
        oxidite_proto::TermSize,
        std::result::Result<oxidite_ssh::SshBackend, String>,
    )>(4);
    let (server_tx, mut server_rx) = mpsc::channel::<ServerMsg>(256);
    let (ping_tx, mut ping_rx) = mpsc::channel::<()>(4);

    // Forward messages & protocol pings from server to WebSocket client
    let forward_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(msg) = server_rx.recv() => {
                    let bytes = pack_server_msg(&msg).unwrap_or_default();
                    if ws_sink.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                Some(()) = ping_rx.recv() => {
                    if ws_sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
                else => break,
            }
        }
        let _ = ws_sink.close().await;
    });

    let mut session_forward_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut last_ws_pong = Instant::now();
    let mut ws_heartbeat = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(15),
        Duration::from_secs(15),
    );
    ws_heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut revalidate_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(5),
        Duration::from_secs(5),
    );
    revalidate_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        let msg = tokio::select! {
            _ = close_rx.recv() => {
                break;
            }
            _ = ws_heartbeat.tick() => {
                if last_ws_pong.elapsed() >= Duration::from_secs(45) {
                    break;
                }
                let _ = ping_tx.try_send(());
                continue;
            }
            _ = revalidate_interval.tick() => {
                if let Some(dev_id) = device_id {
                    if state.device_connections.requires_revalidation(client_id, Duration::from_secs(30)) {
                        match state.devices.is_revoked(dev_id).await {
                            Ok(true) | Err(_) => {
                                state.device_connections.close_local(dev_id);
                                break;
                            }
                            Ok(false) => {
                                state.device_connections.mark_validated(client_id);
                            }
                        }
                    }
                }
                continue;
            }
            Some(prompt) = ssh_prompt_rx.recv() => {
                match prompt {
                    oxidite_ssh::SshPrompt::HostKey {
                        host,
                        fingerprint,
                        known,
                        reply,
                    } => {
                        ssh_hostkey_reply = Some(reply);
                        let _ = server_tx
                            .send(ServerMsg::HostKeyPrompt {
                                host,
                                fingerprint,
                                known,
                            })
                            .await;
                    }
                    oxidite_ssh::SshPrompt::Auth { prompts, reply } => {
                        ssh_auth_reply = Some(reply);
                        let _ = server_tx.send(ServerMsg::AuthPrompt { prompts }).await;
                    }
                }
                continue;
            }
            Some((sid, size, res)) = ssh_done_rx.recv() => {
                if pending_ssh_sid == Some(sid) {
                    pending_ssh_sid = None;
                    ssh_hostkey_reply = None;
                    ssh_auth_reply = None;
                    match res {
                        Ok(backend) => {
                            state
                                .manager
                                .create_with_backend(sid, size, Box::new(backend));
                            let _ = state
                                .directory
                                .claim(sid, state.node_id.clone(), Duration::from_secs(60))
                                .await;
                            state.auth.set_session_owner(sid, &user_id);
                            AuditLogger::record(AuditEvent::SessionCreated {
                                session_id: &sid.to_string(),
                                backend_kind: "ssh",
                                user: &user_id,
                            });
                            current_session = Some(sid);
                            let _ = server_tx.send(ServerMsg::Created { session: sid }).await;
                        }
                        Err(message) => {
                            let code = if message.contains("not accepted") {
                                ErrorCode::Unauthorized
                            } else {
                                ErrorCode::BackendError
                            };
                            let _ = server_tx.send(ServerMsg::Error { code, message }).await;
                        }
                    }
                } else if let Ok(mut superseded) = res {
                    let _ = oxidite_session::Backend::kill(&mut superseded).await;
                }
                continue;
            }
            res = ws_stream.next() => {
                match res {
                    Some(Ok(m)) => {
                        last_ws_pong = Instant::now();
                        m
                    }
                    _ => break,
                }
            }
        };

        let bytes = match msg {
            Message::Binary(b) => b,
            Message::Close(_) => break,
            _ => continue,
        };

        if bytes.len() > state.security.max_message_size {
            let _ = server_tx
                .send(ServerMsg::Error {
                    code: ErrorCode::InvalidMessage,
                    message: "Message exceeds maximum allowed size".into(),
                })
                .await;
            continue;
        }

        let client_msg = match unpack_client_msg(&bytes) {
            Ok(m) => m,
            Err(e) => {
                let _ = server_tx
                    .send(ServerMsg::Error {
                        code: ErrorCode::InvalidMessage,
                        message: e.to_string(),
                    })
                    .await;
                continue;
            }
        };

        match client_msg {
            ClientMsg::Create { backend, size } => {
                if shared_access.is_some() {
                    let _ = server_tx
                        .send(ServerMsg::Error {
                            code: ErrorCode::Unauthorized,
                            message: "Share tokens cannot create sessions".into(),
                        })
                        .await;
                    continue;
                }

                let current_user_sessions = state.auth.user_sessions(&user_id);
                if user_id != "admin"
                    && current_user_sessions.len() >= state.security.max_sessions_per_user
                {
                    let _ = server_tx
                        .send(ServerMsg::Error {
                            code: ErrorCode::BackendError,
                            message: format!(
                                "Maximum session limit ({}) reached",
                                state.security.max_sessions_per_user
                            ),
                        })
                        .await;
                    continue;
                }

                match backend {
                    oxidite_proto::BackendSpec::LocalPty { shell, cwd } => {
                        if !state.security.is_local_pty_allowed() {
                            let _ = server_tx
                                .send(ServerMsg::Error {
                                    code: ErrorCode::Unauthorized,
                                    message: "Local PTY is disabled on non-loopback binds".into(),
                                })
                                .await;
                            continue;
                        }

                        if let Err(e) = oxidite_pty::resolve_safe_shell(shell.as_deref()) {
                            let code = match e {
                                oxidite_pty::ShellError::ArbitraryPathRejected(_) => {
                                    ErrorCode::InvalidMessage
                                }
                                oxidite_pty::ShellError::PowerShellUnavailable => {
                                    ErrorCode::BackendError
                                }
                            };
                            let _ = server_tx
                                .send(ServerMsg::Error {
                                    code,
                                    message: e.to_string(),
                                })
                                .await;
                            continue;
                        }

                        let sec_for_cwd = state.security.clone();
                        let resolved_cwd = match cwd {
                            Some(raw) if !raw.trim().is_empty() => {
                                let raw_trimmed = raw.trim().to_string();
                                match tokio::task::spawn_blocking(move || {
                                    let canon = security::canonical_clean_path(
                                        std::path::Path::new(&raw_trimmed),
                                    )
                                    .ok_or_else(|| {
                                        std::io::Error::new(
                                            std::io::ErrorKind::NotFound,
                                            "working directory does not exist",
                                        )
                                    })?;
                                    if !canon.is_dir() {
                                        return Err(std::io::Error::new(
                                            std::io::ErrorKind::NotADirectory,
                                            "working directory is not a directory",
                                        ));
                                    }
                                    if !sec_for_cwd.is_workspace_path_allowed(&canon) {
                                        return Err(std::io::Error::new(
                                            std::io::ErrorKind::PermissionDenied,
                                            "working directory outside permitted workspace roots",
                                        ));
                                    }
                                    Ok(canon)
                                })
                                .await
                                {
                                    Ok(Ok(canon)) => Some(canon),
                                    Ok(Err(e))
                                        if e.kind() == std::io::ErrorKind::PermissionDenied =>
                                    {
                                        let _ = server_tx
                                            .send(ServerMsg::Error {
                                                code: ErrorCode::Unauthorized,
                                                message: "Working directory is outside allowed workspace roots".into(),
                                            })
                                            .await;
                                        continue;
                                    }
                                    Ok(Err(e)) => {
                                        let _ = server_tx
                                            .send(ServerMsg::Error {
                                                code: ErrorCode::InvalidMessage,
                                                message: format!("Invalid working directory: {e}"),
                                            })
                                            .await;
                                        continue;
                                    }
                                    Err(e) => {
                                        let _ = server_tx
                                            .send(ServerMsg::Error {
                                                code: ErrorCode::BackendError,
                                                message: e.to_string(),
                                            })
                                            .await;
                                        continue;
                                    }
                                }
                            }
                            _ => tokio::task::spawn_blocking(move || {
                                sec_for_cwd.resolve_default_cwd()
                            })
                            .await
                            .ok()
                            .flatten(),
                        };

                        let cwd_for_pty = resolved_cwd
                            .as_ref()
                            .map(|p| p.to_string_lossy().into_owned());

                        match state
                            .manager
                            .create(
                                oxidite_proto::BackendSpec::LocalPty {
                                    shell,
                                    cwd: cwd_for_pty,
                                },
                                size,
                            )
                            .await
                        {
                            Ok(sid) => {
                                if let Some(cwd_path) = resolved_cwd {
                                    state.manager.set_cwd(sid, cwd_path);
                                }
                                let _ = state
                                    .directory
                                    .claim(sid, state.node_id.clone(), Duration::from_secs(60))
                                    .await;
                                state.auth.set_session_owner(sid, &user_id);
                                AuditLogger::record(AuditEvent::SessionCreated {
                                    session_id: &sid.to_string(),
                                    backend_kind: "local_pty",
                                    user: &user_id,
                                });
                                current_session = Some(sid);
                                let _ = server_tx.send(ServerMsg::Created { session: sid }).await;
                            }
                            Err(e) => {
                                let _ = server_tx
                                    .send(ServerMsg::Error {
                                        code: ErrorCode::BackendError,
                                        message: e.to_string(),
                                    })
                                    .await;
                            }
                        }
                    }
                    oxidite_proto::BackendSpec::Ssh {
                        host,
                        port,
                        user,
                        auth,
                        multiplexer,
                    } => {
                        if let oxidite_proto::AuthSpec::PublicKey { ref key_path, .. } = auth {
                            let sec_ssh = state.security.clone();
                            let kp = key_path.clone();
                            let check = tokio::task::spawn_blocking(move || {
                                sec_ssh.validate_ssh_key_path(&kp)
                            })
                            .await;
                            if let Ok(Err(msg)) = check {
                                let _ = server_tx
                                    .send(ServerMsg::Error {
                                        code: ErrorCode::Unauthorized,
                                        message: msg.to_string(),
                                    })
                                    .await;
                                continue;
                            }
                        }

                        let sid = oxidite_proto::SessionId::new_v4();
                        pending_ssh_sid = Some(sid);
                        ssh_hostkey_reply = None;
                        ssh_auth_reply = None;

                        let ssh_cfg = oxidite_ssh::SshConfig {
                            host,
                            port,
                            user,
                            auth,
                            multiplexer,
                            keepalive_interval_secs: 15,
                            keepalive_max_misses: 3,
                        };
                        let known_hosts = state.known_hosts.clone();
                        let ptx = ssh_prompt_tx.clone();
                        let dtx = ssh_done_tx.clone();
                        tokio::spawn(async move {
                            let res = oxidite_ssh::SshBackend::connect(
                                sid,
                                ssh_cfg,
                                size,
                                known_hosts,
                                ptx,
                            )
                            .await;
                            if let Err(tokio::sync::mpsc::error::SendError((
                                _,
                                _,
                                Ok(mut backend),
                            ))) = dtx.send((sid, size, res)).await
                            {
                                let _ = oxidite_session::Backend::kill(&mut backend).await;
                            }
                        });
                    }
                }
            }
            ClientMsg::HostKeyDecision { accept } => {
                if let Some(reply) = ssh_hostkey_reply.take() {
                    let _ = reply.send(accept);
                }
            }
            ClientMsg::AuthResponse { answers } => {
                if let Some(reply) = ssh_auth_reply.take() {
                    let _ = reply.send(answers);
                }
            }
            ClientMsg::Attach {
                session,
                last_offset,
                mode,
            } => {
                let actual_mode = if let Some((shared_sid, share_mode)) = shared_access {
                    if session != shared_sid {
                        let _ = server_tx
                            .send(ServerMsg::Error {
                                code: ErrorCode::Unauthorized,
                                message: "Share token not valid for this session".into(),
                            })
                            .await;
                        continue;
                    }
                    share_mode
                } else {
                    if user_id != "admin" && !state.auth.is_session_owner(session, &user_id) {
                        let _ = server_tx
                            .send(ServerMsg::Error {
                                code: ErrorCode::Unauthorized,
                                message: "You do not own this session".into(),
                            })
                            .await;
                        continue;
                    }
                    mode
                };

                // Multi-node ownership lookup check
                if let Ok(Some(owner)) = state.directory.lookup(session).await {
                    if owner.node != state.node_id {
                        let _ = server_tx
                            .send(ServerMsg::Error {
                                code: ErrorCode::SessionNotFound,
                                message: format!(
                                    "Session {session} is hosted on remote node {}",
                                    owner.node
                                ),
                            })
                            .await;
                        continue;
                    }
                }

                if let Some(handle) = state.manager.get(session) {
                    current_session = Some(session);
                    current_tx = Some(handle.tx.clone());

                    let (resp_tx, resp_rx) = oneshot::channel();
                    if handle
                        .tx
                        .send(ActorCmd::Attach {
                            mode: actual_mode,
                            last_offset,
                            client_id: Some(client_id),
                            resp: resp_tx,
                        })
                        .await
                        .is_ok()
                    {
                        if let Ok(mut attach_res) = resp_rx.await {
                            AuditLogger::record(AuditEvent::SessionAttached {
                                session_id: &session.to_string(),
                                mode: match actual_mode {
                                    oxidite_proto::AttachMode::Writer => "writer",
                                    oxidite_proto::AttachMode::Viewer => "viewer",
                                },
                                user: &user_id,
                            });

                            let _ = server_tx
                                .send(ServerMsg::Attached {
                                    session,
                                    mode: actual_mode,
                                    start: attach_res.start.clone(),
                                    client_id: Some(client_id.to_string()),
                                })
                                .await;

                            if let Some(snap) = attach_res.snapshot {
                                let _ = server_tx
                                    .send(ServerMsg::Snapshot {
                                        offset: attach_res.current_offset,
                                        bytes: snap,
                                    })
                                    .await;
                            } else {
                                for chunk in attach_res.replay_chunks {
                                    let _ = server_tx
                                        .send(ServerMsg::Output {
                                            offset: last_offset.unwrap_or(0),
                                            bytes: chunk,
                                        })
                                        .await;
                                }
                            }

                            if let Some(prev) = session_forward_task.take() {
                                prev.abort();
                            }
                            let stx = server_tx.clone();
                            session_forward_task = Some(tokio::spawn(async move {
                                while let Ok(smsg) = attach_res.rx.recv().await {
                                    if stx.send(smsg).await.is_err() {
                                        break;
                                    }
                                }
                            }));
                        }
                    }
                } else if let Some(exit_state) = state.manager.get_exit_state(session) {
                    let message = match &exit_state {
                        SessionState::Exited { code: Some(c) } => {
                            format!("Session {session} exited with code {c}")
                        }
                        SessionState::Exited { code: None } => {
                            format!("Session {session} exited")
                        }
                        SessionState::Failed { reason } => reason.clone(),
                        _ => format!("Session {session} already ended"),
                    };
                    let _ = server_tx
                        .send(ServerMsg::State {
                            session,
                            state: exit_state.clone(),
                        })
                        .await;
                    if let SessionState::Exited { code } = exit_state {
                        let _ = server_tx.send(ServerMsg::Exit { code }).await;
                    }
                    let _ = server_tx
                        .send(ServerMsg::Error {
                            code: ErrorCode::SessionExited,
                            message,
                        })
                        .await;
                } else {
                    let _ = server_tx
                        .send(ServerMsg::Error {
                            code: ErrorCode::SessionNotFound,
                            message: format!("Session {session} not found"),
                        })
                        .await;
                }
            }
            ClientMsg::TakeoverWriter => {
                if let Some(tx) = &current_tx {
                    let (resp_tx, resp_rx) = oneshot::channel();
                    let _ = tx
                        .send(ActorCmd::TakeoverWriter {
                            client_id,
                            resp: resp_tx,
                        })
                        .await;
                    let _ = resp_rx.await;
                }
            }
            ClientMsg::Input(bytes) => {
                if let Some(tx) = &current_tx {
                    let _ = tx
                        .send(ActorCmd::Input {
                            client_id: Some(client_id),
                            bytes,
                        })
                        .await;
                }
            }
            ClientMsg::Resize(size) => {
                if let Some(tx) = &current_tx {
                    let _ = tx.send(ActorCmd::Resize { size }).await;
                }
            }
            ClientMsg::Ack { offset } => {
                if let Some(tx) = &current_tx {
                    let _ = tx
                        .send(ActorCmd::Ack {
                            client_id: Some(client_id),
                            offset,
                        })
                        .await;
                }
            }
            ClientMsg::Ping { nonce } => {
                let _ = server_tx.send(ServerMsg::Pong { nonce }).await;
            }
            ClientMsg::Detach => {
                if let Some(tx) = &current_tx {
                    let _ = tx
                        .send(ActorCmd::Detach {
                            client_id: Some(client_id),
                        })
                        .await;
                }
                break;
            }
            ClientMsg::Kill => {
                if let Some(sid) = current_session {
                    if user_id != "admin" && !state.auth.is_session_owner(sid, &user_id) {
                        let _ = server_tx
                            .send(ServerMsg::Error {
                                code: ErrorCode::Unauthorized,
                                message: "You do not own this session".into(),
                            })
                            .await;
                        continue;
                    }
                    AuditLogger::record(AuditEvent::SessionTerminated {
                        session_id: &sid.to_string(),
                        user: &user_id,
                    });
                    state.auth.remove_session_owner(sid);
                    if let Some(ref tx) = current_tx {
                        let _ = tx.send(ActorCmd::Kill).await;
                    }
                    state.manager.remove(sid);
                    current_tx = None;
                }
                if let Some(tx) = &current_tx {
                    let _ = tx.send(ActorCmd::Kill).await;
                }
                break;
            }
            _ => {}
        }
    }

    state.device_connections.unregister(client_id);
    if let Some(h) = session_forward_task.take() {
        h.abort();
    }
    if let Some(tx) = current_tx {
        let _ = tx
            .send(ActorCmd::Detach {
                client_id: Some(client_id),
            })
            .await;
    }
    forward_task.abort();
}
