pub mod admin;
pub mod auth;
pub mod db;
pub mod health;
pub mod logging;
pub mod proxy;
pub mod static_files;

use std::sync::Arc;

/// Shared HTTP client for every upstream call.
///
/// A `reqwest::Client` owns a connection pool and a TLS configuration; building
/// one per request throws both away and re-handshakes on every call.
static HTTP_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

/// The client used for all proxying and probing.
pub fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("reqwest client configuration is valid")
    })
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<sluice_core::config::AppConfig>,
    pub db_pool: Option<db::DbPool>,
    pub sessions: auth::Sessions,
    pub login_limiter: auth::LoginLimiter,
    /// Cached view of which vendors exist in the database.
    pub vendors: db::VendorIndex,
}

pub use admin::admin_router;
pub use admin::{RESERVED_VENDOR_NAMES, is_reserved_vendor_name};
pub use auth::{LoginLimiter, Sessions};
pub use db::{DbPool, VendorIndex};
pub use health::health_handler;
pub use logging::PathOnlySpan;
pub use proxy::proxy_handler;
pub use static_files::serve_static;
