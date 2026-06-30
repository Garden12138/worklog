use crate::error::{AppError, AppResult};
use crate::models::MigrationResult;
use chrono::Utc;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Connection, Executor, Row, SqliteConnection, SqlitePool};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_WEEKLY: &str = "# {{ title }}\n\n周期：{{ period_start }} - {{ period_end }}\n\n{{ ai_content }}\n\n## 原始工作记录\n\n{{ work_items }}\n";
const DEFAULT_MONTHLY: &str = "# {{ title }}\n\n周期：{{ period_start }} - {{ period_end }}\n\n{{ ai_content }}\n\n## 关键成果\n\n{{ highlights }}\n\n## 原始工作记录\n\n{{ work_items }}\n";
const DEFAULT_PERFORMANCE: &str = "# {{ title }}\n\n考核周期：{{ period_start }} - {{ period_end }}\n\n{{ ai_content }}\n\n## 贡献证据\n\n{{ highlights }}\n\n## 可改进事项\n\n{{ blockers }}\n";

#[derive(Clone)]
pub struct Database {
    pub pool: SqlitePool,
    pub path: PathBuf,
    pub startup_migration: MigrationResult,
}

impl Database {
    pub async fn open(app_data_dir: PathBuf) -> AppResult<Self> {
        Self::open_inner(app_data_dir, None).await
    }

    #[cfg(test)]
    pub async fn open_for_test(
        app_data_dir: PathBuf,
        legacy_source: Option<PathBuf>,
    ) -> AppResult<Self> {
        Self::open_inner(app_data_dir, Some(legacy_source)).await
    }

    async fn open_inner(
        app_data_dir: PathBuf,
        legacy_override: Option<Option<PathBuf>>,
    ) -> AppResult<Self> {
        fs::create_dir_all(&app_data_dir)?;
        let path = app_data_dir.join("worklog.db");
        let mut migration = MigrationResult {
            imported: false,
            source_path: None,
            database_path: path.display().to_string(),
            message: "Using desktop database".into(),
        };

        if !path.exists() {
            let legacy_source = match legacy_override {
                Some(source) => source,
                None => detect_legacy_database(&path),
            };
            if let Some(source) = legacy_source {
                verify_integrity(&source).await?;
                copy_sqlite_database(&source, &path)?;
                let backup = app_data_dir.join("worklog-v1-import-backup.db");
                copy_sqlite_database(&source, &backup)?;
                migration.imported = true;
                migration.source_path = Some(source.display().to_string());
                migration.message = "Legacy database was copied and migrated".into();
            }
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(10));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|error| AppError::new("migration_error", error.to_string()))?;
        ensure_legacy_columns(&pool).await?;
        seed_defaults(&pool).await?;
        if migration.imported {
            let now = now_string();
            sqlx::query(
                "UPDATE desktop_preferences SET legacy_database_path=?, migrated_at=?, updated_at=? WHERE id=1",
            )
            .bind(migration.source_path.as_deref())
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await?;
        }
        verify_pool_integrity(&pool).await?;

        Ok(Self {
            pool,
            path,
            startup_migration: migration,
        })
    }

    pub async fn import_legacy(&self, source: &Path) -> AppResult<MigrationResult> {
        if source == self.path {
            return Err(AppError::validation(
                "path",
                "Source and destination database are identical",
            ));
        }
        if !source.is_file() {
            return Err(AppError::validation(
                "path",
                "Selected database file does not exist",
            ));
        }
        verify_integrity(source).await?;

        let user_rows: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM work_logs) + (SELECT COUNT(*) FROM reports) + \
             (SELECT COUNT(*) FROM recipients) + (SELECT COUNT(*) FROM llm_settings) + \
             (SELECT COUNT(*) FROM email_settings) + (SELECT COUNT(*) FROM report_email_deliveries) + \
             (SELECT COUNT(*) FROM generation_tasks)",
        )
        .fetch_one(&self.pool)
        .await?;
        if user_rows > 0 {
            return Err(AppError::new(
                "migration_conflict",
                "The desktop database already contains user data; import is only available before data entry",
            ));
        }

        let backup = self.path.with_extension("before-import.db");
        if self.path.exists() {
            copy_sqlite_database(&self.path, &backup)?;
        }

        let mut connection = self.pool.acquire().await?;
        sqlx::query("ATTACH DATABASE ? AS legacy")
            .bind(source.display().to_string())
            .execute(&mut *connection)
            .await?;
        let result = async {
            connection.execute("PRAGMA foreign_keys = OFF").await?;
            connection.execute("BEGIN IMMEDIATE").await?;
            connection.execute("DELETE FROM report_schedule_recipients").await?;
            connection.execute("DELETE FROM report_schedules").await?;
            connection.execute("DELETE FROM templates").await?;
            for statement in import_statements() {
                connection.execute(*statement).await?;
            }
            let now = now_string();
            sqlx::query(
                "INSERT INTO desktop_preferences(id, launch_at_login, legacy_database_path, migrated_at, created_at, updated_at) \
                 VALUES(1, 0, ?, ?, ?, ?) \
                 ON CONFLICT(id) DO UPDATE SET legacy_database_path=excluded.legacy_database_path, migrated_at=excluded.migrated_at, updated_at=excluded.updated_at",
            )
            .bind(source.display().to_string())
            .bind(&now)
            .bind(&now)
            .bind(&now)
            .execute(&mut *connection)
            .await?;
            connection.execute("COMMIT").await?;
            AppResult::<()>::Ok(())
        }
        .await;
        if result.is_err() {
            let _ = connection.execute("ROLLBACK").await;
        }
        let _ = connection.execute("DETACH DATABASE legacy").await;
        result?;
        seed_defaults(&self.pool).await?;
        verify_pool_integrity(&self.pool).await?;

        Ok(MigrationResult {
            imported: true,
            source_path: Some(source.display().to_string()),
            database_path: self.path.display().to_string(),
            message: "Legacy database imported successfully".into(),
        })
    }
}

fn import_statements() -> &'static [&'static str] {
    &[
        "INSERT INTO work_logs(id, work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, notes, created_at, updated_at) SELECT id, work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, notes, created_at, updated_at FROM legacy.work_logs",
        "INSERT INTO templates(id, name, template_type, content, is_default, created_at, updated_at) SELECT id, name, template_type, content, is_default, created_at, updated_at FROM legacy.templates",
        "INSERT INTO llm_settings(id, provider, base_url, model, api_key, extra_headers, timeout_seconds, is_active, created_at, updated_at) SELECT id, provider, base_url, model, api_key, extra_headers, timeout_seconds, is_active, created_at, updated_at FROM legacy.llm_settings",
        "INSERT INTO email_settings(id, host, port, security, username, password, sender_address, sender_name, is_active, created_at, updated_at) SELECT id, host, port, security, username, password, sender_address, sender_name, is_active, created_at, updated_at FROM legacy.email_settings",
        "INSERT INTO recipients(id, name, email, is_default, created_at, updated_at) SELECT id, name, email, is_default, created_at, updated_at FROM legacy.recipients",
        "INSERT INTO report_schedules(id, report_type, enabled, weekday, day_of_month, template_id, run_time, auto_send, created_at, updated_at) SELECT id, report_type, enabled, weekday, day_of_month, template_id, run_time, auto_send, created_at, updated_at FROM legacy.report_schedules",
        "INSERT INTO reports(id, report_type, title, period_start, period_end, template_id, content_markdown, status, source_log_ids, generated_at, edited_at, created_at, updated_at) SELECT id, report_type, title, period_start, period_end, template_id, content_markdown, status, source_log_ids, generated_at, edited_at, created_at, updated_at FROM legacy.reports",
        "INSERT INTO report_email_deliveries(id, report_id, subject, recipients_json, content_markdown, status, error_message, sent_at, created_at, updated_at) SELECT id, report_id, subject, recipients_json, content_markdown, status, error_message, sent_at, created_at, updated_at FROM legacy.report_email_deliveries",
        "INSERT INTO generation_tasks(id, report_type, period_start, period_end, status, message, report_id, completed_at, created_at, updated_at) SELECT id, report_type, period_start, period_end, status, message, report_id, completed_at, created_at, updated_at FROM legacy.generation_tasks",
        "INSERT INTO report_schedule_recipients(schedule_id, recipient_id) SELECT schedule_id, recipient_id FROM legacy.report_schedule_recipients",
    ]
}

async fn ensure_legacy_columns(pool: &SqlitePool) -> AppResult<()> {
    let work_log_columns = table_columns(pool, "work_logs").await?;
    if !work_log_columns.contains("start_date") {
        sqlx::query("ALTER TABLE work_logs ADD COLUMN start_date DATE")
            .execute(pool)
            .await?;
        sqlx::query("UPDATE work_logs SET start_date = work_date WHERE start_date IS NULL")
            .execute(pool)
            .await?;
    }
    if !work_log_columns.contains("end_date") {
        sqlx::query("ALTER TABLE work_logs ADD COLUMN end_date DATE")
            .execute(pool)
            .await?;
        sqlx::query("UPDATE work_logs SET end_date = work_date WHERE end_date IS NULL")
            .execute(pool)
            .await?;
    }
    let llm_columns = table_columns(pool, "llm_settings").await?;
    if !llm_columns.is_empty() && !llm_columns.contains("timeout_seconds") {
        sqlx::query(
            "ALTER TABLE llm_settings ADD COLUMN timeout_seconds INTEGER NOT NULL DEFAULT 60",
        )
        .execute(pool)
        .await?;
        sqlx::query("UPDATE llm_settings SET timeout_seconds = 180 WHERE provider = 'nvidia'")
            .execute(pool)
            .await?;
    }
    let schedule_columns = table_columns(pool, "report_schedules").await?;
    if !schedule_columns.is_empty() && !schedule_columns.contains("template_id") {
        sqlx::query("ALTER TABLE report_schedules ADD COLUMN template_id INTEGER")
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn table_columns(pool: &SqlitePool, table: &str) -> AppResult<HashSet<String>> {
    let query = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(&query).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .collect())
}

async fn seed_defaults(pool: &SqlitePool) -> AppResult<()> {
    let now = now_string();
    let templates = [
        ("weekly_report", "默认周报模板", DEFAULT_WEEKLY),
        ("monthly_report", "默认月报模板", DEFAULT_MONTHLY),
        (
            "performance_review",
            "默认绩效考核模板",
            DEFAULT_PERFORMANCE,
        ),
    ];
    for (kind, name, content) in templates {
        sqlx::query(
            "INSERT INTO templates(name, template_type, content, is_default, created_at, updated_at) \
             SELECT ?, ?, ?, 1, ?, ? WHERE NOT EXISTS(SELECT 1 FROM templates WHERE template_type=? AND is_default=1)",
        )
        .bind(name)
        .bind(kind)
        .bind(content)
        .bind(&now)
        .bind(&now)
        .bind(kind)
        .execute(pool)
        .await?;
    }

    let schedules = [
        ("weekly_report", Some("fri")),
        ("monthly_report", None),
        ("performance_review", None),
    ];
    for (kind, weekday) in schedules {
        sqlx::query(
            "INSERT INTO report_schedules(report_type, enabled, weekday, day_of_month, template_id, run_time, auto_send, created_at, updated_at) \
             SELECT ?, 1, ?, NULL, NULL, '15:00:00', 0, ?, ? WHERE NOT EXISTS(SELECT 1 FROM report_schedules WHERE report_type=?)",
        )
        .bind(kind)
        .bind(weekday)
        .bind(&now)
        .bind(&now)
        .bind(kind)
        .execute(pool)
        .await?;
    }
    sqlx::query(
        "INSERT INTO desktop_preferences(id, launch_at_login, created_at, updated_at) \
         VALUES(1, 0, ?, ?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

async fn verify_pool_integrity(pool: &SqlitePool) -> AppResult<()> {
    let result: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(pool)
        .await?;
    if result != "ok" {
        return Err(AppError::new(
            "database_integrity_error",
            format!("SQLite integrity check failed: {result}"),
        ));
    }
    Ok(())
}

async fn verify_integrity(path: &Path) -> AppResult<()> {
    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    let mut connection = SqliteConnection::connect_with(&options).await?;
    let result: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut connection)
        .await?;
    connection.close().await?;
    if result != "ok" {
        return Err(AppError::new(
            "database_integrity_error",
            format!("Legacy SQLite integrity check failed: {result}"),
        ));
    }
    Ok(())
}

fn detect_legacy_database(destination: &Path) -> Option<PathBuf> {
    if std::env::var("WORKLOG_SKIP_LEGACY_MIGRATION").as_deref() == Ok("1") {
        return None;
    }
    let mut candidates = Vec::new();
    if let Ok(database_url) = std::env::var("WORKLOG_DATABASE_URL") {
        if let Some(path) = database_url.strip_prefix("sqlite:///") {
            candidates.push(PathBuf::from(path));
        }
    }

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf);
    if let Some(repo) = repo {
        candidates.push(repo.join("data/worklog.db"));
        add_env_database_candidate(&repo.join(".env"), &mut candidates);
    }
    if let Ok(current) = std::env::current_dir() {
        candidates.push(current.join("data/worklog.db"));
        add_env_database_candidate(&current.join(".env"), &mut candidates);
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join("data/worklog.db"));
            candidates.push(parent.join("worklog.db"));
        }
    }

    candidates.into_iter().find(|candidate| {
        candidate != destination
            && candidate.is_file()
            && fs::metadata(candidate)
                .map(|meta| meta.len() > 0)
                .unwrap_or(false)
    })
}

fn copy_sqlite_database(source: &Path, destination: &Path) -> AppResult<()> {
    fs::copy(source, destination)?;
    for suffix in ["-wal", "-shm"] {
        let source_sidecar = PathBuf::from(format!("{}{suffix}", source.display()));
        if source_sidecar.is_file() {
            let destination_sidecar = PathBuf::from(format!("{}{suffix}", destination.display()));
            fs::copy(source_sidecar, destination_sidecar)?;
        }
    }
    Ok(())
}

fn add_env_database_candidate(env_file: &Path, candidates: &mut Vec<PathBuf>) {
    let Ok(content) = fs::read_to_string(env_file) else {
        return;
    };
    for line in content.lines() {
        let Some(value) = line.trim().strip_prefix("WORKLOG_DATABASE_URL=") else {
            continue;
        };
        if let Some(path) = value.trim_matches(['\'', '"']).strip_prefix("sqlite:///") {
            candidates.push(PathBuf::from(path));
        }
    }
}

pub fn now_string() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{llm_secret_key, SecretStore};

    fn test_directory(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("worklog-{label}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[tokio::test]
    async fn initializes_and_seeds_new_database() {
        let directory = test_directory("seed");
        let database = Database::open_for_test(directory, None).await.unwrap();
        let templates: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM templates")
            .fetch_one(&database.pool)
            .await
            .unwrap();
        let schedules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM report_schedules")
            .fetch_one(&database.pool)
            .await
            .unwrap();
        assert_eq!(templates, 3);
        assert_eq!(schedules, 3);
        assert!(database.path.exists());
    }

    #[tokio::test]
    async fn imports_legacy_data_idempotently_and_moves_secrets() {
        let legacy_directory = test_directory("legacy");
        let legacy = Database::open_for_test(legacy_directory, None)
            .await
            .unwrap();
        let now = now_string();
        sqlx::query(
            "INSERT INTO work_logs(id, work_date, start_date, end_date, project, task, progress, priority, created_at, updated_at) \
             VALUES(41, '2026-06-30', '2026-06-30', '2026-06-30', 'Worklog', 'Rust migration', 'done', 'high', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&legacy.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO llm_settings(id, provider, base_url, model, api_key, extra_headers, timeout_seconds, is_active, created_at, updated_at) \
             VALUES(7, 'openai', 'https://api.openai.com/v1', 'test', 'sk-legacy-secret', '{}', 60, 1, ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&legacy.pool)
        .await
        .unwrap();
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&legacy.pool)
            .await
            .unwrap();
        legacy.pool.close().await;
        let source_path = legacy.path.clone();
        let original_bytes = fs::read(&source_path).unwrap();

        let target_directory = test_directory("target");
        let target = Database::open_for_test(target_directory.clone(), Some(source_path.clone()))
            .await
            .unwrap();
        assert!(target.startup_migration.imported);
        let work_log: String = sqlx::query_scalar("SELECT task FROM work_logs WHERE id=41")
            .fetch_one(&target.pool)
            .await
            .unwrap();
        assert_eq!(work_log, "Rust migration");

        let secrets = SecretStore::memory();
        secrets.migrate_plaintext(&target.pool).await.unwrap();
        assert_eq!(
            secrets.get(&llm_secret_key(7)).as_deref(),
            Some("sk-legacy-secret")
        );
        let plaintext: String = sqlx::query_scalar("SELECT api_key FROM llm_settings WHERE id=7")
            .fetch_one(&target.pool)
            .await
            .unwrap();
        assert!(plaintext.is_empty());
        assert_eq!(fs::read(&source_path).unwrap(), original_bytes);

        target.pool.close().await;
        let reopened = Database::open_for_test(target_directory, Some(source_path))
            .await
            .unwrap();
        assert!(!reopened.startup_migration.imported);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM work_logs WHERE id=41")
            .fetch_one(&reopened.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }
}
