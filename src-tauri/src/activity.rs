use crate::db::now_string;
use crate::error::{AppError, AppResult};
use crate::llm;
use crate::models::{
    ActivityEvidence, ActivitySource, ActivitySourceCandidate, DailyCaptureRun,
    DailyCaptureSettings, DailyCaptureSettingsRow, DailyWorkSummary, WorkLog,
};
use crate::secrets::SecretStore;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::{Asia::Shanghai, Tz};
use serde_json::{json, Value};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, Row, SqliteConnection, SqlitePool};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_SUMMARY_BYTES: usize = 4 * 1024;
const MAX_LLM_INPUT_BYTES: usize = 64 * 1024;
const WORK_LOG_COLUMNS: &str = "id, work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, notes, origin, auto_capture_date, manually_edited, git_commit_count, agent_session_count, pending_evidence_count, created_at, updated_at";

#[derive(Debug, Clone)]
struct CollectedEvidence {
    source_id: i64,
    source_type: String,
    source_key: String,
    project: String,
    summary: String,
    occurred_at: DateTime<Utc>,
    metadata: Value,
}

pub fn validate_source_type(source_type: &str) -> AppResult<()> {
    if matches!(source_type, "git" | "codex" | "cursor") {
        Ok(())
    } else {
        Err(AppError::validation(
            "source_type",
            "source_type must be git, codex, or cursor",
        ))
    }
}

pub fn canonicalize_source_path(path: &str) -> AppResult<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(
            "path",
            "A source directory is required",
        ));
    }
    let canonical = fs::canonicalize(trimmed)
        .map_err(|_| AppError::validation("path", "The selected directory does not exist"))?;
    if !canonical.is_dir() {
        return Err(AppError::validation(
            "path",
            "The selected path must be a directory",
        ));
    }
    Ok(canonical.to_string_lossy().to_string())
}

fn is_git_repo(path: &Path) -> bool {
    let git_dir = path.join(".git");
    git_dir.is_dir() || git_dir.is_file()
}

pub fn discover_git_repos(root: &Path) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    if !root.is_dir() {
        return repos;
    }

    // 1. Check direct children for Git repositories
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && is_git_repo(&path) {
                repos.push(path);
            }
        }
    }

    // 2. If direct child git repos were found, root is a workspace containing child repos
    if !repos.is_empty() {
        if is_git_repo(root) {
            let has_commits = Command::new("git")
                .args(["-C"])
                .arg(root)
                .args(["rev-parse", "--verify", "HEAD"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if has_commits {
                repos.push(root.to_path_buf());
            }
        }
        repos.sort();
        return repos;
    }

    // 3. If root itself is a git repo, return it directly
    if is_git_repo(root) {
        return vec![root.to_path_buf()];
    }

    // 4. If no direct child git repos were found, check 1 more level (e.g. packages/repo)
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && !path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .starts_with('.')
            {
                if let Ok(sub_entries) = fs::read_dir(&path) {
                    for sub_entry in sub_entries.flatten() {
                        let sub_path = sub_entry.path();
                        if sub_path.is_dir() && is_git_repo(&sub_path) {
                            repos.push(sub_path);
                        }
                    }
                }
            }
        }
    }

    if !repos.is_empty() {
        repos.sort();
        return repos;
    }

    // 5. Fallback: if root is a subdirectory inside a git repo
    let output = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            let top_level = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !top_level.is_empty() {
                repos.push(PathBuf::from(top_level));
            }
        }
    }

    repos.sort();
    repos
}

pub async fn validate_source(source_type: &str, path: &str) -> AppResult<()> {
    validate_source_type(source_type)?;
    let path = PathBuf::from(path);
    match source_type {
        "git" => {
            let repos = discover_git_repos(&path);
            if repos.is_empty() {
                return Err(AppError::validation(
                    "path",
                    "The selected directory is not a Git repository or workspace containing Git repositories",
                ));
            }
        }
        "codex" => {
            if !path.join("sessions").is_dir()
                && path.file_name().and_then(|value| value.to_str()) != Some("sessions")
            {
                return Err(AppError::validation(
                    "path",
                    "A Codex directory must contain a sessions directory",
                ));
            }
        }
        "cursor" => {
            if cursor_workspace_roots(&path).is_empty() && cursor_global_dbs(&path).is_empty() {
                return Err(AppError::validation(
                    "path",
                    "A Cursor directory must contain workspaceStorage or globalStorage",
                ));
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub async fn normalize_source_path(source_type: &str, path: &str) -> AppResult<String> {
    validate_source_type(source_type)?;
    let canonical = canonicalize_source_path(path)?;
    if source_type != "git" {
        validate_source(source_type, &canonical).await?;
        return Ok(canonical);
    }
    let candidate = PathBuf::from(&canonical);
    let repos = discover_git_repos(&candidate);
    if repos.is_empty() {
        return Err(AppError::validation(
            "path",
            "The selected directory is not a Git repository or workspace containing Git repositories",
        ));
    }
    // If it's a multi-repo workspace containing child git repositories, preserve canonical workspace root!
    if repos.iter().any(|r| r.starts_with(&candidate) && r != &candidate) {
        return Ok(canonical);
    }
    // Single git repository: resolve top-level root
    let output = tokio::task::spawn_blocking(move || {
        Command::new("git")
            .args(["-C"])
            .arg(&candidate)
            .args(["rev-parse", "--show-toplevel"])
            .output()
    })
    .await
    .map_err(|error| AppError::new("git_error", error.to_string()))??;
    if !output.status.success() {
        return Err(AppError::validation(
            "path",
            "The selected directory is not a readable Git repository",
        ));
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    canonicalize_source_path(&root)
}

pub fn default_source_candidates(home: &Path) -> Vec<ActivitySourceCandidate> {
    let mut candidates = Vec::new();
    let codex = home.join(".codex");
    if codex.join("sessions").is_dir() {
        candidates.push(ActivitySourceCandidate {
            source_type: "codex".into(),
            path: codex.to_string_lossy().to_string(),
            display_name: "Codex".into(),
        });
    }

    let cursor_paths = if cfg!(target_os = "macos") {
        vec![home.join("Library/Application Support/Cursor/User")]
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| vec![path.join("Cursor/User")])
            .unwrap_or_default()
    } else {
        vec![home.join(".config/Cursor/User")]
    };
    for cursor in cursor_paths {
        if !cursor_workspace_roots(&cursor).is_empty() || !cursor_global_dbs(&cursor).is_empty() {
            candidates.push(ActivitySourceCandidate {
                source_type: "cursor".into(),
                path: cursor.to_string_lossy().to_string(),
                display_name: "Cursor".into(),
            });
        }
    }
    candidates
}

pub async fn load_settings(pool: &SqlitePool) -> AppResult<DailyCaptureSettings> {
    let row = sqlx::query_as::<_, DailyCaptureSettingsRow>(
        "SELECT enabled, run_time, timezone, lookback_days, last_success_at, created_at, updated_at FROM daily_capture_settings WHERE id=1",
    )
    .fetch_one(pool)
    .await?;
    let next_run_at = if row.enabled {
        Some(next_run(&row.run_time, Utc::now().with_timezone(&Shanghai))?.to_rfc3339())
    } else {
        None
    };
    Ok(DailyCaptureSettings {
        enabled: row.enabled,
        run_time: row.run_time,
        timezone: row.timezone,
        lookback_days: row.lookback_days,
        last_success_at: row.last_success_at,
        next_run_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub async fn list_runs(pool: &SqlitePool, limit: i64) -> AppResult<Vec<DailyCaptureRun>> {
    Ok(sqlx::query_as::<_, DailyCaptureRun>(
        "SELECT id, capture_date, status, work_log_id, used_llm, source_count, failed_source_count, git_commit_count, agent_session_count, pending_evidence_count, message, started_at, completed_at, created_at, updated_at FROM daily_capture_runs ORDER BY capture_date DESC LIMIT ?",
    )
    .bind(limit.clamp(1, 60))
    .fetch_all(pool)
    .await?)
}

pub async fn list_evidence(pool: &SqlitePool, date: NaiveDate) -> AppResult<Vec<ActivityEvidence>> {
    Ok(sqlx::query_as::<_, ActivityEvidence>(
        "SELECT id, activity_date, source_id, source_type, source_key, project, summary, occurred_at, metadata_json, created_at, updated_at FROM activity_evidence WHERE activity_date=? ORDER BY occurred_at, id",
    )
    .bind(date.to_string())
    .fetch_all(pool)
    .await?)
}

pub async fn run_capture(
    pool: &SqlitePool,
    secrets: &SecretStore,
    capture_lock: &Arc<Mutex<()>>,
    date: NaiveDate,
    force_overwrite: bool,
) -> AppResult<DailyCaptureRun> {
    let _guard = capture_lock.lock().await;
    let started_at = now_string();
    upsert_pending_run(pool, date, &started_at).await?;

    let sources = sqlx::query_as::<_, ActivitySource>(
        "SELECT id, source_type, path, display_name, enabled, discovered, last_scanned_at, last_error, created_at, updated_at FROM activity_sources WHERE enabled=1 ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    let mut workspaces = Vec::new();
    for source in sources.iter().filter(|s| s.source_type == "git") {
        let p = PathBuf::from(&source.path);
        workspaces.push((p.clone(), source.display_name.clone()));
        for child in discover_git_repos(&p) {
            if child != p {
                workspaces.push((child, source.display_name.clone()));
            }
        }
    }
    let source_count = sources.len() as i64;

    let mut source_errors = Vec::new();
    for source in &sources {
        match collect_source(source.clone(), date, workspaces.clone()).await {
            Ok(items) => {
                let source_keys = items
                    .iter()
                    .map(|item| item.source_key.clone())
                    .collect::<HashSet<_>>();
                for item in &items {
                    upsert_evidence(pool, date, item).await?;
                }
                prune_source_evidence(pool, source.id, date, &source_keys).await?;
                update_source_scan(pool, source.id, None).await?;
            }
            Err(error) => {
                let message = error.message.chars().take(500).collect::<String>();
                source_errors.push(format!("{}: {message}", source.display_name));
                update_source_scan(pool, source.id, Some(&message)).await?;
            }
        }
    }
    let failed_source_count = source_errors.len() as i64;

    let evidence = list_evidence(pool, date).await?;
    let git_commit_count = evidence
        .iter()
        .filter(|item| item.source_type == "git")
        .count() as i64;
    let agent_session_count = evidence.len() as i64 - git_commit_count;
    if evidence.is_empty() {
        let status = if source_errors.is_empty() {
            "skipped"
        } else {
            "failed"
        };
        let message = if source_errors.is_empty() {
            Some("当天没有发现已完成事项".to_string())
        } else {
            Some(source_errors.join("；"))
        };
        finish_run(
            pool,
            date,
            status,
            None,
            false,
            source_count,
            failed_source_count,
            0,
            0,
            0,
            message.as_deref(),
        )
        .await?;
        return load_run(pool, date).await;
    }

    let existing = load_auto_work_log(pool, date).await?;
    if let Some(current) = existing.as_ref().filter(|item| item.manually_edited) {
        if !force_overwrite {
            let pending = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM activity_evidence WHERE activity_date=? AND julianday(updated_at) > julianday(?)",
            )
            .bind(date.to_string())
            .bind(&current.updated_at)
            .fetch_one(pool)
            .await?;
            sqlx::query(
                "UPDATE work_logs SET pending_evidence_count=?, updated_at=updated_at WHERE id=?",
            )
            .bind(pending)
            .bind(current.id)
            .execute(pool)
            .await?;
            let mut messages = source_errors;
            if pending > 0 {
                messages.push(format!("自动记录已手工编辑，另有 {pending} 条新来源待合并"));
            }
            let message = (!messages.is_empty()).then(|| messages.join("；"));
            finish_run(
                pool,
                date,
                if messages.is_empty() {
                    "success"
                } else {
                    "partial"
                },
                Some(current.id),
                false,
                source_count,
                failed_source_count,
                git_commit_count,
                agent_session_count,
                pending,
                message.as_deref(),
            )
            .await?;
            mark_capture_success(pool).await?;
            return load_run(pool, date).await;
        }
    }

    let evidence_text = evidence_prompt(&evidence);
    let config = llm::active_config(pool, secrets).await?;
    let (summary, used_llm, fallback_error) =
        match llm::summarize_daily_activity(config.as_ref(), &date.to_string(), &evidence_text)
            .await
        {
            Ok(summary) => (summary, true, None),
            Err(error) => (
                fallback_summary(date, &evidence),
                false,
                Some(format!(
                    "AI 汇总不可用，已生成规则化记录：{}",
                    error.message
                )),
            ),
        };
    let notes = format!(
        "自动采集：Git {} 条，Agent {} 条。{}",
        git_commit_count,
        agent_session_count,
        if used_llm {
            "已使用当前 LLM 汇总。"
        } else {
            "待 AI 优化。"
        }
    );
    let work_log_id = write_auto_work_log(
        pool,
        date,
        existing.as_ref().map(|item| item.id),
        &summary,
        &notes,
        git_commit_count,
        agent_session_count,
    )
    .await?;

    let mut messages = source_errors;
    if let Some(message) = fallback_error {
        messages.push(message);
    }
    let status = if messages.is_empty() {
        "success"
    } else {
        "partial"
    };
    let message = (!messages.is_empty()).then(|| messages.join("；"));
    finish_run(
        pool,
        date,
        status,
        Some(work_log_id),
        used_llm,
        source_count,
        failed_source_count,
        git_commit_count,
        agent_session_count,
        0,
        message.as_deref(),
    )
    .await?;
    mark_capture_success(pool).await?;
    load_run(pool, date).await
}

pub async fn run_due(pool: &SqlitePool, secrets: &SecretStore, capture_lock: &Arc<Mutex<()>>) {
    let Ok(settings) = load_settings(pool).await else {
        return;
    };
    if !settings.enabled {
        return;
    }
    let Ok(run_time) = parse_time(&settings.run_time) else {
        return;
    };
    let now = Utc::now().with_timezone(&Shanghai);
    if now.time().hour() != run_time.hour() || now.time().minute() != run_time.minute() {
        return;
    }
    let scheduled_at = local_datetime(now.date_naive(), run_time)
        .map(|value| value.with_timezone(&Utc).to_rfc3339())
        .unwrap_or_default();
    let yesterday = now.date_naive() - Duration::days(1);
    for date in [yesterday, now.date_naive()] {
        if !run_completed_since(pool, date, &scheduled_at).await {
            let _ = run_capture(pool, secrets, capture_lock, date, false).await;
        }
    }
}

pub async fn run_catchups(pool: &SqlitePool, secrets: &SecretStore, capture_lock: &Arc<Mutex<()>>) {
    let Ok(settings) = load_settings(pool).await else {
        return;
    };
    if !settings.enabled {
        return;
    }
    let Ok(run_time) = parse_time(&settings.run_time) else {
        return;
    };
    let now = Utc::now().with_timezone(&Shanghai);
    for date in catchup_dates(now, run_time, settings.lookback_days) {
        let scheduled = local_datetime(date, run_time)
            .map(|value| value.with_timezone(&Utc).to_rfc3339())
            .unwrap_or_default();
        if !run_completed_since(pool, date, &scheduled).await {
            let _ = run_capture(pool, secrets, capture_lock, date, false).await;
        }
    }
}

fn catchup_dates(now: DateTime<Tz>, run_time: NaiveTime, lookback_days: i64) -> Vec<NaiveDate> {
    let latest_due = if now.time() >= run_time {
        now.date_naive()
    } else {
        now.date_naive() - Duration::days(1)
    };
    (0..lookback_days.clamp(1, 30))
        .rev()
        .map(|offset| latest_due - Duration::days(offset))
        .collect()
}

async fn collect_source(
    source: ActivitySource,
    date: NaiveDate,
    workspaces: Vec<(PathBuf, String)>,
) -> AppResult<Vec<CollectedEvidence>> {
    match source.source_type.as_str() {
        "git" => tokio::task::spawn_blocking(move || collect_git(&source, date))
            .await
            .map_err(|error| AppError::new("git_error", error.to_string()))?,
        "codex" => tokio::task::spawn_blocking(move || collect_codex(&source, date, &workspaces))
            .await
            .map_err(|error| AppError::new("codex_error", error.to_string()))?,
        "cursor" => collect_cursor(&source, date, &workspaces).await,
        _ => Err(AppError::new("source_error", "Unsupported activity source")),
    }
}

fn collect_git(source: &ActivitySource, date: NaiveDate) -> AppResult<Vec<CollectedEvidence>> {
    let source_path = Path::new(&source.path);
    let repos = discover_git_repos(source_path);
    if repos.is_empty() {
        return Err(AppError::new(
            "git_error",
            "The configured Git repository or workspace directory was not found or contains no Git repositories",
        ));
    }

    let global_email = git_config(source_path, "user.email").unwrap_or_default();
    let global_name = git_config(source_path, "user.name").unwrap_or_default();

    let has_any_identity = !global_email.is_empty()
        || !global_name.is_empty()
        || repos.iter().any(|r| {
            !git_config(r, "user.email").unwrap_or_default().is_empty()
                || !git_config(r, "user.name").unwrap_or_default().is_empty()
        });
    if !has_any_identity {
        return Err(AppError::new(
            "git_identity_missing",
            "Git user.email or user.name is required to identify your commits",
        ));
    }

    let (start, end) = day_bounds(date)?;
    let format = "%x1e%H%x1f%cI%x1f%an%x1f%ae%x1f%s%x1f%b%x1d";
    let is_workspace = repos.len() > 1 || repos.first() != Some(&source_path.to_path_buf());

    let mut result = Vec::new();
    let mut seen_commits = HashSet::new();

    for repo in &repos {
        let repo_email = git_config(repo, "user.email").unwrap_or_default();
        let repo_name = git_config(repo, "user.name").unwrap_or_default();

        let effective_email = if !repo_email.is_empty() {
            repo_email
        } else {
            global_email.clone()
        };
        let effective_name = if !repo_name.is_empty() {
            repo_name
        } else {
            global_name.clone()
        };

        if effective_email.is_empty() && effective_name.is_empty() {
            continue;
        }

        let output = Command::new("git")
            .args(["-C"])
            .arg(repo)
            .args([
                "-c",
                "core.quotepath=false",
                "log",
                "--all",
                "--no-merges",
                &format!("--since={}", start.to_rfc3339()),
                &format!("--until={}", end.to_rfc3339()),
                &format!("--format={format}"),
                "--numstat",
            ])
            .output()?;
        if !output.status.success() {
            continue;
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let repo_sub_name = repo.file_name().and_then(|n| n.to_str()).unwrap_or("");

        for record in text
            .split('\x1e')
            .filter(|record| !record.trim().is_empty())
        {
            let Some((header, file_text)) = record.split_once('\x1d') else {
                continue;
            };
            let fields = header.splitn(6, '\x1f').collect::<Vec<_>>();
            if fields.len() != 6 {
                continue;
            }
            let commit_sha = fields[0].trim();
            let author_name = fields[2].trim();
            let author_email = fields[3].trim();

            let author_matches = if !effective_email.is_empty() {
                author_email.eq_ignore_ascii_case(&effective_email)
                    || (!global_email.is_empty() && author_email.eq_ignore_ascii_case(&global_email))
            } else {
                author_name == effective_name
                    || (!global_name.is_empty() && author_name == global_name)
            };
            if !author_matches {
                continue;
            }

            let source_key = format!("git:{}:{}", repo.display(), commit_sha);
            if !seen_commits.insert(source_key.clone()) {
                continue;
            }

            let occurred_at = match DateTime::parse_from_rfc3339(fields[1].trim()) {
                Ok(dt) => dt.with_timezone(&Utc),
                Err(_) => continue,
            };
            if occurred_at < start || occurred_at >= end {
                continue;
            }

            let stats = file_text
                .lines()
                .filter_map(parse_numstat)
                .collect::<Vec<_>>();
            let file_count = stats.len();
            let additions = stats.iter().map(|(added, _, _)| added).sum::<i64>();
            let deletions = stats.iter().map(|(_, deleted, _)| deleted).sum::<i64>();
            let files = stats
                .iter()
                .map(|(_, _, file)| file.as_str())
                .take(20)
                .collect::<Vec<_>>();
            let body = fields[5].trim();
            let commit_subject = fields[4].trim();

            let mut summary = if is_workspace && !repo_sub_name.is_empty() {
                format!("[{repo_sub_name}] {commit_subject}")
            } else {
                commit_subject.to_string()
            };

            let clean_body = clean_commit_body(body);
            if !clean_body.is_empty() {
                summary.push_str(" — ");
                summary.push_str(&clean_body);
            }
            if file_count > 0 {
                summary.push_str(&format!(
                    "（涉及 {file_count} 个文件，+{additions}/-{deletions} 行：{}）",
                    files.join("、")
                ));
            }
            result.push(CollectedEvidence {
                source_id: source.id,
                source_type: "git".into(),
                source_key,
                project: source.display_name.clone(),
                summary: truncate_utf8_bytes(&summary, MAX_SUMMARY_BYTES),
                occurred_at,
                metadata: json!({
                    "repo": repo_sub_name,
                    "commit": commit_sha,
                    "files_changed": file_count,
                    "lines_added": additions,
                    "lines_removed": deletions,
                    "files": files
                }),
            });
        }
    }
    Ok(result)
}

fn parse_numstat(line: &str) -> Option<(i64, i64, String)> {
    let mut fields = line.trim().splitn(3, '\t');
    let added = fields.next()?;
    let deleted = fields.next()?;
    let file = fields.next()?.trim();
    if file.is_empty() {
        return None;
    }
    Some((
        added.parse().unwrap_or(0),
        deleted.parse().unwrap_or(0),
        file.to_string(),
    ))
}

fn clean_commit_body(body: &str) -> String {
    let mut lines = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("co-authored-by:")
            || lower.starts_with("signed-off-by:")
            || lower.starts_with("change-id:")
            || lower.starts_with("reviewed-by:")
        {
            continue;
        }
        lines.push(trimmed);
    }
    lines.join("；")
}

fn git_config(path: &Path, key: &str) -> AppResult<String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(["config", "--get", key])
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Ok(String::new())
    }
}

fn collect_codex(
    source: &ActivitySource,
    date: NaiveDate,
    workspaces: &[(PathBuf, String)],
) -> AppResult<Vec<CollectedEvidence>> {
    let base = if Path::new(&source.path).join("sessions").is_dir() {
        Path::new(&source.path).join("sessions")
    } else {
        PathBuf::from(&source.path)
    };
    let mut files = Vec::new();
    collect_files(&base, "jsonl", &mut files)?;
    let (start, end) = day_bounds(date)?;
    let mut result = Vec::new();
    for file in files {
        if let Ok(modified) = fs::metadata(&file).and_then(|metadata| metadata.modified()) {
            let modified = DateTime::<Utc>::from(modified);
            if modified < start - Duration::days(1) {
                continue;
            }
        }
        let reader = BufReader::new(fs::File::open(&file)?);
        let mut session_id = file
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("session")
            .to_string();
        let mut cwd: Option<PathBuf> = None;
        let mut final_answer: Option<String> = None;
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let record_type = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let payload = value.get("payload").unwrap_or(&Value::Null);
            if record_type == "session_meta" {
                if let Some(value) = payload
                    .get("session_id")
                    .or_else(|| payload.get("id"))
                    .and_then(Value::as_str)
                {
                    session_id = value.to_string();
                }
                cwd = payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(PathBuf::from);
            } else if record_type == "response_item"
                && payload.get("type").and_then(Value::as_str) == Some("message")
                && payload.get("role").and_then(Value::as_str) == Some("assistant")
                && payload.get("phase").and_then(Value::as_str) == Some("final_answer")
            {
                final_answer = response_text(payload.get("content").unwrap_or(&Value::Null));
            } else if record_type == "event_msg"
                && payload.get("type").and_then(Value::as_str) == Some("task_complete")
            {
                let Some(occurred_at) = record_timestamp(&value) else {
                    continue;
                };
                if occurred_at < start || occurred_at >= end {
                    continue;
                }
                let summary = payload
                    .get("last_agent_message")
                    .or_else(|| payload.get("final_response"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| final_answer.take());
                let Some(summary) = summary.filter(|text| !text.trim().is_empty()) else {
                    continue;
                };
                let ordinal = value
                    .get("ordinal")
                    .and_then(Value::as_i64)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| occurred_at.timestamp_millis().to_string());
                result.push(CollectedEvidence {
                    source_id: source.id,
                    source_type: "codex".into(),
                    source_key: format!("codex:{session_id}:{ordinal}"),
                    project: project_for_path(cwd.as_deref(), workspaces, &source.display_name),
                    summary: truncate_utf8_bytes(&summary, MAX_SUMMARY_BYTES),
                    occurred_at,
                    metadata: json!({"session_id": session_id}),
                });
            }
        }
    }
    Ok(result)
}

async fn collect_cursor(
    source: &ActivitySource,
    date: NaiveDate,
    workspaces: &[(PathBuf, String)],
) -> AppResult<Vec<CollectedEvidence>> {
    let (start, end) = day_bounds(date)?;
    let mut result = Vec::new();
    let mut seen_keys = HashSet::new();

    let path = Path::new(&source.path);
    let workspace_roots = cursor_workspace_roots(path);
    let global_dbs = cursor_global_dbs(path);

    // 1. Scan modern Cursor globalStorage/state.vscdb (composerHeaders table)
    for db_path in global_dbs {
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .read_only(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        let mut connection = match SqliteConnection::connect_with(&options).await {
            Ok(connection) => connection,
            Err(_) => continue,
        };

        let has_headers = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='composerHeaders'",
        )
        .fetch_one(&mut connection)
        .await
        .unwrap_or(0);

        if has_headers > 0 {
            let rows = sqlx::query(
                "SELECT composerId, workspaceId, createdAt, lastUpdatedAt, isArchived, isSubagent, checkpointAt, value FROM composerHeaders",
            )
            .fetch_all(&mut connection)
            .await
            .unwrap_or_default();

            for row in rows {
                let is_subagent: Option<i64> = row.try_get("isSubagent").ok();
                if is_subagent == Some(1) {
                    continue;
                }
                let raw_value: Option<String> = row.try_get("value").ok();
                let Some(raw_value) = raw_value else {
                    continue;
                };
                let Ok(composer) = serde_json::from_str::<Value>(&raw_value) else {
                    continue;
                };

                if composer.get("isDraft").and_then(Value::as_bool) == Some(true)
                    || composer
                        .get("hasBlockingPendingActions")
                        .and_then(Value::as_bool)
                        == Some(true)
                    || composer.get("isEphemeral").and_then(Value::as_bool) == Some(true)
                    || composer.get("isSubagent").and_then(Value::as_bool) == Some(true)
                    || composer.get("subagentInfo").is_some()
                {
                    continue;
                }

                let composer_id_col: Option<String> = row.try_get("composerId").ok();
                let composer_id = composer
                    .get("composerId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or(composer_id_col);
                let Some(composer_id) = composer_id else {
                    continue;
                };

                let last_updated_col: Option<i64> = row.try_get("lastUpdatedAt").ok();
                let checkpoint_at_col: Option<i64> = row.try_get("checkpointAt").ok();
                let created_at_col: Option<i64> = row.try_get("createdAt").ok();

                let occurred_at = composer
                    .get("lastUpdatedAt")
                    .or_else(|| composer.get("conversationCheckpointLastUpdatedAt"))
                    .or_else(|| composer.get("createdAt"))
                    .and_then(json_timestamp)
                    .or_else(|| last_updated_col.and_then(timestamp_from_millis))
                    .or_else(|| checkpoint_at_col.and_then(timestamp_from_millis))
                    .or_else(|| created_at_col.and_then(timestamp_from_millis));

                let Some(occurred_at) = occurred_at else {
                    continue;
                };
                if occurred_at < start || occurred_at >= end {
                    continue;
                }

                let raw_name = composer.get("name").and_then(Value::as_str);
                let name = raw_name.unwrap_or("Cursor 完成会话");
                let subtitle = composer
                    .get("summary")
                    .or_else(|| composer.get("subtitle"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let files = composer
                    .get("filesChangedCount")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let added = composer
                    .get("totalLinesAdded")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let removed = composer
                    .get("totalLinesRemoved")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);

                if files == 0 && added == 0 && removed == 0 {
                    continue;
                }

                let clean_subtitle = if subtitle.starts_with("Edited ") || subtitle.starts_with("Read ") {
                    ""
                } else {
                    subtitle
                };
                let summary = if name == "Cursor 完成会话" && !clean_subtitle.is_empty() {
                    format!("{clean_subtitle}（涉及 {files} 个文件变更，+{added}/-{removed} 行）")
                } else if clean_subtitle.is_empty() || clean_subtitle == name {
                    format!("{name}（涉及 {files} 个文件变更，+{added}/-{removed} 行）")
                } else {
                    format!("{name}：{clean_subtitle}（涉及 {files} 个文件变更，+{added}/-{removed} 行）")
                };

                let workspace_id_col: Option<String> = row.try_get("workspaceId").ok();
                let workspace_path = cursor_composer_workspace_path(
                    &composer,
                    workspace_id_col.as_deref(),
                    &workspace_roots,
                );

                let source_key = format!("cursor:{composer_id}");
                if !seen_keys.insert(source_key.clone()) {
                    continue;
                }

                result.push(CollectedEvidence {
                    source_id: source.id,
                    source_type: "cursor".into(),
                    source_key,
                    project: project_for_path(
                        workspace_path.as_deref(),
                        workspaces,
                        &source.display_name,
                    ),
                    summary: truncate_utf8_bytes(&summary, MAX_SUMMARY_BYTES),
                    occurred_at,
                    metadata: json!({
                        "composer_id": composer_id,
                        "files_changed": files,
                        "lines_added": added,
                        "lines_removed": removed
                    }),
                });
            }
        }
    }

    // 2. Scan legacy Cursor workspaceStorage/*/state.vscdb
    for root in &workspace_roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                continue;
            };
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let db_path = entry.path().join("state.vscdb");
            if !db_path.is_file() {
                continue;
            }
            let workspace_path = cursor_workspace_path(&entry.path().join("workspace.json"));
            let options = SqliteConnectOptions::new()
                .filename(&db_path)
                .read_only(true)
                .busy_timeout(std::time::Duration::from_secs(5));
            let mut connection = match SqliteConnection::connect_with(&options).await {
                Ok(connection) => connection,
                Err(_) => continue,
            };
            let raw = sqlx::query_scalar::<_, String>(
                "SELECT value FROM ItemTable WHERE key='composer.composerData' LIMIT 1",
            )
            .fetch_optional(&mut connection)
            .await
            .unwrap_or(None);
            let Some(raw) = raw else { continue };
            let Ok(value) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            let Some(composers) = value.get("allComposers").and_then(Value::as_array) else {
                continue;
            };
            for composer in composers {
                if composer.get("isDraft").and_then(Value::as_bool) == Some(true)
                    || composer
                        .get("hasBlockingPendingActions")
                        .and_then(Value::as_bool)
                        == Some(true)
                {
                    continue;
                }
                let Some(composer_id) = composer.get("composerId").and_then(Value::as_str) else {
                    continue;
                };
                let source_key = format!("cursor:{composer_id}");
                if !seen_keys.insert(source_key.clone()) {
                    continue;
                }
                let Some(occurred_at) = composer
                    .get("lastUpdatedAt")
                    .or_else(|| composer.get("createdAt"))
                    .and_then(json_timestamp)
                else {
                    continue;
                };
                if occurred_at < start || occurred_at >= end {
                    continue;
                }
                let name = composer
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Cursor 完成会话");
                let subtitle = composer
                    .get("summary")
                    .or_else(|| composer.get("subtitle"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let files = composer
                    .get("filesChangedCount")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let added = composer
                    .get("totalLinesAdded")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let removed = composer
                    .get("totalLinesRemoved")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                if files == 0 && added == 0 && removed == 0 {
                    continue;
                }
                let clean_subtitle = if subtitle.starts_with("Edited ") || subtitle.starts_with("Read ") {
                    ""
                } else {
                    subtitle
                };
                let summary = if name == "Cursor 完成会话" && !clean_subtitle.is_empty() {
                    format!("{clean_subtitle}（涉及 {files} 个文件变更，+{added}/-{removed} 行）")
                } else if clean_subtitle.is_empty() || clean_subtitle == name {
                    format!("{name}（涉及 {files} 个文件变更，+{added}/-{removed} 行）")
                } else {
                    format!("{name}：{clean_subtitle}（涉及 {files} 个文件变更，+{added}/-{removed} 行）")
                };
                result.push(CollectedEvidence {
                    source_id: source.id,
                    source_type: "cursor".into(),
                    source_key,
                    project: project_for_path(
                        workspace_path.as_deref(),
                        workspaces,
                        &source.display_name,
                    ),
                    summary: truncate_utf8_bytes(&summary, MAX_SUMMARY_BYTES),
                    occurred_at,
                    metadata: json!({
                        "composer_id": composer_id,
                        "files_changed": files,
                        "lines_added": added,
                        "lines_removed": removed
                    }),
                });
            }
        }
    }
    Ok(result)
}

fn cursor_global_dbs(path: &Path) -> Vec<PathBuf> {
    let mut dbs = Vec::new();
    let candidates = [
        path.join("globalStorage/state.vscdb"),
        path.join("User/globalStorage/state.vscdb"),
        path.join("state.vscdb"),
    ];
    for candidate in candidates {
        if candidate.is_file() && !dbs.contains(&candidate) {
            dbs.push(candidate);
        }
    }
    if let Some(parent) = path.parent() {
        for candidate in [
            parent.join("globalStorage/state.vscdb"),
            parent.join("state.vscdb"),
        ] {
            if candidate.is_file() && !dbs.contains(&candidate) {
                dbs.push(candidate);
            }
        }
    }
    dbs
}

fn cursor_workspace_roots(path: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if path.file_name().and_then(|value| value.to_str()) == Some("workspaceStorage")
        && path.is_dir()
    {
        roots.push(path.to_path_buf());
    }
    for candidate in [
        path.join("workspaceStorage"),
        path.join("User/workspaceStorage"),
    ] {
        if candidate.is_dir() && !roots.contains(&candidate) {
            roots.push(candidate);
        }
    }
    if let Some(parent) = path.parent() {
        let sibling = parent.join("workspaceStorage");
        if sibling.is_dir() && !roots.contains(&sibling) {
            roots.push(sibling);
        }
    }
    roots
}

fn parse_file_uri_or_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(url) = url::Url::parse(trimmed) {
        if let Ok(path) = url.to_file_path() {
            return Some(path);
        }
    }
    if let Some(stripped) = trimmed.strip_prefix("file://") {
        return Some(PathBuf::from(stripped));
    }
    Some(PathBuf::from(trimmed))
}

fn cursor_workspace_path(path: &Path) -> Option<PathBuf> {
    let raw = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    let folder = value.get("folder").and_then(Value::as_str)?;
    parse_file_uri_or_path(folder)
}

fn cursor_composer_workspace_path(
    composer: &Value,
    workspace_id: Option<&str>,
    workspace_roots: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(identifier) = composer.get("workspaceIdentifier") {
        if let Some(uri) = identifier.get("uri") {
            if let Some(fs_path) = uri.get("fsPath").and_then(Value::as_str) {
                if let Some(path) = parse_file_uri_or_path(fs_path) {
                    return Some(path);
                }
            }
            if let Some(p) = uri.get("path").and_then(Value::as_str) {
                if let Some(path) = parse_file_uri_or_path(p) {
                    return Some(path);
                }
            }
            if let Some(ext) = uri.get("external").and_then(Value::as_str) {
                if let Some(path) = parse_file_uri_or_path(ext) {
                    return Some(path);
                }
            }
            if let Some(raw) = uri.as_str() {
                if let Some(path) = parse_file_uri_or_path(raw) {
                    return Some(path);
                }
            }
        }
    }
    if let Some(repos) = composer.get("trackedGitRepos").and_then(Value::as_array) {
        for repo in repos {
            if let Some(repo_path) = repo.get("repoPath").and_then(Value::as_str) {
                if let Some(path) = parse_file_uri_or_path(repo_path) {
                    return Some(path);
                }
            }
        }
    }
    let target_wid = workspace_id.or_else(|| {
        composer
            .get("workspaceIdentifier")
            .and_then(|id| id.get("id"))
            .and_then(Value::as_str)
    });
    if let Some(wid) = target_wid {
        if wid != "empty-window" {
            for root in workspace_roots {
                let candidate = root.join(wid).join("workspace.json");
                if candidate.is_file() {
                    if let Some(path) = cursor_workspace_path(&candidate) {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

fn collect_files(path: &Path, extension: &str, output: &mut Vec<PathBuf>) -> AppResult<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(&entry.path(), extension, output)?;
        } else if file_type.is_file()
            && entry.path().extension().and_then(|value| value.to_str()) == Some(extension)
        {
            output.push(entry.path());
        }
    }
    Ok(())
}

fn response_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    let text = content
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    (!text.trim().is_empty()).then_some(text)
}

fn record_timestamp(value: &Value) -> Option<DateTime<Utc>> {
    value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| {
            value
                .get("payload")
                .and_then(|payload| payload.get("completed_at_ms"))
                .and_then(json_timestamp)
        })
}

fn timestamp_from_millis(value: i64) -> Option<DateTime<Utc>> {
    if value.abs() < 100_000_000_000 {
        DateTime::<Utc>::from_timestamp(value, 0)
    } else {
        DateTime::<Utc>::from_timestamp_millis(value)
    }
}

fn json_timestamp(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(value) = value.as_str() {
        return DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|value| value.with_timezone(&Utc));
    }
    value.as_i64().and_then(timestamp_from_millis)
}

fn project_for_path(
    path: Option<&Path>,
    workspaces: &[(PathBuf, String)],
    fallback: &str,
) -> String {
    let Some(path) = path else {
        return fallback.to_string();
    };
    workspaces
        .iter()
        .filter(|(workspace, _)| path.starts_with(workspace))
        .max_by_key(|(workspace, _)| workspace.components().count())
        .map(|(_, name)| name.clone())
        .or_else(|| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| fallback.to_string())
}

fn clean_summary_for_display(summary: &str) -> String {
    let mut text = summary.trim();
    // Strip leading [repo-name] if present
    if text.starts_with('[') {
        if let Some(pos) = text.find(']') {
            text = text[pos + 1..].trim();
        }
    }
    // Strip conventional commit type but keep scope (e.g. docs(V3.20.2): -> (V3.20.2): )
    let raw_prefixes = ["feat", "fix", "docs", "chore", "refactor", "style", "perf", "test", "merge"];
    for prefix in raw_prefixes {
        if text.starts_with(&format!("{prefix}:")) {
            text = text[prefix.len() + 1..].trim();
            break;
        } else if text.starts_with(&format!("{prefix}(")) {
            text = text[prefix.len()..].trim();
            break;
        }
    }
    // Strip trailing file / line stats like "（涉及 2 个文件，+44/-33 行：...）"
    if let Some(pos) = text.rfind("（涉及 ") {
        text = text[..pos].trim();
    }
    // Clean up cursor session subtitle file listings like "：Edited ..."
    if let Some(pos) = text.find("：Edited ") {
        text = text[..pos].trim();
    }
    if let Some(pos) = text.find("：Read ") {
        text = text[..pos].trim();
    }
    // Strip out git trailers like Co-authored-by
    let mut cleaned_parts = Vec::new();
    for part in text.split(" — ") {
        let p = part.trim();
        let lower = p.to_ascii_lowercase();
        if lower.starts_with("co-authored-by:")
            || lower.starts_with("signed-off-by:")
            || lower.starts_with("change-id:")
            || lower.starts_with("reviewed-by:")
        {
            continue;
        }
        cleaned_parts.push(p);
    }
    cleaned_parts.join(" — ")
}

fn evidence_prompt(evidence: &[ActivityEvidence]) -> String {
    let mut grouped: BTreeMap<&str, Vec<&ActivityEvidence>> = BTreeMap::new();
    for item in evidence {
        grouped.entry(&item.project).or_default().push(item);
    }
    let mut output = String::new();
    for (project, items) in grouped {
        output.push_str(&format!("## {project}\n"));
        for item in items {
            let clean = clean_summary_for_display(&item.summary);
            if clean.is_empty() {
                continue;
            }
            let repo_tag = serde_json::from_str::<Value>(&item.metadata_json)
                .ok()
                .and_then(|v| v.get("repo").and_then(Value::as_str).map(String::from))
                .filter(|r| !r.is_empty())
                .map(|r| format!("[{r}] "))
                .unwrap_or_default();
            let kind = match item.source_type.as_str() {
                "git" => "代码提交",
                "codex" => "开发任务",
                "cursor" => "开发任务",
                _ => "开发活动",
            };
            output.push_str(&format!("- [{kind}] {repo_tag}{clean}\n"));
            if output.len() >= MAX_LLM_INPUT_BYTES {
                return truncate_utf8_bytes(&output, MAX_LLM_INPUT_BYTES);
            }
        }
    }
    output
}

fn fallback_summary(date: NaiveDate, evidence: &[ActivityEvidence]) -> DailyWorkSummary {
    let mut by_project: BTreeMap<&str, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut detected_version = None;
    let mut detected_topic = None;

    let has_git_commits = evidence.iter().any(|e| e.source_type == "git");

    for item in evidence {
        if detected_version.is_none() {
            if let Some(pos) = item.summary.find("V3.") {
                let slice = &item.summary[pos..];
                let v = slice
                    .split(|c: char| !c.is_alphanumeric() && c != '.')
                    .next()
                    .unwrap_or("");
                if v.len() >= 4 {
                    detected_version = Some(v.to_string());
                }
            }
        }
        if detected_topic.is_none() {
            if item.summary.contains("5G消息")
                || item.summary.contains("5gmc")
                || item.summary.to_lowercase().contains("5g message")
            {
                detected_topic = Some("5G消息模板".to_string());
            }
        }

        // If we already have git commits, skip cursor session slugs like "5G message template migration:fix"
        if has_git_commits && item.source_type == "cursor" {
            let lower = item.summary.to_lowercase();
            if lower.contains("migration:")
                || lower.starts_with("untitled-")
                || lower.chars().all(|c| c.is_ascii())
            {
                continue;
            }
        }

        let repo_name = serde_json::from_str::<Value>(&item.metadata_json)
            .ok()
            .and_then(|v| v.get("repo").and_then(Value::as_str).map(String::from))
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| {
                if item.source_type == "cursor" {
                    "开发任务".to_string()
                } else {
                    item.project.clone()
                }
            });

        let desc = clean_summary_for_display(&item.summary);
        if !desc.is_empty() {
            by_project
                .entry(&item.project)
                .or_default()
                .entry(repo_name)
                .or_default()
                .push(desc);
        }
    }

    let mut progress_lines = Vec::new();
    let mut item_idx = 1;
    for (project, modules) in by_project {
        for (module, items) in modules {
            let unique_items = items
                .into_iter()
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let details = unique_items.join("；");
            if module == *project || module == "开发任务" {
                progress_lines.push(format!("{item_idx}、{details}"));
            } else {
                let friendly_name = match module.as_str() {
                    "biz-5gmc-edit-web" => "5G消息前端编辑器",
                    "biz-template-cloud" => "模板服务端与状态流转",
                    "biz-ai-pm" => "产品设计与验收文档",
                    "biz-bjzt-cloud" => "智推微服务",
                    other => other,
                };
                progress_lines.push(format!("{item_idx}、【{friendly_name}】{details}"));
            }
            item_idx += 1;
        }
    }

    let progress = if progress_lines.is_empty() {
        evidence_prompt(evidence)
    } else {
        progress_lines.join("\n")
    };

    let task = match (detected_version, detected_topic) {
        (Some(v), Some(t)) => format!("{v} {t} 研发进展"),
        (Some(v), None) => format!("{v} 业务功能研发进展"),
        (None, Some(t)) => format!("{t} 业务功能研发进展"),
        (None, None) => format!("{date} 核心业务功能研发"),
    };

    DailyWorkSummary {
        task,
        progress,
        result: Some(format!("共完成 {} 项模块功能与接口提交验收。", evidence.len())),
        blockers: None,
    }
}

async fn write_auto_work_log(
    pool: &SqlitePool,
    date: NaiveDate,
    existing_id: Option<i64>,
    summary: &DailyWorkSummary,
    notes: &str,
    git_commit_count: i64,
    agent_session_count: i64,
) -> AppResult<i64> {
    let now = now_string();
    let task = truncate_chars(summary.task.trim(), 240);
    if task.is_empty() || summary.progress.trim().is_empty() {
        return Err(AppError::new(
            "capture_error",
            "Daily summary did not contain a task and progress",
        ));
    }
    if let Some(id) = existing_id {
        sqlx::query(
            "UPDATE work_logs SET work_date=?, start_date=?, end_date=?, project='多项目', task=?, progress=?, result=?, blockers=?, hours=NULL, priority='medium', notes=?, origin='auto', auto_capture_date=?, manually_edited=0, git_commit_count=?, agent_session_count=?, pending_evidence_count=0, updated_at=? WHERE id=?",
        )
        .bind(date.to_string())
        .bind(date.to_string())
        .bind(date.to_string())
        .bind(task)
        .bind(summary.progress.trim())
        .bind(clean_text(summary.result.as_deref()))
        .bind(clean_text(summary.blockers.as_deref()))
        .bind(notes)
        .bind(date.to_string())
        .bind(git_commit_count)
        .bind(agent_session_count)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
        return Ok(id);
    }
    Ok(sqlx::query(
        "INSERT INTO work_logs(work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, notes, origin, auto_capture_date, manually_edited, git_commit_count, agent_session_count, pending_evidence_count, created_at, updated_at) VALUES(?, ?, ?, '多项目', ?, ?, ?, ?, NULL, 'medium', ?, 'auto', ?, 0, ?, ?, 0, ?, ?)",
    )
    .bind(date.to_string())
    .bind(date.to_string())
    .bind(date.to_string())
    .bind(task)
    .bind(summary.progress.trim())
    .bind(clean_text(summary.result.as_deref()))
    .bind(clean_text(summary.blockers.as_deref()))
    .bind(notes)
    .bind(date.to_string())
    .bind(git_commit_count)
    .bind(agent_session_count)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?
    .last_insert_rowid())
}

fn clean_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn load_auto_work_log(pool: &SqlitePool, date: NaiveDate) -> AppResult<Option<WorkLog>> {
    Ok(sqlx::query_as::<_, WorkLog>(&format!(
        "SELECT {WORK_LOG_COLUMNS} FROM work_logs WHERE auto_capture_date=? LIMIT 1"
    ))
    .bind(date.to_string())
    .fetch_optional(pool)
    .await?)
}

async fn upsert_evidence(
    pool: &SqlitePool,
    date: NaiveDate,
    item: &CollectedEvidence,
) -> AppResult<()> {
    let now = now_string();
    sqlx::query(
        "INSERT INTO activity_evidence(activity_date, source_id, source_type, source_key, project, summary, occurred_at, metadata_json, created_at, updated_at) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(source_key) DO UPDATE SET activity_date=excluded.activity_date, source_id=excluded.source_id, source_type=excluded.source_type, project=excluded.project, summary=excluded.summary, occurred_at=excluded.occurred_at, metadata_json=excluded.metadata_json, updated_at=excluded.updated_at WHERE activity_evidence.activity_date<>excluded.activity_date OR activity_evidence.source_id<>excluded.source_id OR activity_evidence.source_type<>excluded.source_type OR activity_evidence.project<>excluded.project OR activity_evidence.summary<>excluded.summary OR activity_evidence.occurred_at<>excluded.occurred_at OR activity_evidence.metadata_json<>excluded.metadata_json",
    )
    .bind(date.to_string())
    .bind(item.source_id)
    .bind(&item.source_type)
    .bind(&item.source_key)
    .bind(&item.project)
    .bind(&item.summary)
    .bind(item.occurred_at.to_rfc3339())
    .bind(serde_json::to_string(&item.metadata)?)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

async fn prune_source_evidence(
    pool: &SqlitePool,
    source_id: i64,
    date: NaiveDate,
    current_keys: &HashSet<String>,
) -> AppResult<()> {
    let existing = sqlx::query_scalar::<_, String>(
        "SELECT source_key FROM activity_evidence WHERE source_id=? AND activity_date=?",
    )
    .bind(source_id)
    .bind(date.to_string())
    .fetch_all(pool)
    .await?;
    for source_key in existing {
        if !current_keys.contains(&source_key) {
            sqlx::query("DELETE FROM activity_evidence WHERE source_key=?")
                .bind(source_key)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

async fn update_source_scan(
    pool: &SqlitePool,
    source_id: i64,
    error: Option<&str>,
) -> AppResult<()> {
    let now = now_string();
    sqlx::query(
        "UPDATE activity_sources SET last_scanned_at=?, last_error=?, updated_at=? WHERE id=?",
    )
    .bind(&now)
    .bind(error)
    .bind(&now)
    .bind(source_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn upsert_pending_run(pool: &SqlitePool, date: NaiveDate, started_at: &str) -> AppResult<()> {
    let now = now_string();
    sqlx::query(
        "INSERT INTO daily_capture_runs(capture_date, status, started_at, created_at, updated_at) VALUES(?, 'pending', ?, ?, ?) ON CONFLICT(capture_date) DO UPDATE SET status='pending', started_at=excluded.started_at, completed_at=NULL, updated_at=excluded.updated_at",
    )
    .bind(date.to_string())
    .bind(started_at)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn finish_run(
    pool: &SqlitePool,
    date: NaiveDate,
    status: &str,
    work_log_id: Option<i64>,
    used_llm: bool,
    source_count: i64,
    failed_source_count: i64,
    git_commit_count: i64,
    agent_session_count: i64,
    pending_evidence_count: i64,
    message: Option<&str>,
) -> AppResult<()> {
    let now = now_string();
    sqlx::query(
        "UPDATE daily_capture_runs SET status=?, work_log_id=?, used_llm=?, source_count=?, failed_source_count=?, git_commit_count=?, agent_session_count=?, pending_evidence_count=?, message=?, completed_at=?, updated_at=? WHERE capture_date=?",
    )
    .bind(status)
    .bind(work_log_id)
    .bind(used_llm)
    .bind(source_count)
    .bind(failed_source_count)
    .bind(git_commit_count)
    .bind(agent_session_count)
    .bind(pending_evidence_count)
    .bind(message)
    .bind(&now)
    .bind(&now)
    .bind(date.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

async fn load_run(pool: &SqlitePool, date: NaiveDate) -> AppResult<DailyCaptureRun> {
    sqlx::query_as::<_, DailyCaptureRun>(
        "SELECT id, capture_date, status, work_log_id, used_llm, source_count, failed_source_count, git_commit_count, agent_session_count, pending_evidence_count, message, started_at, completed_at, created_at, updated_at FROM daily_capture_runs WHERE capture_date=?",
    )
    .bind(date.to_string())
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}

async fn mark_capture_success(pool: &SqlitePool) -> AppResult<()> {
    let now = now_string();
    sqlx::query("UPDATE daily_capture_settings SET last_success_at=?, updated_at=? WHERE id=1")
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
    Ok(())
}

async fn run_completed_since(pool: &SqlitePool, date: NaiveDate, scheduled_at: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM daily_capture_runs WHERE capture_date=? AND completed_at IS NOT NULL AND datetime(completed_at) >= datetime(?))",
    )
    .bind(date.to_string())
    .bind(scheduled_at)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

fn day_bounds(date: NaiveDate) -> AppResult<(DateTime<Utc>, DateTime<Utc>)> {
    let start = local_datetime(date, NaiveTime::MIN)?.with_timezone(&Utc);
    let end = local_datetime(date + Duration::days(1), NaiveTime::MIN)?.with_timezone(&Utc);
    Ok((start, end))
}

fn parse_time(value: &str) -> AppResult<NaiveTime> {
    NaiveTime::parse_from_str(value, "%H:%M:%S%.f")
        .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M"))
        .map_err(|_| AppError::validation("run_time", "run_time must use HH:MM format"))
}

fn local_datetime(date: NaiveDate, time: NaiveTime) -> AppResult<DateTime<Tz>> {
    Shanghai
        .from_local_datetime(&date.and_time(time))
        .single()
        .ok_or_else(|| AppError::new("schedule_error", "Invalid local schedule time"))
}

fn next_run(value: &str, now: DateTime<Tz>) -> AppResult<DateTime<Tz>> {
    let time = parse_time(value)?;
    let today = local_datetime(now.date_naive(), time)?;
    if today > now {
        Ok(today)
    } else {
        local_datetime(now.date_naive() + Duration::days(1), time)
    }
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push('…');
    }
    result
}

fn truncate_utf8_bytes(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }
    let suffix = "…";
    let available = limit.saturating_sub(suffix.len());
    let mut result = String::new();
    for character in value.chars() {
        if result.len() + character.len_utf8() > available {
            break;
        }
        result.push(character);
    }
    if limit >= suffix.len() {
        result.push_str(suffix);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::secrets::SecretStore;

    fn test_directory(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "worklog-activity-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn run_git(path: &Path, args: &[&str], date: Option<&str>) -> String {
        let mut command = Command::new("git");
        command.args(["-C"]).arg(path).args(args);
        if let Some(date) = date {
            command.env("GIT_AUTHOR_DATE", date);
            command.env("GIT_COMMITTER_DATE", date);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn commit_file(path: &Path, file: &str, content: &str, message: &str, date: &str) {
        fs::write(path.join(file), content).unwrap();
        run_git(path, &["add", "--", file], None);
        run_git(path, &["commit", "-m", message], Some(date));
    }

    #[test]
    fn truncates_agent_summaries() {
        let value = "中".repeat(MAX_SUMMARY_BYTES);
        let truncated = truncate_utf8_bytes(&value, MAX_SUMMARY_BYTES);
        assert!(truncated.len() <= MAX_SUMMARY_BYTES);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn finds_cursor_workspace_roots() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("worklog-cursor-{unique}"));
        fs::create_dir_all(root.join("User/workspaceStorage")).unwrap();
        assert_eq!(cursor_workspace_roots(&root).len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn groups_rule_fallback_by_project() {
        let evidence = vec![ActivityEvidence {
            id: 1,
            activity_date: "2026-09-17".into(),
            source_id: 1,
            source_type: "git".into(),
            source_key: "git:test".into(),
            project: "Worklog".into(),
            summary: "完成自动采集".into(),
            occurred_at: "2026-09-17T10:00:00Z".into(),
            metadata_json: "{}".into(),
            created_at: "2026-09-17T10:00:00Z".into(),
            updated_at: "2026-09-17T10:00:00Z".into(),
        }];
        let summary = fallback_summary(NaiveDate::from_ymd_opt(2026, 9, 17).unwrap(), &evidence);
        assert!(summary.progress.contains("完成自动采集"));
        assert!(summary.task.contains("研发"));
    }

    #[test]
    fn caps_llm_evidence_in_utf8_bytes() {
        let evidence = (0..20)
            .map(|id| ActivityEvidence {
                id,
                activity_date: "2026-09-17".into(),
                source_id: 1,
                source_type: "codex".into(),
                source_key: format!("codex:{id}"),
                project: "Worklog".into(),
                summary: "中".repeat(MAX_SUMMARY_BYTES),
                occurred_at: "2026-09-17T10:00:00Z".into(),
                metadata_json: "{}".into(),
                created_at: "2026-09-17T10:00:00Z".into(),
                updated_at: "2026-09-17T10:00:00Z".into(),
            })
            .collect::<Vec<_>>();
        let prompt = evidence_prompt(&evidence);
        assert!(prompt.len() <= MAX_LLM_INPUT_BYTES);
        assert!(prompt.is_char_boundary(prompt.len()));
    }

    #[test]
    fn computes_seven_day_catchup_window_before_daily_run() {
        let now = Shanghai
            .with_ymd_and_hms(2026, 9, 17, 17, 30, 0)
            .single()
            .unwrap();
        let dates = catchup_dates(now, NaiveTime::from_hms_opt(18, 0, 0).unwrap(), 7);
        assert_eq!(dates.len(), 7);
        assert_eq!(dates[0], NaiveDate::from_ymd_opt(2026, 9, 10).unwrap());
        assert_eq!(dates[6], NaiveDate::from_ymd_opt(2026, 9, 16).unwrap());
    }

    #[test]
    fn collects_only_current_author_git_commits_across_branches() {
        let repository = test_directory("git 中文 path");
        run_git(&repository, &["init"], None);
        run_git(&repository, &["config", "user.name", "Test User"], None);
        run_git(
            &repository,
            &["config", "user.email", "me@example.com"],
            None,
        );
        run_git(&repository, &["config", "commit.gpgsign", "false"], None);
        run_git(&repository, &["checkout", "-b", "main"], None);
        commit_file(
            &repository,
            "initial.txt",
            "base\n",
            "initial",
            "2026-09-16T10:00:00+08:00",
        );
        run_git(&repository, &["checkout", "-b", "feature"], None);
        commit_file(
            &repository,
            "中文 文件.txt",
            "完成\n",
            "完成跨分支功能",
            "2026-09-17T10:00:00+08:00",
        );
        run_git(&repository, &["checkout", "main"], None);
        fs::write(repository.join("other.txt"), "other\n").unwrap();
        run_git(&repository, &["add", "--", "other.txt"], None);
        run_git(
            &repository,
            &[
                "commit",
                "--author=Other User <other@example.com>",
                "-m",
                "其他作者提交",
            ],
            Some("2026-09-17T11:00:00+08:00"),
        );
        commit_file(
            &repository,
            "next-day.txt",
            "later\n",
            "次日边界",
            "2026-09-18T00:00:00+08:00",
        );

        let source = ActivitySource {
            id: 1,
            source_type: "git".into(),
            path: repository.to_string_lossy().to_string(),
            display_name: "Git 测试".into(),
            enabled: true,
            discovered: false,
            last_scanned_at: None,
            last_error: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let evidence = collect_git(&source, NaiveDate::from_ymd_opt(2026, 9, 17).unwrap()).unwrap();
        assert_eq!(evidence.len(), 1);
        assert!(evidence[0].summary.contains("完成跨分支功能"));
        assert!(evidence[0].summary.contains("中文 文件.txt"));
        assert_eq!(evidence[0].metadata["files_changed"], 1);
        assert_eq!(evidence[0].metadata["lines_added"], 1);
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn git_collects_across_multi_repo_workspace() {
        let workspace = test_directory("git-workspace");
        let repo_a = workspace.join("biz-a");
        let repo_b = workspace.join("biz-b");
        fs::create_dir_all(&repo_a).unwrap();
        fs::create_dir_all(&repo_b).unwrap();

        for (repo, file, msg) in [
            (&repo_a, "a.txt", "feat: 完成组件A开发"),
            (&repo_b, "b.txt", "fix: 修复组件B边界异常"),
        ] {
            run_git(repo, &["init"], None);
            run_git(repo, &["config", "user.name", "Test User"], None);
            run_git(repo, &["config", "user.email", "me@example.com"], None);
            run_git(repo, &["config", "commit.gpgsign", "false"], None);
            run_git(repo, &["checkout", "-b", "main"], None);
            commit_file(
                repo,
                file,
                "content\n",
                msg,
                "2026-09-17T10:00:00+08:00",
            );
        }

        let source = ActivitySource {
            id: 1,
            source_type: "git".into(),
            path: workspace.to_string_lossy().to_string(),
            display_name: "多仓库工作区".into(),
            enabled: true,
            discovered: false,
            last_scanned_at: None,
            last_error: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let evidence = collect_git(&source, NaiveDate::from_ymd_opt(2026, 9, 17).unwrap()).unwrap();
        assert_eq!(evidence.len(), 2);
        assert!(evidence.iter().all(|e| e.project == "多仓库工作区"));
        assert!(evidence.iter().any(|e| e.summary.contains("[biz-a] feat: 完成组件A开发")));
        assert!(evidence.iter().any(|e| e.summary.contains("[biz-b] fix: 修复组件B边界异常")));

        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn codex_requires_task_complete_and_only_keeps_final_answer() {
        let root = test_directory("codex");
        let sessions = root.join("sessions/2026/09/17");
        fs::create_dir_all(&sessions).unwrap();
        let records = [
            json!({"timestamp":"2026-09-17T01:59:00Z","type":"session_meta","payload":{"id":"session-1","cwd":"/workspace/project"}}),
            json!({"timestamp":"2026-09-17T02:00:00Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"不应采集的用户提示"}]}}),
            json!({"timestamp":"2026-09-17T02:01:00Z","type":"response_item","payload":{"type":"message","role":"assistant","phase":"final_answer","content":[{"text":"已完成自动采集功能"}]}}),
            json!({"timestamp":"2026-09-17T02:02:00Z","type":"event_msg","payload":{"type":"task_complete"}}),
            json!({"timestamp":"2026-09-17T03:00:00Z","type":"response_item","payload":{"type":"message","role":"assistant","phase":"final_answer","content":[{"text":"尚未完成的内容"}]}}),
        ];
        let mut content = records
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        content.push_str("\n{broken json}\n");
        fs::write(sessions.join("session-1.jsonl"), content).unwrap();
        let source = ActivitySource {
            id: 2,
            source_type: "codex".into(),
            path: root.to_string_lossy().to_string(),
            display_name: "Codex".into(),
            enabled: true,
            discovered: true,
            last_scanned_at: None,
            last_error: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let evidence =
            collect_codex(&source, NaiveDate::from_ymd_opt(2026, 9, 17).unwrap(), &[]).unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].summary, "已完成自动采集功能");
        assert!(!evidence[0].summary.contains("用户提示"));
        assert_eq!(evidence[0].source_key, "codex:session-1:1789610520000");
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn cursor_skips_drafts_and_reads_completion_statistics() {
        let root = test_directory("cursor");
        let workspace_root = root.join("User/workspaceStorage/workspace-a");
        let project = root.join("project");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            workspace_root.join("workspace.json"),
            json!({"folder": url::Url::from_directory_path(&project).unwrap().to_string()})
                .to_string(),
        )
        .unwrap();
        let database_path = workspace_root.join("state.vscdb");
        let options = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
        sqlx::query("CREATE TABLE ItemTable(key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(&mut connection)
            .await
            .unwrap();
        let data = json!({"allComposers":[
            {"composerId":"done","name":"实现采集器","summary":"支持 SQLite 固件","isDraft":false,"lastUpdatedAt":1789610400000_i64,"filesChangedCount":3,"totalLinesAdded":42,"totalLinesRemoved":7},
            {"composerId":"draft","name":"草稿","isDraft":true,"lastUpdatedAt":1789610400000_i64},
            {"composerId":"next-day","name":"边界外","isDraft":false,"lastUpdatedAt":1789660800000_i64}
        ]});
        sqlx::query("INSERT INTO ItemTable(key, value) VALUES('composer.composerData', ?)")
            .bind(data.to_string())
            .execute(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
        let source = ActivitySource {
            id: 3,
            source_type: "cursor".into(),
            path: root.to_string_lossy().to_string(),
            display_name: "Cursor".into(),
            enabled: true,
            discovered: true,
            last_scanned_at: None,
            last_error: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let evidence = collect_cursor(
            &source,
            NaiveDate::from_ymd_opt(2026, 9, 17).unwrap(),
            &[(project, "Cursor 项目".into())],
        )
        .await
        .unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].project, "Cursor 项目");
        assert!(evidence[0].summary.contains("+42/-7"));
        assert_eq!(evidence[0].source_key, "cursor:done");
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn cursor_collects_from_composer_headers_and_resolves_workspace() {
        let root = test_directory("cursor-global");
        let global_storage = root.join("User/globalStorage");
        let workspace_storage = root.join("User/workspaceStorage/workspace-hash");
        let project_dir = root.join("project-b");
        fs::create_dir_all(&global_storage).unwrap();
        fs::create_dir_all(&workspace_storage).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        fs::write(
            workspace_storage.join("workspace.json"),
            json!({"folder": url::Url::from_directory_path(&project_dir).unwrap().to_string()})
                .to_string(),
        )
        .unwrap();

        let database_path = global_storage.join("state.vscdb");
        let options = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options).await.unwrap();

        sqlx::query(
            "CREATE TABLE composerHeaders (
                composerId TEXT PRIMARY KEY,
                workspaceId TEXT,
                createdAt INTEGER,
                lastUpdatedAt INTEGER,
                isArchived INTEGER,
                isSubagent INTEGER,
                recency INTEGER,
                checkpointAt INTEGER,
                value TEXT,
                subagentTypeName TEXT
            )",
        )
        .execute(&mut connection)
        .await
        .unwrap();

        // 1. Valid completed session with workspaceId mapping
        let val1 = json!({
            "composerId": "comp-1",
            "name": "重构 Cursor 采集",
            "subtitle": "支持 composerHeaders 表",
            "isDraft": false,
            "hasBlockingPendingActions": false,
            "lastUpdatedAt": 1789610400000_i64,
            "filesChangedCount": 5,
            "totalLinesAdded": 120,
            "totalLinesRemoved": 15,
            "workspaceIdentifier": {"id": "workspace-hash"}
        });
        sqlx::query(
            "INSERT INTO composerHeaders (composerId, workspaceId, createdAt, lastUpdatedAt, isArchived, isSubagent, value)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("comp-1")
        .bind("workspace-hash")
        .bind(1789610400000_i64)
        .bind(1789610400000_i64)
        .bind(0)
        .bind(0)
        .bind(val1.to_string())
        .execute(&mut connection)
        .await
        .unwrap();

        // 2. Draft session (should be skipped)
        let val2 = json!({
            "composerId": "comp-draft",
            "name": "草稿任务",
            "isDraft": true,
            "lastUpdatedAt": 1789610400000_i64
        });
        sqlx::query(
            "INSERT INTO composerHeaders (composerId, workspaceId, createdAt, lastUpdatedAt, isArchived, isSubagent, value)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("comp-draft")
        .bind("workspace-hash")
        .bind(1789610400000_i64)
        .bind(1789610400000_i64)
        .bind(0)
        .bind(0)
        .bind(val2.to_string())
        .execute(&mut connection)
        .await
        .unwrap();

        // 3. Subagent session (should be skipped)
        let val3 = json!({
            "composerId": "comp-subagent",
            "name": "内部探索子任务",
            "isDraft": false,
            "lastUpdatedAt": 1789610400000_i64,
            "isSubagent": true
        });
        sqlx::query(
            "INSERT INTO composerHeaders (composerId, workspaceId, createdAt, lastUpdatedAt, isArchived, isSubagent, value)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("comp-subagent")
        .bind("workspace-hash")
        .bind(1789610400000_i64)
        .bind(1789610400000_i64)
        .bind(0)
        .bind(1)
        .bind(val3.to_string())
        .execute(&mut connection)
        .await
        .unwrap();

        // 4. Session with direct workspaceIdentifier fsPath
        let project_c = root.join("project-c");
        fs::create_dir_all(&project_c).unwrap();
        let val4 = json!({
            "composerId": "comp-direct",
            "name": "直接指定路径会话",
            "isDraft": false,
            "lastUpdatedAt": 1789610500000_i64,
            "filesChangedCount": 2,
            "totalLinesAdded": 30,
            "totalLinesRemoved": 5,
            "workspaceIdentifier": {
                "uri": {"fsPath": project_c.to_string_lossy().to_string()}
            }
        });
        sqlx::query(
            "INSERT INTO composerHeaders (composerId, workspaceId, createdAt, lastUpdatedAt, isArchived, isSubagent, value)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("comp-direct")
        .bind("workspace-direct")
        .bind(1789610500000_i64)
        .bind(1789610500000_i64)
        .bind(0)
        .bind(0)
        .bind(val4.to_string())
        .execute(&mut connection)
        .await
        .unwrap();

        connection.close().await.unwrap();

        let source = ActivitySource {
            id: 10,
            source_type: "cursor".into(),
            path: root.join("User").to_string_lossy().to_string(),
            display_name: "Cursor".into(),
            enabled: true,
            discovered: true,
            last_scanned_at: None,
            last_error: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let evidence = collect_cursor(
            &source,
            NaiveDate::from_ymd_opt(2026, 9, 17).unwrap(),
            &[(project_dir, "Project B".into()), (project_c, "Project C".into())],
        )
        .await
        .unwrap();

        assert_eq!(evidence.len(), 2);
        assert_eq!(evidence[0].source_key, "cursor:comp-1");
        assert_eq!(evidence[0].project, "Project B");
        assert!(evidence[0].summary.contains("+120/-15"));
        assert_eq!(evidence[1].source_key, "cursor:comp-direct");
        assert_eq!(evidence[1].project, "Project C");
        assert!(evidence[1].summary.contains("+30/-5"));

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn capture_is_idempotent_and_protects_manually_edited_logs() {
        let root = test_directory("capture");
        let database = Database::open_for_test(root.join("data"), None)
            .await
            .unwrap();
        let codex = root.join("codex");
        let sessions = codex.join("sessions/2026/09/17");
        fs::create_dir_all(&sessions).unwrap();
        let write_session = |name: &str, session_id: &str, minute: u32, summary: &str| {
            let records = [
                json!({"timestamp":format!("2026-09-17T02:{minute:02}:00Z"),"type":"session_meta","payload":{"id":session_id,"cwd":"/workspace/project"}}),
                json!({"timestamp":format!("2026-09-17T02:{:02}:10Z", minute),"type":"event_msg","payload":{"type":"task_complete","last_agent_message":summary}}),
            ];
            fs::write(
                sessions.join(name),
                records
                    .iter()
                    .map(Value::to_string)
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            .unwrap();
        };
        write_session("one.jsonl", "session-one", 10, "完成第一项");
        let now = now_string();
        sqlx::query(
            "INSERT INTO activity_sources(source_type, path, display_name, enabled, discovered, created_at, updated_at) VALUES('codex', ?, 'Codex', 1, 0, ?, ?)",
        )
        .bind(codex.to_string_lossy().to_string())
        .bind(&now)
        .bind(&now)
        .execute(&database.pool)
        .await
        .unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 9, 17).unwrap();
        let secrets = SecretStore::memory();
        let capture_lock = Arc::new(Mutex::new(()));

        let first = run_capture(&database.pool, &secrets, &capture_lock, date, false)
            .await
            .unwrap();
        assert_eq!(first.agent_session_count, 1);
        assert_eq!(first.source_count, 1);
        assert_eq!(first.failed_source_count, 0);
        assert!(!first.used_llm);
        run_capture(&database.pool, &secrets, &capture_lock, date, false)
            .await
            .unwrap();
        let log_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM work_logs WHERE auto_capture_date='2026-09-17'",
        )
        .fetch_one(&database.pool)
        .await
        .unwrap();
        let evidence_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_evidence WHERE activity_date='2026-09-17'",
        )
        .fetch_one(&database.pool)
        .await
        .unwrap();
        assert_eq!(log_count, 1);
        assert_eq!(evidence_count, 1);

        sqlx::query(
            "UPDATE work_logs SET progress='用户手工编辑', manually_edited=1, updated_at=? WHERE auto_capture_date='2026-09-17'",
        )
        .bind(now_string())
        .execute(&database.pool)
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        write_session("two.jsonl", "session-two", 20, "完成第二项");
        let protected = run_capture(&database.pool, &secrets, &capture_lock, date, false)
            .await
            .unwrap();
        assert_eq!(protected.pending_evidence_count, 1);
        let protected_progress: String = sqlx::query_scalar(
            "SELECT progress FROM work_logs WHERE auto_capture_date='2026-09-17'",
        )
        .fetch_one(&database.pool)
        .await
        .unwrap();
        assert_eq!(protected_progress, "用户手工编辑");

        let overwritten = run_capture(&database.pool, &secrets, &capture_lock, date, true)
            .await
            .unwrap();
        assert_eq!(overwritten.pending_evidence_count, 0);
        assert_eq!(overwritten.agent_session_count, 2);
        let (manually_edited, progress): (bool, String) = sqlx::query_as(
            "SELECT manually_edited, progress FROM work_logs WHERE auto_capture_date='2026-09-17'",
        )
        .fetch_one(&database.pool)
        .await
        .unwrap();
        assert!(!manually_edited);
        assert!(progress.contains("完成第一项"));
        assert!(progress.contains("完成第二项"));
        database.pool.close().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn capture_skips_empty_days_and_isolates_broken_sources() {
        let root = test_directory("partial");
        let database = Database::open_for_test(root.join("data"), None)
            .await
            .unwrap();
        let codex = root.join("codex");
        let sessions = codex.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let now = now_string();
        sqlx::query(
            "INSERT INTO activity_sources(source_type, path, display_name, enabled, discovered, created_at, updated_at) VALUES('codex', ?, 'Codex', 1, 0, ?, ?)",
        )
        .bind(codex.to_string_lossy().to_string())
        .bind(&now)
        .bind(&now)
        .execute(&database.pool)
        .await
        .unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 9, 17).unwrap();
        let secrets = SecretStore::memory();
        let capture_lock = Arc::new(Mutex::new(()));
        let empty = run_capture(&database.pool, &secrets, &capture_lock, date, false)
            .await
            .unwrap();
        assert_eq!(empty.status, "skipped");
        let log_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM work_logs")
            .fetch_one(&database.pool)
            .await
            .unwrap();
        assert_eq!(log_count, 0);

        fs::write(
            sessions.join("done.jsonl"),
            [
                json!({"timestamp":"2026-09-17T02:00:00Z","type":"session_meta","payload":{"id":"done","cwd":"/workspace"}}),
                json!({"timestamp":"2026-09-17T02:01:00Z","type":"event_msg","payload":{"type":"task_complete","last_agent_message":"完成可用来源"}}),
            ]
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        )
        .unwrap();
        sqlx::query(
            "INSERT INTO activity_sources(source_type, path, display_name, enabled, discovered, created_at, updated_at) VALUES('git', ?, '已删除仓库', 1, 0, ?, ?)",
        )
        .bind(root.join("missing-repository").to_string_lossy().to_string())
        .bind(&now)
        .bind(&now)
        .execute(&database.pool)
        .await
        .unwrap();
        let partial = run_capture(&database.pool, &secrets, &capture_lock, date, false)
            .await
            .unwrap();
        assert_eq!(partial.status, "partial");
        assert_eq!(partial.source_count, 2);
        assert_eq!(partial.failed_source_count, 1);
        assert_eq!(partial.agent_session_count, 1);
        assert!(partial.message.as_deref().unwrap().contains("已删除仓库"));
        let broken_error: Option<String> = sqlx::query_scalar(
            "SELECT last_error FROM activity_sources WHERE display_name='已删除仓库'",
        )
        .fetch_one(&database.pool)
        .await
        .unwrap();
        assert!(broken_error.is_some());
        database.pool.close().await;
        let _ = fs::remove_dir_all(root);
    }
}
