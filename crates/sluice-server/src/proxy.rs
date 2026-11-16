use crate::AppState;
use crate::db::{ProviderInfo, get_provider, get_provider_key, resolve_active_token};
use axum::body::Body;
use futures_util::TryStreamExt;
use sluice_core::crypto::decrypt;
use sluice_core::token::sha2_hash;

/// Total time a single upstream call may take, streaming included.
///
/// `connect_timeout` only covers the TCP/TLS handshake; without this, an
/// upstream that accepts the connection and then stalls would hold the client
/// connection open indefinitely.
const UPSTREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

pub async fn proxy_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::http::Request<Body>,
) -> Result<axum::response::Response<Body>> {
    let headers = req.headers().clone();
    let uri = req.uri().clone();
    let method = req.method().clone();

    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?
        .trim();

    if token.is_empty() {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    let Some(pool) = state.db_pool.clone() else {
        // Without a database there is no token store to validate against, so
        // accepting the request would hand out the vendor's key to anyone who
        // typed a non-empty string. Refuse instead.
        tracing::error!(
            "DATABASE_URL is not set: refusing to proxy, because tokens cannot be validated"
        );
        return Err(axum::http::StatusCode::SERVICE_UNAVAILABLE);
    };

    // Resolved once so usage rows can be attributed to this key.
    let token_id = resolve_active_token(&pool, &sha2_hash(token))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to look up token");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    let provider_name = extract_provider(&uri)?;
    let method_str = method.as_str().to_string();

    // Look up provider from the database; config-file providers are a legacy
    // fallback for deployments that predate the admin console.
    let (provider_config, api_key) = if let Some(ref pool) = state.db_pool {
        let provider = get_provider(pool, &provider_name).await.map_err(|e| {
            tracing::error!(error = %e, provider = %provider_name, "failed to load provider");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;
        match provider {
            Some(p) => {
                let encrypted_key = get_provider_key(pool, p.id)
                    .await
                    .map_err(|e| {
                        tracing::error!(error = %e, provider = %provider_name, "failed to load vendor key");
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR
                    })?;
                let api_key = match encrypted_key {
                    Some(k) => decrypt(&k, &state.config.master_key).map_err(|e| {
                        tracing::error!(error = %e, provider = %provider_name, "failed to decrypt vendor key");
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR
                    })?,
                    None => {
                        tracing::error!(provider = %provider_name, "vendor has no stored key");
                        return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
                    }
                };
                (p, api_key)
            }
            None => return Err(axum::http::StatusCode::NOT_FOUND),
        }
    } else if let Some(p) = state.config.providers.get(&provider_name) {
        // Unreachable in practice: the branch above already returned 503 when
        // there is no pool. Kept so the config path stays testable.
        let info = ProviderInfo {
            id: 0,
            name: p.config.name.clone(),
            upstream: p.config.upstream.clone(),
            auth_header: p.config.auth_header.clone(),
            auth_format: p.config.auth_format.clone(),
            models: String::new(),
        };
        (info, p.api_key.clone())
    } else {
        return Err(axum::http::StatusCode::NOT_FOUND);
    };

    let upstream_uri = build_upstream_uri(&uri, &provider_config)?;

    let new_headers = build_upstream_headers(&headers, &provider_config, &api_key)?;

    // Enforce the vendor's model whitelist, if one is configured. An empty list
    // means "unrestricted".
    //
    // The body has to be inspected before it is forwarded, so the request side is
    // buffered. That is bounded by the router's body limit and is the only place
    // the whole payload is held in memory.
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    if let Some(denied) = model_not_allowed(&provider_config.models, &body_bytes) {
        tracing::info!(
            provider = %provider_name,
            model = %denied,
            "rejected request for a model outside the vendor's list"
        );
        return Err(axum::http::StatusCode::FORBIDDEN);
    }

    let client = crate::http_client();
    let start = std::time::Instant::now();

    let upstream_result = client
        .request(method, upstream_uri.to_string())
        .headers(new_headers)
        .body(body_bytes)
        .timeout(UPSTREAM_TIMEOUT)
        .send()
        .await;

    let duration_ms = start.elapsed().as_millis() as i32;

    // Record the attempt whether or not the upstream answered, so the usage
    // charts reflect failures too rather than only successful calls.
    {
        let pool = pool.clone();
        let provider_name = provider_name.clone();
        let path = uri.path().to_string();
        let status_code = match &upstream_result {
            Ok(resp) => resp.status().as_u16() as i32,
            Err(_) => 502,
        };
        tokio::spawn(async move {
            if let Err(e) = crate::db::record_usage(
                &pool,
                Some(token_id),
                &provider_name,
                &path,
                &method_str,
                status_code,
                duration_ms,
            )
            .await
            {
                tracing::warn!(error = %e, "failed to record usage");
            }
        });
    }

    let upstream_resp = upstream_result
        .inspect_err(|e| tracing::error!(error = %e, "upstream request failed"))
        .map_err(|_| axum::http::StatusCode::BAD_GATEWAY)?;

    let status = upstream_resp.status();
    let mut response_builder = axum::response::Response::builder().status(status);

    for (name, value) in upstream_resp.headers() {
        if !is_hop_by_hop(name)
            && let Ok(header_value) = axum::http::HeaderValue::from_bytes(value.as_bytes())
        {
            response_builder = response_builder.header(name, header_value);
        }
    }

    // Relay the body as a stream rather than collecting it: SSE responses arrive
    // over minutes and must reach the client as they are produced.
    let stream = upstream_resp.bytes_stream().map_err(std::io::Error::other);

    let response = response_builder
        .body(Body::from_stream(stream))
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(response)
}

fn extract_provider(uri: &axum::http::Uri) -> Result<String> {
    let path = uri.path();
    let path = path.strip_prefix('/').unwrap_or(path);
    let vendor = path
        .split('/')
        .next()
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    if vendor.is_empty() {
        return Err(axum::http::StatusCode::NOT_FOUND);
    }
    Ok(vendor.to_string())
}

fn build_upstream_uri(
    original: &axum::http::Uri,
    provider: &impl ProviderSpec,
) -> Result<axum::http::Uri> {
    let path = original.path();
    let path = path.strip_prefix('/').unwrap_or(path);
    let remainder = path.split('/').skip(1).collect::<Vec<_>>().join("/");
    let remainder = format!("/{}", remainder);

    let upstream = provider.upstream().trim_end_matches('/');
    let query = original
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();

    let full = format!("{}{}{}", upstream, remainder, query);
    full.parse::<axum::http::Uri>()
        .map_err(|_| axum::http::StatusCode::BAD_GATEWAY)
}

fn build_upstream_headers(
    client_headers: &axum::http::HeaderMap,
    provider: &impl ProviderSpec,
    api_key: &str,
) -> Result<reqwest::header::HeaderMap> {
    use reqwest::header::*;

    let mut new_headers = reqwest::header::HeaderMap::new();

    for (name, value) in client_headers {
        let name_lower = name.as_str().to_lowercase();
        if is_hop_by_hop_lower(&name_lower)
            // The client's own credentials are replaced by the vendor's.
            || name_lower == "authorization"
            || name_lower == provider.auth_header().to_lowercase()
            // The client's Host points at the gateway, not the upstream. Let
            // reqwest derive the correct Host from the upstream URL.
            || name_lower == "host"
            // Framing is decided by the body actually being sent. Forwarding the
            // client's value would let a client declare a length that does not
            // match the bytes, and hyper honours a user-set Content-Length.
            || name_lower == "content-length"
            // Client cookies have nothing to do with the upstream vendor.
            || name_lower == "cookie"
        {
            continue;
        }

        if let Ok(h_name) = HeaderName::from_bytes(name.as_str().as_bytes())
            && let Ok(h_value) = HeaderValue::from_bytes(value.as_bytes())
        {
            new_headers.insert(h_name, h_value);
        }
    }

    let auth_value = provider.auth_format().replace("{key}", api_key);
    let auth_header_name = HeaderName::from_bytes(provider.auth_header().as_bytes())
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let auth_header_value = HeaderValue::from_str(&auth_value)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    new_headers.insert(auth_header_name, auth_header_value);

    Ok(new_headers)
}

fn is_hop_by_hop(name: &axum::http::HeaderName) -> bool {
    is_hop_by_hop_lower(&name.as_str().to_lowercase())
}

fn is_hop_by_hop_lower(lower: &str) -> bool {
    matches!(
        lower,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

pub type Result<T> = std::result::Result<T, axum::http::StatusCode>;

/// Probes a single model with a minimal chat request.
///
/// Used by the admin "test" action when specific models are selected, so the
/// result reflects the models the gateway will actually serve rather than just
/// whether the credentials work. Returns the round-trip time in milliseconds.
pub async fn probe_upstream_model(
    upstream: &str,
    auth_header: &str,
    auth_format: &str,
    api_key: &str,
    model: &str,
) -> std::result::Result<u128, UpstreamProbeError> {
    use reqwest::header::*;

    let upstream = upstream.trim_end_matches('/');
    if !upstream.starts_with("http://") && !upstream.starts_with("https://") {
        return Err(UpstreamProbeError {
            status: None,
            message: format!("invalid upstream URL: {upstream} (expected http:// or https://)"),
        });
    }

    let header_name =
        HeaderName::from_bytes(auth_header.as_bytes()).map_err(|_| UpstreamProbeError {
            status: None,
            message: format!("invalid auth header name: {auth_header}"),
        })?;
    let header_value =
        HeaderValue::from_str(&auth_format.replace("{key}", api_key)).map_err(|_| {
            UpstreamProbeError {
                status: None,
                message: "invalid auth header value".into(),
            }
        })?;

    // Smallest possible request: one token of output, no streaming.
    let payload = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1,
        "stream": false,
    });

    let started = std::time::Instant::now();
    let resp = crate::http_client()
        .post(format!("{upstream}/v1/chat/completions"))
        .header(header_name, header_value)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .timeout(PROBE_TIMEOUT)
        .json(&payload)
        .send()
        .await
        .map_err(|e| UpstreamProbeError {
            status: None,
            message: format!("could not reach upstream: {e}"),
        })?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let latency_ms = started.elapsed().as_millis();

    if !status.is_success() {
        return Err(UpstreamProbeError {
            status: Some(status.as_u16()),
            message: extract_upstream_error(&body),
        });
    }

    Ok(latency_ms)
}

/// Returns `Some(model)` when the request body asks for a model that is not in
/// the vendor's configured list.
///
/// Bodies that are absent, not JSON, or carry no `model` field pass through
/// untouched — the restriction only applies where a model is actually named.
/// The check is intentionally cheap: a single JSON parse of an already-buffered
/// body, and only when the vendor has a non-empty list.
fn model_not_allowed(allowed_csv: &str, body: &[u8]) -> Option<String> {
    let allowed: Vec<&str> = allowed_csv
        .split(',')
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .collect();

    if allowed.is_empty() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(body).ok()?;
    let model = json.get("model")?.as_str()?;

    if allowed.contains(&model) {
        None
    } else {
        Some(model.to_string())
    }
}

/// A successful probe of an upstream's model list.
pub struct UpstreamProbe {
    pub models: Vec<String>,
    pub latency_ms: u128,
}

/// Why a probe failed. `status` is `None` when no HTTP response arrived at all
/// (DNS, TLS, connection refused, timeout).
pub struct UpstreamProbeError {
    pub status: Option<u16>,
    pub message: String,
}

/// Total time a probe may take. Probes run in the admin UI while an operator
/// waits, so they must give up rather than hang the request.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Calls the upstream's model-list endpoint.
///
/// `GET {upstream}/v1/models` is the standard OpenAI-compatible way to both
/// validate credentials and enumerate models, so the admin UI's "test" and
/// "fetch models" actions share this single call.
pub async fn probe_upstream_models(
    upstream: &str,
    auth_header: &str,
    auth_format: &str,
    api_key: &str,
) -> std::result::Result<UpstreamProbe, UpstreamProbeError> {
    use reqwest::header::*;

    // Validate the URL up front: reqwest would otherwise report an opaque
    // "builder error" once the request is sent.
    let upstream = upstream.trim_end_matches('/');
    if !upstream.starts_with("http://") && !upstream.starts_with("https://") {
        return Err(UpstreamProbeError {
            status: None,
            message: format!("invalid upstream URL: {upstream} (expected http:// or https://)"),
        });
    }
    let url = format!("{upstream}/v1/models");

    let header_name =
        HeaderName::from_bytes(auth_header.as_bytes()).map_err(|_| UpstreamProbeError {
            status: None,
            message: format!("invalid auth header name: {auth_header}"),
        })?;
    let header_value =
        HeaderValue::from_str(&auth_format.replace("{key}", api_key)).map_err(|_| {
            UpstreamProbeError {
                status: None,
                message: "invalid auth header value".into(),
            }
        })?;

    let started = std::time::Instant::now();
    let resp = crate::http_client()
        .get(&url)
        .header(header_name, header_value)
        .header(ACCEPT, "application/json")
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .map_err(|e| UpstreamProbeError {
            status: None,
            message: format!("could not reach upstream: {e}"),
        })?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let latency_ms = started.elapsed().as_millis();

    if !status.is_success() {
        return Err(UpstreamProbeError {
            status: Some(status.as_u16()),
            message: extract_upstream_error(&body),
        });
    }

    let models = parse_model_ids(&body).ok_or_else(|| UpstreamProbeError {
        status: Some(status.as_u16()),
        message: "upstream replied without a model list".into(),
    })?;

    Ok(UpstreamProbe { models, latency_ms })
}

/// Pulls a human-readable message out of an upstream error body.
fn extract_upstream_error(body: &str) -> String {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        // OpenAI-compatible shape: {"error": {"message": "..."}}
        for pointer in ["/error/message", "/error/msg", "/message", "/detail"] {
            if let Some(msg) = json.pointer(pointer).and_then(|v| v.as_str()) {
                return msg.to_string();
            }
        }
        if let Some(msg) = json.get("error").and_then(|v| v.as_str()) {
            return msg.to_string();
        }
    }

    let trimmed = body.trim();
    if trimmed.is_empty() {
        "upstream returned an error with no details".to_string()
    } else {
        // Keep HTML error pages from flooding the UI.
        trimmed.chars().take(300).collect()
    }
}

/// Reads model ids from an OpenAI-compatible response, tolerating a few shapes.
fn parse_model_ids(body: &str) -> Option<Vec<String>> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;

    let from_array = |arr: &[serde_json::Value]| -> Vec<String> {
        arr.iter()
            .filter_map(|m| {
                m.as_str()
                    .or_else(|| m.get("id").and_then(|v| v.as_str()))
                    .or_else(|| m.get("name").and_then(|v| v.as_str()))
            })
            .map(String::from)
            .collect()
    };

    // {"data": [{"id": "..."}]}
    if let Some(data) = json.get("data").and_then(|v| v.as_array()) {
        let ids = from_array(data);
        if !ids.is_empty() {
            return Some(ids);
        }
    }

    // {"models": [...]}
    if let Some(models) = json.get("models").and_then(|v| v.as_array()) {
        let ids = from_array(models);
        if !ids.is_empty() {
            return Some(ids);
        }
    }

    // Bare array: ["..."]
    if let Some(arr) = json.as_array() {
        let ids = from_array(arr);
        if !ids.is_empty() {
            return Some(ids);
        }
    }

    None
}

trait ProviderSpec {
    fn upstream(&self) -> &str;
    fn auth_header(&self) -> &str;
    fn auth_format(&self) -> &str;
}

impl ProviderSpec for ProviderInfo {
    fn upstream(&self) -> &str {
        &self.upstream
    }
    fn auth_header(&self) -> &str {
        &self.auth_header
    }
    fn auth_format(&self) -> &str {
        &self.auth_format
    }
}

impl ProviderSpec for sluice_core::config::ProviderConfig {
    fn upstream(&self) -> &str {
        &self.upstream
    }
    fn auth_header(&self) -> &str {
        &self.auth_header
    }
    fn auth_format(&self) -> &str {
        &self.auth_format
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ProviderInfo;

    fn provider() -> ProviderInfo {
        ProviderInfo {
            id: 1,
            name: "anthropic".into(),
            upstream: "https://api.anthropic.com".into(),
            auth_header: "x-api-key".into(),
            auth_format: "{key}".into(),
            models: String::new(),
        }
    }

    fn client_headers(pairs: &[(&str, &str)]) -> axum::http::HeaderMap {
        let mut headers = axum::http::HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(value).unwrap(),
            );
        }
        headers
    }

    /// The vendor's credential must be the only auth the upstream ever sees.
    #[test]
    fn the_clients_own_credentials_are_replaced() {
        let headers = client_headers(&[
            ("authorization", "Bearer client-token"),
            ("x-api-key", "client-supplied-key"),
            ("content-type", "application/json"),
        ]);

        let built = build_upstream_headers(&headers, &provider(), "real-vendor-key").unwrap();

        assert_eq!(built.get("authorization"), None);
        assert_eq!(
            built.get("x-api-key").unwrap(),
            "real-vendor-key",
            "the vendor key must win over anything the client sent"
        );
        // Unrelated headers still pass through.
        assert_eq!(built.get("content-type").unwrap(), "application/json");
    }

    /// Framing is decided by the body actually being sent. Forwarding the client's
    /// value would let a client declare a length that does not match the bytes,
    /// and hyper honours a user-set Content-Length.
    #[test]
    fn client_framing_and_cookies_are_not_forwarded() {
        let headers = client_headers(&[
            ("content-length", "5"),
            ("transfer-encoding", "chunked"),
            ("connection", "keep-alive"),
            ("cookie", "session=abc"),
            ("host", "gateway.example.com"),
            ("accept", "application/json"),
        ]);

        let built = build_upstream_headers(&headers, &provider(), "k").unwrap();

        assert_eq!(built.get("content-length"), None);
        assert_eq!(built.get("transfer-encoding"), None);
        assert_eq!(built.get("connection"), None);
        assert_eq!(built.get("cookie"), None);
        assert_eq!(built.get("host"), None);
        // Everything else survives.
        assert_eq!(built.get("accept").unwrap(), "application/json");
    }

    /// The vendor's auth header name is configurable, so it has to be stripped
    /// whatever it is called.
    #[test]
    fn a_custom_auth_header_is_replaced_too() {
        let mut p = provider();
        p.auth_header = "x-goog-api-key".into();
        p.auth_format = "Bearer {key}".into();

        let headers = client_headers(&[("x-goog-api-key", "stale")]);
        let built = build_upstream_headers(&headers, &p, "fresh").unwrap();

        assert_eq!(built.get("x-goog-api-key").unwrap(), "Bearer fresh");
    }

    /// The vendor prefix is stripped and the remainder is appended to the
    /// upstream, so routing cannot rewrite the host.
    #[test]
    fn the_upstream_uri_keeps_the_vendors_host() {
        let original: axum::http::Uri = "http://gateway/anthropic/v1/messages?stream=true"
            .parse()
            .unwrap();

        let uri = build_upstream_uri(&original, &provider()).unwrap();
        assert_eq!(
            uri.to_string(),
            "https://api.anthropic.com/v1/messages?stream=true"
        );
    }

    /// A trailing slash on the configured upstream must not produce `//`.
    #[test]
    fn a_trailing_slash_on_the_upstream_is_normalised() {
        let mut p = provider();
        p.upstream = "https://api.anthropic.com/".into();

        let original: axum::http::Uri = "http://gateway/anthropic/v1/models".parse().unwrap();
        assert_eq!(
            build_upstream_uri(&original, &p).unwrap().to_string(),
            "https://api.anthropic.com/v1/models"
        );
    }

    /// An empty list means unrestricted; a populated one only admits its members.
    #[test]
    fn the_model_allow_list_only_applies_when_populated() {
        // Unrestricted: any body, even one naming a model, passes.
        assert_eq!(model_not_allowed("", b"{\"model\":\"anything\"}"), None);
        assert_eq!(
            model_not_allowed("  ,  ", b"{\"model\":\"anything\"}"),
            None
        );

        // Restricted.
        let allowed = "claude-3-5-sonnet-20241022, claude-3-haiku";
        assert_eq!(
            model_not_allowed(allowed, b"{\"model\":\"claude-3-haiku\"}"),
            None
        );
        assert_eq!(
            model_not_allowed(allowed, b"{\"model\":\"gpt-4o\"}"),
            Some("gpt-4o".to_string())
        );

        // Bodies that are absent, not JSON, or carry no model are left alone.
        assert_eq!(model_not_allowed(allowed, b""), None);
        assert_eq!(model_not_allowed(allowed, b"not json"), None);
        assert_eq!(model_not_allowed(allowed, b"{\"messages\":[]}"), None);
        assert_eq!(model_not_allowed(allowed, b"{\"model\":123}"), None);
    }
}
