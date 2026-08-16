use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use tokio::sync::Mutex;

use super::{
    dto::{ApiResponse, BackendInfo, SetBackendRequest},
    AppState,
};
use crate::maker_manager::{MakerBackend, MakerManager, RuntimeBackend};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/backend", get(get_backend))
        .route("/backend", post(set_backend))
}

/// Get the currently configured backend (runtime-only, never persisted).
/// The RPC password is never returned.
#[utoipa::path(
    get, path = "/api/backend", tag = "onboarding",
    responses((status = 200, description = "Current backend config", body = ApiResponse<BackendInfo>))
)]
async fn get_backend(
    State(state): State<Arc<Mutex<MakerManager>>>,
) -> Json<ApiResponse<BackendInfo>> {
    let mgr = state.lock().await;
    let info = match mgr.backend() {
        Some(b) => BackendInfo {
            configured: true,
            kind: Some(b.kind),
            rpc: Some(b.rpc.clone()),
            zmq: Some(b.zmq.clone()),
            rpc_user: Some(b.rpc_user.clone()),
            electrum_url: Some(b.electrum_url.clone()),
        },
        None => BackendInfo {
            configured: false,
            kind: None,
            rpc: None,
            zmq: None,
            rpc_user: None,
            electrum_url: None,
        },
    };
    Json(ApiResponse::ok(info))
}

/// Set the backend for all makers. Entered from the startup screen at every
/// dashboard start; held in memory only, never written to disk. Setting it
/// initializes all registered makers and auto-starts those whose wallet opens
/// without a password.
#[utoipa::path(
    post, path = "/api/backend", tag = "onboarding",
    request_body = SetBackendRequest,
    responses(
        (status = 200, description = "Backend set", body = ApiResponse<BackendInfo>),
        (status = 400, description = "Bad request", body = ApiResponse<BackendInfo>)
    )
)]
async fn set_backend(
    State(state): State<Arc<Mutex<MakerManager>>>,
    Json(body): Json<SetBackendRequest>,
) -> (StatusCode, Json<ApiResponse<BackendInfo>>) {
    let defaults = RuntimeBackend::default();

    if body.kind == MakerBackend::Bitcoind {
        match (&body.rpc_user, &body.rpc_password) {
            (Some(_), Some(_)) => {}
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::err(
                        "Both rpc_user and rpc_password must be provided for the bitcoind backend",
                    )),
                )
            }
        }
    }

    let backend = RuntimeBackend {
        kind: body.kind,
        rpc: body.rpc.unwrap_or(defaults.rpc),
        zmq: body.zmq.unwrap_or(defaults.zmq),
        rpc_user: body.rpc_user.unwrap_or(defaults.rpc_user),
        rpc_password: body.rpc_password.unwrap_or(defaults.rpc_password),
        electrum_url: body.electrum_url.unwrap_or(defaults.electrum_url),
    };

    let info = BackendInfo {
        configured: true,
        kind: Some(backend.kind),
        rpc: Some(backend.rpc.clone()),
        zmq: Some(backend.zmq.clone()),
        rpc_user: Some(backend.rpc_user.clone()),
        electrum_url: Some(backend.electrum_url.clone()),
    };

    state.lock().await.set_backend(backend);
    (StatusCode::OK, Json(ApiResponse::ok(info)))
}
