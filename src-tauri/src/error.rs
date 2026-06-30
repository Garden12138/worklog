use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_errors: Option<BTreeMap<String, String>>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field_errors: None,
        }
    }

    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        let message = message.into();
        let mut errors = BTreeMap::new();
        errors.insert(field.into(), message.clone());
        Self {
            code: "validation_error".into(),
            message,
            field_errors: Some(errors),
        }
    }

    pub fn not_found(entity: &str) -> Self {
        Self::new("not_found", format!("{entity} not found"))
    }
}

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self {
        let message = value.to_string();
        if message.contains("UNIQUE constraint failed") {
            return Self::new("conflict", "A record with the same value already exists");
        }
        Self::new("database_error", message)
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::new("io_error", value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::new("invalid_json", value.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
