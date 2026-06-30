use crate::error::{AppError, AppResult};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const SERVICE: &str = "com.garden12138.worklog";

#[derive(Clone, Default)]
pub struct SecretStore {
    fallback: Arc<RwLock<HashMap<String, String>>>,
    keyring_enabled: bool,
}

impl SecretStore {
    pub fn system() -> Self {
        Self {
            fallback: Arc::new(RwLock::new(HashMap::new())),
            keyring_enabled: true,
        }
    }

    #[cfg(test)]
    pub fn memory() -> Self {
        Self::default()
    }

    pub fn set(&self, key: &str, value: &str) -> AppResult<()> {
        if value.is_empty() {
            return Ok(());
        }
        if self.keyring_enabled {
            let entry = keyring::Entry::new(SERVICE, key)
                .map_err(|error| AppError::new("credential_error", error.to_string()))?;
            entry
                .set_password(value)
                .map_err(|error| AppError::new("credential_error", error.to_string()))?;
        }
        self.fallback
            .write()
            .map_err(|_| AppError::new("credential_error", "Credential store lock was poisoned"))?
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if self.keyring_enabled {
            if let Ok(entry) = keyring::Entry::new(SERVICE, key) {
                if let Ok(value) = entry.get_password() {
                    return Some(value);
                }
            }
        }
        self.fallback.read().ok()?.get(key).cloned()
    }

    pub fn delete(&self, key: &str) {
        if self.keyring_enabled {
            if let Ok(entry) = keyring::Entry::new(SERVICE, key) {
                let _ = entry.delete_credential();
            }
        }
        if let Ok(mut fallback) = self.fallback.write() {
            fallback.remove(key);
        }
    }

    pub async fn migrate_plaintext(&self, pool: &SqlitePool) -> AppResult<()> {
        let rows = sqlx::query(
            "SELECT id, api_key FROM llm_settings WHERE api_key IS NOT NULL AND api_key != ''",
        )
        .fetch_all(pool)
        .await?;
        for row in rows {
            let id: i64 = row.try_get("id")?;
            let value: String = row.try_get("api_key")?;
            self.set(&llm_secret_key(id), &value)?;
            sqlx::query("UPDATE llm_settings SET api_key='' WHERE id=?")
                .bind(id)
                .execute(pool)
                .await?;
        }

        let rows = sqlx::query("SELECT id, password FROM email_settings WHERE password != ''")
            .fetch_all(pool)
            .await?;
        for row in rows {
            let id: i64 = row.try_get("id")?;
            let value: String = row.try_get("password")?;
            self.set(&email_secret_key(id), &value)?;
            sqlx::query("UPDATE email_settings SET password='' WHERE id=?")
                .bind(id)
                .execute(pool)
                .await?;
        }
        Ok(())
    }
}

pub fn llm_secret_key(id: i64) -> String {
    format!("llm:{id}")
}

pub fn email_secret_key(id: i64) -> String {
    format!("smtp:{id}")
}

pub fn mask_secret(value: Option<&str>) -> Option<String> {
    let value = value.filter(|value| !value.is_empty())?;
    if value.len() <= 8 {
        Some("********".into())
    } else {
        let start: String = value.chars().take(4).collect();
        let end: String = value
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        Some(format!("{start}...{end}"))
    }
}

pub fn is_masked(value: &str) -> bool {
    value == "********" || (value.len() >= 11 && value.contains("..."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_and_round_trips_memory_secrets() {
        let store = SecretStore::memory();
        store.set("llm:1", "sk-1234567890").unwrap();
        assert_eq!(store.get("llm:1").as_deref(), Some("sk-1234567890"));
        assert_eq!(
            mask_secret(store.get("llm:1").as_deref()).as_deref(),
            Some("sk-1...7890")
        );
        assert!(is_masked("sk-1...7890"));
    }
}
