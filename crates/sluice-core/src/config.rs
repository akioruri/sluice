use crate::error::{Result, SluiceError};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub upstream: String,
    pub auth_header: String,
    #[serde(default = "default_auth_format")]
    pub auth_format: String,
}

fn default_auth_format() -> String {
    "Bearer {key}".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SluiceConfig {
    pub providers: Vec<ProviderConfig>,
}

impl SluiceConfig {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(&path).map_err(|e| {
            SluiceError::ConfigError(format!("failed to read {}: {}", path.as_ref().display(), e))
        })?;
        toml::from_str(&content)
            .map_err(|e| SluiceError::ConfigError(format!("failed to parse TOML: {e}")))
    }
}

#[derive(Debug, Default, Clone)]
pub struct EnvOverrides {
    pub provider_keys: HashMap<String, String>,
    pub master_key: Option<String>,
    pub bind_addr: Option<String>,
    pub db_url: Option<String>,
    pub unified_tokens: Vec<String>,
    pub admin_password: Option<String>,
}

impl EnvOverrides {
    pub fn from_env() -> Self {
        let mut overrides = Self::default();

        for (key, value) in std::env::vars() {
            if key.starts_with("SLUICE_PROVIDER_") && key.ends_with("_KEY") {
                let name = key["SLUICE_PROVIDER_".len()..key.len() - 4].to_lowercase();
                overrides.provider_keys.insert(name, value);
            }
        }

        overrides.bind_addr = std::env::var("SLUICE_BIND_ADDR").ok();
        overrides.db_url = std::env::var("DATABASE_URL").ok();
        overrides.master_key = std::env::var("SLUICE_MASTER_KEY").ok();
        overrides.admin_password = std::env::var("SLUICE_ADMIN_PASSWORD").ok();

        if let Ok(tokens) = std::env::var("SLUICE_UNIFIED_TOKENS") {
            for token in tokens.split(',') {
                let token = token.trim();
                if !token.is_empty() {
                    overrides.unified_tokens.push(token.to_string());
                }
            }
        }

        overrides
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedProvider {
    pub config: ProviderConfig,
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_addr: String,
    pub providers: HashMap<String, ResolvedProvider>,
    pub master_key: Vec<u8>,
    pub admin_password: String,
    pub db_url: Option<String>,
    /// True when no `SLUICE_MASTER_KEY` was supplied and a random one was made up.
    ///
    /// Every key already stored in the database is then unreadable, so the caller
    /// should say so loudly rather than let the failure surface as a 500 later.
    pub master_key_is_ephemeral: bool,
}

impl AppConfig {
    pub fn load(providers_path: Option<&str>, env: &EnvOverrides) -> Result<Self> {
        let providers_toml = providers_path.unwrap_or("config/providers.toml");

        // Providers config file is optional — all provider config can be managed via the web UI
        let providers = if std::path::Path::new(providers_toml).exists() {
            let config = SluiceConfig::load(providers_toml)?;
            config
                .providers
                .into_iter()
                .map(|provider_cfg| {
                    // Fail at startup rather than at the first request: a provider
                    // that can never be routed to is a configuration mistake.
                    validate_provider_config(&provider_cfg)?;
                    let api_key = env
                        .provider_keys
                        .get(&provider_cfg.name)
                        .cloned()
                        .unwrap_or_default();
                    Ok((
                        provider_cfg.name.clone(),
                        ResolvedProvider {
                            config: provider_cfg,
                            api_key,
                        },
                    ))
                })
                .collect::<Result<HashMap<_, _>>>()?
        } else {
            HashMap::new()
        };

        let supplied_key = env
            .master_key
            .as_ref()
            .map(|b64| {
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64).map_err(
                    |e| {
                        SluiceError::ConfigError(format!(
                            "invalid SLUICE_MASTER_KEY (must be 32 bytes, base64-encoded): {e}"
                        ))
                    },
                )
            })
            .transpose()?;

        let (master_key, master_key_is_ephemeral) = match supplied_key {
            Some(key) => {
                if key.len() != 32 {
                    return Err(SluiceError::ConfigError(format!(
                        "SLUICE_MASTER_KEY must decode to 32 bytes, got {}",
                        key.len()
                    )));
                }
                (key, false)
            }
            None => {
                use rand::RngCore;
                let mut key = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                (key, true)
            }
        };

        let admin_password = env
            .admin_password
            .clone()
            .unwrap_or_else(|| "change-me".to_string());

        let bind_addr = env
            .bind_addr
            .clone()
            .unwrap_or_else(|| "127.0.0.1:8080".to_string());

        Ok(Self {
            bind_addr,
            providers,
            master_key,
            admin_password,
            db_url: env.db_url.clone(),
            master_key_is_ephemeral,
        })
    }
}

/// Checks a config-file provider for the mistakes that would otherwise only show
/// up as a 502 on the first request through that vendor.
fn validate_provider_config(provider: &ProviderConfig) -> Result<()> {
    validate_vendor_name(&provider.name)?;
    validate_upstream_url(&provider.upstream)?;

    if provider.auth_header.trim().is_empty() {
        return Err(SluiceError::ConfigError(format!(
            "provider '{}': auth_header must not be empty",
            provider.name
        )));
    }
    if !provider.auth_format.contains("{key}") {
        return Err(SluiceError::ConfigError(format!(
            "provider '{}': auth_format must contain the {{key}} placeholder",
            provider.name
        )));
    }
    Ok(())
}

/// A vendor name becomes the first path segment of every request routed to it,
/// so it has to be a single, plain, non-reserved segment.
pub fn validate_vendor_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(SluiceError::ConfigError(
            "vendor name must not be empty".into(),
        ));
    }
    if trimmed.len() > 64 {
        return Err(SluiceError::ConfigError(
            "vendor name must be at most 64 characters".into(),
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(SluiceError::ConfigError(format!(
            "vendor name '{trimmed}' may only contain letters, digits, '-', '_' and '.'"
        )));
    }
    Ok(trimmed.to_string())
}

/// Only absolute `http://` and `https://` URLs can be proxied to; anything else
/// would either fail to parse or silently drop the scheme.
pub fn validate_upstream_url(upstream: &str) -> Result<()> {
    let trimmed = upstream.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err(SluiceError::ConfigError(format!(
            "upstream '{trimmed}' must start with http:// or https://"
        )));
    }
    // A URL with no host cannot be requested against.
    if trimmed
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .is_empty()
    {
        return Err(SluiceError::ConfigError(format!(
            "upstream '{trimmed}' has no host"
        )));
    }
    Ok(())
}

pub const ADMIN_USERNAME: &str = "root";
