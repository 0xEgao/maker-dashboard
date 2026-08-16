//! Tests for dashboard authentication and first-run setup.

use std::sync::Arc;

use axum::http::StatusCode;
use axum::Router;
use serde_json::json;
use tokio::sync::Mutex;

use maker_dashboard::{
    api::{api_router, AppState},
    auth::SessionStore,
    maker_manager::MakerManager,
};

use super::{get, post, temp_config_dir};

fn setup_app(config_dir: std::path::PathBuf) -> Router {
    setup_app_with_secure_cookies(config_dir, true)
}

fn setup_app_with_secure_cookies(config_dir: std::path::PathBuf, secure_cookies: bool) -> Router {
    let manager = MakerManager::new_for_testing(config_dir.clone()).expect("MakerManager::new");
    let state = AppState {
        makers: Arc::new(Mutex::new(manager)),
        sessions: Arc::new(Mutex::new(SessionStore::new())),
        auth: Arc::new(std::sync::RwLock::new(None)),
        setup_lock: Arc::new(Mutex::new(())),
        config_dir: Arc::new(config_dir),
        secure_cookies,
    };
    api_router().with_state(state)
}

#[tokio::test]
async fn setup_initializes_fresh_dashboard() {
    let config_dir = temp_config_dir();
    std::fs::create_dir_all(&config_dir).unwrap();
    let app = setup_app(config_dir.clone());

    let (status, body) = post(
        app.clone(),
        "/auth/setup",
        json!({ "password": "test-password" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "success": true, "data": null }));

    let (status, body) = get(app, "/auth/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["password_exists"], true);

    // Login-only model: auth.json holds just the argon2id hash, and no
    // makers.json is ever written (maker config lives in per-maker config.toml).
    let auth = std::fs::read_to_string(config_dir.join("auth.json")).unwrap();
    let auth: serde_json::Value = serde_json::from_str(&auth).unwrap();
    assert!(auth["password_hash"].is_string());
    assert!(auth.get("enc_salt").is_none());
    assert!(!config_dir.join("makers.json").exists());
}

#[tokio::test]
async fn setup_refuses_when_already_initialized() {
    let config_dir = temp_config_dir();
    std::fs::create_dir_all(&config_dir).unwrap();
    let app = setup_app(config_dir);

    let (status, _) = post(
        app.clone(),
        "/auth/setup",
        json!({ "password": "test-password" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = post(app, "/auth/setup", json!({ "password": "other-password" })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn setup_session_cookie_can_disable_secure_attribute() {
    let config_dir = temp_config_dir();
    std::fs::create_dir_all(&config_dir).unwrap();
    let app = setup_app_with_secure_cookies(config_dir, false);

    let (status, body) = post(app, "/auth/setup", json!({ "password": "test-password" })).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "success": true, "data": null }));
}
