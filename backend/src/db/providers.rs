use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sqlx::prelude::FromRow;

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
    pub image_models: Option<String>,
}

#[allow(clippy::too_many_arguments)]
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
    image_models: Option<&str>,
) -> Result<UserProvider, sqlx::Error> {
    let actual_name = name.unwrap_or(provider);
    let actual_id = match id {
        Some(existing) => existing.to_string(),
        None => uuid::Uuid::new_v4().to_string(),
    };

    // If this provider should be default, clear default from all other providers for this user.
    if is_default {
        sqlx::query(
            "UPDATE user_providers SET is_default = 0 \
             WHERE user_id = ? AND id != ?",
        )
        .bind(user_id)
        .bind(&actual_id)
        .execute(pool)
        .await?;
    }

    sqlx::query_as::<_, UserProvider>(
        "INSERT INTO user_providers (id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, models, name, image_models) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
         user_id = excluded.user_id, \
         provider = excluded.provider, \
         api_key_encrypted = excluded.api_key_encrypted, \
         endpoint_url = excluded.endpoint_url, \
         model_name = excluded.model_name, \
         is_default = excluded.is_default, \
         models = excluded.models, \
         name = excluded.name, \
         image_models = excluded.image_models \
         WHERE user_providers.user_id = excluded.user_id \
         RETURNING id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, created_at, models, name, image_models",
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
    .bind(image_models)
    .fetch_one(pool)
    .await
}

pub async fn list_providers(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<UserProvider>, sqlx::Error> {
    sqlx::query_as::<_, UserProvider>(
        "SELECT id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, created_at, models, name, image_models \
         FROM user_providers WHERE user_id = ? \
         ORDER BY created_at ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn get_provider_by_id(
    pool: &SqlitePool,
    user_id: &str,
    id: &str,
) -> Result<Option<UserProvider>, sqlx::Error> {
    sqlx::query_as::<_, UserProvider>(
        "SELECT id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, created_at, models, name, image_models \
         FROM user_providers \
         WHERE user_id = ? AND id = ?",
    )
    .bind(user_id)
    .bind(id)
    .fetch_optional(pool)
    .await
}

#[allow(dead_code)]
pub async fn get_provider_by_name(
    pool: &SqlitePool,
    user_id: &str,
    name: &str,
) -> Result<Option<UserProvider>, sqlx::Error> {
    sqlx::query_as::<_, UserProvider>(
        "SELECT id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, created_at, models, name, image_models \
         FROM user_providers \
         WHERE user_id = ? AND name = ?\n         ORDER BY created_at DESC\n         LIMIT 1",
    )
    .bind(user_id)
    .bind(name)
    .fetch_optional(pool)
    .await
}

pub async fn get_default_provider(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Option<UserProvider>, sqlx::Error> {
    sqlx::query_as::<_, UserProvider>(
        "SELECT id, user_id, provider, api_key_encrypted, \
         endpoint_url, model_name, is_default, created_at, models, name, image_models \
         FROM user_providers \
         WHERE user_id = ? AND is_default = 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn delete_provider_by_id(
    pool: &SqlitePool,
    user_id: &str,
    id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM user_providers WHERE user_id = ? AND id = ?")
        .bind(user_id)
        .bind(id)
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
        let user = create_user(&pool, "testuser", "test@example.com", "hash")
            .await
            .unwrap();
        (pool, user.id)
    }

    #[tokio::test]
    async fn test_upsert_provider_create() {
        let (pool, user_id) = setup().await;
        let prov = upsert_provider(
            &pool,
            None,
            &user_id,
            "openai",
            "enc_key_1",
            Some("https://api.openai.com"),
            Some("gpt-4"),
            true,
            Some("[\"gpt-4\"]"),
            Some("My OpenAI"),
            None,
        )
        .await
        .unwrap();
        assert_eq!(prov.user_id, user_id);
        assert_eq!(prov.provider, "openai");
        assert_eq!(prov.name.as_deref(), Some("My OpenAI"));
        assert!(prov.is_default);
    }

    #[tokio::test]
    async fn test_upsert_provider_update_by_id_and_rename() {
        let (pool, user_id) = setup().await;
        let created = upsert_provider(
            &pool,
            None,
            &user_id,
            "openai",
            "old_key",
            None,
            None,
            false,
            None,
            Some("Old Name"),
            None,
        )
        .await
        .unwrap();

        let updated = upsert_provider(
            &pool,
            Some(&created.id),
            &user_id,
            "openai",
            "new_key",
            Some("https://new.endpoint"),
            Some("gpt-4o"),
            true,
            Some("[\"gpt-4o\"]"),
            Some("New Name"),
            None,
        )
        .await
        .unwrap();

        assert_eq!(updated.id, created.id);
        assert_eq!(updated.api_key_encrypted, "new_key");
        assert_eq!(updated.name.as_deref(), Some("New Name"));
        let all = list_providers(&pool, &user_id).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_duplicate_provider_names_allowed_for_same_user() {
        let (pool, user_id) = setup().await;
        let first = upsert_provider(
            &pool,
            None,
            &user_id,
            "openai",
            "k1",
            None,
            None,
            false,
            None,
            Some("Shared Name"),
            None,
        )
        .await
        .unwrap();
        let second = upsert_provider(
            &pool,
            None,
            &user_id,
            "anthropic",
            "k2",
            None,
            None,
            false,
            None,
            Some("Shared Name"),
            None,
        )
        .await
        .unwrap();

        assert_ne!(first.id, second.id);
        let all = list_providers(&pool, &user_id).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_get_provider_by_id() {
        let (pool, user_id) = setup().await;
        let created = upsert_provider(
            &pool,
            None,
            &user_id,
            "openai",
            "k1",
            None,
            None,
            false,
            None,
            Some("Work OpenAI"),
            None,
        )
        .await
        .unwrap();

        let fetched = get_provider_by_id(&pool, &user_id, &created.id)
            .await
            .unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().provider, "openai");
    }

    #[tokio::test]
    async fn test_get_default_provider() {
        let (pool, user_id) = setup().await;
        upsert_provider(
            &pool,
            None,
            &user_id,
            "openai",
            "k1",
            None,
            None,
            false,
            None,
            Some("openai"),
            None,
        )
        .await
        .unwrap();
        upsert_provider(
            &pool,
            None,
            &user_id,
            "anthropic",
            "k2",
            None,
            None,
            true,
            None,
            Some("anthropic"),
            None,
        )
        .await
        .unwrap();
        let default = get_default_provider(&pool, &user_id).await.unwrap();
        assert!(default.is_some());
        assert_eq!(default.unwrap().provider, "anthropic");
    }

    #[tokio::test]
    async fn test_delete_provider_by_id() {
        let (pool, user_id) = setup().await;
        let created = upsert_provider(
            &pool,
            None,
            &user_id,
            "openai",
            "k1",
            None,
            None,
            false,
            None,
            Some("My OpenAI"),
            None,
        )
        .await
        .unwrap();

        let deleted = delete_provider_by_id(&pool, &user_id, &created.id)
            .await
            .unwrap();
        assert!(deleted);
        let deleted_again = delete_provider_by_id(&pool, &user_id, &created.id)
            .await
            .unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_upsert_provider_update_image_models() {
        let (pool, user_id) = setup().await;
        let created = upsert_provider(
            &pool,
            None,
            &user_id,
            "google",
            "k1",
            None,
            None,
            false,
            None,
            Some("My Google"),
            None,
        )
        .await
        .unwrap();

        let updated = upsert_provider(
            &pool,
            Some(&created.id),
            &user_id,
            "google",
            "k1",
            None,
            None,
            false,
            None,
            Some("My Google"),
            Some("[\"gemini-img\"]"),
        )
        .await
        .unwrap();
        assert_eq!(updated.image_models.as_deref(), Some("[\"gemini-img\"]"));

        let cleared = upsert_provider(
            &pool,
            Some(&created.id),
            &user_id,
            "google",
            "k1",
            None,
            None,
            false,
            None,
            Some("My Google"),
            None,
        )
        .await
        .unwrap();
        assert!(cleared.image_models.is_none());
    }
}
