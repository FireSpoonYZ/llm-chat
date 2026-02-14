-- user_providers: add image_models column (JSON array, same format as models)
ALTER TABLE user_providers ADD COLUMN image_models TEXT;

-- conversations: add image model fields
ALTER TABLE conversations ADD COLUMN image_provider TEXT;
ALTER TABLE conversations ADD COLUMN image_model TEXT;

-- Migrate conversations.provider from provider type to provider name.
-- Previously provider stored the type (e.g. "openai"), now it stores the
-- user_providers.name so we can distinguish multiple providers of the same type.
-- Only migrate when the user has exactly one provider of that type;
-- ambiguous cases (multiple providers of the same type) are left unchanged
-- and require manual resolution.
UPDATE conversations SET provider = (
    SELECT COALESCE(up.name, up.provider)
    FROM user_providers up
    WHERE up.user_id = conversations.user_id
      AND up.provider = conversations.provider
) WHERE provider IS NOT NULL AND provider != ''
  AND (
    SELECT COUNT(*) FROM user_providers up
    WHERE up.user_id = conversations.user_id
      AND up.provider = conversations.provider
  ) = 1;
