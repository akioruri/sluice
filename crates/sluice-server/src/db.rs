use sqlx::postgres::PgPoolOptions;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub type DbPool = sqlx::PgPool;

pub async fn create_pool(database_url: &str) -> anyhow::Result<DbPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

// --- Token repository ---
pub async fn create_token(
    pool: &DbPool,
    token_hash: &str,
    token_prefix: &str,
    name: Option<&str>,
) -> anyhow::Result<uuid::Uuid> {
    let id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO unified_tokens (token_hash, token_prefix, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(token_hash)
    .bind(token_prefix)
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Resolves a token hash to its id, or `None` when the token is unknown or revoked.
///
/// One query does both the validity check and the id lookup: the previous
/// two-step version raced nothing but cost a round trip on every request.
pub async fn resolve_active_token(
    pool: &DbPool,
    token_hash: &str,
) -> anyhow::Result<Option<uuid::Uuid>> {
    let row = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM unified_tokens WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn revoke_token(pool: &DbPool, token_hash: &str) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE unified_tokens SET revoked_at = now() WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(token_hash)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Retires a token: it stops validating immediately, and its prefix is renamed to
/// `<prefix>.old.<created date>` so historical usage rows stay attributable. The
/// row itself is kept so `usage_stats.token_id` stays valid.
pub async fn revoke_token_by_prefix(pool: &DbPool, prefix: &str) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE unified_tokens
         SET revoked_at = now(),
             token_prefix = token_prefix || '.old.' || to_char(created_at, 'YYYYMMDD')
         WHERE token_prefix = $1 AND revoked_at IS NULL",
    )
    .bind(prefix)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Lists tokens that have not been revoked.
///
/// Retired tokens stay in the table (usage stats reference them) but are hidden
/// from the admin list.
pub async fn list_tokens(pool: &DbPool) -> anyhow::Result<Vec<TokenInfo>> {
    let rows = sqlx::query_as::<_, TokenInfo>(
        "SELECT name, token_prefix, created_at, revoked_at FROM unified_tokens
         WHERE revoked_at IS NULL ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(sqlx::FromRow)]
pub struct TokenInfo {
    pub name: Option<String>,
    pub token_prefix: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

// --- Provider repository ---
#[derive(sqlx::FromRow, serde::Serialize, Clone)]
pub struct ProviderInfo {
    pub id: i32,
    pub name: String,
    pub upstream: String,
    pub auth_header: String,
    pub auth_format: String,
    pub models: String,
}

pub async fn list_providers(pool: &DbPool) -> anyhow::Result<Vec<ProviderInfo>> {
    let rows = sqlx::query_as::<_, ProviderInfo>(
        "SELECT id, name, upstream, auth_header, auth_format, models FROM providers ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_provider(pool: &DbPool, name: &str) -> anyhow::Result<Option<ProviderInfo>> {
    let row = sqlx::query_as::<_, ProviderInfo>(
        "SELECT id, name, upstream, auth_header, auth_format, models FROM providers WHERE name = $1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn provider_exists(pool: &DbPool, name: &str) -> anyhow::Result<bool> {
    let row: Option<i32> =
        sqlx::query_scalar::<_, i32>("SELECT 1 FROM providers WHERE name = $1 LIMIT 1")
            .bind(name)
            .fetch_optional(pool)
            .await?;
    Ok(row.is_some())
}

pub async fn create_provider(
    pool: &DbPool,
    name: &str,
    upstream: &str,
    auth_header: &str,
    auth_format: &str,
    models: &str,
) -> anyhow::Result<i32> {
    let id: i32 = sqlx::query_scalar::<_, i32>(
        "INSERT INTO providers (name, upstream, auth_header, auth_format, models) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(name)
    .bind(upstream)
    .bind(auth_header)
    .bind(auth_format)
    .bind(models)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Rewrites a provider's settings in one statement.
///
/// The name is not updatable: it is the routing prefix, and renaming it would
/// orphan every client that calls through the old path.
pub async fn update_provider(
    pool: &DbPool,
    name: &str,
    upstream: &str,
    auth_header: &str,
    auth_format: &str,
    models: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE providers SET upstream = $1, auth_header = $2, auth_format = $3, models = $4 WHERE name = $5",
    )
    .bind(upstream)
    .bind(auth_header)
    .bind(auth_format)
    .bind(models)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_provider(pool: &DbPool, name: &str) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM providers WHERE name = $1")
        .bind(name)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_provider_key(pool: &DbPool, provider_id: i32) -> anyhow::Result<Option<Vec<u8>>> {
    let row: Option<Vec<u8>> = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT encrypted_key FROM provider_keys WHERE provider_id = $1",
    )
    .bind(provider_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Stores (or replaces) the encrypted upstream key for a provider.
///
/// The provider row must already exist — this only touches `provider_keys`.
pub async fn upsert_provider_key(
    pool: &DbPool,
    provider_id: i32,
    encrypted_key: &[u8],
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO provider_keys (provider_id, encrypted_key) VALUES ($1, $2)
         ON CONFLICT (provider_id) DO UPDATE SET encrypted_key = EXCLUDED.encrypted_key, created_at = now()",
    )
    .bind(provider_id)
    .bind(encrypted_key)
    .execute(pool)
    .await?;
    Ok(())
}

// --- Usage stats ---
pub async fn record_usage(
    pool: &DbPool,
    token_id: Option<uuid::Uuid>,
    provider_name: &str,
    path: &str,
    method: &str,
    status_code: i32,
    duration_ms: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO usage_stats (token_id, provider_name, path, method, status_code, duration_ms)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(token_id)
    .bind(provider_name)
    .bind(path)
    .bind(method)
    .bind(status_code)
    .bind(duration_ms)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_stats(pool: &DbPool) -> anyhow::Result<StatsSummary> {
    let total: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM usage_stats")
        .fetch_one(pool)
        .await?;

    let by_provider: Vec<ProviderStat> = sqlx::query_as::<_, ProviderStat>(
        "SELECT provider_name, COUNT(*) as count, MIN(created_at) as first_seen, MAX(created_at) as last_seen
         FROM usage_stats GROUP BY provider_name ORDER BY count DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(StatsSummary {
        total_requests: total,
        by_provider,
    })
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub struct ProviderStat {
    pub provider_name: String,
    pub count: i64,
    pub first_seen: chrono::DateTime<chrono::Utc>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

/// One point of a daily usage series.
#[derive(sqlx::FromRow, serde::Serialize)]
pub struct DailyPoint {
    pub day: chrono::NaiveDate,
    pub series: String,
    pub requests: i64,
}

/// Daily request counts, split by provider and (optionally) by API key.
///
/// `days` bounds the window; `0` means "no limit". Grouping by key only
/// includes rows whose token is still known.
pub async fn get_daily_usage(
    pool: &DbPool,
    days: i64,
) -> anyhow::Result<(Vec<DailyPoint>, Vec<DailyPoint>)> {
    // The window is a bound parameter rather than interpolated text: the value is
    // not user-controlled today, but string-built SQL is one refactor away from
    // being injectable.
    let window = if days > 0 {
        "AND u.created_at >= date_trunc('day', now()) - ($1::int * interval '1 day')"
    } else {
        ""
    };

    let by_provider: Vec<DailyPoint> = sqlx::query_as::<_, DailyPoint>(&format!(
        "SELECT u.created_at::date AS day, u.provider_name AS series, COUNT(*) AS requests
         FROM usage_stats u
         WHERE true {window}
         GROUP BY 1, 2 ORDER BY 1"
    ))
    .bind(days)
    .fetch_all(pool)
    .await?;

    // Tokens keep their original prefix here; retired ones were renamed to
    // "<prefix>.old.<date>" so their history stays identifiable. The series
    // label is the key's name, falling back to the prefix when unnamed.
    let by_token: Vec<DailyPoint> = sqlx::query_as::<_, DailyPoint>(&format!(
        "SELECT u.created_at::date AS day,
                COALESCE(t.name, t.token_prefix) AS series,
                COUNT(*) AS requests
         FROM usage_stats u
         JOIN unified_tokens t ON t.id = u.token_id
         WHERE true {window}
         GROUP BY 1, 2 ORDER BY 1"
    ))
    .bind(days)
    .fetch_all(pool)
    .await?;

    Ok((by_provider, by_token))
}

#[derive(serde::Serialize)]
pub struct StatsSummary {
    pub total_requests: i64,
    pub by_provider: Vec<ProviderStat>,
}

/// How long a cached vendor list is considered fresh.
const VENDOR_CACHE_TTL: Duration = Duration::from_secs(2);

/// A snapshot of the vendor names, with the time it was taken.
type VendorSnapshot = (Instant, HashSet<String>);

/// Short-lived cache of the vendor names present in the database.
///
/// The fallback route asks "is this first path segment a vendor?" for *every*
/// path that does not match a gateway route — the favicon, every SvelteKit
/// asset, every typo. Without a cache each of those is a database round trip.
#[derive(Clone)]
pub struct VendorIndex {
    inner: std::sync::Arc<Mutex<Option<VendorSnapshot>>>,
}

impl Default for VendorIndex {
    fn default() -> Self {
        Self {
            inner: std::sync::Arc::new(Mutex::new(None)),
        }
    }
}

impl VendorIndex {
    /// Whether `name` is a vendor in the database, using the cache when fresh.
    pub async fn contains(&self, pool: &DbPool, name: &str) -> bool {
        if let Ok(guard) = self.inner.lock()
            && let Some((fetched_at, names)) = guard.as_ref()
            && fetched_at.elapsed() < VENDOR_CACHE_TTL
        {
            return names.contains(name);
        }

        match list_provider_names(pool).await {
            Ok(names) => {
                let hit = names.contains(name);
                if let Ok(mut guard) = self.inner.lock() {
                    *guard = Some((Instant::now(), names));
                }
                hit
            }
            // On a database error, fall through to the uncached check so a stale
            // cache can never be the reason a vendor stops working.
            Err(_) => provider_exists(pool, name).await.unwrap_or(false),
        }
    }

    /// Drops the cache after a vendor is created, renamed or deleted.
    pub fn invalidate(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
    }
}

async fn list_provider_names(pool: &DbPool) -> anyhow::Result<HashSet<String>> {
    let rows = sqlx::query_scalar::<_, String>("SELECT name FROM providers")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().collect())
}
