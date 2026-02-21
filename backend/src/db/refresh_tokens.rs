use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RefreshToken {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub expires_at: String,
    pub created_at: String,
}

pub async fn create_refresh_token(
    pool: &SqlitePool,
    user_id: &str,
    token_hash: &str,
    expires_at: &str,
) -> Result<RefreshToken, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query_as::<_, RefreshToken>(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at) \
         VALUES (?, ?, ?, ?) \
         RETURNING id, user_id, token_hash, expires_at, created_at",
    )
    .bind(&id)
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .fetch_one(pool)
    .await
}

pub async fn create_refresh_token_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    user_id: &str,
    token_hash: &str,
    expires_at: &str,
) -> Result<RefreshToken, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query_as::<_, RefreshToken>(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at) \
         VALUES (?, ?, ?, ?) \
         RETURNING id, user_id, token_hash, expires_at, created_at",
    )
    .bind(&id)
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .fetch_one(&mut **tx)
    .await
}

#[cfg(test)]
pub async fn get_refresh_token_by_hash(
    pool: &SqlitePool,
    token_hash: &str,
) -> Result<Option<RefreshToken>, sqlx::Error> {
    sqlx::query_as::<_, RefreshToken>(
        "SELECT id, user_id, token_hash, expires_at, created_at \
         FROM refresh_tokens WHERE token_hash = ?",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
pub async fn delete_refresh_token(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM refresh_tokens WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
pub async fn delete_user_refresh_tokens(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn delete_refresh_token_by_hash(
    pool: &SqlitePool,
    token_hash: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM refresh_tokens WHERE token_hash = ?")
        .bind(token_hash)
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
    async fn test_create_refresh_token() {
        let (pool, user_id) = setup().await;
        let token = create_refresh_token(&pool, &user_id, "hash_abc", "2099-12-31T23:59:59")
            .await
            .unwrap();
        assert_eq!(token.user_id, user_id);
        assert_eq!(token.token_hash, "hash_abc");
        assert_eq!(token.expires_at, "2099-12-31T23:59:59");
        assert!(!token.id.is_empty());
        assert!(!token.created_at.is_empty());
    }

    #[tokio::test]
    async fn test_get_refresh_token_by_hash() {
        let (pool, user_id) = setup().await;
        create_refresh_token(&pool, &user_id, "hash_xyz", "2099-12-31T23:59:59")
            .await
            .unwrap();
        let fetched = get_refresh_token_by_hash(&pool, "hash_xyz").await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().token_hash, "hash_xyz");
        // Non-existent hash
        let missing = get_refresh_token_by_hash(&pool, "no_such_hash")
            .await
            .unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_delete_refresh_token() {
        let (pool, user_id) = setup().await;
        let token = create_refresh_token(&pool, &user_id, "hash_del", "2099-12-31T23:59:59")
            .await
            .unwrap();
        let deleted = delete_refresh_token(&pool, &token.id).await.unwrap();
        assert!(deleted);
        // Should be gone
        let fetched = get_refresh_token_by_hash(&pool, "hash_del").await.unwrap();
        assert!(fetched.is_none());
        // Deleting again should return false
        let deleted_again = delete_refresh_token(&pool, &token.id).await.unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_delete_refresh_token_by_hash() {
        let (pool, user_id) = setup().await;
        create_refresh_token(&pool, &user_id, "hash_bh", "2099-12-31T23:59:59")
            .await
            .unwrap();
        let deleted = delete_refresh_token_by_hash(&pool, "hash_bh")
            .await
            .unwrap();
        assert!(deleted);
        let fetched = get_refresh_token_by_hash(&pool, "hash_bh").await.unwrap();
        assert!(fetched.is_none());
        // Deleting again should return false
        let deleted_again = delete_refresh_token_by_hash(&pool, "hash_bh")
            .await
            .unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_delete_user_refresh_tokens() {
        let (pool, user_id) = setup().await;
        create_refresh_token(&pool, &user_id, "hash_1", "2099-12-31T23:59:59")
            .await
            .unwrap();
        create_refresh_token(&pool, &user_id, "hash_2", "2099-12-31T23:59:59")
            .await
            .unwrap();
        // Create a token for a different user to ensure it is not deleted
        let other = create_user(&pool, "other", "other@example.com", "hash")
            .await
            .unwrap();
        create_refresh_token(&pool, &other.id, "hash_other", "2099-12-31T23:59:59")
            .await
            .unwrap();

        delete_user_refresh_tokens(&pool, &user_id).await.unwrap();

        assert!(
            get_refresh_token_by_hash(&pool, "hash_1")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            get_refresh_token_by_hash(&pool, "hash_2")
                .await
                .unwrap()
                .is_none()
        );
        // Other user's token should still exist
        assert!(
            get_refresh_token_by_hash(&pool, "hash_other")
                .await
                .unwrap()
                .is_some()
        );
    }
}
