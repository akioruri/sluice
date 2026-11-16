use crate::AppState;
use crate::auth::{generate_api_token, password_matches, require_admin};
use axum::{
    Router,
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
};
use sluice_core::config::{ADMIN_USERNAME, validate_upstream_url, validate_vendor_name};
use sluice_core::token::sha2_hash;

/// Path segments owned by the gateway itself.
///
/// The proxy picks the upstream vendor from the first path segment, so a vendor
/// named `vendors` would answer `/vendors` instead of the web UI, and one named
/// `api` would swallow `/api/admin/*`. These names are therefore refused at
/// creation time and never proxied.
pub const RESERVED_VENDOR_NAMES: &[&str] = &[
    "api",
    "_app",
    "healthz",
    "dashboard",
    "vendors",
    "tokens",
    "stats",
    "auth",
];

pub fn is_reserved_vendor_name(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    RESERVED_VENDOR_NAMES.contains(&lowered.as_str())
}

#[derive(serde::Deserialize)]
struct CreateProviderRequest {
    name: String,
    upstream: String,
    auth_header: String,
    auth_format: String,
    #[serde(default)]
    models: String,
    api_key: String,
}

#[derive(serde::Deserialize)]
struct UpdateProviderRequest {
    upstream: Option<String>,
    auth_header: Option<String>,
    auth_format: Option<String>,
    models: Option<String>,
    api_key: Option<String>,
}

async fn list_providers(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(ref pool) = state.db_pool {
        match crate::db::list_providers(pool).await {
            Ok(providers) => {
                let providers_json: Vec<_> = providers
                    .into_iter()
                    .map(|p| {
                        serde_json::json!({
                            "id": p.id,
                            "name": p.name,
                            "upstream": p.upstream,
                            "auth_header": p.auth_header,
                            "auth_format": p.auth_format,
                            "models": p.models,
                        })
                    })
                    .collect();
                Ok(Json(serde_json::json!({ "providers": providers_json })))
            }
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    } else {
        Ok(Json(
            serde_json::json!({ "providers": [], "note": "database not configured" }),
        ))
    }
}

async fn list_tokens(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(ref pool) = state.db_pool {
        match crate::db::list_tokens(pool).await {
            Ok(tokens) => {
                let tokens_json: Vec<_> = tokens
                    .into_iter()
                    .map(|t| {
                        serde_json::json!({
                            "prefix": t.token_prefix,
                            "name": t.name,
                            "created_at": t.created_at,
                            "revoked": t.revoked_at.is_some(),
                        })
                    })
                    .collect();
                Ok(Json(serde_json::json!({ "tokens": tokens_json })))
            }
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    } else {
        Ok(Json(
            serde_json::json!({ "tokens": [], "note": "database not configured" }),
        ))
    }
}

async fn get_stats(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(ref pool) = state.db_pool {
        match crate::db::get_stats(pool).await {
            Ok(stats) => Ok(Json(serde_json::json!({
                "total_requests": stats.total_requests,
                "by_provider": stats.by_provider,
            }))),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    } else {
        Ok(Json(
            serde_json::json!({ "total_requests": 0, "by_provider": [], "note": "database not configured" }),
        ))
    }
}

/// Serves the values the console needs that are not stored per-vendor.
///
/// The reserved-name list is the important one: it lives on the server so the
/// browser cannot drift out of step with what the router will actually refuse.
async fn get_config() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "reserved_names": RESERVED_VENDOR_NAMES,
        "default_auth_format": "Bearer {key}",
        "default_auth_header": "Authorization",
    }))
}

/// Rejects a vendor name that could never be routed to.
fn check_vendor_name(name: &str) -> Result<String, StatusCode> {
    let name = validate_vendor_name(name).map_err(|_| StatusCode::BAD_REQUEST)?;
    if is_reserved_vendor_name(&name) {
        return Err(StatusCode::CONFLICT);
    }
    Ok(name)
}

fn check_upstream(upstream: &str) -> Result<(), StatusCode> {
    validate_upstream_url(upstream)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .map(|_| ())
}

fn check_auth(header: &str, format: &str) -> Result<(), StatusCode> {
    if header.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if axum::http::HeaderName::from_bytes(header.trim().as_bytes()).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !format.contains("{key}") {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

async fn create_provider(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<CreateProviderRequest>,
) -> Result<StatusCode, StatusCode> {
    let Some(pool) = state.db_pool.clone() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let name = check_vendor_name(&req.name)?;
    check_upstream(&req.upstream)?;
    check_auth(&req.auth_header, &req.auth_format)?;

    // A duplicate name would violate the unique constraint and surface as a 500;
    // report it as a conflict the UI can explain instead.
    if crate::db::get_provider(&pool, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_some()
    {
        return Err(StatusCode::CONFLICT);
    }

    // Encrypt before touching the database, so a bad key fails the request
    // rather than leaving a half-created vendor behind.
    let encrypted_key = sluice_core::crypto::encrypt(&req.api_key, &state.config.master_key)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let provider_id = crate::db::create_provider(
        &pool,
        &name,
        &req.upstream,
        &req.auth_header,
        &req.auth_format,
        &req.models,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    crate::db::upsert_provider_key(&pool, provider_id, &encrypted_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state.vendors.invalidate();
    Ok(StatusCode::CREATED)
}

async fn update_provider(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<UpdateProviderRequest>,
) -> Result<StatusCode, StatusCode> {
    let Some(pool) = state.db_pool.clone() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let existing = crate::db::get_provider(&pool, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Partial updates merge onto the stored values, so the request only has to
    // carry the fields it actually changes.
    let upstream = match &req.upstream {
        Some(u) => {
            check_upstream(u)?;
            u.clone()
        }
        None => existing.upstream.clone(),
    };
    let auth_header = match &req.auth_header {
        Some(h) => {
            if h.trim().is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            h.clone()
        }
        None => existing.auth_header.clone(),
    };
    let auth_format = match &req.auth_format {
        Some(f) => f.clone(),
        None => existing.auth_format.clone(),
    };
    check_auth(&auth_header, &auth_format)?;

    let models = match &req.models {
        Some(m) => m.clone(),
        None => existing.models.clone(),
    };

    // One statement, so a failed update cannot leave half the settings applied.
    crate::db::update_provider(&pool, &name, &upstream, &auth_header, &auth_format, &models)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(api_key) = &req.api_key {
        if api_key.trim().is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        let encrypted_key = sluice_core::crypto::encrypt(api_key, &state.config.master_key)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        crate::db::upsert_provider_key(&pool, existing.id, &encrypted_key)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    state.vendors.invalidate();
    Ok(StatusCode::OK)
}

async fn delete_provider(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<StatusCode, StatusCode> {
    let Some(pool) = state.db_pool.clone() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let deleted = crate::db::delete_provider(&pool, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    state.vendors.invalidate();
    Ok(StatusCode::OK)
}

pub fn admin_router(state: AppState) -> Router<AppState> {
    let protected = Router::new()
        .route("/providers", get(list_providers).post(create_provider))
        .route(
            "/providers/{name}",
            put(update_provider).delete(delete_provider),
        )
        .route("/providers/{name}/test", post(test_provider))
        .route("/providers/{name}/models", post(list_provider_models))
        .route("/tokens", get(list_tokens).post(create_token))
        .route("/tokens/{prefix}", delete(revoke_token))
        .route("/stats", get(get_stats))
        .route("/stats/daily", get(get_daily_stats))
        .route("/config", get(get_config))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_admin,
        ));

    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .merge(protected)
}

#[derive(serde::Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(serde::Deserialize)]
struct CreateTokenRequest {
    #[serde(default)]
    name: Option<String>,
}

/// Optional inline config so a vendor can be verified before it is saved.
///
/// `upstream` + `api_key` together mean "use these"; anything missing falls back
/// to the stored provider. That lets the UI test a brand-new vendor (nothing
/// stored yet) and an existing one (whose key is never sent back to the browser).
#[derive(serde::Deserialize, Default)]
struct ProviderProbeRequest {
    #[serde(default)]
    upstream: Option<String>,
    #[serde(default)]
    auth_header: Option<String>,
    #[serde(default)]
    auth_format: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
    /// When set, only these models are probed. Empty/absent means "all of them".
    #[serde(default)]
    models: Option<Vec<String>>,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

/// Resolves the credentials to probe: inline values when supplied, otherwise the
/// stored provider and its decrypted key.
async fn resolve_probe_credentials(
    state: &AppState,
    name: &str,
    req: &ProviderProbeRequest,
) -> Result<(String, String, String, String), StatusCode> {
    if let (Some(upstream), Some(api_key)) = (
        non_empty(req.upstream.clone()),
        non_empty(req.api_key.clone()),
    ) {
        check_upstream(&upstream)?;
        return Ok((
            upstream,
            non_empty(req.auth_header.clone()).unwrap_or_else(|| "Authorization".to_string()),
            non_empty(req.auth_format.clone()).unwrap_or_else(|| "Bearer {key}".to_string()),
            api_key,
        ));
    }

    let pool = state.db_pool.as_ref().ok_or(StatusCode::NOT_FOUND)?;

    let provider = crate::db::get_provider(pool, name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let encrypted = crate::db::get_provider_key(pool, provider.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let api_key = sluice_core::crypto::decrypt(&encrypted, &state.config.master_key)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        provider.upstream,
        provider.auth_header,
        provider.auth_format,
        api_key,
    ))
}

/// Verifies a vendor's upstream and credentials.
///
/// With `models` set, each listed model is probed individually and the response
/// reports which ones worked, so the UI can show per-model results. Without it,
/// a single model-list call is used as a cheap connectivity check.
async fn test_provider(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<ProviderProbeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (upstream, auth_header, auth_format, api_key) =
        resolve_probe_credentials(&state, &name, &req).await?;

    let wanted: Vec<String> = req
        .models
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();

    // Per-model mode: probe each selected model on its own.
    if !wanted.is_empty() {
        let mut results = Vec::with_capacity(wanted.len());
        let mut ok_count = 0usize;

        for model in &wanted {
            let outcome = crate::proxy::probe_upstream_model(
                &upstream,
                &auth_header,
                &auth_format,
                &api_key,
                model,
            )
            .await;

            match outcome {
                Ok(latency) => {
                    ok_count += 1;
                    results.push(
                        serde_json::json!({ "model": model, "ok": true, "latency_ms": latency }),
                    );
                }
                Err(e) => {
                    results.push(serde_json::json!({
                        "model": model,
                        "ok": false,
                        "status": e.status,
                        "error": e.message,
                    }));
                }
            }
        }

        return Ok(Json(serde_json::json!({
            "ok": ok_count == wanted.len(),
            "tested": wanted.len(),
            "passed": ok_count,
            "results": results,
        })));
    }

    // Whole-vendor mode.
    let result =
        crate::proxy::probe_upstream_models(&upstream, &auth_header, &auth_format, &api_key).await;

    let body = match result {
        Ok(probe) => serde_json::json!({
            "ok": true,
            "models": probe.models.len(),
            "latency_ms": probe.latency_ms,
        }),
        Err(e) => serde_json::json!({
            "ok": false,
            "status": e.status,
            "error": e.message,
        }),
    };

    Ok(Json(body))
}

/// Lists the models an upstream exposes, so the UI can populate the field.
async fn list_provider_models(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<ProviderProbeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (upstream, auth_header, auth_format, api_key) =
        resolve_probe_credentials(&state, &name, &req).await?;

    let result =
        crate::proxy::probe_upstream_models(&upstream, &auth_header, &auth_format, &api_key).await;

    let body = match result {
        Ok(probe) => serde_json::json!({
            "models": probe.models,
            "latency_ms": probe.latency_ms,
        }),
        Err(e) => serde_json::json!({
            "models": [],
            "error": e.message,
            "status": e.status,
        }),
    };

    Ok(Json(body))
}

/// Admin login. The only admin account is `root`; its password comes from
/// `SLUICE_ADMIN_PASSWORD` (default `change-me`).
async fn login(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Throttle before touching the password, so a brute-force spends its time
    // waiting rather than guessing.
    if !state.login_limiter.allows() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    // Both comparisons run in constant time, so neither the wrong username nor a
    // partially correct password shortens the response.
    let username_ok = password_matches(&req.username, ADMIN_USERNAME);
    let password_ok = password_matches(&req.password, &state.config.admin_password);

    if !(username_ok && password_ok) {
        state.login_limiter.record_failure();
        tracing::warn!("failed admin login attempt");
        return Err(StatusCode::UNAUTHORIZED);
    }

    state.login_limiter.record_success();
    let session = crate::auth::generate_session_token();
    state.sessions.insert(session.clone());

    Ok(Json(serde_json::json!({
        "token": session,
        "username": req.username,
    })))
}

async fn logout(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, StatusCode> {
    if let Some(bearer) = crate::auth::bearer_token(&headers) {
        state.sessions.remove(&bearer);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Creates a platform API key. The raw token is returned exactly once — only its
/// hash is persisted.
///
/// A name is required and must be unique among keys that are still in use, so
/// the admin list and the usage charts stay unambiguous.
async fn create_token(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<CreateTokenRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let pool = state
        .db_pool
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_string();

    // Retired keys keep their row for usage history, so only active ones are
    // considered when checking for a clash.
    let taken = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM unified_tokens WHERE name = $1 AND revoked_at IS NULL",
    )
    .bind(&name)
    .fetch_one(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if taken > 0 {
        return Err(StatusCode::CONFLICT);
    }

    let token = generate_api_token();
    let token_hash = sha2_hash(&token);
    // Keep the prefix inside the column: "sk-" + 8 chars.
    let prefix: String = token.chars().take(11).collect();

    crate::db::create_token(pool, &token_hash, &prefix, Some(&name))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "token": token,
        "prefix": prefix,
        "name": name,
    })))
}

/// Daily request counts for the dashboard charts.
async fn get_daily_stats(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let pool = state
        .db_pool
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let (by_provider, by_token) = crate::db::get_daily_usage(pool, 30)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "by_provider": by_provider,
        "by_token": by_token,
    })))
}

/// Retires an API key. Returns 404 when the prefix matches no active key, so a
/// stale UI cannot report success for something that was never revoked.
async fn revoke_token(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(prefix): axum::extract::Path<String>,
) -> Result<StatusCode, StatusCode> {
    let pool = state
        .db_pool
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let revoked = crate::db::revoke_token_by_prefix(pool, &prefix)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !revoked {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
