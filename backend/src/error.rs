use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found")]
    NotFound,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Payload too large: {0}")]
    PayloadTooLarge(String),

    #[error("Not implemented")]
    NotImplemented,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::PayloadTooLarge(_) => (StatusCode::PAYLOAD_TOO_LARGE, self.to_string()),
            AppError::NotImplemented => (StatusCode::NOT_IMPLEMENTED, self.to_string()),
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {msg}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AppError::Sqlx(e) => {
                tracing::error!("Database error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            }
        };
        (status, Json(serde_json::json!({ "message": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn extract_status_and_body(error: AppError) -> (StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    #[tokio::test]
    async fn not_found_returns_404() {
        let (status, body) = extract_status_and_body(AppError::NotFound).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], "Not found");
    }

    #[tokio::test]
    async fn bad_request_returns_400() {
        let (status, body) =
            extract_status_and_body(AppError::BadRequest("invalid input".into())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["message"].as_str().unwrap().contains("invalid input"));
    }

    #[tokio::test]
    async fn unauthorized_returns_401() {
        let (status, _) = extract_status_and_body(AppError::Unauthorized("bad token".into())).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn forbidden_returns_403() {
        let (status, _) = extract_status_and_body(AppError::Forbidden("denied".into())).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn conflict_returns_409() {
        let (status, _) = extract_status_and_body(AppError::Conflict("duplicate".into())).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn payload_too_large_returns_413() {
        let (status, body) =
            extract_status_and_body(AppError::PayloadTooLarge("archive exceeds limit".into()))
                .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(
            body["message"]
                .as_str()
                .unwrap()
                .contains("archive exceeds limit")
        );
    }

    #[tokio::test]
    async fn internal_returns_500_and_hides_details() {
        let (status, body) =
            extract_status_and_body(AppError::Internal("secret db path /var/lib/data".into()))
                .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        // Must NOT contain the internal error details
        let msg = body["message"].as_str().unwrap();
        assert!(!msg.contains("secret"));
        assert!(!msg.contains("/var/lib"));
        assert_eq!(msg, "Internal server error");
    }

    #[tokio::test]
    async fn not_implemented_returns_501() {
        let (status, _) = extract_status_and_body(AppError::NotImplemented).await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }
}
