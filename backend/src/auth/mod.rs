pub mod middleware;
pub mod password;

use axum::http::{HeaderMap, header};
use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

pub const ACCESS_COOKIE_NAME: &str = "access_token";
pub const REFRESH_COOKIE_NAME: &str = "refresh_token";

/// Claims embedded in a user-facing JWT access token.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// User ID (subject).
    pub sub: String,
    /// Human-readable username.
    pub username: String,
    /// Whether the user has admin privileges.
    pub is_admin: bool,
    /// Expiry time as a UTC Unix timestamp.
    pub exp: usize,
    /// Issued-at time as a UTC Unix timestamp.
    pub iat: usize,
}

/// Claims embedded in a container-scoped JWT token.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerClaims {
    /// Conversation ID (subject).
    pub sub: String,
    /// The user who owns the conversation.
    pub user_id: String,
    /// Expiry time as a UTC Unix timestamp.
    pub exp: usize,
    /// Issued-at time as a UTC Unix timestamp.
    pub iat: usize,
}

/// Create an access token for a user with the given TTL in seconds.
pub fn create_access_token(
    user_id: &str,
    username: &str,
    is_admin: bool,
    secret: &str,
    ttl_secs: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id.to_owned(),
        username: username.to_owned(),
        is_admin,
        exp: now + ttl_secs as usize,
        iat: now,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Create a container token scoped to a single conversation with the given TTL in seconds.
pub fn create_container_token(
    conversation_id: &str,
    user_id: &str,
    secret: &str,
    ttl_secs: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now().timestamp() as usize;
    let claims = ContainerClaims {
        sub: conversation_id.to_owned(),
        user_id: user_id.to_owned(),
        exp: now + ttl_secs as usize,
        iat: now,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Verify and decode a user access token.
pub fn verify_access_token(
    token: &str,
    secret: &str,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

/// Verify and decode a container token.
pub fn verify_container_token(
    token: &str,
    secret: &str,
) -> Result<ContainerClaims, jsonwebtoken::errors::Error> {
    let token_data = decode::<ContainerClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

pub fn get_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let trimmed = part.trim();
        let (k, v) = trimmed.split_once('=')?;
        if k == name {
            return Some(v.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-jwt-secret-that-is-long-enough";

    #[test]
    fn access_token_round_trip() {
        let token = create_access_token("user-1", "alice", false, SECRET, 7200).unwrap();
        let claims = verify_access_token(&token, SECRET).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.username, "alice");
        assert!(!claims.is_admin);
    }

    #[test]
    fn admin_flag_preserved() {
        let token = create_access_token("user-2", "bob", true, SECRET, 7200).unwrap();
        let claims = verify_access_token(&token, SECRET).unwrap();
        assert!(claims.is_admin);
    }

    #[test]
    fn container_token_round_trip() {
        let token = create_container_token("conv-1", "user-1", SECRET, 3600).unwrap();
        let claims = verify_container_token(&token, SECRET).unwrap();
        assert_eq!(claims.sub, "conv-1");
        assert_eq!(claims.user_id, "user-1");
    }

    #[test]
    fn wrong_secret_fails() {
        let token = create_access_token("user-1", "alice", false, SECRET, 7200).unwrap();
        assert!(verify_access_token(&token, "wrong-secret").is_err());
    }
}
