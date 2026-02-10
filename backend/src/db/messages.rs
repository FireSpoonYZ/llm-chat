use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub token_count: Option<i64>,
    pub created_at: String,
}

pub async fn create_message(
    pool: &SqlitePool,
    conversation_id: &str,
    role: &str,
    content: &str,
    tool_calls: Option<&str>,
    tool_call_id: Option<&str>,
    token_count: Option<i64>,
) -> Message {
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query_as::<_, Message>(
        "INSERT INTO messages (id, conversation_id, role, content, \
         tool_calls, tool_call_id, token_count) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, conversation_id, role, content, \
         tool_calls, tool_call_id, token_count, created_at",
    )
    .bind(&id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(tool_calls)
    .bind(tool_call_id)
    .bind(token_count)
    .fetch_one(pool)
    .await
    .expect("Failed to create message")
}

pub async fn list_messages(
    pool: &SqlitePool,
    conversation_id: &str,
    limit: i64,
    offset: i64,
) -> Vec<Message> {
    sqlx::query_as::<_, Message>(
        "SELECT id, conversation_id, role, content, \
         tool_calls, tool_call_id, token_count, created_at \
         FROM messages \
         WHERE conversation_id = ? \
         ORDER BY created_at ASC \
         LIMIT ? OFFSET ?",
    )
    .bind(conversation_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .expect("Failed to list messages")
}

#[derive(Debug, Clone, FromRow)]
struct CountRow {
    count: i64,
}

pub async fn count_messages(pool: &SqlitePool, conversation_id: &str) -> i64 {
    let row = sqlx::query_as::<_, CountRow>(
        "SELECT COUNT(*) as count FROM messages WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_one(pool)
    .await
    .expect("Failed to count messages");

    row.count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use crate::db::conversations::create_conversation;
    use crate::db::users::create_user;

    async fn setup() -> (SqlitePool, String) {
        let pool = init_db("sqlite::memory:").await;
        let user = create_user(&pool, "testuser", "test@example.com", "hash").await;
        let conv = create_conversation(&pool, &user.id, "Test Conv").await;
        (pool, conv.id)
    }

    #[tokio::test]
    async fn test_create_message() {
        let (pool, conv_id) = setup().await;
        let msg = create_message(
            &pool, &conv_id, "user", "Hello!", None, None, Some(5),
        )
        .await;
        assert_eq!(msg.conversation_id, conv_id);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Hello!");
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
        assert_eq!(msg.token_count, Some(5));
        assert!(!msg.id.is_empty());
    }

    #[tokio::test]
    async fn test_list_messages_with_pagination() {
        let (pool, conv_id) = setup().await;
        for i in 0..5 {
            create_message(
                &pool,
                &conv_id,
                "user",
                &format!("Message {i}"),
                None,
                None,
                None,
            )
            .await;
        }
        // Fetch first page (limit 3, offset 0)
        let page1 = list_messages(&pool, &conv_id, 3, 0).await;
        assert_eq!(page1.len(), 3);
        assert_eq!(page1[0].content, "Message 0");

        // Fetch second page (limit 3, offset 3)
        let page2 = list_messages(&pool, &conv_id, 3, 3).await;
        assert_eq!(page2.len(), 2);
        assert_eq!(page2[0].content, "Message 3");
    }

    #[tokio::test]
    async fn test_count_messages() {
        let (pool, conv_id) = setup().await;
        assert_eq!(count_messages(&pool, &conv_id).await, 0);
        create_message(&pool, &conv_id, "user", "Hi", None, None, None).await;
        create_message(&pool, &conv_id, "assistant", "Hello", None, None, None).await;
        assert_eq!(count_messages(&pool, &conv_id).await, 2);
    }
}
