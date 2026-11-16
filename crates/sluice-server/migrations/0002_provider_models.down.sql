DROP INDEX IF EXISTS provider_keys_provider_id_key;
ALTER TABLE providers DROP COLUMN IF EXISTS models;
