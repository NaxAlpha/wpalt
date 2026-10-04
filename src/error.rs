use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub struct Error(pub StatusCode, pub &'static str);
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn invalid(message: &'static str) -> Self {
        Self(StatusCode::UNPROCESSABLE_ENTITY, message)
    }
    pub fn forbidden() -> Self {
        Self(StatusCode::FORBIDDEN, "This operation is not permitted.")
    }
    pub fn not_found() -> Self {
        Self(StatusCode::NOT_FOUND, "Not found.")
    }
    pub fn conflict() -> Self {
        Self(
            StatusCode::CONFLICT,
            "This item changed elsewhere. Reload before saving; your changes have not overwritten it.",
        )
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        if self.0 == StatusCode::FORBIDDEN || self.0 == StatusCode::UNAUTHORIZED {
            tracing::warn!(event = "access_denied", status = self.0.as_u16());
        }
        (self.0, axum::Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e {
            if db.message() == "shop_record_limit" {
                return Self::invalid(
                    "Commerce record limit reached. Increase the configured budget before retrying.",
                );
            }
            if db.message() == "member_record_limit" {
                return Self::invalid(
                    "Membership record limit reached. Increase the configured budget or remove records before retrying.",
                );
            }
            if db.is_unique_violation() {
                return Self(StatusCode::CONFLICT, "That identifier is already in use.");
            }
        }
        let code = match &e {
            sqlx::Error::Database(db) => db
                .code()
                .map(|c| c.into_owned())
                .unwrap_or_else(|| "database".into()),
            sqlx::Error::PoolTimedOut => "pool_timeout".into(),
            _ => "driver_error".into(),
        };
        tracing::error!(event = "database_operation_failed", error_code=%code);
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database operation failed. Consult the correlated server diagnostics.",
        )
    }
}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        tracing::error!(event = "storage_operation_failed");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Storage operation failed.",
        )
    }
}
