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
    pub deep_thinking: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn create_conversation(
    pool: &SqlitePool,
    user_id: &str,
    title: &str,
    system_prompt_override: Option<&str>,
    provider: Option<&str>,
    model_name: Option<&str>,
    deep_thinking: bool,
) -> Result<Conversation, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query_as::<_, Conversation>(
        "INSERT INTO conversations (id, user_id, title, system_prompt_override, provider, model_name, deep_thinking)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING id, user_id, title, provider, model_name,
                   system_prompt_override, deep_thinking, created_at, updated_at",
    )
    .bind(&id)
    .bind(user_id)
    .bind(title)
    .bind(system_prompt_override)
    .bind(provider)
    .bind(model_name)
    .bind(deep_thinking)
    .fetch_one(pool)
    .await
}

pub async fn list_conversations(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<Conversation>, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        "SELECT id, user_id, title, provider, model_name,
                system_prompt_override, deep_thinking, created_at, updated_at
         FROM conversations
         WHERE user_id = ?
         ORDER BY updated_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn get_conversation(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        "SELECT id, user_id, title, provider, model_name,
                system_prompt_override, deep_thinking, created_at, updated_at
         FROM conversations
         WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn update_conversation(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
    title: &str,
    provider: Option<&str>,
    model_name: Option<&str>,
    system_prompt_override: Option<&str>,
    deep_thinking: bool,
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        "UPDATE conversations
         SET title = ?, provider = ?, model_name = ?,
             system_prompt_override = ?, deep_thinking = ?, updated_at = datetime('now')
         WHERE id = ? AND user_id = ?
         RETURNING id, user_id, title, provider, model_name,
                   system_prompt_override, deep_thinking, created_at, updated_at",
    )
    .bind(title)
    .bind(provider)
    .bind(model_name)
    .bind(system_prompt_override)
    .bind(deep_thinking)
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn delete_conversation(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM conversations WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use crate::db::users::create_user;

    async fn setup() -> (SqlitePool, String) {
        let pool = init_db("sqlite::memory:").await;
        let user = create_user(&pool, "testuser", "test@example.com", "hash").await.unwrap();
        (pool, user.id)
    }

    #[tokio::test]
    async fn test_create_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(&pool, &user_id, "My Chat", None, None, None, false).await.unwrap();
        assert_eq!(conv.user_id, user_id);
        assert_eq!(conv.title, "My Chat");
        assert!(conv.provider.is_none());
        assert!(conv.model_name.is_none());
        assert!(conv.system_prompt_override.is_none());
        assert!(!conv.deep_thinking);
        assert!(!conv.id.is_empty());
    }

    #[tokio::test]
    async fn test_create_conversation_with_system_prompt() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(&pool, &user_id, "Prompted Chat", Some("You are a pirate."), None, None, false).await.unwrap();
        assert_eq!(conv.system_prompt_override.as_deref(), Some("You are a pirate."));
    }

    #[tokio::test]
    async fn test_create_conversation_with_provider_and_model() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool, &user_id, "Provider Chat", None,
            Some("openai"), Some("gpt-4o"), false,
        ).await.unwrap();
        assert_eq!(conv.provider.as_deref(), Some("openai"));
        assert_eq!(conv.model_name.as_deref(), Some("gpt-4o"));
        assert!(conv.system_prompt_override.is_none());
    }

    #[tokio::test]
    async fn test_create_conversation_with_all_fields() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool, &user_id, "Full Chat", Some("Be helpful."),
            Some("anthropic"), Some("claude-3"), true,
        ).await.unwrap();
        assert_eq!(conv.title, "Full Chat");
        assert_eq!(conv.system_prompt_override.as_deref(), Some("Be helpful."));
        assert_eq!(conv.provider.as_deref(), Some("anthropic"));
        assert_eq!(conv.model_name.as_deref(), Some("claude-3"));
        assert!(conv.deep_thinking);
    }

    #[tokio::test]
    async fn test_list_conversations() {
        let (pool, user_id) = setup().await;
        create_conversation(&pool, &user_id, "Chat 1", None, None, None, false).await.unwrap();
        create_conversation(&pool, &user_id, "Chat 2", None, None, None, false).await.unwrap();
        let convs = list_conversations(&pool, &user_id).await.unwrap();
        assert_eq!(convs.len(), 2);
    }

    #[tokio::test]
    async fn test_get_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(&pool, &user_id, "Findable Chat", None, None, None, false).await.unwrap();
        let fetched = get_conversation(&pool, &conv.id, &user_id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().title, "Findable Chat");
    }

    #[tokio::test]
    async fn test_update_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(&pool, &user_id, "Old Title", None, None, None, false).await.unwrap();
        let updated = update_conversation(
            &pool,
            &conv.id,
            &user_id,
            "New Title",
            Some("openai"),
            Some("gpt-4"),
            Some("You are helpful."),
            true,
        )
        .await.unwrap();
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.provider.as_deref(), Some("openai"));
        assert_eq!(updated.model_name.as_deref(), Some("gpt-4"));
        assert_eq!(updated.system_prompt_override.as_deref(), Some("You are helpful."));
        assert!(updated.deep_thinking);
    }

    #[tokio::test]
    async fn test_delete_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(&pool, &user_id, "To Delete", None, None, None, false).await.unwrap();
        let deleted = delete_conversation(&pool, &conv.id, &user_id).await.unwrap();
        assert!(deleted);
        let fetched = get_conversation(&pool, &conv.id, &user_id).await.unwrap();
        assert!(fetched.is_none());
        // Deleting again should return false
        let deleted_again = delete_conversation(&pool, &conv.id, &user_id).await.unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_get_other_users_conversation_returns_none() {
        let (pool, user_id) = setup().await;
        let other_user = create_user(&pool, "other", "other@example.com", "hash2").await.unwrap();
        let conv = create_conversation(&pool, &user_id, "Private Chat", None, None, None, false).await.unwrap();
        // The other user should not be able to see this conversation
        let fetched = get_conversation(&pool, &conv.id, &other_user.id).await.unwrap();
        assert!(fetched.is_none());
    }
}
