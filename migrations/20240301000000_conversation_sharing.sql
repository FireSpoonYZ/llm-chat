ALTER TABLE conversations ADD COLUMN share_token TEXT;
CREATE UNIQUE INDEX idx_conversations_share_token ON conversations(share_token) WHERE share_token IS NOT NULL;
