use sluice_core::config::{AppConfig, ProviderConfig, ResolvedProvider};
use sluice_server::{AppState, LoginLimiter, Sessions, VendorIndex, proxy_handler};
use std::collections::HashMap;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_state(providers: HashMap<String, ResolvedProvider>) -> AppState {
    let config = AppConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        providers,
        master_key: vec![0u8; 32],
        admin_password: "change-me".to_string(),
        db_url: None,
        master_key_is_ephemeral: false,
    };
    AppState {
        config: std::sync::Arc::new(config),
        db_pool: None,
        sessions: Sessions::default(),
        login_limiter: LoginLimiter::default(),
        vendors: VendorIndex::default(),
    }
}

fn make_router(state: AppState) -> axum::Router {
    axum::Router::new()
        .fallback(axum::routing::any(proxy_handler))
        .with_state(state)
}

fn anthropic_provider(mock_uri: String) -> ResolvedProvider {
    ResolvedProvider {
        config: ProviderConfig {
            name: "anthropic".into(),
            upstream: mock_uri,
            auth_header: "x-api-key".into(),
            auth_format: "{key}".into(),
        },
        api_key: "sk-test-key".into(),
    }
}

fn state_with_anthropic(mock_uri: String) -> AppState {
    let mut providers = HashMap::new();
    providers.insert("anthropic".into(), anthropic_provider(mock_uri));
    make_state(providers)
}

#[tokio::test]
async fn byte_exact_body_forwarded() {
    let mock = MockServer::start().await;

    let payload = br#"{"model":"claude-3-5-sonnet-20241022","thinking":"{\"type\":\"enabled\",\"budget_tokens\":4096}","messages":[{"role":"user","content":"hello"}]}"#;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(payload.to_vec()))
        .expect(1)
        .mount(&mock)
        .await;

    let state = state_with_anthropic(mock.uri());
    let router = make_router(state);

    let req = axum::http::Request::builder()
        .method("POST")
        .uri("http://localhost/anthropic/v1/messages")
        .header("Authorization", "Bearer test-token")
        .body(axum::body::Body::from(payload.to_vec()))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], payload);
}

#[tokio::test]
async fn sse_response_body_byte_exact() {
    let mock = MockServer::start().await;

    let sse_body = b"data: hello\n\ndata: world\n\ndata: done\n\n";

    Mock::given(method("GET"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(sse_body.to_vec())
                .insert_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&mock)
        .await;

    let state = state_with_anthropic(mock.uri());
    let router = make_router(state);

    let req = axum::http::Request::builder()
        .method("GET")
        .uri("http://localhost/anthropic/v1/messages")
        .header("Authorization", "Bearer test-token")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], sse_body);
}

#[tokio::test]
async fn missing_token_returns_401_no_upstream_call() {
    let mock = MockServer::start().await;

    Mock::given(method("*"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&mock)
        .await;

    let state = state_with_anthropic(mock.uri());
    let router = make_router(state);

    let req = axum::http::Request::builder()
        .method("GET")
        .uri("http://localhost/anthropic/v1/messages")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 401);
}

/// A vendor configured in the config file is reachable without a database, but
/// only through the DB path in `proxy_handler` — which returns 503 when there is
/// no pool. This test pins that behaviour: without a token store, the gateway
/// must refuse rather than accept any non-empty token.
#[tokio::test]
async fn no_database_refuses_to_proxy() {
    let mock = MockServer::start().await;

    Mock::given(method("*"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&mock)
        .await;

    let state = state_with_anthropic(mock.uri());
    let router = make_router(state);

    let req = axum::http::Request::builder()
        .method("GET")
        .uri("http://localhost/anthropic/v1/messages")
        .header("Authorization", "Bearer any-non-empty-token")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 503);
}
