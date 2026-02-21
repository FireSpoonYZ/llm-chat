-- Provider identity migration:
-- 1) allow duplicate provider names per user
-- 2) switch conversation/default-model references from provider name to provider id

DROP INDEX IF EXISTS idx_user_providers_name;

ALTER TABLE conversations ADD COLUMN provider_id TEXT;
ALTER TABLE conversations ADD COLUMN subagent_provider_id TEXT;
ALTER TABLE conversations ADD COLUMN image_provider_id TEXT;

ALTER TABLE user_model_defaults ADD COLUMN chat_provider_id TEXT;
ALTER TABLE user_model_defaults ADD COLUMN subagent_provider_id TEXT;
ALTER TABLE user_model_defaults ADD COLUMN image_provider_id TEXT;

-- Backfill conversations.provider_id from legacy provider (name/type)
UPDATE conversations
SET provider_id = COALESCE(
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = conversations.user_id
          AND up.name = conversations.provider
        ORDER BY up.created_at DESC
        LIMIT 1
    ),
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = conversations.user_id
          AND up.provider = conversations.provider
        ORDER BY up.created_at DESC
        LIMIT 1
    )
)
WHERE conversations.provider IS NOT NULL
  AND conversations.provider != ''
  AND (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = conversations.user_id
            AND up.name = conversations.provider
      ) >= 1
      OR
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = conversations.user_id
            AND up.provider = conversations.provider
      ) = 1
  );

-- Backfill conversations.subagent_provider_id from legacy subagent_provider (name/type)
UPDATE conversations
SET subagent_provider_id = COALESCE(
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = conversations.user_id
          AND up.name = conversations.subagent_provider
        ORDER BY up.created_at DESC
        LIMIT 1
    ),
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = conversations.user_id
          AND up.provider = conversations.subagent_provider
        ORDER BY up.created_at DESC
        LIMIT 1
    )
)
WHERE conversations.subagent_provider IS NOT NULL
  AND conversations.subagent_provider != ''
  AND (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = conversations.user_id
            AND up.name = conversations.subagent_provider
      ) >= 1
      OR
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = conversations.user_id
            AND up.provider = conversations.subagent_provider
      ) = 1
  );

-- Backfill conversations.image_provider_id from legacy image_provider (name/type)
UPDATE conversations
SET image_provider_id = COALESCE(
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = conversations.user_id
          AND up.name = conversations.image_provider
        ORDER BY up.created_at DESC
        LIMIT 1
    ),
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = conversations.user_id
          AND up.provider = conversations.image_provider
        ORDER BY up.created_at DESC
        LIMIT 1
    )
)
WHERE conversations.image_provider IS NOT NULL
  AND conversations.image_provider != ''
  AND (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = conversations.user_id
            AND up.name = conversations.image_provider
      ) >= 1
      OR
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = conversations.user_id
            AND up.provider = conversations.image_provider
      ) = 1
  );

-- Backfill user_model_defaults chat/subagent/image provider ids
UPDATE user_model_defaults
SET chat_provider_id = COALESCE(
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = user_model_defaults.user_id
          AND up.name = user_model_defaults.chat_provider_name
        ORDER BY up.created_at DESC
        LIMIT 1
    ),
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = user_model_defaults.user_id
          AND up.provider = user_model_defaults.chat_provider_name
        ORDER BY up.created_at DESC
        LIMIT 1
    )
)
WHERE user_model_defaults.chat_provider_name IS NOT NULL
  AND user_model_defaults.chat_provider_name != ''
  AND (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = user_model_defaults.user_id
            AND up.name = user_model_defaults.chat_provider_name
      ) >= 1
      OR
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = user_model_defaults.user_id
            AND up.provider = user_model_defaults.chat_provider_name
      ) = 1
  );

UPDATE user_model_defaults
SET subagent_provider_id = COALESCE(
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = user_model_defaults.user_id
          AND up.name = user_model_defaults.subagent_provider_name
        ORDER BY up.created_at DESC
        LIMIT 1
    ),
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = user_model_defaults.user_id
          AND up.provider = user_model_defaults.subagent_provider_name
        ORDER BY up.created_at DESC
        LIMIT 1
    )
)
WHERE user_model_defaults.subagent_provider_name IS NOT NULL
  AND user_model_defaults.subagent_provider_name != ''
  AND (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = user_model_defaults.user_id
            AND up.name = user_model_defaults.subagent_provider_name
      ) >= 1
      OR
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = user_model_defaults.user_id
            AND up.provider = user_model_defaults.subagent_provider_name
      ) = 1
  );

UPDATE user_model_defaults
SET image_provider_id = COALESCE(
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = user_model_defaults.user_id
          AND up.name = user_model_defaults.image_provider_name
        ORDER BY up.created_at DESC
        LIMIT 1
    ),
    (
        SELECT up.id
        FROM user_providers up
        WHERE up.user_id = user_model_defaults.user_id
          AND up.provider = user_model_defaults.image_provider_name
        ORDER BY up.created_at DESC
        LIMIT 1
    )
)
WHERE user_model_defaults.image_provider_name IS NOT NULL
  AND user_model_defaults.image_provider_name != ''
  AND (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = user_model_defaults.user_id
            AND up.name = user_model_defaults.image_provider_name
      ) >= 1
      OR
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = user_model_defaults.user_id
            AND up.provider = user_model_defaults.image_provider_name
      ) = 1
  );

-- Any unresolved provider reference must clear its paired model value.
UPDATE user_model_defaults
SET chat_model_name = NULL
WHERE chat_provider_id IS NULL;

UPDATE user_model_defaults
SET subagent_model_name = NULL
WHERE subagent_provider_id IS NULL;

UPDATE user_model_defaults
SET image_model_name = NULL
WHERE image_provider_id IS NULL;
