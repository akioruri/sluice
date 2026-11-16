use sluice_core::config::{AppConfig, EnvOverrides, validate_upstream_url, validate_vendor_name};
use std::io::Write;

fn write_config(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let path = dir.join("providers.toml");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(body.as_bytes()).unwrap();
    path
}

#[test]
fn rejects_upstream_without_a_scheme() {
    let err = validate_upstream_url("api.openai.com").unwrap_err();
    assert!(err.to_string().contains("http://"), "{err}");

    // A scheme with no host is just as unusable.
    assert!(validate_upstream_url("https://").is_err());
}

#[test]
fn accepts_http_and_https_upstreams() {
    assert!(validate_upstream_url("https://api.openai.com").is_ok());
    assert!(validate_upstream_url("http://localhost:11434").is_ok());
}

#[test]
fn rejects_vendor_names_that_cannot_be_routed() {
    for bad in ["", "  ", "a/b", "with space", "café", &"x".repeat(65)] {
        assert!(
            validate_vendor_name(bad).is_err(),
            "expected {bad:?} to be rejected"
        );
    }
    assert!(validate_vendor_name("openai").is_ok());
    assert!(validate_vendor_name("my-vendor_2.local").is_ok());
}

#[test]
fn a_config_file_provider_with_a_bad_upstream_fails_at_startup() {
    let dir = temp_dir("bad-upstream");
    let path = write_config(
        &dir,
        r#"
[[providers]]
name = "openai"
upstream = "api.openai.com"
auth_header = "Authorization"
auth_format = "Bearer {key}"
"#,
    );

    let err = AppConfig::load(Some(path.to_str().unwrap()), &EnvOverrides::default()).unwrap_err();
    assert!(err.to_string().contains("http://"), "{err}");
}

#[test]
fn a_config_file_provider_with_a_broken_auth_format_fails_at_startup() {
    let dir = temp_dir("bad-auth-format");
    let path = write_config(
        &dir,
        r#"
[[providers]]
name = "openai"
upstream = "https://api.openai.com"
auth_header = "Authorization"
auth_format = "Bearer key"
"#,
    );

    let err = AppConfig::load(Some(path.to_str().unwrap()), &EnvOverrides::default()).unwrap_err();
    assert!(err.to_string().contains("{key}"), "{err}");
}

#[test]
fn a_valid_config_file_provider_loads() {
    let dir = temp_dir("good");
    let path = write_config(
        &dir,
        r#"
[[providers]]
name = "openai"
upstream = "https://api.openai.com"
auth_header = "Authorization"
"#,
    );

    let config = AppConfig::load(Some(path.to_str().unwrap()), &EnvOverrides::default()).unwrap();
    let provider = config.providers.get("openai").expect("provider loaded");
    assert_eq!(provider.config.upstream, "https://api.openai.com");
    // The default auth format is applied when the field is absent.
    assert_eq!(provider.config.auth_format, "Bearer {key}");
}

#[test]
fn a_short_master_key_is_rejected() {
    let env = EnvOverrides {
        master_key: Some(base64_of(&[0u8; 16])),
        ..EnvOverrides::default()
    };
    let err = AppConfig::load(None, &env).unwrap_err();
    assert!(err.to_string().contains("32 bytes"), "{err}");
}

#[test]
fn a_non_base64_master_key_is_rejected() {
    // This is the value docker-compose.example.yml used to ship.
    let env = EnvOverrides {
        master_key: Some("change-me-32-byte-base64-key".to_string()),
        ..EnvOverrides::default()
    };
    let err = AppConfig::load(None, &env).unwrap_err();
    assert!(err.to_string().contains("SLUICE_MASTER_KEY"), "{err}");
}

#[test]
fn an_ephemeral_master_key_is_flagged() {
    let config = AppConfig::load(None, &EnvOverrides::default()).unwrap();
    assert!(config.master_key_is_ephemeral);
}

#[test]
fn a_supplied_master_key_is_not_flagged() {
    let env = EnvOverrides {
        master_key: Some(base64_of(&[7u8; 32])),
        ..EnvOverrides::default()
    };
    let config = AppConfig::load(None, &env).unwrap();
    assert!(!config.master_key_is_ephemeral);
    assert_eq!(config.master_key.len(), 32);
}

#[test]
fn crypto_round_trips_and_rejects_a_wrong_length_key() {
    let key = [9u8; 32];
    let ciphertext = sluice_core::crypto::encrypt("sk-secret-vendor-key", &key).unwrap();
    assert_eq!(
        sluice_core::crypto::decrypt(&ciphertext, &key).unwrap(),
        "sk-secret-vendor-key"
    );

    // The same plaintext encrypts to different bytes each time: the nonce is
    // fresh per call.
    let again = sluice_core::crypto::encrypt("sk-secret-vendor-key", &key).unwrap();
    assert_ne!(ciphertext, again);

    // A short key is an error, not a panic.
    assert!(sluice_core::crypto::encrypt("x", &[0u8; 16]).is_err());
    assert!(sluice_core::crypto::decrypt(&ciphertext, &[0u8; 16]).is_err());
    // Truncated ciphertext is an error, not a panic.
    assert!(sluice_core::crypto::decrypt(&ciphertext[..5], &key).is_err());
}

#[test]
fn digests_are_comparable_in_constant_time() {
    let a = sluice_core::token::sha2_hash("token-a");
    let b = sluice_core::token::sha2_hash("token-a");
    let c = sluice_core::token::sha2_hash("token-b");
    assert!(sluice_core::token::constant_time_eq(&a, &b));
    assert!(!sluice_core::token::constant_time_eq(&a, &c));
    // Differing lengths are simply unequal, never a crash.
    assert!(!sluice_core::token::constant_time_eq(&a, "short"));
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sluice-config-test-{}-{}-{:?}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
