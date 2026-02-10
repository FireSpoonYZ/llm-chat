-- Add name column to user_providers and remove old UNIQUE(user_id, provider) constraint
-- SQLite requires table recreation to drop inline constraints

DROP TABLE IF EXISTS user_providers_new;

CREATE TABLE user_providers_new (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    api_key_encrypted TEXT NOT NULL,
    endpoint_url TEXT,
    model_name TEXT,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    models TEXT,
    name TEXT
);

INSERT OR IGNORE INTO user_providers_new (id, user_id, provider, api_key_encrypted, endpoint_url, model_name, is_default, created_at, models, name)
    SELECT id, user_id, provider, api_key_encrypted, endpoint_url, model_name, is_default, created_at, models, provider
    FROM user_providers;

DROP TABLE IF EXISTS user_providers;

ALTER TABLE user_providers_new RENAME TO user_providers;

CREATE UNIQUE INDEX IF NOT EXISTS idx_user_providers_name ON user_providers(user_id, name);
CREATE INDEX IF NOT EXISTS idx_user_providers_user_id ON user_providers(user_id);
