use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sqlx::prelude::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Conversation {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub subagent_provider: Option<String>,
    pub subagent_model: Option<String>,
    pub system_prompt_override: Option<String>,
    pub deep_thinking: bool,
    pub created_at: String,
    pub updated_at: String,
    pub image_provider: Option<String>,
    pub image_model: Option<String>,
    pub share_token: Option<String>,
    pub thinking_budget: Option<i64>,
    pub subagent_thinking_budget: Option<i64>,
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub async fn create_conversation(
    pool: &SqlitePool,
    user_id: &str,
    title: &str,
    system_prompt_override: Option<&str>,
    provider: Option<&str>,
    model_name: Option<&str>,
    deep_thinking: bool,
    image_provider: Option<&str>,
    image_model: Option<&str>,
    thinking_budget: Option<i64>,
) -> Result<Conversation, sqlx::Error> {
    create_conversation_with_subagent(
        pool,
        user_id,
        title,
        system_prompt_override,
        provider,
        model_name,
        provider,
        model_name,
        deep_thinking,
        image_provider,
        image_model,
        thinking_budget,
        thinking_budget,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_conversation_with_subagent(
    pool: &SqlitePool,
    user_id: &str,
    title: &str,
    system_prompt_override: Option<&str>,
    provider: Option<&str>,
    model_name: Option<&str>,
    subagent_provider: Option<&str>,
    subagent_model: Option<&str>,
    deep_thinking: bool,
    image_provider: Option<&str>,
    image_model: Option<&str>,
    thinking_budget: Option<i64>,
    subagent_thinking_budget: Option<i64>,
) -> Result<Conversation, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query_as::<_, Conversation>(
        "INSERT INTO conversations (id, user_id, title, system_prompt_override, provider, model_name, subagent_provider, subagent_model, deep_thinking, image_provider, image_model, thinking_budget, subagent_thinking_budget)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id, user_id, title, provider, model_name, subagent_provider, subagent_model,
                   system_prompt_override, deep_thinking, created_at, updated_at,
                   image_provider, image_model, share_token, thinking_budget, subagent_thinking_budget",
    )
    .bind(&id)
    .bind(user_id)
    .bind(title)
    .bind(system_prompt_override)
    .bind(provider)
    .bind(model_name)
    .bind(subagent_provider)
    .bind(subagent_model)
    .bind(deep_thinking)
    .bind(image_provider)
    .bind(image_model)
    .bind(thinking_budget)
    .bind(subagent_thinking_budget)
    .fetch_one(pool)
    .await
}

pub async fn list_conversations(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<Conversation>, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        "SELECT id, user_id, title, provider, model_name, subagent_provider, subagent_model,
                system_prompt_override, deep_thinking, created_at, updated_at,
                image_provider, image_model, share_token, thinking_budget, subagent_thinking_budget
         FROM conversations
         WHERE user_id = ?
         ORDER BY updated_at DESC, created_at DESC, id DESC",
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
        "SELECT id, user_id, title, provider, model_name, subagent_provider, subagent_model,
                system_prompt_override, deep_thinking, created_at, updated_at,
                image_provider, image_model, share_token, thinking_budget, subagent_thinking_budget
         FROM conversations
         WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub async fn update_conversation(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
    title: &str,
    provider: Option<&str>,
    model_name: Option<&str>,
    system_prompt_override: Option<&str>,
    deep_thinking: bool,
    image_provider: Option<&str>,
    image_model: Option<&str>,
    thinking_budget: Option<i64>,
) -> Result<Option<Conversation>, sqlx::Error> {
    update_conversation_with_subagent(
        pool,
        id,
        user_id,
        title,
        provider,
        model_name,
        provider,
        model_name,
        system_prompt_override,
        deep_thinking,
        image_provider,
        image_model,
        thinking_budget,
        thinking_budget,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn update_conversation_with_subagent(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
    title: &str,
    provider: Option<&str>,
    model_name: Option<&str>,
    subagent_provider: Option<&str>,
    subagent_model: Option<&str>,
    system_prompt_override: Option<&str>,
    deep_thinking: bool,
    image_provider: Option<&str>,
    image_model: Option<&str>,
    thinking_budget: Option<i64>,
    subagent_thinking_budget: Option<i64>,
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        "UPDATE conversations
         SET title = ?, provider = ?, model_name = ?,
             subagent_provider = ?, subagent_model = ?,
             system_prompt_override = ?, deep_thinking = ?,
             image_provider = ?, image_model = ?,
             thinking_budget = ?,
             subagent_thinking_budget = ?,
             updated_at = datetime('now')
         WHERE id = ? AND user_id = ?
         RETURNING id, user_id, title, provider, model_name, subagent_provider, subagent_model,
                   system_prompt_override, deep_thinking, created_at, updated_at,
                   image_provider, image_model, share_token, thinking_budget, subagent_thinking_budget",
    )
    .bind(title)
    .bind(provider)
    .bind(model_name)
    .bind(subagent_provider)
    .bind(subagent_model)
    .bind(system_prompt_override)
    .bind(deep_thinking)
    .bind(image_provider)
    .bind(image_model)
    .bind(thinking_budget)
    .bind(subagent_thinking_budget)
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn touch_conversation_activity(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE conversations
         SET updated_at = datetime('now')
         WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn delete_conversation(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM conversations WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn set_share_token(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
    share_token: &str,
) -> Result<Option<Conversation>, sqlx::Error> {
    // Only set if share_token is currently NULL to avoid race conditions
    sqlx::query_as::<_, Conversation>(
        "UPDATE conversations
         SET share_token = ?, updated_at = datetime('now')
         WHERE id = ? AND user_id = ? AND share_token IS NULL
         RETURNING id, user_id, title, provider, model_name, subagent_provider, subagent_model,
                   system_prompt_override, deep_thinking, created_at, updated_at,
                   image_provider, image_model, share_token, thinking_budget, subagent_thinking_budget",
    )
    .bind(share_token)
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn remove_share_token(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE conversations SET share_token = NULL, updated_at = datetime('now')
         WHERE id = ? AND user_id = ? AND share_token IS NOT NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_conversation_by_share_token(
    pool: &SqlitePool,
    share_token: &str,
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        "SELECT id, user_id, title, provider, model_name, subagent_provider, subagent_model,
                system_prompt_override, deep_thinking, created_at, updated_at,
                image_provider, image_model, share_token, thinking_budget, subagent_thinking_budget
         FROM conversations
         WHERE share_token = ?",
    )
    .bind(share_token)
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use crate::db::users::create_user;

    async fn setup() -> (SqlitePool, String) {
        let pool = init_db("sqlite::memory:").await;
        let user = create_user(&pool, "testuser", "test@example.com", "hash")
            .await
            .unwrap();
        (pool, user.id)
    }

    #[tokio::test]
    async fn test_create_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool, &user_id, "My Chat", None, None, None, false, None, None, None,
        )
        .await
        .unwrap();
        assert_eq!(conv.user_id, user_id);
        assert_eq!(conv.title, "My Chat");
        assert!(conv.provider.is_none());
        assert!(conv.model_name.is_none());
        assert!(conv.system_prompt_override.is_none());
        assert!(!conv.deep_thinking);
        assert!(!conv.id.is_empty());
        assert!(conv.image_provider.is_none());
        assert!(conv.image_model.is_none());
    }

    #[tokio::test]
    async fn test_create_conversation_with_system_prompt() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool,
            &user_id,
            "Prompted Chat",
            Some("You are a pirate."),
            None,
            None,
            false,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            conv.system_prompt_override.as_deref(),
            Some("You are a pirate.")
        );
    }

    #[tokio::test]
    async fn test_create_conversation_with_provider_and_model() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool,
            &user_id,
            "Provider Chat",
            None,
            Some("openai"),
            Some("gpt-4o"),
            false,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(conv.provider.as_deref(), Some("openai"));
        assert_eq!(conv.model_name.as_deref(), Some("gpt-4o"));
        assert!(conv.system_prompt_override.is_none());
    }

    #[tokio::test]
    async fn test_create_conversation_with_all_fields() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool,
            &user_id,
            "Full Chat",
            Some("Be helpful."),
            Some("anthropic"),
            Some("claude-3"),
            true,
            Some("My Google"),
            Some("gemini-3-pro-image-preview"),
            None,
        )
        .await
        .unwrap();
        assert_eq!(conv.title, "Full Chat");
        assert_eq!(conv.system_prompt_override.as_deref(), Some("Be helpful."));
        assert_eq!(conv.provider.as_deref(), Some("anthropic"));
        assert_eq!(conv.model_name.as_deref(), Some("claude-3"));
        assert!(conv.deep_thinking);
        assert_eq!(conv.image_provider.as_deref(), Some("My Google"));
        assert_eq!(
            conv.image_model.as_deref(),
            Some("gemini-3-pro-image-preview")
        );
    }

    #[tokio::test]
    async fn test_list_conversations() {
        let (pool, user_id) = setup().await;
        create_conversation(
            &pool, &user_id, "Chat 1", None, None, None, false, None, None, None,
        )
        .await
        .unwrap();
        create_conversation(
            &pool, &user_id, "Chat 2", None, None, None, false, None, None, None,
        )
        .await
        .unwrap();
        let convs = list_conversations(&pool, &user_id).await.unwrap();
        assert_eq!(convs.len(), 2);
    }

    #[tokio::test]
    async fn test_get_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool,
            &user_id,
            "Findable Chat",
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
        let fetched = get_conversation(&pool, &conv.id, &user_id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().title, "Findable Chat");
    }

    #[tokio::test]
    async fn test_update_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool,
            &user_id,
            "Old Title",
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
        let updated = update_conversation(
            &pool,
            &conv.id,
            &user_id,
            "New Title",
            Some("openai"),
            Some("gpt-4"),
            Some("You are helpful."),
            true,
            Some("My Google"),
            Some("gemini-img"),
            None,
        )
        .await
        .unwrap();
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.provider.as_deref(), Some("openai"));
        assert_eq!(updated.model_name.as_deref(), Some("gpt-4"));
        assert_eq!(
            updated.system_prompt_override.as_deref(),
            Some("You are helpful.")
        );
        assert!(updated.deep_thinking);
        assert_eq!(updated.image_provider.as_deref(), Some("My Google"));
        assert_eq!(updated.image_model.as_deref(), Some("gemini-img"));
    }

    #[tokio::test]
    async fn test_delete_conversation() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool,
            &user_id,
            "To Delete",
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
        let deleted = delete_conversation(&pool, &conv.id, &user_id)
            .await
            .unwrap();
        assert!(deleted);
        let fetched = get_conversation(&pool, &conv.id, &user_id).await.unwrap();
        assert!(fetched.is_none());
        // Deleting again should return false
        let deleted_again = delete_conversation(&pool, &conv.id, &user_id)
            .await
            .unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_get_other_users_conversation_returns_none() {
        let (pool, user_id) = setup().await;
        let other_user = create_user(&pool, "other", "other@example.com", "hash2")
            .await
            .unwrap();
        let conv = create_conversation(
            &pool,
            &user_id,
            "Private Chat",
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
        // The other user should not be able to see this conversation
        let fetched = get_conversation(&pool, &conv.id, &other_user.id)
            .await
            .unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_set_share_token() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool,
            &user_id,
            "Shared Chat",
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
        assert!(conv.share_token.is_none());

        let updated = set_share_token(&pool, &conv.id, &user_id, "abc123")
            .await
            .unwrap();
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().share_token.as_deref(), Some("abc123"));
    }

    #[tokio::test]
    async fn test_remove_share_token() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool,
            &user_id,
            "Shared Chat",
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
        set_share_token(&pool, &conv.id, &user_id, "abc123")
            .await
            .unwrap();

        let removed = remove_share_token(&pool, &conv.id, &user_id).await.unwrap();
        assert!(removed);

        let fetched = get_conversation(&pool, &conv.id, &user_id)
            .await
            .unwrap()
            .unwrap();
        assert!(fetched.share_token.is_none());

        // Removing again should return false
        let removed_again = remove_share_token(&pool, &conv.id, &user_id).await.unwrap();
        assert!(!removed_again);
    }

    #[tokio::test]
    async fn test_get_by_share_token() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool,
            &user_id,
            "Shared Chat",
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
        set_share_token(&pool, &conv.id, &user_id, "token123")
            .await
            .unwrap();

        let fetched = get_conversation_by_share_token(&pool, "token123")
            .await
            .unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, conv.id);
    }

    #[tokio::test]
    async fn test_get_by_invalid_token() {
        let (pool, _user_id) = setup().await;
        let fetched = get_conversation_by_share_token(&pool, "nonexistent")
            .await
            .unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_set_share_token_wrong_user() {
        let (pool, user_id) = setup().await;
        let other_user = create_user(&pool, "other", "other@example.com", "hash2")
            .await
            .unwrap();
        let conv = create_conversation(
            &pool, &user_id, "My Chat", None, None, None, false, None, None, None,
        )
        .await
        .unwrap();

        let result = set_share_token(&pool, &conv.id, &other_user.id, "stolen")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_create_conversation_with_thinking_budget() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool,
            &user_id,
            "Budget Chat",
            None,
            None,
            None,
            true,
            None,
            None,
            Some(100000),
        )
        .await
        .unwrap();
        assert_eq!(conv.thinking_budget, Some(100000));
        assert_eq!(conv.subagent_thinking_budget, Some(100000));
        assert!(conv.deep_thinking);
    }

    #[tokio::test]
    async fn test_update_conversation_thinking_budget() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool, &user_id, "Chat", None, None, None, false, None, None, None,
        )
        .await
        .unwrap();
        assert!(conv.thinking_budget.is_none());

        let updated = update_conversation(
            &pool,
            &conv.id,
            &user_id,
            "Chat",
            None,
            None,
            None,
            true,
            None,
            None,
            Some(200000),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.thinking_budget, Some(200000));
        assert_eq!(updated.subagent_thinking_budget, Some(200000));
    }

    #[tokio::test]
    async fn test_touch_conversation_activity_updates_timestamp() {
        let (pool, user_id) = setup().await;
        let conv = create_conversation(
            &pool, &user_id, "Chat", None, None, None, false, None, None, None,
        )
        .await
        .unwrap();

        sqlx::query("UPDATE conversations SET updated_at = '2000-01-01 00:00:00' WHERE id = ?")
            .bind(&conv.id)
            .execute(&pool)
            .await
            .unwrap();

        let before = get_conversation(&pool, &conv.id, &user_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before.updated_at, "2000-01-01 00:00:00");

        let touched = touch_conversation_activity(&pool, &conv.id, &user_id)
            .await
            .unwrap();
        assert!(touched);

        let after = get_conversation(&pool, &conv.id, &user_id)
            .await
            .unwrap()
            .unwrap();
        assert!(after.updated_at > before.updated_at);
    }

    #[tokio::test]
    async fn test_list_conversations_orders_by_activity_after_touch() {
        let (pool, user_id) = setup().await;
        let conv1 = create_conversation(
            &pool, &user_id, "Chat 1", None, None, None, false, None, None, None,
        )
        .await
        .unwrap();
        let conv2 = create_conversation(
            &pool, &user_id, "Chat 2", None, None, None, false, None, None, None,
        )
        .await
        .unwrap();

        sqlx::query("UPDATE conversations SET updated_at = '2000-01-01 00:00:00' WHERE id = ?")
            .bind(&conv1.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE conversations SET updated_at = '2001-01-01 00:00:00' WHERE id = ?")
            .bind(&conv2.id)
            .execute(&pool)
            .await
            .unwrap();

        let touched = touch_conversation_activity(&pool, &conv1.id, &user_id)
            .await
            .unwrap();
        assert!(touched);

        let convs = list_conversations(&pool, &user_id).await.unwrap();
        assert_eq!(convs.len(), 2);
        assert_eq!(convs[0].id, conv1.id);
    }

    #[tokio::test]
    async fn test_list_conversations_uses_stable_tiebreakers() {
        let (pool, user_id) = setup().await;

        sqlx::query(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at)
             VALUES (?, ?, 'Chat A', ?, ?)",
        )
        .bind("aaa")
        .bind(&user_id)
        .bind("2000-01-01 00:00:00")
        .bind("2000-01-01 00:00:00")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at)
             VALUES (?, ?, 'Chat Z', ?, ?)",
        )
        .bind("zzz")
        .bind(&user_id)
        .bind("2000-01-01 00:00:00")
        .bind("2000-01-01 00:00:00")
        .execute(&pool)
        .await
        .unwrap();

        let convs = list_conversations(&pool, &user_id).await.unwrap();
        assert!(convs.len() >= 2);
        assert_eq!(convs[0].id, "zzz");
        assert_eq!(convs[1].id, "aaa");
    }

    #[tokio::test]
    async fn test_touch_conversation_activity_wrong_user_returns_false() {
        let (pool, user_id) = setup().await;
        let other_user = create_user(&pool, "other", "other@example.com", "hash2")
            .await
            .unwrap();
        let conv = create_conversation(
            &pool,
            &user_id,
            "Private Chat",
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

        let touched = touch_conversation_activity(&pool, &conv.id, &other_user.id)
            .await
            .unwrap();
        assert!(!touched);
    }
}
