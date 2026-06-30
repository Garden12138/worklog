use crate::db::now_string;
use crate::error::{AppError, AppResult};
use crate::llm;
use crate::mail;
use crate::models::*;
use crate::reports;
use crate::secrets::{email_secret_key, is_masked, llm_secret_key, mask_secret};
use crate::templates::validate_template;
use crate::AppState;
use sqlx::{QueryBuilder, Row, Sqlite};
use std::path::PathBuf;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn require_text(value: Option<String>, field: &str, max: Option<usize>) -> AppResult<String> {
    let value = value.unwrap_or_default().trim().to_string();
    if value.is_empty() {
        return Err(AppError::validation(field, format!("{field} is required")));
    }
    if max.is_some_and(|limit| value.chars().count() > limit) {
        return Err(AppError::validation(field, format!("{field} is too long")));
    }
    Ok(value)
}

#[tauri::command]
pub async fn list_work_logs(
    state: State<'_, AppState>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> AppResult<PaginatedWorkLogs> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(10).clamp(1, 100);
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM work_logs")
        .fetch_one(&state.db.pool)
        .await?;
    let items = sqlx::query_as::<_, WorkLog>(
        "SELECT id, work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, notes, created_at, updated_at \
         FROM work_logs ORDER BY start_date DESC, end_date DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(page_size)
    .bind((page - 1) * page_size)
    .fetch_all(&state.db.pool)
    .await?;
    Ok(PaginatedWorkLogs {
        items,
        total,
        page,
        page_size,
        total_pages: ((total + page_size - 1) / page_size).max(1),
    })
}

#[tauri::command]
pub async fn create_work_log(
    state: State<'_, AppState>,
    payload: WorkLogInput,
) -> AppResult<WorkLog> {
    let start = payload
        .start_date
        .or(payload.work_date)
        .ok_or_else(|| AppError::validation("start_date", "start_date is required"))?;
    let end = payload.end_date.unwrap_or_else(|| start.clone());
    let start_date = reports::parse_date(&start, "start_date")?;
    let end_date = reports::parse_date(&end, "end_date")?;
    if end_date < start_date {
        return Err(AppError::validation(
            "end_date",
            "end_date must be on or after start_date",
        ));
    }
    validate_hours(payload.hours)?;
    let priority = validate_priority(payload.priority.as_deref().unwrap_or("medium"))?;
    let project = require_text(payload.project, "project", Some(160))?;
    let task = require_text(payload.task, "task", Some(240))?;
    let progress = require_text(payload.progress, "progress", None)?;
    let now = now_string();
    let id = sqlx::query(
        "INSERT INTO work_logs(work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, notes, created_at, updated_at) \
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&start)
    .bind(&start)
    .bind(&end)
    .bind(project)
    .bind(task)
    .bind(progress)
    .bind(clean_optional(payload.result))
    .bind(clean_optional(payload.blockers))
    .bind(payload.hours)
    .bind(priority)
    .bind(clean_optional(payload.notes))
    .bind(&now)
    .bind(&now)
    .execute(&state.db.pool)
    .await?
    .last_insert_rowid();
    load_work_log(&state, id).await
}

#[tauri::command]
pub async fn update_work_log(
    state: State<'_, AppState>,
    id: i64,
    payload: WorkLogInput,
) -> AppResult<WorkLog> {
    let current = load_work_log(&state, id).await?;
    let start = payload
        .start_date
        .or(payload.work_date)
        .unwrap_or(current.start_date);
    let end = payload.end_date.unwrap_or(current.end_date);
    let start_date = reports::parse_date(&start, "start_date")?;
    let end_date = reports::parse_date(&end, "end_date")?;
    if end_date < start_date {
        return Err(AppError::validation(
            "end_date",
            "end_date must be on or after start_date",
        ));
    }
    validate_hours(payload.hours)?;
    let priority = validate_priority(payload.priority.as_deref().unwrap_or(&current.priority))?;
    let project = require_text(
        payload.project.or(Some(current.project)),
        "project",
        Some(160),
    )?;
    let task = require_text(payload.task.or(Some(current.task)), "task", Some(240))?;
    let progress = require_text(
        payload.progress.or(Some(current.progress)),
        "progress",
        None,
    )?;
    let now = now_string();
    sqlx::query(
        "UPDATE work_logs SET work_date=?, start_date=?, end_date=?, project=?, task=?, progress=?, result=?, blockers=?, hours=?, priority=?, notes=?, updated_at=? WHERE id=?",
    )
    .bind(&start)
    .bind(&start)
    .bind(&end)
    .bind(project)
    .bind(task)
    .bind(progress)
    .bind(clean_optional(payload.result))
    .bind(clean_optional(payload.blockers))
    .bind(payload.hours)
    .bind(priority)
    .bind(clean_optional(payload.notes))
    .bind(&now)
    .bind(id)
    .execute(&state.db.pool)
    .await?;
    load_work_log(&state, id).await
}

#[tauri::command]
pub async fn delete_work_log(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM work_logs WHERE id=?")
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("Work log"));
    }
    Ok(())
}

async fn load_work_log(state: &AppState, id: i64) -> AppResult<WorkLog> {
    sqlx::query_as::<_, WorkLog>(
        "SELECT id, work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, notes, created_at, updated_at FROM work_logs WHERE id=?",
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::not_found("Work log"))
}

fn validate_hours(hours: Option<f64>) -> AppResult<()> {
    if hours.is_some_and(|hours| !(0.0..=24.0).contains(&hours)) {
        return Err(AppError::validation(
            "hours",
            "hours must be between 0 and 24",
        ));
    }
    Ok(())
}

fn validate_priority(priority: &str) -> AppResult<String> {
    match priority {
        "low" | "medium" | "high" | "urgent" => Ok(priority.into()),
        _ => Err(AppError::validation("priority", "Unsupported priority")),
    }
}

#[tauri::command]
pub async fn list_templates(
    state: State<'_, AppState>,
    template_type: Option<String>,
) -> AppResult<Vec<Template>> {
    if let Some(template_type) = template_type {
        reports::validate_report_type(&template_type)?;
        Ok(sqlx::query_as::<_, Template>(
            "SELECT id, name, template_type, content, is_default, created_at, updated_at FROM templates WHERE template_type=? ORDER BY is_default DESC, updated_at DESC",
        )
        .bind(template_type)
        .fetch_all(&state.db.pool)
        .await?)
    } else {
        Ok(sqlx::query_as::<_, Template>(
            "SELECT id, name, template_type, content, is_default, created_at, updated_at FROM templates ORDER BY template_type, is_default DESC, updated_at DESC",
        )
        .fetch_all(&state.db.pool)
        .await?)
    }
}

#[tauri::command]
pub async fn create_template(
    state: State<'_, AppState>,
    payload: TemplateInput,
) -> AppResult<Template> {
    let name = require_text(payload.name, "name", Some(160))?;
    let template_type = payload
        .template_type
        .ok_or_else(|| AppError::validation("template_type", "template_type is required"))?;
    reports::validate_report_type(&template_type)?;
    let content = require_text(payload.content, "content", None)?;
    validate_template(&content)?;
    let is_default = payload.is_default.unwrap_or(false);
    let mut transaction = state.db.pool.begin().await?;
    if is_default {
        sqlx::query("UPDATE templates SET is_default=0 WHERE template_type=?")
            .bind(&template_type)
            .execute(&mut *transaction)
            .await?;
    }
    let now = now_string();
    let id = sqlx::query(
        "INSERT INTO templates(name, template_type, content, is_default, created_at, updated_at) VALUES(?, ?, ?, ?, ?, ?)",
    )
    .bind(name)
    .bind(template_type)
    .bind(content)
    .bind(is_default)
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await?
    .last_insert_rowid();
    transaction.commit().await?;
    load_template(&state, id).await
}

#[tauri::command]
pub async fn update_template(
    state: State<'_, AppState>,
    id: i64,
    payload: TemplateInput,
) -> AppResult<Template> {
    let current = load_template(&state, id).await?;
    let name = require_text(payload.name.or(Some(current.name)), "name", Some(160))?;
    let template_type = payload
        .template_type
        .unwrap_or(current.template_type.clone());
    reports::validate_report_type(&template_type)?;
    let content = require_text(payload.content.or(Some(current.content)), "content", None)?;
    validate_template(&content)?;
    let is_default = payload.is_default.unwrap_or(current.is_default);
    if current.template_type != template_type {
        let in_use: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM report_schedules WHERE template_id=?")
                .bind(id)
                .fetch_one(&state.db.pool)
                .await?;
        if in_use > 0 {
            return Err(AppError::new(
                "conflict",
                "Template type cannot change while used by a report schedule",
            ));
        }
    }
    if current.is_default && !is_default {
        return Err(AppError::new(
            "conflict",
            "A default template cannot be unset; select another default template instead",
        ));
    }
    let mut transaction = state.db.pool.begin().await?;
    if is_default {
        sqlx::query("UPDATE templates SET is_default=0 WHERE template_type=? AND id!=?")
            .bind(&template_type)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query("UPDATE templates SET name=?, template_type=?, content=?, is_default=?, updated_at=? WHERE id=?")
        .bind(name)
        .bind(template_type)
        .bind(content)
        .bind(is_default)
        .bind(now_string())
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    load_template(&state, id).await
}

#[tauri::command]
pub async fn delete_template(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    let template = load_template(&state, id).await?;
    if template.is_default {
        return Err(AppError::new(
            "conflict",
            "Default templates cannot be deleted",
        ));
    }
    let mut transaction = state.db.pool.begin().await?;
    sqlx::query("UPDATE report_schedules SET template_id=NULL WHERE template_id=?")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM templates WHERE id=?")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

async fn load_template(state: &AppState, id: i64) -> AppResult<Template> {
    sqlx::query_as::<_, Template>(
        "SELECT id, name, template_type, content, is_default, created_at, updated_at FROM templates WHERE id=?",
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::not_found("Template"))
}

#[tauri::command]
pub async fn import_template_example(
    state: State<'_, AppState>,
    payload: TemplateImportInput,
) -> AppResult<TemplateTransformResponse> {
    reports::validate_report_type(&payload.template_type)?;
    if payload.example_content.trim().chars().count() < 20 {
        return Err(AppError::validation(
            "example_content",
            "Example must contain at least 20 characters",
        ));
    }
    let config = llm::active_config(&state.db.pool, &state.secrets).await?;
    let generated = llm::template_from_example(
        config.as_ref(),
        &payload.template_type,
        &payload.example_content,
    )
    .await?;
    validate_template(&generated.content)?;
    Ok(TemplateTransformResponse {
        template_type: payload.template_type,
        content: generated.content,
        used_llm: generated.used_llm,
    })
}

#[tauri::command]
pub async fn optimize_template(
    state: State<'_, AppState>,
    payload: TemplateOptimizeInput,
) -> AppResult<TemplateTransformResponse> {
    reports::validate_report_type(&payload.template_type)?;
    validate_template(&payload.content)?;
    if payload.optimization_request.trim().chars().count() < 2 {
        return Err(AppError::validation(
            "optimization_request",
            "Optimization request is too short",
        ));
    }
    let config = llm::active_config(&state.db.pool, &state.secrets).await?;
    let optimized = llm::optimize_template(
        config.as_ref(),
        &payload.template_type,
        &payload.content,
        payload.optimization_request.trim(),
    )
    .await?;
    validate_template(&optimized.content)?;
    Ok(TemplateTransformResponse {
        template_type: payload.template_type,
        content: optimized.content,
        used_llm: optimized.used_llm,
    })
}

#[tauri::command]
pub async fn list_reports(state: State<'_, AppState>) -> AppResult<Vec<Report>> {
    reports::list_reports(&state.db.pool).await
}

#[tauri::command]
pub async fn update_report(
    state: State<'_, AppState>,
    id: i64,
    payload: ReportUpdateInput,
) -> AppResult<Report> {
    let current = reports::load_report(&state.db.pool, id).await?;
    let title = payload.title.unwrap_or(current.title).trim().to_string();
    let content = payload
        .content_markdown
        .unwrap_or(current.content_markdown)
        .trim()
        .to_string();
    if title.is_empty() || content.is_empty() {
        return Err(AppError::validation(
            "content_markdown",
            "Report title and content are required",
        ));
    }
    let status = payload.status.unwrap_or(current.status);
    let now = now_string();
    sqlx::query(
        "UPDATE reports SET title=?, content_markdown=?, status=?, edited_at=?, updated_at=? WHERE id=?",
    )
    .bind(title)
    .bind(content)
    .bind(status)
    .bind(&now)
    .bind(&now)
    .bind(id)
    .execute(&state.db.pool)
    .await?;
    reports::load_report(&state.db.pool, id).await
}

#[tauri::command]
pub async fn delete_report(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM reports WHERE id=?")
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("Report"));
    }
    Ok(())
}

#[tauri::command]
pub async fn optimize_report(
    state: State<'_, AppState>,
    id: i64,
    payload: ReportOptimizeInput,
) -> AppResult<ReportOptimizeResponse> {
    let report = reports::load_report(&state.db.pool, id).await?;
    if payload.optimization_request.trim().chars().count() < 2 {
        return Err(AppError::validation(
            "optimization_request",
            "Optimization request is too short",
        ));
    }
    let config = llm::active_config(&state.db.pool, &state.secrets).await?;
    let optimized = llm::optimize_report(
        config.as_ref(),
        &report.report_type,
        &report.period_start,
        &report.period_end,
        &payload.content,
        payload.optimization_request.trim(),
    )
    .await?;
    Ok(ReportOptimizeResponse {
        report_id: id,
        content: optimized.content,
        used_llm: optimized.used_llm,
    })
}

#[tauri::command]
pub async fn generate_report(
    state: State<'_, AppState>,
    payload: ReportGenerateInput,
) -> AppResult<GenerateResponse> {
    reports::generate(
        &state.db.pool,
        &state.secrets,
        &state.active_generations,
        payload,
    )
    .await
}

#[tauri::command]
pub async fn export_report_docx(
    state: State<'_, AppState>,
    report_id: i64,
    path: String,
) -> AppResult<String> {
    let report = reports::load_report(&state.db.pool, report_id).await?;
    let bytes = crate::documents::markdown_to_docx(&report.content_markdown)?;
    let mut path = PathBuf::from(path);
    if path.extension().and_then(|value| value.to_str()) != Some("docx") {
        path.set_extension("docx");
    }
    tokio::fs::write(&path, bytes).await?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn list_report_email_deliveries(
    state: State<'_, AppState>,
    report_id: i64,
) -> AppResult<Vec<ReportEmailDelivery>> {
    let rows = sqlx::query_as::<_, ReportEmailDeliveryRow>(
        "SELECT id, report_id, subject, recipients_json, status, error_message, sent_at, created_at, updated_at \
         FROM report_email_deliveries WHERE report_id=? ORDER BY id DESC",
    )
    .bind(report_id)
    .fetch_all(&state.db.pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn send_report_email(
    state: State<'_, AppState>,
    report_id: i64,
    payload: ReportEmailSendInput,
) -> AppResult<ReportEmailDelivery> {
    let report = reports::load_report(&state.db.pool, report_id).await?;
    if payload.subject.trim().is_empty() {
        return Err(AppError::validation("subject", "Email subject is required"));
    }
    mail::deliver_report(
        &state.db.pool,
        &state.secrets,
        &report,
        &payload.recipient_ids,
        &payload.additional_recipients,
        payload.subject.trim(),
    )
    .await
}

fn default_base_url(provider: &str) -> AppResult<&'static str> {
    match provider {
        "openai" => Ok("https://api.openai.com/v1"),
        "nvidia" => Ok("https://integrate.api.nvidia.com/v1"),
        "openrouter" => Ok("https://openrouter.ai/api/v1"),
        _ => Err(AppError::validation("provider", "Unsupported LLM provider")),
    }
}

fn llm_read(row: LlmSettingRow, state: &AppState) -> LlmSetting {
    let secret = state
        .secrets
        .get(&llm_secret_key(row.id))
        .or_else(|| row.api_key.clone());
    LlmSetting {
        id: row.id,
        provider: row.provider,
        base_url: row.base_url,
        model: row.model,
        api_key: mask_secret(secret.as_deref()),
        extra_headers: row
            .extra_headers
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default(),
        timeout_seconds: row.timeout_seconds,
        is_active: row.is_active,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

async fn load_llm_row(state: &AppState, id: i64) -> AppResult<LlmSettingRow> {
    sqlx::query_as::<_, LlmSettingRow>(
        "SELECT id, provider, base_url, model, api_key, extra_headers, timeout_seconds, is_active, created_at, updated_at FROM llm_settings WHERE id=?",
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::not_found("LLM setting"))
}

#[tauri::command]
pub async fn get_llm_setting(state: State<'_, AppState>) -> AppResult<Option<LlmSetting>> {
    let row = sqlx::query_as::<_, LlmSettingRow>(
        "SELECT id, provider, base_url, model, api_key, extra_headers, timeout_seconds, is_active, created_at, updated_at \
         FROM llm_settings WHERE is_active=1 ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(&state.db.pool)
    .await?;
    Ok(row.map(|row| llm_read(row, &state)))
}

#[tauri::command]
pub async fn list_llm_settings(state: State<'_, AppState>) -> AppResult<Vec<LlmSetting>> {
    let rows = sqlx::query_as::<_, LlmSettingRow>(
        "SELECT id, provider, base_url, model, api_key, extra_headers, timeout_seconds, is_active, created_at, updated_at FROM llm_settings ORDER BY id DESC",
    )
    .fetch_all(&state.db.pool)
    .await?;
    Ok(rows.into_iter().map(|row| llm_read(row, &state)).collect())
}

#[tauri::command]
pub async fn create_llm_setting(
    state: State<'_, AppState>,
    payload: LlmSettingInput,
) -> AppResult<LlmSetting> {
    let provider = payload.provider.clone();
    let base_url = clean_optional(payload.base_url).unwrap_or(default_base_url(&provider)?.into());
    url::Url::parse(&base_url)
        .map_err(|_| AppError::validation("base_url", "A valid base URL is required"))?;
    let model = require_text(Some(payload.model), "model", Some(160))?;
    if !(5..=600).contains(&payload.timeout_seconds) {
        return Err(AppError::validation(
            "timeout_seconds",
            "timeout_seconds must be between 5 and 600",
        ));
    }
    let provided_secret = clean_optional(payload.api_key);
    let reused_secret = if provided_secret.is_none() {
        let prior_id = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM llm_settings WHERE provider=? ORDER BY id DESC LIMIT 1",
        )
        .bind(&provider)
        .fetch_optional(&state.db.pool)
        .await?;
        prior_id.and_then(|id| state.secrets.get(&llm_secret_key(id)))
    } else {
        None
    };
    let now = now_string();
    let mut transaction = state.db.pool.begin().await?;
    sqlx::query("UPDATE llm_settings SET is_active=0, updated_at=?")
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    let id = sqlx::query(
        "INSERT INTO llm_settings(provider, base_url, model, api_key, extra_headers, timeout_seconds, is_active, created_at, updated_at) \
         VALUES(?, ?, ?, '', ?, ?, 1, ?, ?)",
    )
    .bind(provider)
    .bind(base_url)
    .bind(model)
    .bind(serde_json::to_string(&payload.extra_headers)?)
    .bind(payload.timeout_seconds)
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await?
    .last_insert_rowid();
    transaction.commit().await?;
    if let Some(secret) = provided_secret.or(reused_secret) {
        if let Err(error) = state.secrets.set(&llm_secret_key(id), &secret) {
            let _ = sqlx::query("DELETE FROM llm_settings WHERE id=?")
                .bind(id)
                .execute(&state.db.pool)
                .await;
            return Err(error);
        }
    }
    Ok(llm_read(load_llm_row(&state, id).await?, &state))
}

#[tauri::command]
pub async fn update_llm_setting(
    state: State<'_, AppState>,
    id: i64,
    payload: LlmSettingInput,
) -> AppResult<LlmSetting> {
    let current = load_llm_row(&state, id).await?;
    let base_url =
        clean_optional(payload.base_url).unwrap_or(default_base_url(&payload.provider)?.into());
    url::Url::parse(&base_url)
        .map_err(|_| AppError::validation("base_url", "A valid base URL is required"))?;
    let model = require_text(Some(payload.model), "model", Some(160))?;
    if !(5..=600).contains(&payload.timeout_seconds) {
        return Err(AppError::validation(
            "timeout_seconds",
            "timeout_seconds must be between 5 and 600",
        ));
    }
    let now = now_string();
    let mut transaction = state.db.pool.begin().await?;
    sqlx::query("UPDATE llm_settings SET is_active=0, updated_at=?")
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "UPDATE llm_settings SET provider=?, base_url=?, model=?, extra_headers=?, timeout_seconds=?, is_active=1, updated_at=? WHERE id=?",
    )
    .bind(payload.provider)
    .bind(base_url)
    .bind(model)
    .bind(serde_json::to_string(&payload.extra_headers)?)
    .bind(payload.timeout_seconds)
    .bind(&now)
    .bind(id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    if let Some(secret) = clean_optional(payload.api_key) {
        if !is_masked(&secret) {
            state.secrets.set(&llm_secret_key(id), &secret)?;
        }
    } else if current
        .api_key
        .as_deref()
        .is_some_and(|value| !value.is_empty())
    {
        state
            .secrets
            .set(&llm_secret_key(id), current.api_key.as_deref().unwrap())?;
        sqlx::query("UPDATE llm_settings SET api_key='' WHERE id=?")
            .bind(id)
            .execute(&state.db.pool)
            .await?;
    }
    Ok(llm_read(load_llm_row(&state, id).await?, &state))
}

#[tauri::command]
pub async fn apply_llm_setting(state: State<'_, AppState>, id: i64) -> AppResult<LlmSetting> {
    load_llm_row(&state, id).await?;
    let mut transaction = state.db.pool.begin().await?;
    let now = now_string();
    sqlx::query("UPDATE llm_settings SET is_active=0, updated_at=?")
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE llm_settings SET is_active=1, updated_at=? WHERE id=?")
        .bind(&now)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(llm_read(load_llm_row(&state, id).await?, &state))
}

#[tauri::command]
pub async fn delete_llm_setting(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    let row = load_llm_row(&state, id).await?;
    sqlx::query("DELETE FROM llm_settings WHERE id=?")
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    state.secrets.delete(&llm_secret_key(id));
    if row.is_active {
        if let Some(next_id) =
            sqlx::query_scalar::<_, i64>("SELECT id FROM llm_settings ORDER BY id DESC LIMIT 1")
                .fetch_optional(&state.db.pool)
                .await?
        {
            sqlx::query("UPDATE llm_settings SET is_active=1, updated_at=? WHERE id=?")
                .bind(now_string())
                .bind(next_id)
                .execute(&state.db.pool)
                .await?;
        }
    }
    Ok(())
}

fn email_read(row: &EmailSettingRow, state: &AppState) -> EmailSetting {
    let secret = state
        .secrets
        .get(&email_secret_key(row.id))
        .or_else(|| (!row.password.is_empty()).then_some(row.password.clone()));
    EmailSetting {
        host: row.host.clone(),
        port: row.port,
        security: row.security.clone(),
        username: row.username.clone(),
        password: mask_secret(secret.as_deref()),
        sender_address: row.sender_address.clone(),
        sender_name: row.sender_name.clone(),
    }
}

#[tauri::command]
pub async fn get_email_setting(state: State<'_, AppState>) -> AppResult<Option<EmailSetting>> {
    Ok(mail::active_email_setting(&state.db.pool)
        .await?
        .map(|row| email_read(&row, &state)))
}

#[tauri::command]
pub async fn update_email_setting(
    state: State<'_, AppState>,
    payload: EmailSettingInput,
) -> AppResult<EmailSetting> {
    let host = require_text(Some(payload.host), "host", Some(255))?;
    let username = require_text(Some(payload.username), "username", Some(320))?;
    let sender_address = mail::normalize_email(&payload.sender_address)?;
    if !(1..=65535).contains(&payload.port) {
        return Err(AppError::validation(
            "port",
            "port must be between 1 and 65535",
        ));
    }
    if payload.security != "ssl" && payload.security != "starttls" {
        return Err(AppError::validation(
            "security",
            "security must be ssl or starttls",
        ));
    }
    let existing = mail::active_email_setting(&state.db.pool).await?;
    let now = now_string();
    let id = if let Some(existing) = existing {
        sqlx::query(
            "UPDATE email_settings SET host=?, port=?, security=?, username=?, sender_address=?, sender_name=?, is_active=1, updated_at=? WHERE id=?",
        )
        .bind(host)
        .bind(payload.port)
        .bind(payload.security)
        .bind(username)
        .bind(sender_address)
        .bind(clean_optional(payload.sender_name))
        .bind(&now)
        .bind(existing.id)
        .execute(&state.db.pool)
        .await?;
        existing.id
    } else {
        sqlx::query(
            "INSERT INTO email_settings(host, port, security, username, password, sender_address, sender_name, is_active, created_at, updated_at) \
             VALUES(?, ?, ?, ?, '', ?, ?, 1, ?, ?)",
        )
        .bind(host)
        .bind(payload.port)
        .bind(payload.security)
        .bind(username)
        .bind(sender_address)
        .bind(clean_optional(payload.sender_name))
        .bind(&now)
        .bind(&now)
        .execute(&state.db.pool)
        .await?
        .last_insert_rowid()
    };
    if let Some(password) = clean_optional(payload.password) {
        if !is_masked(&password) {
            state.secrets.set(&email_secret_key(id), &password)?;
        }
    }
    let row = mail::active_email_setting(&state.db.pool)
        .await?
        .ok_or_else(|| AppError::not_found("Email setting"))?;
    Ok(email_read(&row, &state))
}

#[tauri::command]
pub async fn send_email_test(
    state: State<'_, AppState>,
    recipient_email: String,
) -> AppResult<bool> {
    let setting = mail::active_email_setting(&state.db.pool)
        .await?
        .ok_or_else(|| AppError::new("email_not_configured", "请先在设置中完成 SMTP 邮箱配置"))?;
    let password = state
        .secrets
        .get(&email_secret_key(setting.id))
        .or_else(|| (!setting.password.is_empty()).then_some(setting.password.clone()))
        .ok_or_else(|| AppError::new("email_not_configured", "SMTP password is required"))?;
    mail::send_test_email(&setting, &password, &recipient_email).await?;
    Ok(true)
}

#[tauri::command]
pub async fn list_recipients(state: State<'_, AppState>) -> AppResult<Vec<Recipient>> {
    Ok(sqlx::query_as::<_, Recipient>(
        "SELECT id, name, email, is_default, created_at, updated_at FROM recipients ORDER BY id",
    )
    .fetch_all(&state.db.pool)
    .await?)
}

#[tauri::command]
pub async fn create_recipient(
    state: State<'_, AppState>,
    payload: RecipientInput,
) -> AppResult<Recipient> {
    let name = require_text(payload.name, "name", Some(160))?;
    let email = mail::normalize_email(payload.email.as_deref().unwrap_or_default())?;
    let now = now_string();
    let id = sqlx::query(
        "INSERT INTO recipients(name, email, is_default, created_at, updated_at) VALUES(?, ?, ?, ?, ?)",
    )
    .bind(name)
    .bind(email)
    .bind(payload.is_default.unwrap_or(false))
    .bind(&now)
    .bind(&now)
    .execute(&state.db.pool)
    .await?
    .last_insert_rowid();
    load_recipient(&state, id).await
}

#[tauri::command]
pub async fn update_recipient(
    state: State<'_, AppState>,
    id: i64,
    payload: RecipientInput,
) -> AppResult<Recipient> {
    let current = load_recipient(&state, id).await?;
    let name = require_text(payload.name.or(Some(current.name)), "name", Some(160))?;
    let email = mail::normalize_email(payload.email.as_deref().unwrap_or(&current.email))?;
    sqlx::query("UPDATE recipients SET name=?, email=?, is_default=?, updated_at=? WHERE id=?")
        .bind(name)
        .bind(email)
        .bind(payload.is_default.unwrap_or(current.is_default))
        .bind(now_string())
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    load_recipient(&state, id).await
}

#[tauri::command]
pub async fn delete_recipient(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    load_recipient(&state, id).await?;
    let in_use: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM report_schedule_recipients WHERE recipient_id=?")
            .bind(id)
            .fetch_one(&state.db.pool)
            .await?;
    if in_use > 0 {
        return Err(AppError::new(
            "conflict",
            "Recipient is used by a report schedule and cannot be deleted",
        ));
    }
    sqlx::query("DELETE FROM recipients WHERE id=?")
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    Ok(())
}

async fn load_recipient(state: &AppState, id: i64) -> AppResult<Recipient> {
    sqlx::query_as::<_, Recipient>(
        "SELECT id, name, email, is_default, created_at, updated_at FROM recipients WHERE id=?",
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::not_found("Recipient"))
}

#[tauri::command]
pub async fn list_report_schedules(state: State<'_, AppState>) -> AppResult<Vec<ReportSchedule>> {
    crate::scheduler::list_schedules(&state.db.pool).await
}

#[tauri::command]
pub async fn update_report_schedule(
    state: State<'_, AppState>,
    report_type: String,
    payload: ReportScheduleInput,
) -> AppResult<ReportSchedule> {
    reports::validate_report_type(&report_type)?;
    let run_time = chrono::NaiveTime::parse_from_str(&payload.run_time, "%H:%M:%S%.f")
        .or_else(|_| chrono::NaiveTime::parse_from_str(&payload.run_time, "%H:%M"))
        .map_err(|_| AppError::validation("run_time", "run_time must use HH:MM format"))?
        .format("%H:%M:%S")
        .to_string();
    if report_type == "weekly_report" {
        let weekday = payload
            .weekday
            .as_deref()
            .and_then(reports::weekday_from_code)
            .ok_or_else(|| {
                AppError::validation("weekday", "A weekday is required for weekly reports")
            })?;
        let _ = weekday;
        if payload.day_of_month.is_some() {
            return Err(AppError::validation(
                "day_of_month",
                "Weekly schedules do not use day_of_month",
            ));
        }
    } else {
        if payload.weekday.is_some() {
            return Err(AppError::validation(
                "weekday",
                "Monthly schedules do not use weekday",
            ));
        }
        if payload
            .day_of_month
            .is_some_and(|day| !(1..=28).contains(&day))
        {
            return Err(AppError::validation(
                "day_of_month",
                "day_of_month must be between 1 and 28",
            ));
        }
    }
    if let Some(template_id) = payload.template_id {
        let template_type: Option<String> =
            sqlx::query_scalar("SELECT template_type FROM templates WHERE id=?")
                .bind(template_id)
                .fetch_optional(&state.db.pool)
                .await?;
        if template_type.as_deref() != Some(&report_type) {
            return Err(AppError::validation(
                "template_id",
                "Template not found for report type",
            ));
        }
    }
    if !payload.recipient_ids.is_empty() {
        let unique = payload
            .recipient_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let mut builder =
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM recipients WHERE id IN (");
        let mut separated = builder.separated(", ");
        for id in &unique {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        let count: i64 = builder
            .build_query_scalar()
            .fetch_one(&state.db.pool)
            .await?;
        if count as usize != unique.len() {
            return Err(AppError::validation(
                "recipient_ids",
                "One or more recipients no longer exist",
            ));
        }
    }
    if payload.auto_send {
        if payload.recipient_ids.is_empty() {
            return Err(AppError::validation(
                "recipient_ids",
                "Auto-send requires at least one recipient",
            ));
        }
        if mail::active_email_setting(&state.db.pool).await?.is_none() {
            return Err(AppError::validation(
                "auto_send",
                "Configure SMTP before enabling auto-send",
            ));
        }
    }
    let row = sqlx::query_as::<_, ReportScheduleRow>(
        "SELECT id, report_type, enabled, weekday, day_of_month, template_id, run_time, auto_send, created_at, updated_at \
         FROM report_schedules WHERE report_type=?",
    )
    .bind(&report_type)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::not_found("Report schedule"))?;
    let mut transaction = state.db.pool.begin().await?;
    sqlx::query(
        "UPDATE report_schedules SET enabled=?, weekday=?, day_of_month=?, template_id=?, run_time=?, auto_send=?, updated_at=? WHERE id=?",
    )
    .bind(payload.enabled)
    .bind(payload.weekday)
    .bind(payload.day_of_month)
    .bind(payload.template_id)
    .bind(run_time)
    .bind(payload.auto_send)
    .bind(now_string())
    .bind(row.id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DELETE FROM report_schedule_recipients WHERE schedule_id=?")
        .bind(row.id)
        .execute(&mut *transaction)
        .await?;
    for recipient_id in payload.recipient_ids {
        sqlx::query(
            "INSERT INTO report_schedule_recipients(schedule_id, recipient_id) VALUES(?, ?)",
        )
        .bind(row.id)
        .bind(recipient_id)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    state.scheduler_notify.notify_one();
    crate::scheduler::load_schedule(&state.db.pool, &report_type).await
}

#[tauri::command]
pub async fn get_desktop_preferences(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<DesktopPreferences> {
    let row =
        sqlx::query("SELECT legacy_database_path, migrated_at FROM desktop_preferences WHERE id=1")
            .fetch_one(&state.db.pool)
            .await?;
    let launch_at_login = app
        .autolaunch()
        .is_enabled()
        .map_err(|error| AppError::new("autostart_error", error.to_string()))?;
    Ok(DesktopPreferences {
        launch_at_login,
        database_path: state.db.path.display().to_string(),
        legacy_database_path: row.try_get("legacy_database_path")?,
        migrated_at: row.try_get("migrated_at")?,
    })
}

#[tauri::command]
pub async fn set_launch_at_login(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> AppResult<DesktopPreferences> {
    if enabled {
        app.autolaunch()
            .enable()
            .map_err(|error| AppError::new("autostart_error", error.to_string()))?;
    } else {
        app.autolaunch()
            .disable()
            .map_err(|error| AppError::new("autostart_error", error.to_string()))?;
    }
    sqlx::query("UPDATE desktop_preferences SET launch_at_login=?, updated_at=? WHERE id=1")
        .bind(enabled)
        .bind(now_string())
        .execute(&state.db.pool)
        .await?;
    get_desktop_preferences(app, state).await
}

#[tauri::command]
pub async fn get_startup_migration(state: State<'_, AppState>) -> AppResult<MigrationResult> {
    Ok(state.db.startup_migration.clone())
}

#[tauri::command]
pub async fn import_legacy_database(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<MigrationResult> {
    let result = state.db.import_legacy(&PathBuf::from(path)).await?;
    state.secrets.migrate_plaintext(&state.db.pool).await?;
    state.scheduler_notify.notify_one();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_work_log_fields() {
        assert!(validate_hours(Some(-1.0)).is_err());
        assert!(validate_hours(Some(24.0)).is_ok());
        assert_eq!(validate_priority("urgent").unwrap(), "urgent");
        assert!(validate_priority("unknown").is_err());
        assert!(require_text(Some("   ".into()), "project", Some(160)).is_err());
    }

    #[test]
    fn applies_provider_defaults() {
        assert_eq!(
            default_base_url("openai").unwrap(),
            "https://api.openai.com/v1"
        );
        assert!(default_base_url("unsupported").is_err());
    }
}
