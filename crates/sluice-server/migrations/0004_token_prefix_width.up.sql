-- Widen token_prefix so a retired key can carry its retirement date.
--
-- Revoking renames the prefix to "<prefix>.old.<YYYYMMDD>" (24 chars) so
-- historical usage rows stay identifiable. The original 16-char column forced a
-- truncation that always cut the date off, defeating the rename entirely.
ALTER TABLE unified_tokens ALTER COLUMN token_prefix TYPE VARCHAR(32);
