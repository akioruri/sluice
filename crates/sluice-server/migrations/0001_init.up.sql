CREATE TABLE providers (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) UNIQUE NOT NULL,
    upstream VARCHAR(512) NOT NULL,
    auth_header VARCHAR(255) NOT NULL,
    auth_format TEXT NOT NULL DEFAULT '{key}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE provider_keys (
    id SERIAL PRIMARY KEY,
    provider_id INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    encrypted_key BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE unified_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash VARCHAR(64) NOT NULL,
    token_prefix VARCHAR(16) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    UNIQUE(token_hash)
);

CREATE INDEX idx_tokens_hash ON unified_tokens(token_hash);
CREATE INDEX idx_tokens_active ON unified_tokens(revoked_at);

CREATE TABLE usage_stats (
    id SERIAL PRIMARY KEY,
    token_id UUID REFERENCES unified_tokens(id),
    provider_name VARCHAR(255) NOT NULL,
    path TEXT NOT NULL,
    method VARCHAR(10) NOT NULL,
    status_code INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_usage_token ON usage_stats(token_id);
CREATE INDEX idx_usage_provider ON usage_stats(provider_name);
CREATE INDEX idx_usage_created ON usage_stats(created_at);
