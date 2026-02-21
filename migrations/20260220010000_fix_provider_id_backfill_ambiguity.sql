-- Fix provider-id backfill ambiguity from the provider identity migration.
-- If a legacy provider name cannot be resolved uniquely, clear the new provider id
-- and its paired model field to avoid silently binding to the wrong provider.

-- conversations.provider_id / model_name
UPDATE conversations
SET provider_id = NULL,
    model_name = NULL
WHERE provider IS NOT NULL
  AND provider != ''
  AND NOT (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = conversations.user_id
            AND up.name = conversations.provider
      ) = 1
      OR (
          (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = conversations.user_id
                AND up.name = conversations.provider
          ) = 0
          AND (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = conversations.user_id
                AND up.provider = conversations.provider
          ) = 1
      )
  );

-- conversations.subagent_provider_id / subagent_model
UPDATE conversations
SET subagent_provider_id = NULL,
    subagent_model = NULL
WHERE subagent_provider IS NOT NULL
  AND subagent_provider != ''
  AND NOT (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = conversations.user_id
            AND up.name = conversations.subagent_provider
      ) = 1
      OR (
          (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = conversations.user_id
                AND up.name = conversations.subagent_provider
          ) = 0
          AND (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = conversations.user_id
                AND up.provider = conversations.subagent_provider
          ) = 1
      )
  );

-- conversations.image_provider_id / image_model
UPDATE conversations
SET image_provider_id = NULL,
    image_model = NULL
WHERE image_provider IS NOT NULL
  AND image_provider != ''
  AND NOT (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = conversations.user_id
            AND up.name = conversations.image_provider
      ) = 1
      OR (
          (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = conversations.user_id
                AND up.name = conversations.image_provider
          ) = 0
          AND (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = conversations.user_id
                AND up.provider = conversations.image_provider
          ) = 1
      )
  );

-- user_model_defaults.chat_provider_id / chat_model_name
UPDATE user_model_defaults
SET chat_provider_id = NULL,
    chat_model_name = NULL
WHERE chat_provider_name IS NOT NULL
  AND chat_provider_name != ''
  AND NOT (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = user_model_defaults.user_id
            AND up.name = user_model_defaults.chat_provider_name
      ) = 1
      OR (
          (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = user_model_defaults.user_id
                AND up.name = user_model_defaults.chat_provider_name
          ) = 0
          AND (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = user_model_defaults.user_id
                AND up.provider = user_model_defaults.chat_provider_name
          ) = 1
      )
  );

-- user_model_defaults.subagent_provider_id / subagent_model_name
UPDATE user_model_defaults
SET subagent_provider_id = NULL,
    subagent_model_name = NULL
WHERE subagent_provider_name IS NOT NULL
  AND subagent_provider_name != ''
  AND NOT (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = user_model_defaults.user_id
            AND up.name = user_model_defaults.subagent_provider_name
      ) = 1
      OR (
          (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = user_model_defaults.user_id
                AND up.name = user_model_defaults.subagent_provider_name
          ) = 0
          AND (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = user_model_defaults.user_id
                AND up.provider = user_model_defaults.subagent_provider_name
          ) = 1
      )
  );

-- user_model_defaults.image_provider_id / image_model_name
UPDATE user_model_defaults
SET image_provider_id = NULL,
    image_model_name = NULL
WHERE image_provider_name IS NOT NULL
  AND image_provider_name != ''
  AND NOT (
      (
          SELECT COUNT(*)
          FROM user_providers up
          WHERE up.user_id = user_model_defaults.user_id
            AND up.name = user_model_defaults.image_provider_name
      ) = 1
      OR (
          (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = user_model_defaults.user_id
                AND up.name = user_model_defaults.image_provider_name
          ) = 0
          AND (
              SELECT COUNT(*)
              FROM user_providers up
              WHERE up.user_id = user_model_defaults.user_id
                AND up.provider = user_model_defaults.image_provider_name
          ) = 1
      )
  );
