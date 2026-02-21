use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserPreset {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: String,
    pub content: String,
    pub builtin_id: Option<String>,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_presets(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<UserPreset>, sqlx::Error> {
    sqlx::query_as::<_, UserPreset>(
        "SELECT id, user_id, name, description, content, builtin_id, is_default, \
         created_at, updated_at FROM user_presets \
         WHERE user_id = ? ORDER BY created_at ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
async fn count_presets(pool: &SqlitePool, user_id: &str) -> Result<i64, sqlx::Error> {
    Ok(
        sqlx::query_scalar::<_, i32>("SELECT COUNT(*) FROM user_presets WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await? as i64,
    )
}

pub async fn create_preset(
    pool: &SqlitePool,
    user_id: &str,
    name: &str,
    description: &str,
    content: &str,
    is_default: bool,
) -> Result<UserPreset, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();

    if is_default {
        sqlx::query("UPDATE user_presets SET is_default = 0 WHERE user_id = ?")
            .bind(user_id)
            .execute(pool)
            .await?;
    }

    sqlx::query_as::<_, UserPreset>(
        "INSERT INTO user_presets (id, user_id, name, description, content, builtin_id, is_default) \
         VALUES (?, ?, ?, ?, ?, NULL, ?) \
         RETURNING id, user_id, name, description, content, builtin_id, \
         is_default, created_at, updated_at",
    )
    .bind(&id)
    .bind(user_id)
    .bind(name)
    .bind(description)
    .bind(content)
    .bind(is_default)
    .fetch_one(pool)
    .await
}

pub async fn update_preset(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
    name: Option<&str>,
    description: Option<&str>,
    content: Option<&str>,
    is_default: Option<bool>,
) -> Result<Option<UserPreset>, sqlx::Error> {
    let existing = sqlx::query_as::<_, UserPreset>(
        "SELECT id, user_id, name, description, content, builtin_id, is_default, \
         created_at, updated_at FROM user_presets WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let existing = match existing {
        Some(e) => e,
        None => return Ok(None),
    };
    let new_name = name.unwrap_or(&existing.name);
    let new_desc = description.unwrap_or(&existing.description);
    let new_content = content.unwrap_or(&existing.content);
    let new_default = is_default.unwrap_or(existing.is_default);

    if new_default {
        sqlx::query("UPDATE user_presets SET is_default = 0 WHERE user_id = ? AND id != ?")
            .bind(user_id)
            .bind(id)
            .execute(pool)
            .await?;
    }

    sqlx::query_as::<_, UserPreset>(
        "UPDATE user_presets SET name = ?, description = ?, content = ?, \
         is_default = ?, updated_at = datetime('now') \
         WHERE id = ? AND user_id = ? \
         RETURNING id, user_id, name, description, content, builtin_id, \
         is_default, created_at, updated_at",
    )
    .bind(new_name)
    .bind(new_desc)
    .bind(new_content)
    .bind(new_default)
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn delete_preset(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM user_presets WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg_attr(not(test), allow(dead_code))]
pub async fn ensure_builtin_presets_for_user(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    ensure_builtin_presets_for_user_in_tx(&mut tx, user_id).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn ensure_builtin_presets_for_user_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    user_id: &str,
) -> Result<(), sqlx::Error> {
    let builtins = crate::prompts::builtin_presets();
    let mut has_default = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM user_presets WHERE user_id = ? AND is_default = 1",
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?
        > 0;

    for preset in &builtins {
        let is_default = preset.id == "default" && !has_default;
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO user_presets \
             (id, user_id, name, description, content, builtin_id, is_default) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(user_id)
        .bind(preset.name)
        .bind(preset.description)
        .bind(preset.content)
        .bind(preset.id)
        .bind(is_default)
        .execute(&mut **tx)
        .await?;

        if is_default && inserted.rows_affected() > 0 {
            has_default = true;
        }
    }

    if !has_default {
        sqlx::query(
            "UPDATE user_presets SET is_default = 1 \
             WHERE user_id = ? AND builtin_id = 'default' \
             AND NOT EXISTS (SELECT 1 FROM user_presets WHERE user_id = ? AND is_default = 1)",
        )
        .bind(user_id)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
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
    async fn test_create_and_list_presets() {
        let (pool, uid) = setup().await;
        create_preset(&pool, &uid, "Test", "desc", "content", false)
            .await
            .unwrap();
        let all = list_presets(&pool, &uid).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Test");
        assert!(all[0].builtin_id.is_none());
    }

    #[tokio::test]
    async fn test_default_flag_clears_others() {
        let (pool, uid) = setup().await;
        create_preset(&pool, &uid, "A", "", "", true).await.unwrap();
        create_preset(&pool, &uid, "B", "", "", true).await.unwrap();
        let all = list_presets(&pool, &uid).await.unwrap();
        let defaults: Vec<_> = all.iter().filter(|p| p.is_default).collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "B");
    }

    #[tokio::test]
    async fn test_update_preset() {
        let (pool, uid) = setup().await;
        let p = create_preset(&pool, &uid, "Old", "old desc", "old", false)
            .await
            .unwrap();
        let updated = update_preset(&pool, &p.id, &uid, Some("New"), None, Some("new"), None)
            .await
            .unwrap();
        assert!(updated.is_some());
        let u = updated.unwrap();
        assert_eq!(u.name, "New");
        assert_eq!(u.content, "new");
        assert_eq!(u.description, "old desc");
    }

    #[tokio::test]
    async fn test_delete_preset() {
        let (pool, uid) = setup().await;
        let p = create_preset(&pool, &uid, "Del", "", "", false)
            .await
            .unwrap();
        assert!(delete_preset(&pool, &p.id, &uid).await.unwrap());
        assert!(!delete_preset(&pool, &p.id, &uid).await.unwrap());
    }

    #[tokio::test]
    async fn test_ensure_builtin_presets_for_user() {
        let (pool, uid) = setup().await;
        ensure_builtin_presets_for_user(&pool, &uid).await.unwrap();
        let all = list_presets(&pool, &uid).await.unwrap();
        assert!(all.len() >= 4);
        assert!(all.iter().any(|p| p.name == "Claude Cowork"));
        assert!(
            all.iter()
                .any(|p| p.builtin_id.as_deref() == Some("default"))
        );
        assert!(
            all.iter()
                .any(|p| p.builtin_id.as_deref() == Some("claude-ai"))
        );
        assert!(
            all.iter()
                .any(|p| p.builtin_id.as_deref() == Some("claude-code"))
        );
        assert!(
            all.iter()
                .any(|p| p.builtin_id.as_deref() == Some("claude-cowork"))
        );
        let defaults: Vec<_> = all.iter().filter(|p| p.is_default).collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "Default");
        // Ensuring again should be a no-op
        ensure_builtin_presets_for_user(&pool, &uid).await.unwrap();
        assert_eq!(list_presets(&pool, &uid).await.unwrap().len(), all.len());
    }

    #[tokio::test]
    async fn test_count_presets() {
        let (pool, uid) = setup().await;
        assert_eq!(count_presets(&pool, &uid).await.unwrap(), 0);
        create_preset(&pool, &uid, "A", "", "", false)
            .await
            .unwrap();
        assert_eq!(count_presets(&pool, &uid).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_update_preset_sets_default_clears_others() {
        let (pool, uid) = setup().await;
        let a = create_preset(&pool, &uid, "A", "", "", true).await.unwrap();
        let b = create_preset(&pool, &uid, "B", "", "", false)
            .await
            .unwrap();
        assert!(a.is_default);
        assert!(!b.is_default);

        // Update B to be default — should clear A
        let updated_b = update_preset(&pool, &b.id, &uid, None, None, None, Some(true))
            .await
            .unwrap()
            .unwrap();
        assert!(updated_b.is_default);
        let all = list_presets(&pool, &uid).await.unwrap();
        let a_now = all.iter().find(|p| p.name == "A").unwrap();
        assert!(!a_now.is_default);
    }

    #[tokio::test]
    async fn test_update_nonexistent_preset_returns_none() {
        let (pool, uid) = setup().await;
        let result = update_preset(&pool, "nonexistent-id", &uid, Some("X"), None, None, None)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_other_users_preset_returns_false() {
        let (pool, uid) = setup().await;
        let other = crate::db::users::create_user(&pool, "other", "other@example.com", "hash2")
            .await
            .unwrap();
        let p = create_preset(&pool, &uid, "Mine", "", "", false)
            .await
            .unwrap();
        // Other user cannot delete this preset
        assert!(!delete_preset(&pool, &p.id, &other.id).await.unwrap());
        // Original user can
        assert!(delete_preset(&pool, &p.id, &uid).await.unwrap());
    }

    #[tokio::test]
    async fn test_preset_isolation_between_users() {
        let (pool, uid) = setup().await;
        let other = crate::db::users::create_user(&pool, "other", "other@example.com", "hash2")
            .await
            .unwrap();
        create_preset(&pool, &uid, "UserA Preset", "", "", false)
            .await
            .unwrap();
        create_preset(&pool, &other.id, "UserB Preset", "", "", false)
            .await
            .unwrap();
        let a_presets = list_presets(&pool, &uid).await.unwrap();
        let b_presets = list_presets(&pool, &other.id).await.unwrap();
        assert_eq!(a_presets.len(), 1);
        assert_eq!(b_presets.len(), 1);
        assert_eq!(a_presets[0].name, "UserA Preset");
        assert_eq!(b_presets[0].name, "UserB Preset");
    }

    #[tokio::test]
    async fn test_ensure_builtin_presets_does_not_overwrite_existing_default() {
        let (pool, uid) = setup().await;
        // Create a custom preset first
        create_preset(&pool, &uid, "Custom", "my desc", "my content", true)
            .await
            .unwrap();
        // Ensuring should preserve the custom default and add missing built-ins.
        ensure_builtin_presets_for_user(&pool, &uid).await.unwrap();
        let all = list_presets(&pool, &uid).await.unwrap();
        assert!(all.len() >= 5);
        let custom = all.iter().find(|p| p.name == "Custom").unwrap();
        assert!(custom.is_default);
        assert!(all.iter().any(|p| p.name == "Default"));
        assert!(all.iter().any(|p| p.name == "Claude AI"));
        assert!(all.iter().any(|p| p.name == "Claude Code"));
        assert!(all.iter().any(|p| p.name == "Claude Cowork"));

        let defaults: Vec<_> = all.iter().filter(|p| p.is_default).collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "Custom");
    }

    #[tokio::test]
    async fn test_ensure_builtin_presets_is_idempotent_by_builtin_id() {
        let (pool, uid) = setup().await;
        ensure_builtin_presets_for_user(&pool, &uid).await.unwrap();
        ensure_builtin_presets_for_user(&pool, &uid).await.unwrap();
        ensure_builtin_presets_for_user(&pool, &uid).await.unwrap();

        let builtin_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_presets WHERE user_id = ? AND builtin_id IS NOT NULL",
        )
        .bind(&uid)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(builtin_count, 4);
    }

    #[tokio::test]
    async fn test_update_preset_partial_fields() {
        let (pool, uid) = setup().await;
        let p = create_preset(&pool, &uid, "Original", "desc", "content", false)
            .await
            .unwrap();
        // Update only name, leave everything else
        let updated = update_preset(&pool, &p.id, &uid, Some("Renamed"), None, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.description, "desc");
        assert_eq!(updated.content, "content");
        assert!(!updated.is_default);
    }

    #[tokio::test]
    async fn test_duplicate_preset_names_allowed_per_user() {
        let (pool, uid) = setup().await;
        let first = create_preset(&pool, &uid, "Same Name", "v1", "content1", false)
            .await
            .unwrap();
        let second = create_preset(&pool, &uid, "Same Name", "v2", "content2", false)
            .await
            .unwrap();

        assert_ne!(first.id, second.id);
        let all = list_presets(&pool, &uid).await.unwrap();
        let same_name: Vec<_> = all.into_iter().filter(|p| p.name == "Same Name").collect();
        assert_eq!(same_name.len(), 2);
    }
}
