use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn create_user(
    pool: &SqlitePool,
    username: &str,
    email: &str,
    password_hash: &str,
) -> User {
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query_as::<_, User>(
        "INSERT INTO users (id, username, email, password_hash)
         VALUES (?, ?, ?, ?)
         RETURNING id, username, email, password_hash, is_admin, created_at, updated_at",
    )
    .bind(&id)
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .fetch_one(pool)
    .await
    .expect("Failed to create user")
}

pub async fn get_user_by_id(pool: &SqlitePool, id: &str) -> Option<User> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, email, password_hash, is_admin, created_at, updated_at
         FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .expect("Failed to query user by id")
}

pub async fn get_user_by_username(pool: &SqlitePool, username: &str) -> Option<User> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, email, password_hash, is_admin, created_at, updated_at
         FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .expect("Failed to query user by username")
}

pub async fn get_user_by_email(pool: &SqlitePool, email: &str) -> Option<User> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, email, password_hash, is_admin, created_at, updated_at
         FROM users WHERE email = ?",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
    .expect("Failed to query user by email")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    async fn setup() -> SqlitePool {
        init_db("sqlite::memory:").await
    }

    #[tokio::test]
    async fn test_create_user() {
        let pool = setup().await;
        let user = create_user(&pool, "alice", "alice@example.com", "hash123").await;
        assert_eq!(user.username, "alice");
        assert_eq!(user.email, "alice@example.com");
        assert_eq!(user.password_hash, "hash123");
        assert!(!user.is_admin);
        assert!(!user.id.is_empty());
        assert!(!user.created_at.is_empty());
        assert!(!user.updated_at.is_empty());
    }

    #[tokio::test]
    async fn test_get_user_by_id() {
        let pool = setup().await;
        let user = create_user(&pool, "bob", "bob@example.com", "hash456").await;
        let fetched = get_user_by_id(&pool, &user.id).await;
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, user.id);
        assert_eq!(fetched.username, "bob");
        assert_eq!(fetched.email, "bob@example.com");
    }

    #[tokio::test]
    async fn test_get_user_by_username() {
        let pool = setup().await;
        create_user(&pool, "charlie", "charlie@example.com", "hash789").await;
        let fetched = get_user_by_username(&pool, "charlie").await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().username, "charlie");
    }

    #[tokio::test]
    async fn test_get_user_by_email() {
        let pool = setup().await;
        create_user(&pool, "diana", "diana@example.com", "hashabc").await;
        let fetched = get_user_by_email(&pool, "diana@example.com").await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().email, "diana@example.com");
    }

    #[tokio::test]
    async fn test_get_nonexistent_user_returns_none() {
        let pool = setup().await;
        assert!(get_user_by_id(&pool, "nonexistent-id").await.is_none());
        assert!(get_user_by_username(&pool, "nobody").await.is_none());
        assert!(get_user_by_email(&pool, "nobody@example.com").await.is_none());
    }
}
