//! Unified error type for the ops service.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("not found")]
    NotFound,

    #[error("invalid license key")]
    InvalidLicenseKey,

    #[error("invalid state transition: {0}")]
    InvalidTransition(&'static str),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("license is not active")]
    LicenseNotActive,

    #[error("license is already bound to another machine")]
    MachineMismatch,

    #[error("public key does not match the license binding")]
    PublicKeyMismatch,

    #[error("node RPC not configured")]
    NodeNotConfigured,

    #[error("node unreachable: {0}")]
    NodeUnreachable(String),

    #[error("node error: {0}")]
    NodeError(String),

    #[error("release public key not configured")]
    ReleaseKeyNotConfigured,

    #[error("invalid release signature")]
    InvalidSignature,

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::InvalidLicenseKey => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::InvalidTransition(_) => StatusCode::CONFLICT,
            AppError::InvalidInput(_) => StatusCode::BAD_REQUEST,
            AppError::LicenseNotActive => StatusCode::FORBIDDEN,
            AppError::MachineMismatch | AppError::PublicKeyMismatch => StatusCode::CONFLICT,
            AppError::NodeNotConfigured => StatusCode::SERVICE_UNAVAILABLE,
            AppError::NodeUnreachable(_) | AppError::NodeError(_) => StatusCode::BAD_GATEWAY,
            AppError::ReleaseKeyNotConfigured => StatusCode::SERVICE_UNAVAILABLE,
            AppError::InvalidSignature => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Database(_) | AppError::Serialization(_) | AppError::Migration(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    /// Whether the error message is safe to return to a client. Database errors
    /// are logged server-side and mapped to a generic message.
    fn public_message(&self) -> String {
        match self {
            AppError::Database(_) | AppError::Serialization(_) | AppError::Migration(_) => {
                "internal error".to_string()
            }
            other => other.to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let message = self.public_message();

        // Log full context for the operators without leaking it to the client.
        if let AppError::Database(e) = &self {
            tracing::error!(error = %e, "database failure");
        }
        if let AppError::NodeUnreachable(e) | AppError::NodeError(e) = &self {
            tracing::error!(error = %e, "node bridge failure");
        }

        (status, Json(json!({ "error": message }))).into_response()
    }
}
