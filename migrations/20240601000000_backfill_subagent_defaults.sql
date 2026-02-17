UPDATE conversations
SET subagent_provider = provider
WHERE (subagent_provider IS NULL OR trim(subagent_provider) = '')
  AND provider IS NOT NULL
  AND trim(provider) != '';

UPDATE conversations
SET subagent_model = model_name
WHERE (subagent_model IS NULL OR trim(subagent_model) = '')
  AND model_name IS NOT NULL
  AND trim(model_name) != '';
