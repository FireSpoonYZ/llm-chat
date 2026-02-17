ALTER TABLE conversations ADD COLUMN subagent_provider TEXT;
ALTER TABLE conversations ADD COLUMN subagent_model TEXT;
ALTER TABLE mcp_servers ADD COLUMN read_only_overrides TEXT;
