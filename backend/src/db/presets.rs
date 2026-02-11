use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserPreset {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: String,
    pub content: String,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_presets(pool: &SqlitePool, user_id: &str) -> Vec<UserPreset> {
    sqlx::query_as::<_, UserPreset>(
        "SELECT id, user_id, name, description, content, is_default, \
         created_at, updated_at FROM user_presets \
         WHERE user_id = ? ORDER BY created_at ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .expect("Failed to list presets")
}

pub async fn count_presets(pool: &SqlitePool, user_id: &str) -> i64 {
    sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM user_presets WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0) as i64
}

pub async fn create_preset(
    pool: &SqlitePool,
    user_id: &str,
    name: &str,
    description: &str,
    content: &str,
    is_default: bool,
) -> UserPreset {
    let id = uuid::Uuid::new_v4().to_string();

    if is_default {
        sqlx::query("UPDATE user_presets SET is_default = 0 WHERE user_id = ?")
            .bind(user_id)
            .execute(pool)
            .await
            .expect("Failed to clear default presets");
    }

    sqlx::query_as::<_, UserPreset>(
        "INSERT INTO user_presets (id, user_id, name, description, content, is_default) \
         VALUES (?, ?, ?, ?, ?, ?) \
         RETURNING id, user_id, name, description, content, is_default, created_at, updated_at",
    )
    .bind(&id)
    .bind(user_id)
    .bind(name)
    .bind(description)
    .bind(content)
    .bind(is_default)
    .fetch_one(pool)
    .await
    .expect("Failed to create preset")
}

pub async fn update_preset(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
    name: Option<&str>,
    description: Option<&str>,
    content: Option<&str>,
    is_default: Option<bool>,
) -> Option<UserPreset> {
    let existing = sqlx::query_as::<_, UserPreset>(
        "SELECT id, user_id, name, description, content, is_default, \
         created_at, updated_at FROM user_presets WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .expect("Failed to fetch preset");

    let existing = existing?;
    let new_name = name.unwrap_or(&existing.name);
    let new_desc = description.unwrap_or(&existing.description);
    let new_content = content.unwrap_or(&existing.content);
    let new_default = is_default.unwrap_or(existing.is_default);

    if new_default {
        sqlx::query("UPDATE user_presets SET is_default = 0 WHERE user_id = ? AND id != ?")
            .bind(user_id)
            .bind(id)
            .execute(pool)
            .await
            .expect("Failed to clear default presets");
    }

    sqlx::query_as::<_, UserPreset>(
        "UPDATE user_presets SET name = ?, description = ?, content = ?, \
         is_default = ?, updated_at = datetime('now') \
         WHERE id = ? AND user_id = ? \
         RETURNING id, user_id, name, description, content, is_default, created_at, updated_at",
    )
    .bind(new_name)
    .bind(new_desc)
    .bind(new_content)
    .bind(new_default)
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .expect("Failed to update preset")
}

pub async fn delete_preset(pool: &SqlitePool, id: &str, user_id: &str) -> bool {
    let result = sqlx::query("DELETE FROM user_presets WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to delete preset");
    result.rows_affected() > 0
}

pub async fn seed_builtin_presets(pool: &SqlitePool, user_id: &str) {
    if count_presets(pool, user_id).await > 0 {
        return;
    }
    let builtins = crate::prompts::builtin_presets();
    for preset in &builtins {
        let is_default = preset.id == "default";
        create_preset(pool, user_id, preset.name, preset.description, preset.content, is_default)
            .await;
    }
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
    async fn test_create_and_list_presets() {
        let (pool, uid) = setup().await;
        create_preset(&pool, &uid, "Test", "desc", "content", false).await;
        let all = list_presets(&pool, &uid).await;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Test");
    }

    #[tokio::test]
    async fn test_default_flag_clears_others() {
        let (pool, uid) = setup().await;
        create_preset(&pool, &uid, "A", "", "", true).await;
        create_preset(&pool, &uid, "B", "", "", true).await;
        let all = list_presets(&pool, &uid).await;
        let defaults: Vec<_> = all.iter().filter(|p| p.is_default).collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "B");
    }

    #[tokio::test]
    async fn test_update_preset() {
        let (pool, uid) = setup().await;
        let p = create_preset(&pool, &uid, "Old", "old desc", "old", false).await;
        let updated = update_preset(&pool, &p.id, &uid, Some("New"), None, Some("new"), None).await;
        assert!(updated.is_some());
        let u = updated.unwrap();
        assert_eq!(u.name, "New");
        assert_eq!(u.content, "new");
        assert_eq!(u.description, "old desc");
    }

    #[tokio::test]
    async fn test_delete_preset() {
        let (pool, uid) = setup().await;
        let p = create_preset(&pool, &uid, "Del", "", "", false).await;
        assert!(delete_preset(&pool, &p.id, &uid).await);
        assert!(!delete_preset(&pool, &p.id, &uid).await);
    }

    #[tokio::test]
    async fn test_seed_builtin_presets() {
        let (pool, uid) = setup().await;
        seed_builtin_presets(&pool, &uid).await;
        let all = list_presets(&pool, &uid).await;
        assert!(all.len() >= 3);
        let defaults: Vec<_> = all.iter().filter(|p| p.is_default).collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "Default");
        // Seeding again should be a no-op
        seed_builtin_presets(&pool, &uid).await;
        assert_eq!(list_presets(&pool, &uid).await.len(), all.len());
    }

    #[tokio::test]
    async fn test_count_presets() {
        let (pool, uid) = setup().await;
        assert_eq!(count_presets(&pool, &uid).await, 0);
        create_preset(&pool, &uid, "A", "", "", false).await;
        assert_eq!(count_presets(&pool, &uid).await, 1);
    }

    #[tokio::test]
    async fn test_update_preset_sets_default_clears_others() {
        let (pool, uid) = setup().await;
        let a = create_preset(&pool, &uid, "A", "", "", true).await;
        let b = create_preset(&pool, &uid, "B", "", "", false).await;
        assert!(a.is_default);
        assert!(!b.is_default);

        // Update B to be default — should clear A
        let updated_b = update_preset(&pool, &b.id, &uid, None, None, None, Some(true)).await.unwrap();
        assert!(updated_b.is_default);
        let all = list_presets(&pool, &uid).await;
        let a_now = all.iter().find(|p| p.name == "A").unwrap();
        assert!(!a_now.is_default);
    }

    #[tokio::test]
    async fn test_update_nonexistent_preset_returns_none() {
        let (pool, uid) = setup().await;
        let result = update_preset(&pool, "nonexistent-id", &uid, Some("X"), None, None, None).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_other_users_preset_returns_false() {
        let (pool, uid) = setup().await;
        let other = crate::db::users::create_user(&pool, "other", "other@example.com", "hash2").await;
        let p = create_preset(&pool, &uid, "Mine", "", "", false).await;
        // Other user cannot delete this preset
        assert!(!delete_preset(&pool, &p.id, &other.id).await);
        // Original user can
        assert!(delete_preset(&pool, &p.id, &uid).await);
    }

    #[tokio::test]
    async fn test_preset_isolation_between_users() {
        let (pool, uid) = setup().await;
        let other = crate::db::users::create_user(&pool, "other", "other@example.com", "hash2").await;
        create_preset(&pool, &uid, "UserA Preset", "", "", false).await;
        create_preset(&pool, &other.id, "UserB Preset", "", "", false).await;
        let a_presets = list_presets(&pool, &uid).await;
        let b_presets = list_presets(&pool, &other.id).await;
        assert_eq!(a_presets.len(), 1);
        assert_eq!(b_presets.len(), 1);
        assert_eq!(a_presets[0].name, "UserA Preset");
        assert_eq!(b_presets[0].name, "UserB Preset");
    }

    #[tokio::test]
    async fn test_seed_does_not_overwrite_existing() {
        let (pool, uid) = setup().await;
        // Create a custom preset first
        create_preset(&pool, &uid, "Custom", "my desc", "my content", true).await;
        // Seeding should be a no-op since count > 0
        seed_builtin_presets(&pool, &uid).await;
        let all = list_presets(&pool, &uid).await;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Custom");
    }

    #[tokio::test]
    async fn test_update_preset_partial_fields() {
        let (pool, uid) = setup().await;
        let p = create_preset(&pool, &uid, "Original", "desc", "content", false).await;
        // Update only name, leave everything else
        let updated = update_preset(&pool, &p.id, &uid, Some("Renamed"), None, None, None).await.unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.description, "desc");
        assert_eq!(updated.content, "content");
        assert!(!updated.is_default);
    }
}
