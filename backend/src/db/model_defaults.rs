use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sqlx::prelude::FromRow;

use crate::db::providers::{self, UserProvider};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserModelDefaults {
    pub user_id: String,
    pub chat_provider_id: Option<String>,
    pub chat_model_name: Option<String>,
    pub subagent_provider_id: Option<String>,
    pub subagent_model_name: Option<String>,
    pub image_provider_id: Option<String>,
    pub image_model_name: Option<String>,
    pub updated_at: String,
}

pub async fn get_model_defaults(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Option<UserModelDefaults>, sqlx::Error> {
    sqlx::query_as::<_, UserModelDefaults>(
        "SELECT user_id, chat_provider_id, chat_model_name, \
         subagent_provider_id, subagent_model_name, \
         image_provider_id, image_model_name, updated_at \
         FROM user_model_defaults \
         WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_model_defaults(
    pool: &SqlitePool,
    user_id: &str,
    chat_provider_id: Option<&str>,
    chat_model_name: Option<&str>,
    subagent_provider_id: Option<&str>,
    subagent_model_name: Option<&str>,
    image_provider_id: Option<&str>,
    image_model_name: Option<&str>,
) -> Result<UserModelDefaults, sqlx::Error> {
    sqlx::query_as::<_, UserModelDefaults>(
        "INSERT INTO user_model_defaults (
            user_id,
            chat_provider_id,
            chat_model_name,
            subagent_provider_id,
            subagent_model_name,
            image_provider_id,
            image_model_name
         ) VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
            chat_provider_id = excluded.chat_provider_id,
            chat_model_name = excluded.chat_model_name,
            subagent_provider_id = excluded.subagent_provider_id,
            subagent_model_name = excluded.subagent_model_name,
            image_provider_id = excluded.image_provider_id,
            image_model_name = excluded.image_model_name,
            updated_at = datetime('now')
         RETURNING user_id, chat_provider_id, chat_model_name,
                   subagent_provider_id, subagent_model_name,
                   image_provider_id, image_model_name, updated_at",
    )
    .bind(user_id)
    .bind(chat_provider_id)
    .bind(chat_model_name)
    .bind(subagent_provider_id)
    .bind(subagent_model_name)
    .bind(image_provider_id)
    .bind(image_model_name)
    .fetch_one(pool)
    .await
}

pub async fn get_or_init_model_defaults(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<UserModelDefaults, sqlx::Error> {
    if let Some(existing) = get_model_defaults(pool, user_id).await? {
        return Ok(existing);
    }

    let legacy_default = providers::get_default_provider(pool, user_id).await?;
    let provider_id = legacy_default.as_ref().map(|p| p.id.clone());
    let model_name = legacy_default.and_then(|p| p.model_name);

    upsert_model_defaults(
        pool,
        user_id,
        provider_id.as_deref(),
        model_name.as_deref(),
        provider_id.as_deref(),
        model_name.as_deref(),
        None,
        None,
    )
    .await
}

pub async fn clear_provider_references(
    pool: &SqlitePool,
    user_id: &str,
    provider_id: &str,
) -> Result<UserModelDefaults, sqlx::Error> {
    let current = get_or_init_model_defaults(pool, user_id).await?;

    let mut next = current.clone();
    let mut changed = false;

    if next.chat_provider_id.as_deref() == Some(provider_id) {
        next.chat_provider_id = None;
        next.chat_model_name = None;
        changed = true;
    }
    if next.subagent_provider_id.as_deref() == Some(provider_id) {
        next.subagent_provider_id = None;
        next.subagent_model_name = None;
        changed = true;
    }
    if next.image_provider_id.as_deref() == Some(provider_id) {
        next.image_provider_id = None;
        next.image_model_name = None;
        changed = true;
    }

    if !changed {
        return Ok(current);
    }

    let updated = upsert_model_defaults(
        pool,
        user_id,
        next.chat_provider_id.as_deref(),
        next.chat_model_name.as_deref(),
        next.subagent_provider_id.as_deref(),
        next.subagent_model_name.as_deref(),
        next.image_provider_id.as_deref(),
        next.image_model_name.as_deref(),
    )
    .await?;
    Ok(updated)
}

pub async fn prune_invalid_provider_references(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<UserModelDefaults, sqlx::Error> {
    let current = get_or_init_model_defaults(pool, user_id).await?;
    let providers = providers::list_providers(pool, user_id).await?;
    let mut next = current.clone();

    prune_role(
        &providers,
        &mut next.chat_provider_id,
        &mut next.chat_model_name,
        false,
    );
    prune_role(
        &providers,
        &mut next.subagent_provider_id,
        &mut next.subagent_model_name,
        false,
    );
    prune_role(
        &providers,
        &mut next.image_provider_id,
        &mut next.image_model_name,
        true,
    );

    if current.chat_provider_id == next.chat_provider_id
        && current.chat_model_name == next.chat_model_name
        && current.subagent_provider_id == next.subagent_provider_id
        && current.subagent_model_name == next.subagent_model_name
        && current.image_provider_id == next.image_provider_id
        && current.image_model_name == next.image_model_name
    {
        return Ok(current);
    }

    let updated = upsert_model_defaults(
        pool,
        user_id,
        next.chat_provider_id.as_deref(),
        next.chat_model_name.as_deref(),
        next.subagent_provider_id.as_deref(),
        next.subagent_model_name.as_deref(),
        next.image_provider_id.as_deref(),
        next.image_model_name.as_deref(),
    )
    .await?;
    Ok(updated)
}

fn prune_role(
    providers: &[UserProvider],
    provider_id: &mut Option<String>,
    model_name: &mut Option<String>,
    use_image_models: bool,
) {
    let Some(current_provider_id) = provider_id.clone() else {
        *model_name = None;
        return;
    };
    let Some(current_model) = model_name.clone() else {
        *provider_id = None;
        return;
    };

    let Some(provider) = providers.iter().find(|p| p.id == current_provider_id) else {
        *provider_id = None;
        *model_name = None;
        return;
    };

    let available_models = if use_image_models {
        parse_models_json(provider.image_models.as_deref())
    } else {
        parse_models_json(provider.models.as_deref())
    };

    if !available_models.iter().any(|m| m == &current_model) {
        *provider_id = None;
        *model_name = None;
    }
}

fn parse_models_json(json: Option<&str>) -> Vec<String> {
    json.and_then(|v| serde_json::from_str::<Vec<String>>(v).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use crate::db::users::create_user;

    async fn setup() -> (SqlitePool, String) {
        let pool = init_db("sqlite::memory:").await;
        let user = create_user(&pool, "defaults-user", "defaults@example.com", "hash")
            .await
            .unwrap();
        (pool, user.id)
    }

    #[tokio::test]
    async fn upsert_and_get_defaults() {
        let (pool, user_id) = setup().await;
        let chat = providers::upsert_provider(
            &pool,
            None,
            &user_id,
            "openai",
            "k1",
            None,
            Some("gpt-4o"),
            false,
            Some("[\"gpt-4o\"]"),
            Some("Work OpenAI"),
            None,
        )
        .await
        .unwrap();
        let subagent = providers::upsert_provider(
            &pool,
            None,
            &user_id,
            "anthropic",
            "k2",
            None,
            Some("claude-sonnet"),
            false,
            Some("[\"claude-sonnet\"]"),
            Some("Work Anthropic"),
            None,
        )
        .await
        .unwrap();

        let saved = upsert_model_defaults(
            &pool,
            &user_id,
            Some(&chat.id),
            Some("gpt-4o"),
            Some(&subagent.id),
            Some("claude-sonnet"),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(saved.chat_provider_id.as_deref(), Some(chat.id.as_str()));

        let fetched = get_model_defaults(&pool, &user_id).await.unwrap().unwrap();
        assert_eq!(
            fetched.subagent_provider_id.as_deref(),
            Some(subagent.id.as_str())
        );
    }

    #[tokio::test]
    async fn clear_provider_refs_unsets_all_roles_for_deleted_provider() {
        let (pool, user_id) = setup().await;
        let shared = providers::upsert_provider(
            &pool,
            None,
            &user_id,
            "openai",
            "k1",
            None,
            Some("gpt-4o"),
            false,
            Some("[\"gpt-4o\",\"gpt-4o-mini\"]"),
            Some("Shared"),
            Some("[\"img-v1\"]"),
        )
        .await
        .unwrap();

        upsert_model_defaults(
            &pool,
            &user_id,
            Some(&shared.id),
            Some("gpt-4o"),
            Some(&shared.id),
            Some("gpt-4o-mini"),
            Some(&shared.id),
            Some("img-v1"),
        )
        .await
        .unwrap();

        let updated = clear_provider_references(&pool, &user_id, &shared.id)
            .await
            .unwrap();
        assert!(updated.chat_provider_id.is_none());
        assert!(updated.chat_model_name.is_none());
        assert!(updated.subagent_provider_id.is_none());
        assert!(updated.subagent_model_name.is_none());
        assert!(updated.image_provider_id.is_none());
        assert!(updated.image_model_name.is_none());
    }

    #[tokio::test]
    async fn prune_invalid_refs_clears_removed_models() {
        let (pool, user_id) = setup().await;
        let provider = providers::upsert_provider(
            &pool,
            None,
            &user_id,
            "openai",
            "k1",
            None,
            Some("gpt-4o"),
            false,
            Some("[\"gpt-4o\"]"),
            Some("Work OpenAI"),
            Some("[\"img-v1\"]"),
        )
        .await
        .unwrap();

        upsert_model_defaults(
            &pool,
            &user_id,
            Some(&provider.id),
            Some("gpt-4o"),
            Some(&provider.id),
            Some("gpt-4o"),
            Some(&provider.id),
            Some("img-v1"),
        )
        .await
        .unwrap();

        providers::upsert_provider(
            &pool,
            Some(&provider.id),
            &user_id,
            "openai",
            "k1",
            None,
            None,
            false,
            Some("[\"gpt-5\"]"),
            Some("Work OpenAI"),
            Some("[\"img-v2\"]"),
        )
        .await
        .unwrap();

        let updated = prune_invalid_provider_references(&pool, &user_id)
            .await
            .unwrap();
        assert!(updated.chat_provider_id.is_none());
        assert!(updated.chat_model_name.is_none());
        assert!(updated.subagent_provider_id.is_none());
        assert!(updated.subagent_model_name.is_none());
        assert!(updated.image_provider_id.is_none());
        assert!(updated.image_model_name.is_none());
    }
}
