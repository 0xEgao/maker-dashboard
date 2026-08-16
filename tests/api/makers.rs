//! Tests for maker management endpoints.

use std::net::TcpListener;

use axum::http::StatusCode;
use serde_json::json;

use super::{delete, get, post, put, test_app};

// 200 / success-path

#[tokio::test]
async fn list_makers_returns_empty_array() {
    let (status, body) = get(test_app(), "/makers").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "success": true, "data": [] }));
}

#[tokio::test]
async fn maker_count_is_zero_initially() {
    let (status, body) = get(test_app(), "/makers/count").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "success": true, "data": 0 }));
}

#[tokio::test]
async fn suggested_ports_return_defaults_when_available() {
    let (status, body) = get(test_app(), "/makers/ports/suggested").await;
    assert_eq!(status, StatusCode::OK);
    let network_port = body["data"]["network_port"].as_u64().unwrap();
    let rpc_port = body["data"]["rpc_port"].as_u64().unwrap();
    assert!(network_port > 0);
    assert!(rpc_port > 0);
    assert_ne!(network_port, rpc_port);
}

#[tokio::test]
async fn suggested_ports_skip_taken_defaults() {
    let app = test_app();

    let (first_status, first_body) = get(app.clone(), "/makers/ports/suggested").await;
    assert_eq!(first_status, StatusCode::OK);

    let first_network_port = first_body["data"]["network_port"].as_u64().unwrap();
    let first_rpc_port = first_body["data"]["rpc_port"].as_u64().unwrap();

    let network_listener = TcpListener::bind(format!("127.0.0.1:{first_network_port}")).unwrap();
    let rpc_listener = TcpListener::bind(format!("127.0.0.1:{first_rpc_port}")).unwrap();

    let (second_status, second_body) = get(app, "/makers/ports/suggested").await;

    drop(network_listener);
    drop(rpc_listener);

    assert_eq!(second_status, StatusCode::OK);
    assert_ne!(
        second_body["data"]["network_port"].as_u64().unwrap(),
        first_network_port
    );
    assert_ne!(
        second_body["data"]["rpc_port"].as_u64().unwrap(),
        first_rpc_port
    );
    assert_ne!(
        second_body["data"]["network_port"].as_u64().unwrap(),
        second_body["data"]["rpc_port"].as_u64().unwrap()
    );
}

#[tokio::test]
async fn backend_is_unconfigured_initially() {
    let (status, body) = get(test_app(), "/backend").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "success": true, "data": { "configured": false } })
    );
}

#[tokio::test]
async fn set_backend_bitcoind_requires_both_credentials() {
    let (status, body) = post(
        test_app(),
        "/backend",
        json!({ "kind": "bitcoind", "rpc_user": "alice" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(
        body["error"],
        "Both rpc_user and rpc_password must be provided for the bitcoind backend"
    );
}

#[tokio::test]
async fn set_backend_electrum_succeeds_and_hides_nothing_sensitive() {
    let app = test_app();
    let (status, body) = post(app.clone(), "/backend", json!({ "kind": "electrum" })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["configured"], true);
    assert_eq!(body["data"]["kind"], "electrum");

    let (status, body) = get(app, "/backend").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["configured"], true);
    // The RPC password is never exposed
    assert!(body["data"].get("rpc_password").is_none());
}

#[tokio::test]
async fn list_and_count_stay_empty_after_failed_create() {
    let app = test_app();
    // bitcoind backend pointing at a dead node: create fails at init.
    post(
        app.clone(),
        "/backend",
        json!({ "kind": "bitcoind", "rpc": "127.0.0.1:19998", "zmq": "tcp://127.0.0.1:19997", "rpc_user": "u", "rpc_password": "p" }),
    )
    .await;
    post(app.clone(), "/makers", json!({ "id": "fail" })).await;

    let (list_status, list_body) = get(app.clone(), "/makers").await;
    assert_eq!(list_status, StatusCode::OK);
    assert!(list_body["data"].as_array().unwrap().is_empty());

    let (count_status, count_body) = get(app.clone(), "/makers/count").await;
    assert_eq!(count_status, StatusCode::OK);
    assert_eq!(count_body["data"], 0);
}

// create requires the backend to be set first (startup screen)

#[tokio::test]
async fn create_without_backend_set_is_500() {
    let (status, body) = post(test_app(), "/makers", json!({ "id": "test" })).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!body["success"].as_bool().unwrap());
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Backend is not configured"));
}

// electrum backend: no node or credentials needed

#[tokio::test]
async fn create_with_electrum_backend_succeeds() {
    let app = test_app();
    let (status, _) = post(app.clone(), "/backend", json!({ "kind": "electrum" })).await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = post(app, "/makers", json!({ "id": "test" })).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["success"].as_bool().unwrap());
}

// 500 when Bitcoin is unavailable (bitcoind backend)

#[tokio::test]
async fn create_returns_500_without_bitcoin() {
    let app = test_app();
    post(
        app.clone(),
        "/backend",
        json!({ "kind": "bitcoind", "rpc": "127.0.0.1:19998", "zmq": "tcp://127.0.0.1:19997", "rpc_user": "alice", "rpc_password": "pass" }),
    )
    .await;

    let (status, body) = post(app, "/makers", json!({ "id": "test" })).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!body["success"].as_bool().unwrap());
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn create_skips_taken_local_network_port() {
    let app = test_app();
    post(
        app.clone(),
        "/backend",
        json!({ "kind": "bitcoind", "rpc_user": "alice", "rpc_password": "pass" }),
    )
    .await;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let (status, body) = post(
        app,
        "/makers",
        json!({
            "id": "test",
            "network_port": port,
        }),
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!body["success"].as_bool().unwrap());
    assert_ne!(
        body["error"],
        format!("network_port {port} is already in use on this machine")
    );
}

#[tokio::test]
async fn create_skips_taken_local_rpc_port() {
    let app = test_app();
    post(
        app.clone(),
        "/backend",
        json!({ "kind": "bitcoind", "rpc_user": "alice", "rpc_password": "pass" }),
    )
    .await;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let (status, body) = post(
        app,
        "/makers",
        json!({
            "id": "test",
            "rpc_port": port,
        }),
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!body["success"].as_bool().unwrap());
    assert_ne!(
        body["error"],
        format!("rpc_port {port} is already in use on this machine")
    );
}

// 404 for unknown maker

#[tokio::test]
async fn get_unknown_maker_is_404() {
    let (status, body) = get(test_app(), "/makers/unknown").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["error"], "Maker 'unknown' not found");
}

#[tokio::test]
async fn get_maker_info_unknown_is_404() {
    let (status, body) = get(test_app(), "/makers/unknown/info").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body["success"].as_bool().unwrap());
}

#[tokio::test]
async fn delete_unknown_maker_is_404() {
    let (status, body) = delete(test_app(), "/makers/unknown").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["error"], "Maker 'unknown' not found");
}

#[tokio::test]
async fn start_unknown_maker_is_404() {
    let (status, body) = post(test_app(), "/makers/unknown/start", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["error"], "Maker 'unknown' not found");
}

#[tokio::test]
async fn stop_unknown_maker_is_404() {
    let (status, body) = post(test_app(), "/makers/unknown/stop", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["error"], "Maker 'unknown' not found");
}

#[tokio::test]
async fn restart_unknown_maker_is_404() {
    let (status, body) = post(test_app(), "/makers/unknown/restart", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["error"], "Maker 'unknown' not found");
}

#[tokio::test]
async fn update_config_unknown_maker_is_404() {
    let (status, body) = put(test_app(), "/makers/unknown/config", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["error"], "Maker 'unknown' not found");
}
