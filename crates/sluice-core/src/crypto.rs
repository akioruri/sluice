use crate::error::{Result, SluiceError};
use aes_gcm::Aes256Gcm;

/// Length of the AES-256-GCM nonce, in bytes.
const NONCE_LEN: usize = 12;

/// Required length of the master key, in bytes.
const KEY_LEN: usize = 32;

/// Encrypts a vendor API key.
///
/// The output is `nonce || ciphertext || tag`; the nonce is freshly random on
/// every call, so encrypting the same key twice never produces the same bytes.
pub fn encrypt(plaintext: &str, key: &[u8]) -> Result<Vec<u8>> {
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit},
    };
    use rand::RngCore;
    use rand::rngs::OsRng;

    let cipher = Aes256Gcm::new(key_array(key)?);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from(nonce_bytes);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| SluiceError::CryptoError(format!("encryption failed: {e}")))?;

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

/// Decrypts a vendor API key.
pub fn decrypt(data: &[u8], key: &[u8]) -> Result<String> {
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit},
    };

    if data.len() <= NONCE_LEN {
        return Err(SluiceError::CryptoError("ciphertext too short".into()));
    }

    let cipher = Aes256Gcm::new(key_array(key)?);

    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&data[..NONCE_LEN]);
    let nonce = Nonce::from(nonce_bytes);

    let plaintext = cipher
        .decrypt(&nonce, &data[NONCE_LEN..])
        .map_err(|e| SluiceError::CryptoError(format!("decryption failed: {e}")))?;

    String::from_utf8(plaintext)
        .map_err(|e| SluiceError::CryptoError(format!("invalid UTF-8: {e}")))
}

/// Validates the key length and converts it into the array form the cipher wants.
///
/// Every caller funnels through here so a short key surfaces as a `ConfigError`
/// rather than a panic in the request path.
fn key_array(key: &[u8]) -> Result<&aes_gcm::Key<Aes256Gcm>> {
    key.try_into().map_err(|_| {
        SluiceError::ConfigError(format!(
            "master key must be exactly {KEY_LEN} bytes, got {}",
            key.len()
        ))
    })
}
