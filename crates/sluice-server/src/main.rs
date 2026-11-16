use sluice_server::static_files::FRONTEND_DIR;
use sluice_server::{
    AppState, PathOnlySpan, admin_router, health_handler, is_reserved_vendor_name, proxy_handler,
    serve_static,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config =
        sluice_core::config::AppConfig::load(None, &sluice_core::config::EnvOverrides::from_env())?;

    if config.master_key_is_ephemeral {
        tracing::error!(
            "SLUICE_MASTER_KEY is not set: a random key was generated for this run, \
             so every vendor key already stored in the database is now undecryptable"
        );
    }
    if config.admin_password == "change-me" {
        tracing::warn!(
            "SLUICE_ADMIN_PASSWORD is unset and the default 'change-me' is in use; \
             set it before exposing this gateway"
        );
    }

    let db_pool = if let Some(ref db_url) = config.db_url {
        Some(sluice_server::db::create_pool(db_url).await?)
    } else {
        tracing::error!(
            "DATABASE_URL is not set: the admin console has nothing to show and \
             no request can be proxied, because tokens cannot be validated"
        );
        None
    };

    let state = AppState {
        config: Arc::new(config),
        db_pool,
        sessions: sluice_server::Sessions::default(),
        login_limiter: sluice_server::LoginLimiter::default(),
        vendors: sluice_server::db::VendorIndex::default(),
    };

    let app = axum::Router::new()
        .route("/healthz", axum::routing::get(health_handler))
        .nest("/api/admin", admin_router(state.clone()))
        // Anything the routes above did not claim is either a vendor request or a
        // static asset for the admin console.
        .fallback(axum::routing::any(fallback_handler))
        .layer(axum::middleware::from_fn(security_headers))
        .layer(
            // Record the path only. The default span logs the whole URI, query
            // string included, and a client can put a credential there.
            tower_http::trace::TraceLayer::new_for_http().make_span_with(PathOnlySpan),
        )
        // The gateway is a pass-through: large contexts and image inputs are
        // normal. The limit exists to stop an unbounded upload, not to cap
        // legitimate traffic.
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state.clone());

    let bind_addr: SocketAddr = state.config.bind_addr.parse()?;
    tracing::info!(addr = %bind_addr, frontend = FRONTEND_DIR, "listening");

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

/// Ceiling on a single request body, in bytes.
///
/// Requests are buffered once so the model allow-list can be checked before
/// forwarding, which makes this the bound on how much memory one in-flight
/// request can pin.
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Adds the response headers that keep a browser from guessing at content.
///
/// Applied to every response, including the admin API: there is nothing here
/// that a well-behaved client depends on, and it costs one map lookup.
async fn security_headers(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers
        .entry(axum::http::header::X_CONTENT_TYPE_OPTIONS)
        .or_insert(axum::http::HeaderValue::from_static("nosniff"));
    headers
        .entry(axum::http::header::X_FRAME_OPTIONS)
        .or_insert(axum::http::HeaderValue::from_static("DENY"));
    headers
        .entry(axum::http::header::REFERRER_POLICY)
        .or_insert(axum::http::HeaderValue::from_static("no-referrer"));
    response
}

/// Answers requests no explicit route claimed.
///
/// A first path segment naming a known vendor is proxied; everything else is
/// served from the built frontend.
async fn fallback_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::http::Request<axum::body::Body>,
) -> axum::response::Response {
    let path = req.uri().path().to_string();
    let path = path.strip_prefix('/').unwrap_or(&path);
    let vendor = path.split('/').next();

    // Proxy API paths that match a known vendor (from DB or config)
    if let Some(vendor) = vendor {
        // Never let a vendor shadow a gateway route, even if one somehow got
        // into the database or config.
        let reserved = is_reserved_vendor_name(vendor);

        let known = !reserved
            && if let Some(ref pool) = state.db_pool {
                // The vendor list is cached, so a page load of the console — which
                // pulls dozens of assets through here — costs one query, not one
                // per file.
                state.vendors.contains(pool, vendor).await
                    || state.config.providers.contains_key(vendor)
            } else {
                state.config.providers.contains_key(vendor)
            };

        if known {
            return proxy_handler(axum::extract::State(state), req)
                .await
                .unwrap_or_else(|status| {
                    axum::response::Response::builder()
                        .status(status)
                        .body(axum::body::Body::empty())
                        .unwrap()
                });
        }
    }

    serve_static(path).await
}
