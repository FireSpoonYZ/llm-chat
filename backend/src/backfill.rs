use sqlx::SqlitePool;

use crate::db;

#[derive(sqlx::FromRow)]
struct LegacyMessageRow {
    rowid: i64,
    id: String,
    conversation_id: String,
    role: String,
    content: String,
    tool_calls: Option<String>,
    tool_call_id: Option<String>,
    token_count: Option<i64>,
    created_at: String,
}

impl LegacyMessageRow {
    fn into_message(self) -> db::messages::Message {
        db::messages::Message {
            id: self.id,
            conversation_id: self.conversation_id,
            role: self.role,
            content: self.content,
            tool_calls: self.tool_calls,
            tool_call_id: self.tool_call_id,
            token_count: self.token_count,
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BackfillMessagesV2Stats {
    pub scanned: u64,
    pub inserted: u64,
    pub skipped_existing: u64,
    pub failed: u64,
}

pub async fn backfill_messages_v2(
    pool: &SqlitePool,
) -> Result<BackfillMessagesV2Stats, sqlx::Error> {
    let mut stats = BackfillMessagesV2Stats::default();
    const BATCH_SIZE: i64 = 500;
    let mut last_rowid: i64 = 0;

    loop {
        let batch_rows = sqlx::query_as::<_, LegacyMessageRow>(
            "SELECT rowid, id, conversation_id, role, content, tool_calls, tool_call_id, token_count, created_at \
             FROM messages WHERE rowid > ? ORDER BY rowid ASC LIMIT ?",
        )
        .bind(last_rowid)
        .bind(BATCH_SIZE)
        .fetch_all(pool)
        .await?;

        if batch_rows.is_empty() {
            break;
        }
        last_rowid = batch_rows.last().map(|row| row.rowid).unwrap_or(last_rowid);

        let batch = batch_rows
            .into_iter()
            .map(LegacyMessageRow::into_message)
            .collect::<Vec<_>>();

        let batch_ids = batch.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
        let existing_v2_ids =
            db::messages_v2::list_existing_message_v2_ids(pool, &batch_ids).await?;

        for message in batch {
            stats.scanned += 1;

            if existing_v2_ids.contains(&message.id) {
                stats.skipped_existing += 1;
                continue;
            }

            let parts = db::messages_v2::legacy_message_to_parts(&message);
            let borrowed_parts: Vec<db::messages_v2::NewMessagePart<'_>> = parts
                .iter()
                .map(|p| db::messages_v2::NewMessagePart {
                    part_type: &p.part_type,
                    text: p.text.as_deref(),
                    json_payload: p.json_payload.as_deref(),
                    tool_call_id: p.tool_call_id.as_deref(),
                })
                .collect();
            let token_usage_json = message
                .token_count
                .map(|completion| serde_json::json!({ "completion": completion }).to_string());

            match db::messages_v2::create_message_with_parts(
                pool,
                Some(&message.id),
                &message.conversation_id,
                &message.role,
                None,
                None,
                token_usage_json.as_deref(),
                None,
                &borrowed_parts,
            )
            .await
            {
                Ok(_) => {
                    stats.inserted += 1;
                }
                Err(e) => {
                    stats.failed += 1;
                    tracing::warn!(
                        message_id = %message.id,
                        conversation_id = %message.conversation_id,
                        error = %e,
                        "Failed to backfill message into messages_v2"
                    );
                }
            }
        }
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::conversations::create_conversation;
    use crate::db::init_db;
    use crate::db::messages::create_message;
    use crate::db::messages_v2::list_message_parts;
    use crate::db::users::create_user;

    async fn setup() -> (SqlitePool, String) {
        let pool = init_db("sqlite::memory:").await;
        let user = create_user(&pool, "bf-user", "bf@example.com", "hash")
            .await
            .unwrap();
        let conv = create_conversation(
            &pool, &user.id, "Backfill", None, None, None, false, None, None, None,
        )
        .await
        .unwrap();
        (pool, conv.id)
    }

    #[tokio::test]
    async fn test_backfill_messages_v2_inserts_missing_rows() {
        let (pool, conv_id) = setup().await;

        let user_msg = create_message(&pool, &conv_id, "user", "hello", None, None, None)
            .await
            .unwrap();
        let assistant_blocks = serde_json::json!([
            {"type":"thinking","content":"planning"},
            {"type":"text","content":"running"},
            {
                "type":"tool_call",
                "id":"tc-1",
                "name":"bash",
                "input":{"command":"ls"},
                "result":{"kind":"bash","text":"file1\\nfile2"}
            }
        ]);
        let assistant_msg = create_message(
            &pool,
            &conv_id,
            "assistant",
            "final",
            Some(&assistant_blocks.to_string()),
            None,
            Some(42),
        )
        .await
        .unwrap();

        let stats = backfill_messages_v2(&pool).await.unwrap();
        assert_eq!(stats.scanned, 2);
        assert_eq!(stats.inserted, 2);
        assert_eq!(stats.skipped_existing, 0);
        assert_eq!(stats.failed, 0);

        let user_parts = list_message_parts(&pool, &user_msg.id).await.unwrap();
        assert_eq!(user_parts.len(), 1);
        assert_eq!(user_parts[0].part_type, "text");
        assert_eq!(user_parts[0].text.as_deref(), Some("hello"));

        let assistant_parts = list_message_parts(&pool, &assistant_msg.id).await.unwrap();
        assert_eq!(assistant_parts.len(), 4);
        assert_eq!(assistant_parts[0].part_type, "reasoning");
        assert_eq!(assistant_parts[1].part_type, "text");
        assert_eq!(assistant_parts[2].part_type, "tool_call");
        assert_eq!(assistant_parts[3].part_type, "tool_result");
        assert_eq!(assistant_parts[3].tool_call_id.as_deref(), Some("tc-1"));
    }

    #[tokio::test]
    async fn test_backfill_messages_v2_is_idempotent() {
        let (pool, conv_id) = setup().await;
        create_message(&pool, &conv_id, "user", "hello", None, None, None)
            .await
            .unwrap();

        let first = backfill_messages_v2(&pool).await.unwrap();
        assert_eq!(first.inserted, 1);

        let second = backfill_messages_v2(&pool).await.unwrap();
        assert_eq!(second.scanned, 1);
        assert_eq!(second.inserted, 0);
        assert_eq!(second.skipped_existing, 1);
        assert_eq!(second.failed, 0);
    }

    #[tokio::test]
    async fn test_backfill_malformed_tool_calls_falls_back_to_text_part() {
        let (pool, conv_id) = setup().await;
        let msg = create_message(
            &pool,
            &conv_id,
            "assistant",
            "plain fallback",
            Some("{not-json"),
            None,
            None,
        )
        .await
        .unwrap();

        let stats = backfill_messages_v2(&pool).await.unwrap();
        assert_eq!(stats.scanned, 1);
        assert_eq!(stats.inserted, 1);
        assert_eq!(stats.failed, 0);

        let parts = list_message_parts(&pool, &msg.id).await.unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].part_type, "text");
        assert_eq!(parts[0].text.as_deref(), Some("plain fallback"));
    }
}
