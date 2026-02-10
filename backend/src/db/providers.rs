use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserProvider {
    pub id: String,
    pub user_id: String,
    pub provider: String,
    pub api_key_encrypted: String,
    pub endpoint_url: Option<String>,
    pub model_name: Option<String>,
    pub is_default: bool,
    pub created_at: String,
    pub models: Option<String>,
    pub name: Option<String>,
}

pub async fn upsert_provider(
    pool: &SqlitePool,
    id: Option<&str>,
    user_id: &str,
    provider: &str,
    api_key_encrypted: &str,
    endpoint_url: Option<&str>,
    model_name: Option<&str>,
    is_default: bool,
    models: Option<&str>,
    name: Option<&str>,
) -> UserProvider {
    let actual_name = name.unwrap_or(provider);
    let actual_id = match id {
        Some(existing) => existing.to_string(),
        None => uuid::Uuid::new_v4().to_string(),
    };

    // If this provider should be the default, clear default
    // flag on all other providers for this user first.
    if is_default {
        sqlx::query(
            "UPDATE user_providers SET is_default = 0 \
             WHERE user_id = ? AND name != ?",
        )
        .bind(user_id)
        .bind(actual_name)
        .execute(pool)
        .await
        .expect("Failed to clear default providers");
    }

    sqlx::query_as::<_, UserProvider>(
        "INSERT INTO user_providers (id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, models, name) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(user_id, name) DO UPDATE SET \
         provider = excluded.provider, \
         api_key_encrypted = excluded.api_key_encrypted, \
         endpoint_url = excluded.endpoint_url, \
         model_name = excluded.model_name, \
         is_default = excluded.is_default, \
         models = excluded.models \
         RETURNING id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, created_at, models, name",
    )
    .bind(&actual_id)
    .bind(user_id)
    .bind(provider)
    .bind(api_key_encrypted)
    .bind(endpoint_url)
    .bind(model_name)
    .bind(is_default)
    .bind(models)
    .bind(actual_name)
    .fetch_one(pool)
    .await
    .expect("Failed to upsert provider")
}

pub async fn list_providers(
    pool: &SqlitePool,
    user_id: &str,
) -> Vec<UserProvider> {
    sqlx::query_as::<_, UserProvider>(
        "SELECT id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, created_at, models, name \
         FROM user_providers WHERE user_id = ? \
         ORDER BY created_at ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .expect("Failed to list providers")
}

pub async fn get_provider(
    pool: &SqlitePool,
    user_id: &str,
    provider: &str,
) -> Option<UserProvider> {
    sqlx::query_as::<_, UserProvider>(
        "SELECT id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, created_at, models, name \
         FROM user_providers \
         WHERE user_id = ? AND provider = ?",
    )
    .bind(user_id)
    .bind(provider)
    .fetch_optional(pool)
    .await
    .expect("Failed to get provider")
}

pub async fn get_default_provider(
    pool: &SqlitePool,
    user_id: &str,
) -> Option<UserProvider> {
    sqlx::query_as::<_, UserProvider>(
        "SELECT id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, created_at, models, name \
         FROM user_providers \
         WHERE user_id = ? AND is_default = 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .expect("Failed to get default provider")
}

pub async fn delete_provider(
    pool: &SqlitePool,
    user_id: &str,
    provider: &str,
) -> bool {
    let result = sqlx::query(
        "DELETE FROM user_providers WHERE user_id = ? AND provider = ?",
    )
    .bind(user_id)
    .bind(provider)
    .execute(pool)
    .await
    .expect("Failed to delete provider");

    result.rows_affected() > 0
}

pub async fn delete_provider_by_name(
    pool: &SqlitePool,
    user_id: &str,
    name: &str,
) -> bool {
    let result = sqlx::query(
        "DELETE FROM user_providers WHERE user_id = ? AND name = ?",
    )
    .bind(user_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("Failed to delete provider by name");

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
    async fn test_upsert_provider_create() {
        let (pool, user_id) = setup().await;
        let prov = upsert_provider(
            &pool, None, &user_id, "openai", "enc_key_1",
            Some("https://api.openai.com"), Some("gpt-4"), true, Some("[\"gpt-4\"]"), Some("My OpenAI"),
        )
        .await;
        assert_eq!(prov.user_id, user_id);
        assert_eq!(prov.provider, "openai");
        assert_eq!(prov.api_key_encrypted, "enc_key_1");
        assert_eq!(prov.endpoint_url.as_deref(), Some("https://api.openai.com"));
        assert_eq!(prov.model_name.as_deref(), Some("gpt-4"));
        assert!(prov.is_default);
        assert_eq!(prov.name.as_deref(), Some("My OpenAI"));
    }

    #[tokio::test]
    async fn test_upsert_provider_update() {
        let (pool, user_id) = setup().await;
        upsert_provider(
            &pool, None, &user_id, "openai", "old_key",
            None, None, false, None, Some("My OpenAI"),
        )
        .await;
        let updated = upsert_provider(
            &pool, None, &user_id, "openai", "new_key",
            Some("https://new.endpoint"), Some("gpt-4o"), true, Some("[\"gpt-4o\"]"), Some("My OpenAI"),
        )
        .await;
        assert_eq!(updated.api_key_encrypted, "new_key");
        assert_eq!(updated.endpoint_url.as_deref(), Some("https://new.endpoint"));
        assert_eq!(updated.model_name.as_deref(), Some("gpt-4o"));
        assert!(updated.is_default);
        let all = list_providers(&pool, &user_id).await;
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_list_providers() {
        let (pool, user_id) = setup().await;
        upsert_provider(&pool, None, &user_id, "openai", "k1", None, None, false, None, Some("openai")).await;
        upsert_provider(&pool, None, &user_id, "anthropic", "k2", None, None, false, None, Some("anthropic")).await;
        let all = list_providers(&pool, &user_id).await;
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_get_provider() {
        let (pool, user_id) = setup().await;
        upsert_provider(&pool, None, &user_id, "anthropic", "k1", None, None, false, None, Some("anthropic")).await;
        let fetched = get_provider(&pool, &user_id, "anthropic").await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().provider, "anthropic");
        let missing = get_provider(&pool, &user_id, "google").await;
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_get_default_provider() {
        let (pool, user_id) = setup().await;
        upsert_provider(&pool, None, &user_id, "openai", "k1", None, None, false, None, Some("openai")).await;
        upsert_provider(&pool, None, &user_id, "anthropic", "k2", None, None, true, None, Some("anthropic")).await;
        let default = get_default_provider(&pool, &user_id).await;
        assert!(default.is_some());
        assert_eq!(default.unwrap().provider, "anthropic");
    }

    #[tokio::test]
    async fn test_delete_provider() {
        let (pool, user_id) = setup().await;
        upsert_provider(&pool, None, &user_id, "openai", "k1", None, None, false, None, Some("openai")).await;
        let deleted = delete_provider(&pool, &user_id, "openai").await;
        assert!(deleted);
        let deleted_again = delete_provider(&pool, &user_id, "openai").await;
        assert!(!deleted_again);
    }
}
