-- Structured message storage (v2): message metadata + ordered content parts.
CREATE TABLE IF NOT EXISTS messages_v2 (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    provider TEXT,
    model TEXT,
    token_usage_json TEXT,
    meta_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS message_parts (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages_v2(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    part_type TEXT NOT NULL,
    text TEXT,
    json_payload TEXT,
    tool_call_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(message_id, seq)
);

CREATE INDEX IF NOT EXISTS idx_messages_v2_conversation_id
    ON messages_v2(conversation_id);

CREATE INDEX IF NOT EXISTS idx_messages_v2_conversation_created
    ON messages_v2(conversation_id, created_at, id);

CREATE INDEX IF NOT EXISTS idx_message_parts_message_id
    ON message_parts(message_id);

CREATE INDEX IF NOT EXISTS idx_message_parts_tool_call_id
    ON message_parts(tool_call_id);
