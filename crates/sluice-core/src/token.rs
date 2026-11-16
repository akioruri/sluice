use subtle::ConstantTimeEq;

/// SHA-256 hex digest, used to store token hashes instead of raw tokens.
///
/// This is the single definition for the whole workspace: the gateway, the CLI
/// and the token type all compare digests, so they must agree byte for byte.
pub fn sha2_hash(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Compares two digests without leaking *where* they first differ.
///
/// Hashing already removes the length signal a raw comparison would expose, but
/// the comparison itself still has to be constant-time for the same reason.
/// Returns `false` when the lengths differ, which is not a secret.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

#[derive(Debug, Clone)]
pub struct UnifiedToken {
    pub id: uuid::Uuid,
    pub token_hash: String,
    pub token_prefix: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl UnifiedToken {
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// Builds the row for a freshly minted token. The raw token is never stored.
    pub fn new(token: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            token_hash: sha2_hash(token),
            token_prefix: token.chars().take(8).collect(),
            created_at: chrono::Utc::now(),
            revoked_at: None,
        }
    }
}
