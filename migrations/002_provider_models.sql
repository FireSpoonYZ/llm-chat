-- Add models column (JSON array) to user_providers
ALTER TABLE user_providers ADD COLUMN models TEXT;

-- Migrate existing model_name data to models array
UPDATE user_providers SET models = '["' || model_name || '"]' WHERE model_name IS NOT NULL AND models IS NULL;
