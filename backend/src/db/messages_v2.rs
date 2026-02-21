use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use sqlx::Sqlite;
use sqlx::SqlitePool;
use sqlx::prelude::FromRow;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MessageV2 {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub token_usage_json: Option<String>,
    pub meta_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MessagePart {
    pub id: String,
    pub message_id: String,
    pub seq: i64,
    pub part_type: String,
    pub text: Option<String>,
    pub json_payload: Option<String>,
    pub tool_call_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewMessagePart<'a> {
    pub part_type: &'a str,
    pub text: Option<&'a str>,
    pub json_payload: Option<&'a str>,
    pub tool_call_id: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct NewMessagePartOwned {
    pub part_type: String,
    pub text: Option<String>,
    pub json_payload: Option<String>,
    pub tool_call_id: Option<String>,
}

fn tool_result_text_from_value(value: &serde_json::Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(obj) = value.as_object()
        && let Some(text) = obj.get("text").and_then(|v| v.as_str())
    {
        return text.to_string();
    }
    if let Some(arr) = value.as_array() {
        let joined = arr
            .iter()
            .filter_map(|b| {
                b.as_object().and_then(|obj| {
                    if obj.get("type").and_then(|v| v.as_str()) == Some("text") {
                        obj.get("text")
                            .or_else(|| obj.get("content"))
                            .and_then(|v| v.as_str())
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>()
            .join(" ");
        if !joined.is_empty() {
            return joined;
        }
    }
    value.to_string()
}

pub fn content_blocks_to_parts(
    content: &str,
    tool_calls: Option<&serde_json::Value>,
) -> Vec<NewMessagePartOwned> {
    let mut parts: Vec<NewMessagePartOwned> = Vec::new();

    if let Some(tc) = tool_calls
        && let Some(blocks) = tc.as_array()
    {
        for block in blocks {
            let Some(block_obj) = block.as_object() else {
                continue;
            };
            let block_type = block_obj
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            match block_type {
                "thinking" => {
                    let text = block_obj
                        .get("content")
                        .or_else(|| block_obj.get("thinking"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    parts.push(NewMessagePartOwned {
                        part_type: "reasoning".to_string(),
                        text,
                        json_payload: Some(block.to_string()),
                        tool_call_id: None,
                    });
                }
                "text" => {
                    let text = block_obj
                        .get("content")
                        .or_else(|| block_obj.get("text"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    parts.push(NewMessagePartOwned {
                        part_type: "text".to_string(),
                        text,
                        json_payload: None,
                        tool_call_id: None,
                    });
                }
                "tool_call" => {
                    let tool_call_id = block_obj
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    parts.push(NewMessagePartOwned {
                        part_type: "tool_call".to_string(),
                        text: None,
                        json_payload: Some(block.to_string()),
                        tool_call_id: tool_call_id.clone(),
                    });
                    if let Some(result) = block_obj.get("result") {
                        parts.push(NewMessagePartOwned {
                            part_type: "tool_result".to_string(),
                            text: Some(tool_result_text_from_value(result)),
                            json_payload: Some(result.to_string()),
                            tool_call_id,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    // Legacy fallback when no structured blocks are present.
    if parts.is_empty() && !content.is_empty() {
        parts.push(NewMessagePartOwned {
            part_type: "text".to_string(),
            text: Some(content.to_string()),
            json_payload: None,
            tool_call_id: None,
        });
    }

    parts
}

pub fn legacy_message_to_parts(message: &crate::db::messages::Message) -> Vec<NewMessagePartOwned> {
    let parsed_tool_calls = message
        .tool_calls
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
    content_blocks_to_parts(&message.content, parsed_tool_calls.as_ref())
}

#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub async fn create_message_v2(
    pool: &SqlitePool,
    message_id: Option<&str>,
    conversation_id: &str,
    role: &str,
    provider: Option<&str>,
    model: Option<&str>,
    token_usage_json: Option<&str>,
    meta_json: Option<&str>,
) -> Result<MessageV2, sqlx::Error> {
    let id = message_id
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    sqlx::query_as::<_, MessageV2>(
        "INSERT INTO messages_v2 (id, conversation_id, role, provider, model, token_usage_json, meta_json) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, conversation_id, role, provider, model, token_usage_json, meta_json, created_at",
    )
    .bind(&id)
    .bind(conversation_id)
    .bind(role)
    .bind(provider)
    .bind(model)
    .bind(token_usage_json)
    .bind(meta_json)
    .fetch_one(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_message_with_parts(
    pool: &SqlitePool,
    message_id: Option<&str>,
    conversation_id: &str,
    role: &str,
    provider: Option<&str>,
    model: Option<&str>,
    token_usage_json: Option<&str>,
    meta_json: Option<&str>,
    parts: &[NewMessagePart<'_>],
) -> Result<(MessageV2, Vec<MessagePart>), sqlx::Error> {
    let mut tx = pool.begin().await?;

    let message_id = message_id
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let message = sqlx::query_as::<_, MessageV2>(
        "INSERT INTO messages_v2 (id, conversation_id, role, provider, model, token_usage_json, meta_json) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, conversation_id, role, provider, model, token_usage_json, meta_json, created_at",
    )
    .bind(&message_id)
    .bind(conversation_id)
    .bind(role)
    .bind(provider)
    .bind(model)
    .bind(token_usage_json)
    .bind(meta_json)
    .fetch_one(&mut *tx)
    .await?;

    let mut created_parts = Vec::with_capacity(parts.len());
    for (idx, part) in parts.iter().enumerate() {
        let part_id = uuid::Uuid::new_v4().to_string();
        let created = sqlx::query_as::<_, MessagePart>(
            "INSERT INTO message_parts (id, message_id, seq, part_type, text, json_payload, tool_call_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, message_id, seq, part_type, text, json_payload, tool_call_id, created_at",
        )
        .bind(&part_id)
        .bind(&message_id)
        .bind(idx as i64)
        .bind(part.part_type)
        .bind(part.text)
        .bind(part.json_payload)
        .bind(part.tool_call_id)
        .fetch_one(&mut *tx)
        .await?;
        created_parts.push(created);
    }

    tx.commit().await?;
    Ok((message, created_parts))
}

pub async fn upsert_message_text_part(
    pool: &SqlitePool,
    message_id: &str,
    conversation_id: &str,
    role: &str,
    text: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM messages_v2 WHERE id = ? LIMIT 1")
        .bind(message_id)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();

    if !exists {
        sqlx::query("INSERT INTO messages_v2 (id, conversation_id, role) VALUES (?, ?, ?)")
            .bind(message_id)
            .bind(conversation_id)
            .bind(role)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("DELETE FROM message_parts WHERE message_id = ?")
        .bind(message_id)
        .execute(&mut *tx)
        .await?;

    let part_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO message_parts (id, message_id, seq, part_type, text, json_payload, tool_call_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(part_id)
    .bind(message_id)
    .bind(0_i64)
    .bind("text")
    .bind(text)
    .bind(Option::<&str>::None)
    .bind(Option::<&str>::None)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn get_message_v2(
    pool: &SqlitePool,
    message_id: &str,
) -> Result<Option<MessageV2>, sqlx::Error> {
    sqlx::query_as::<_, MessageV2>(
        "SELECT id, conversation_id, role, provider, model, token_usage_json, meta_json, created_at \
         FROM messages_v2 \
         WHERE id = ?",
    )
    .bind(message_id)
    .fetch_optional(pool)
    .await
}

#[allow(dead_code)]
pub async fn list_messages_v2(
    pool: &SqlitePool,
    conversation_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<MessageV2>, sqlx::Error> {
    sqlx::query_as::<_, MessageV2>(
        "SELECT id, conversation_id, role, provider, model, token_usage_json, meta_json, created_at \
         FROM messages_v2 \
         WHERE conversation_id = ? \
         ORDER BY rowid ASC \
         LIMIT ? OFFSET ?",
    )
    .bind(conversation_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn list_message_parts(
    pool: &SqlitePool,
    message_id: &str,
) -> Result<Vec<MessagePart>, sqlx::Error> {
    sqlx::query_as::<_, MessagePart>(
        "SELECT id, message_id, seq, part_type, text, json_payload, tool_call_id, created_at \
         FROM message_parts \
         WHERE message_id = ? \
         ORDER BY seq ASC",
    )
    .bind(message_id)
    .fetch_all(pool)
    .await
}

pub async fn list_existing_message_v2_ids(
    pool: &SqlitePool,
    message_ids: &[String],
) -> Result<HashSet<String>, sqlx::Error> {
    if message_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new("SELECT id FROM messages_v2 WHERE id IN (");
    {
        let mut separated = query.separated(", ");
        for id in message_ids {
            separated.push_bind(id);
        }
    }
    query.push(")");

    let rows = query.build_query_as::<(String,)>().fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

pub async fn list_message_parts_for_messages(
    pool: &SqlitePool,
    message_ids: &[String],
) -> Result<HashMap<String, Vec<MessagePart>>, sqlx::Error> {
    if message_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT id, message_id, seq, part_type, text, json_payload, tool_call_id, created_at \
         FROM message_parts WHERE message_id IN (",
    );
    {
        let mut separated = query.separated(", ");
        for id in message_ids {
            separated.push_bind(id);
        }
    }
    query.push(") ORDER BY message_id ASC, seq ASC");

    let rows = query
        .build_query_as::<MessagePart>()
        .fetch_all(pool)
        .await?;
    let mut grouped: HashMap<String, Vec<MessagePart>> = HashMap::new();
    for part in rows {
        grouped
            .entry(part.message_id.clone())
            .or_default()
            .push(part);
    }
    Ok(grouped)
}

pub async fn delete_messages_v2_after(
    pool: &SqlitePool,
    conversation_id: &str,
    after_message_id: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM messages_v2 WHERE conversation_id = ? \
         AND rowid > (SELECT rowid FROM messages_v2 WHERE id = ?)",
    )
    .bind(conversation_id)
    .bind(after_message_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::conversations::create_conversation;
    use crate::db::init_db;
    use crate::db::messages::{
        Message, create_message, delete_messages_after, get_message, list_messages,
        update_message_content,
    };
    use crate::db::users::create_user;

    async fn setup() -> (SqlitePool, String) {
        let pool = init_db("sqlite::memory:").await;
        let user = create_user(&pool, "v2user", "v2@example.com", "hash")
            .await
            .unwrap();
        let conv = create_conversation(
            &pool,
            &user.id,
            "V2 Conversation",
            None,
            None,
            None,
            false,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        (pool, conv.id)
    }

    #[tokio::test]
    async fn test_create_and_list_message_with_parts() {
        let (pool, conv_id) = setup().await;
        let (msg, parts) = create_message_with_parts(
            &pool,
            None,
            &conv_id,
            "assistant",
            Some("openai"),
            Some("gpt-4o"),
            Some(r#"{"completion":12}"#),
            Some(r#"{"turn_id":"t-1"}"#),
            &[
                NewMessagePart {
                    part_type: "reasoning",
                    text: Some("analysis"),
                    json_payload: Some(r#"{"raw":"x"}"#),
                    tool_call_id: None,
                },
                NewMessagePart {
                    part_type: "text",
                    text: Some("final answer"),
                    json_payload: None,
                    tool_call_id: None,
                },
            ],
        )
        .await
        .unwrap();

        assert_eq!(msg.conversation_id, conv_id);
        assert_eq!(msg.provider.as_deref(), Some("openai"));
        assert_eq!(msg.model.as_deref(), Some("gpt-4o"));
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].seq, 0);
        assert_eq!(parts[1].seq, 1);

        let listed = list_messages_v2(&pool, &conv_id, 50, 0).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, msg.id);

        let listed_parts = list_message_parts(&pool, &msg.id).await.unwrap();
        assert_eq!(listed_parts.len(), 2);
        assert_eq!(listed_parts[0].part_type, "reasoning");
        assert_eq!(listed_parts[1].part_type, "text");
    }

    #[tokio::test]
    async fn test_delete_messages_after_cascades_parts() {
        let (pool, conv_id) = setup().await;
        let (m1, _) = create_message_with_parts(
            &pool,
            None,
            &conv_id,
            "assistant",
            None,
            None,
            None,
            None,
            &[NewMessagePart {
                part_type: "text",
                text: Some("first"),
                json_payload: None,
                tool_call_id: None,
            }],
        )
        .await
        .unwrap();

        let (m2, _) = create_message_with_parts(
            &pool,
            None,
            &conv_id,
            "assistant",
            None,
            None,
            None,
            None,
            &[NewMessagePart {
                part_type: "text",
                text: Some("second"),
                json_payload: None,
                tool_call_id: None,
            }],
        )
        .await
        .unwrap();

        let deleted = delete_messages_v2_after(&pool, &conv_id, &m1.id)
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        let msgs = list_messages_v2(&pool, &conv_id, 50, 0).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, m1.id);

        let m2_parts = list_message_parts(&pool, &m2.id).await.unwrap();
        assert_eq!(m2_parts.len(), 0);
    }

    #[tokio::test]
    async fn test_create_message_with_explicit_id() {
        let (pool, conv_id) = setup().await;
        let explicit_id = "msg-explicit-id";
        let (msg, parts) = create_message_with_parts(
            &pool,
            Some(explicit_id),
            &conv_id,
            "assistant",
            None,
            None,
            None,
            None,
            &[NewMessagePart {
                part_type: "text",
                text: Some("hello"),
                json_payload: None,
                tool_call_id: None,
            }],
        )
        .await
        .unwrap();
        assert_eq!(msg.id, explicit_id);
        assert_eq!(parts.len(), 1);
        let fetched = get_message_v2(&pool, explicit_id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, explicit_id);
    }

    #[tokio::test]
    async fn test_upsert_message_text_part_inserts_then_replaces() {
        let (pool, conv_id) = setup().await;
        let message_id = "msg-upsert-1";

        upsert_message_text_part(&pool, message_id, &conv_id, "user", "first")
            .await
            .unwrap();
        let msg = get_message_v2(&pool, message_id).await.unwrap().unwrap();
        assert_eq!(msg.id, message_id);
        assert_eq!(msg.role, "user");
        let parts = list_message_parts(&pool, message_id).await.unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].part_type, "text");
        assert_eq!(parts[0].text.as_deref(), Some("first"));

        upsert_message_text_part(&pool, message_id, &conv_id, "user", "edited")
            .await
            .unwrap();
        let parts = list_message_parts(&pool, message_id).await.unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].seq, 0);
        assert_eq!(parts[0].text.as_deref(), Some("edited"));
    }

    #[test]
    fn test_content_blocks_to_parts_includes_tool_result_part() {
        let blocks = serde_json::json!([
            {"type":"thinking","content":"plan"},
            {"type":"text","content":"working"},
            {
                "type":"tool_call",
                "id":"tc-1",
                "name":"bash",
                "input":{"command":"ls"},
                "result":{"kind":"bash","text":"file1\nfile2"}
            }
        ]);

        let parts = content_blocks_to_parts("ignored", Some(&blocks));
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0].part_type, "reasoning");
        assert_eq!(parts[1].part_type, "text");
        assert_eq!(parts[2].part_type, "tool_call");
        assert_eq!(parts[2].tool_call_id.as_deref(), Some("tc-1"));
        assert_eq!(parts[3].part_type, "tool_result");
        assert_eq!(parts[3].text.as_deref(), Some("file1\nfile2"));
        assert_eq!(parts[3].tool_call_id.as_deref(), Some("tc-1"));
    }

    #[test]
    fn test_legacy_message_to_parts_falls_back_to_content() {
        let msg = Message {
            id: "m1".to_string(),
            conversation_id: "c1".to_string(),
            role: "assistant".to_string(),
            content: "plain".to_string(),
            tool_calls: None,
            tool_call_id: None,
            token_count: None,
            created_at: "now".to_string(),
        };
        let parts = legacy_message_to_parts(&msg);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].part_type, "text");
        assert_eq!(parts[0].text.as_deref(), Some("plain"));
    }

    #[tokio::test]
    async fn test_edit_message_syncs_legacy_and_v2_text_part() {
        let (pool, conv_id) = setup().await;
        let msg = create_message(&pool, &conv_id, "user", "before edit", None, None, None)
            .await
            .unwrap();

        upsert_message_text_part(&pool, &msg.id, &conv_id, "user", "before edit")
            .await
            .unwrap();

        update_message_content(&pool, &msg.id, "after edit")
            .await
            .unwrap();
        upsert_message_text_part(&pool, &msg.id, &conv_id, "user", "after edit")
            .await
            .unwrap();

        let legacy = get_message(&pool, &msg.id).await.unwrap().unwrap();
        assert_eq!(legacy.content, "after edit");
        let parts = list_message_parts(&pool, &msg.id).await.unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].part_type, "text");
        assert_eq!(parts[0].text.as_deref(), Some("after edit"));
    }

    #[tokio::test]
    async fn test_regenerate_truncation_keeps_legacy_and_v2_consistent() {
        let (pool, conv_id) = setup().await;
        let m1 = create_message(&pool, &conv_id, "user", "u1", None, None, None)
            .await
            .unwrap();
        let m2 = create_message(&pool, &conv_id, "assistant", "a1", None, None, None)
            .await
            .unwrap();
        let m3 = create_message(&pool, &conv_id, "assistant", "a2", None, None, None)
            .await
            .unwrap();

        upsert_message_text_part(&pool, &m1.id, &conv_id, "user", "u1")
            .await
            .unwrap();
        upsert_message_text_part(&pool, &m2.id, &conv_id, "assistant", "a1")
            .await
            .unwrap();
        upsert_message_text_part(&pool, &m3.id, &conv_id, "assistant", "a2")
            .await
            .unwrap();

        let deleted_legacy = delete_messages_after(&pool, &conv_id, &m1.id)
            .await
            .unwrap();
        let deleted_v2 = delete_messages_v2_after(&pool, &conv_id, &m1.id)
            .await
            .unwrap();
        assert_eq!(deleted_legacy, 2);
        assert_eq!(deleted_v2, 2);

        let legacy_remaining = list_messages(&pool, &conv_id, 50, 0).await.unwrap();
        let v2_remaining = list_messages_v2(&pool, &conv_id, 50, 0).await.unwrap();
        assert_eq!(legacy_remaining.len(), 1);
        assert_eq!(v2_remaining.len(), 1);
        assert_eq!(legacy_remaining[0].id, m1.id);
        assert_eq!(v2_remaining[0].id, m1.id);
    }

    #[tokio::test]
    async fn test_batch_lookup_helpers() {
        let (pool, conv_id) = setup().await;
        let m1 = create_message(&pool, &conv_id, "user", "u1", None, None, None)
            .await
            .unwrap();
        let m2 = create_message(&pool, &conv_id, "assistant", "a1", None, None, None)
            .await
            .unwrap();
        upsert_message_text_part(&pool, &m1.id, &conv_id, "user", "u1")
            .await
            .unwrap();
        upsert_message_text_part(&pool, &m2.id, &conv_id, "assistant", "a1")
            .await
            .unwrap();

        let existing = list_existing_message_v2_ids(
            &pool,
            &[m1.id.clone(), m2.id.clone(), "missing".to_string()],
        )
        .await
        .unwrap();
        assert!(existing.contains(&m1.id));
        assert!(existing.contains(&m2.id));
        assert!(!existing.contains("missing"));

        let grouped = list_message_parts_for_messages(&pool, &[m1.id.clone(), m2.id.clone()])
            .await
            .unwrap();
        assert_eq!(grouped.get(&m1.id).map(Vec::len), Some(1));
        assert_eq!(grouped.get(&m2.id).map(Vec::len), Some(1));
    }
}
