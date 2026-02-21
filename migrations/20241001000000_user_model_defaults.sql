CREATE TABLE IF NOT EXISTS user_model_defaults (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    chat_provider_name TEXT,
    chat_model_name TEXT,
    subagent_provider_name TEXT,
    subagent_model_name TEXT,
    image_provider_name TEXT,
    image_model_name TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Backfill one row per existing user.
-- If a legacy default provider exists, migrate its provider/model into
-- chat + subagent defaults; image defaults remain empty.
INSERT INTO user_model_defaults (
    user_id,
    chat_provider_name,
    chat_model_name,
    subagent_provider_name,
    subagent_model_name
)
SELECT
    u.id,
    (
        SELECT COALESCE(up.name, up.provider)
        FROM user_providers up
        WHERE up.user_id = u.id AND up.is_default = 1
        ORDER BY up.created_at DESC
        LIMIT 1
    ) AS chat_provider_name,
    (
        SELECT up.model_name
        FROM user_providers up
        WHERE up.user_id = u.id AND up.is_default = 1
        ORDER BY up.created_at DESC
        LIMIT 1
    ) AS chat_model_name,
    (
        SELECT COALESCE(up.name, up.provider)
        FROM user_providers up
        WHERE up.user_id = u.id AND up.is_default = 1
        ORDER BY up.created_at DESC
        LIMIT 1
    ) AS subagent_provider_name,
    (
        SELECT up.model_name
        FROM user_providers up
        WHERE up.user_id = u.id AND up.is_default = 1
        ORDER BY up.created_at DESC
        LIMIT 1
    ) AS subagent_model_name
FROM users u
WHERE NOT EXISTS (
    SELECT 1
    FROM user_model_defaults umd
    WHERE umd.user_id = u.id
);
