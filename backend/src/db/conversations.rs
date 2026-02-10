use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Conversation {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub system_prompt_override: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn create_conversation(
    pool: &SqlitePool,
    user_id: &str,
    title: &str,
) -> Conversation {
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query_as::<_, Conversation>(
        "INSERT INTO conversations (id, user_id, title)
         VALUES (?, ?, ?)
         RETURNING id, user_id, title, provider, model_name,
                   system_prompt_override, created_at, updated_at",
    )
    .bind(&id)
    .bind(user_id)
    .bind(title)
    .fetch_one(pool)
    .await
    .expect("Failed to create conversation")
}

pub async fn list_conversations(
    pool: &SqlitePool,
    user_id: &str,
) -> Vec<Conversation> {
    sqlx::query_as::<_, Conversation>(
        "SELECT id, user_id, title, provider, model_name,
                system_prompt_override, created_at, updated_at
         FROM conversations
         WHERE user_id = ?
         ORDER BY updated_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .expect("Failed to list conversations")
}

pub async fn get_conversation(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
) -> Option<Conversation> {
    sqlx::query_as::<_, Conversation>(
        "SELECT id, user_id, title, provider, model_name,
                system_prompt_override, created_at, updated_at
         FROM conversations
         WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .expect("Failed to get conversation")
}

pub async fn update_conversation(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
    title: &str,
    provider: Option<&str>,
    model_name: Option<&str>,
    system_prompt_override: Option<&str>,
) -> Option<Conversation> {
    sqlx::query_as::<_, Conversation>(
        "UPDATE conversations
         SET title = ?, provider = ?, model_name = ?,
             system_prompt_override = ?, updated_at = datetime('now')
         WHERE id = ? AND user_id = ?
         RETURNING id, user_id, title, provider, model_name,
                   system_prompt_override, created_at, updated_at",
    )
    .bind(title)
    .bind(provider)
    .bind(model_name)
    .bind(system_prompt_override)
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .expect("Failed to update conversation")
}

pub async fn delete_conversation(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
) -> bool {
    let result = sqlx::query(
        "DELETE FROM conversations WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("Failed to delete conversation");

    result.rows_affected() > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use crate::db::users::create_user;

    async fn setup() -> (SqlitePool, String) {
        let pool = init_db("sqlite::memory:").await;
        let user = create_user(&pool, "testuser", "test@example.com", "hash").await;
        (pool, user.id)
    }

    #[tokio::test]
    async fn test_create_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(&pool, &user_id, "My Chat").await;
        assert_eq!(conv.user_id, user_id);
        assert_eq!(conv.title, "My Chat");
        assert!(conv.provider.is_none());
        assert!(conv.model_name.is_none());
        assert!(conv.system_prompt_override.is_none());
        assert!(!conv.id.is_empty());
    }

    #[tokio::test]
    async fn test_list_conversations() {
        let (pool, user_id) = setup().await;
        create_conversation(&pool, &user_id, "Chat 1").await;
        create_conversation(&pool, &user_id, "Chat 2").await;
        let convs = list_conversations(&pool, &user_id).await;
        assert_eq!(convs.len(), 2);
    }

    #[tokio::test]
    async fn test_get_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(&pool, &user_id, "Findable Chat").await;
        let fetched = get_conversation(&pool, &conv.id, &user_id).await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().title, "Findable Chat");
    }

    #[tokio::test]
    async fn test_update_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(&pool, &user_id, "Old Title").await;
        let updated = update_conversation(
            &pool,
            &conv.id,
            &user_id,
            "New Title",
            Some("openai"),
            Some("gpt-4"),
            Some("You are helpful."),
        )
        .await;
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.provider.as_deref(), Some("openai"));
        assert_eq!(updated.model_name.as_deref(), Some("gpt-4"));
        assert_eq!(updated.system_prompt_override.as_deref(), Some("You are helpful."));
    }

    #[tokio::test]
    async fn test_delete_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(&pool, &user_id, "To Delete").await;
        let deleted = delete_conversation(&pool, &conv.id, &user_id).await;
        assert!(deleted);
        let fetched = get_conversation(&pool, &conv.id, &user_id).await;
        assert!(fetched.is_none());
        // Deleting again should return false
        let deleted_again = delete_conversation(&pool, &conv.id, &user_id).await;
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_get_other_users_conversation_returns_none() {
        let (pool, user_id) = setup().await;
        let other_user = create_user(&pool, "other", "other@example.com", "hash2").await;
        let conv = create_conversation(&pool, &user_id, "Private Chat").await;
        // The other user should not be able to see this conversation
        let fetched = get_conversation(&pool, &conv.id, &other_user.id).await;
        assert!(fetched.is_none());
    }
}
