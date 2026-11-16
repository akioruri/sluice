-- One key per provider. Required so provider_keys can be upserted by
-- provider_id (INSERT ... ON CONFLICT (provider_id)).
-- Remove any duplicate rows first, keeping the most recently created.
DELETE FROM provider_keys
WHERE id NOT IN (
    SELECT DISTINCT ON (provider_id) id
    FROM provider_keys
    ORDER BY provider_id, created_at DESC, id DESC
);

ALTER TABLE providers ADD COLUMN IF NOT EXISTS models TEXT NOT NULL DEFAULT '';

CREATE UNIQUE INDEX IF NOT EXISTS provider_keys_provider_id_key
    ON provider_keys(provider_id);
