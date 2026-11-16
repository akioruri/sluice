use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sluice_core::token::constant_time_eq;

pub use sluice_core::token::sha2_hash;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_valid_until_it_is_removed() {
        let sessions = Sessions::default();
        sessions.insert("tok".into());
        assert!(sessions.contains("tok"));

        sessions.remove("tok");
        assert!(!sessions.contains("tok"));
    }

    #[test]
    fn an_unknown_session_is_rejected() {
        let sessions = Sessions::default();
        sessions.insert("tok".into());
        assert!(!sessions.contains("other"));
        assert!(!sessions.contains(""));
    }

    /// The ceiling exists so a leak of session tokens cannot grow the set forever.
    #[test]
    fn the_session_count_is_bounded() {
        let sessions = Sessions::default();
        for i in 0..(MAX_SESSIONS * 3) {
            sessions.insert(format!("tok-{i}"));
        }

        let count = sessions.entries.lock().unwrap().len();
        assert!(count <= MAX_SESSIONS, "{count} sessions survived");
        // The most recent logins are the ones still valid.
        assert!(sessions.contains(&format!("tok-{}", MAX_SESSIONS * 3 - 1)));
    }

    #[test]
    fn the_login_limiter_locks_out_after_repeated_failures() {
        let limiter = LoginLimiter::default();
        for _ in 0..MAX_LOGIN_FAILURES {
            assert!(limiter.allows());
            limiter.record_failure();
        }
        assert!(
            !limiter.allows(),
            "expected a lockout after {MAX_LOGIN_FAILURES} failures"
        );

        // A success clears the counter, so a legitimate admin is not punished for
        // someone else's failed attempts in an earlier window.
        limiter.record_success();
        assert!(limiter.allows());
    }

    #[test]
    fn a_single_failure_does_not_lock_anyone_out() {
        let limiter = LoginLimiter::default();
        limiter.record_failure();
        assert!(limiter.allows());
    }

    #[test]
    fn password_comparison_matches_only_the_exact_secret() {
        assert!(password_matches("hunter2", "hunter2"));
        assert!(!password_matches("hunter2", "hunter3"));
        assert!(!password_matches("", "hunter2"));
        // Differing lengths must not crash or match.
        assert!(!password_matches("x", "hunter2"));
    }
}

/// How long an admin session stays valid after it was last used.
const SESSION_TTL: Duration = Duration::from_secs(12 * 60 * 60);

/// Ceiling on concurrently valid admin sessions.
///
/// Without one, a leak of session tokens would grow the set forever and the
/// process would never reclaim the memory.
const MAX_SESSIONS: usize = 64;

/// In-memory registry of valid admin session tokens.
///
/// Sessions are created on successful login and removed on logout. Because the
/// gateway is a single process, an in-memory set is sufficient; a restart simply
/// requires admins to log in again.
#[derive(Clone)]
pub struct Sessions {
    entries: std::sync::Arc<Mutex<HashMap<String, Instant>>>,
}

impl Default for Sessions {
    fn default() -> Self {
        Self {
            entries: std::sync::Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Sessions {
    pub fn insert(&self, token: String) {
        if let Ok(mut set) = self.entries.lock() {
            Self::prune(&mut set);
            // Evict the oldest sessions once the ceiling is reached, so a burst of
            // logins cannot grow the set without bound.
            while set.len() >= MAX_SESSIONS {
                let Some(oldest) = set
                    .iter()
                    .min_by_key(|(_, at)| **at)
                    .map(|(token, _)| token.clone())
                else {
                    break;
                };
                set.remove(&oldest);
            }
            set.insert(token, Instant::now());
        }
    }

    pub fn remove(&self, token: &str) {
        if let Ok(mut set) = self.entries.lock() {
            set.remove(token);
        }
    }

    /// Whether the token is currently valid. Expired sessions are dropped as a
    /// side effect, so an idle admin is logged out rather than trusted forever.
    pub fn contains(&self, token: &str) -> bool {
        let Ok(mut set) = self.entries.lock() else {
            return false;
        };
        Self::prune(&mut set);
        set.contains_key(token)
    }

    fn prune(set: &mut HashMap<String, Instant>) {
        let now = Instant::now();
        set.retain(|_, at| now.duration_since(*at) < SESSION_TTL);
    }
}

/// Generates a high-entropy session token for the admin UI.
pub fn generate_session_token() -> String {
    random_string(43)
}

/// Generates a platform API key that clients use to call models through the gateway.
pub fn generate_api_token() -> String {
    format!("sk-{}", random_string(48))
}

fn random_string(len: usize) -> String {
    use rand::Rng;
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(len)
        .map(|b| b as char)
        .collect()
}

/// Extracts the bearer token from an `Authorization: Bearer <token>` header.
pub fn bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Rejects requests without a valid admin session token.
///
/// Applied to every admin route except `/login`.
pub async fn require_admin(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let authorized = bearer_token(req.headers())
        .map(|t| state.sessions.contains(&t))
        .unwrap_or(false);

    if authorized {
        next.run(req).await
    } else {
        unauthorized()
    }
}

pub fn unauthorized() -> axum::response::Response {
    axum::response::Response::builder()
        .status(axum::http::StatusCode::UNAUTHORIZED)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(r#"{"error":"unauthorized"}"#))
        .unwrap()
}

/// Failed logins allowed before the account is locked out.
const MAX_LOGIN_FAILURES: u32 = 5;

/// How long a lockout lasts.
const LOGIN_LOCKOUT: Duration = Duration::from_secs(60);

/// Throttles admin login attempts.
///
/// There is exactly one admin account, so a single counter is enough: after a
/// handful of wrong passwords the endpoint stops answering for a minute, which
/// takes the profit out of an online brute-force.
#[derive(Clone)]
pub struct LoginLimiter {
    state: std::sync::Arc<Mutex<LoginState>>,
}

#[derive(Default)]
struct LoginState {
    failures: u32,
    window_started: Option<Instant>,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self {
            state: std::sync::Arc::new(Mutex::new(LoginState::default())),
        }
    }
}

impl LoginLimiter {
    /// Whether a login attempt may proceed right now.
    pub fn allows(&self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            // A poisoned lock must not lock everyone out of the console.
            return true;
        };
        let Some(started) = state.window_started else {
            return true;
        };
        if started.elapsed() >= LOGIN_LOCKOUT {
            state.failures = 0;
            state.window_started = None;
            return true;
        }
        state.failures < MAX_LOGIN_FAILURES
    }

    pub fn record_failure(&self) {
        if let Ok(mut state) = self.state.lock() {
            if state.window_started.is_none() {
                state.window_started = Some(Instant::now());
            }
            state.failures += 1;
        }
    }

    pub fn record_success(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.failures = 0;
            state.window_started = None;
        }
    }
}

/// Compares a supplied password against the expected one in constant time.
///
/// The comparison runs over fixed-length SHA-256 digests so neither the length
/// nor the first differing byte of the password leaks through timing.
pub fn password_matches(supplied: &str, expected: &str) -> bool {
    constant_time_eq(&sha2_hash(supplied), &sha2_hash(expected))
}
