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
) -> Result<Message, sqlx::Error> {
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
}

pub async fn list_messages(
    pool: &SqlitePool,
    conversation_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Message>, sqlx::Error> {
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
}

#[derive(Debug, Clone, FromRow)]
struct CountRow {
    count: i64,
}

pub async fn get_message(pool: &SqlitePool, id: &str) -> Result<Option<Message>, sqlx::Error> {
    sqlx::query_as::<_, Message>(
        "SELECT id, conversation_id, role, content, \
         tool_calls, tool_call_id, token_count, created_at \
         FROM messages WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn update_message_content(pool: &SqlitePool, id: &str, content: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("UPDATE messages SET content = ? WHERE id = ?")
        .bind(content)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_messages_after(
    pool: &SqlitePool,
    conversation_id: &str,
    after_message_id: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM messages WHERE conversation_id = ? \
         AND rowid > (SELECT rowid FROM messages WHERE id = ?)",
    )
    .bind(conversation_id)
    .bind(after_message_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn count_messages(pool: &SqlitePool, conversation_id: &str) -> Result<i64, sqlx::Error> {
    let row = sqlx::query_as::<_, CountRow>(
        "SELECT COUNT(*) as count FROM messages WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_one(pool)
    .await?;

    Ok(row.count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use crate::db::conversations::create_conversation;
    use crate::db::users::create_user;

    async fn setup() -> (SqlitePool, String) {
        let pool = init_db("sqlite::memory:").await;
        let user = create_user(&pool, "testuser", "test@example.com", "hash").await.unwrap();
        let conv = create_conversation(&pool, &user.id, "Test Conv", None, None, None, false).await.unwrap();
        (pool, conv.id)
    }

    #[tokio::test]
    async fn test_create_message() {
        let (pool, conv_id) = setup().await;
        let msg = create_message(
            &pool, &conv_id, "user", "Hello!", None, None, Some(5),
        )
        .await
        .unwrap();
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
            .await
            .unwrap();
        }
        // Fetch first page (limit 3, offset 0)
        let page1 = list_messages(&pool, &conv_id, 3, 0).await.unwrap();
        assert_eq!(page1.len(), 3);
        assert_eq!(page1[0].content, "Message 0");

        // Fetch second page (limit 3, offset 3)
        let page2 = list_messages(&pool, &conv_id, 3, 3).await.unwrap();
        assert_eq!(page2.len(), 2);
        assert_eq!(page2[0].content, "Message 3");
    }

    #[tokio::test]
    async fn test_count_messages() {
        let (pool, conv_id) = setup().await;
        assert_eq!(count_messages(&pool, &conv_id).await.unwrap(), 0);
        create_message(&pool, &conv_id, "user", "Hi", None, None, None).await.unwrap();
        create_message(&pool, &conv_id, "assistant", "Hello", None, None, None).await.unwrap();
        assert_eq!(count_messages(&pool, &conv_id).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_get_message() {
        let (pool, conv_id) = setup().await;
        let msg = create_message(&pool, &conv_id, "user", "Hello!", None, None, None).await.unwrap();
        let fetched = get_message(&pool, &msg.id).await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, msg.id);
        assert_eq!(fetched.content, "Hello!");
        assert_eq!(fetched.role, "user");
    }

    #[tokio::test]
    async fn test_get_message_not_found() {
        let (pool, _) = setup().await;
        let fetched = get_message(&pool, "nonexistent-id").await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_update_message_content() {
        let (pool, conv_id) = setup().await;
        let msg = create_message(&pool, &conv_id, "user", "Original", None, None, None).await.unwrap();
        let updated = update_message_content(&pool, &msg.id, "Updated content").await.unwrap();
        assert!(updated);
        let fetched = get_message(&pool, &msg.id).await.unwrap().unwrap();
        assert_eq!(fetched.content, "Updated content");
    }

    #[tokio::test]
    async fn test_update_message_content_not_found() {
        let (pool, _) = setup().await;
        let updated = update_message_content(&pool, "nonexistent-id", "New content").await.unwrap();
        assert!(!updated);
    }

    #[tokio::test]
    async fn test_delete_messages_after() {
        let (pool, conv_id) = setup().await;
        let msg1 = create_message(&pool, &conv_id, "user", "First", None, None, None).await.unwrap();
        let _msg2 = create_message(&pool, &conv_id, "assistant", "Second", None, None, None).await.unwrap();
        let _msg3 = create_message(&pool, &conv_id, "user", "Third", None, None, None).await.unwrap();

        let deleted = delete_messages_after(&pool, &conv_id, &msg1.id).await.unwrap();
        assert_eq!(deleted, 2);

        let remaining = list_messages(&pool, &conv_id, 100, 0).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].content, "First");
    }

    #[tokio::test]
    async fn test_delete_messages_after_none() {
        let (pool, conv_id) = setup().await;
        let msg = create_message(&pool, &conv_id, "user", "Only one", None, None, None).await.unwrap();
        let deleted = delete_messages_after(&pool, &conv_id, &msg.id).await.unwrap();
        assert_eq!(deleted, 0);
    }
}
