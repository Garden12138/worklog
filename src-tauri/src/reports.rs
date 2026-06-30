use crate::db::now_string;
use crate::error::{AppError, AppResult};
use crate::llm::{active_config, fill_template, generate_report};
use crate::models::{GenerateResponse, Report, ReportGenerateInput, ReportRow, Template, WorkLog};
use crate::secrets::SecretStore;
use crate::templates::{
    build_context, render_template, report_title, requires_llm_fill, validate_template,
};
use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type ActiveGenerations = Arc<Mutex<HashSet<String>>>;

pub async fn list_reports(pool: &SqlitePool) -> AppResult<Vec<Report>> {
    Ok(sqlx::query_as::<_, ReportRow>(
        "SELECT id, report_type, title, period_start, period_end, template_id, content_markdown, status, source_log_ids, generated_at, edited_at, created_at, updated_at \
         FROM reports ORDER BY period_end DESC, id DESC",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(Into::into)
    .collect())
}

pub async fn load_report(pool: &SqlitePool, id: i64) -> AppResult<Report> {
    let row = sqlx::query_as::<_, ReportRow>(
        "SELECT id, report_type, title, period_start, period_end, template_id, content_markdown, status, source_log_ids, generated_at, edited_at, created_at, updated_at \
         FROM reports WHERE id=?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("Report"))?;
    Ok(row.into())
}

pub async fn generate(
    pool: &SqlitePool,
    secrets: &SecretStore,
    active: &ActiveGenerations,
    input: ReportGenerateInput,
) -> AppResult<GenerateResponse> {
    validate_report_type(&input.report_type)?;
    let (period_start, period_end) = resolve_period(
        &input.report_type,
        input.anchor_date.as_deref(),
        input.period_start.as_deref(),
        input.period_end.as_deref(),
    )?;
    let key = format!("{}:{period_start}:{period_end}", input.report_type);
    {
        let mut running = active.lock().await;
        if !running.insert(key.clone()) {
            return Err(AppError::new(
                "generation_in_progress",
                "Report generation is already running for this period",
            ));
        }
    }
    let result = generate_inner(
        pool,
        secrets,
        &input.report_type,
        &period_start,
        &period_end,
        input.template_id,
        input.overwrite,
    )
    .await;
    active.lock().await.remove(&key);
    result
}

async fn generate_inner(
    pool: &SqlitePool,
    secrets: &SecretStore,
    report_type: &str,
    period_start: &str,
    period_end: &str,
    template_id: Option<i64>,
    overwrite: bool,
) -> AppResult<GenerateResponse> {
    let existing = sqlx::query_as::<_, ReportRow>(
        "SELECT id, report_type, title, period_start, period_end, template_id, content_markdown, status, source_log_ids, generated_at, edited_at, created_at, updated_at \
         FROM reports WHERE report_type=? AND period_start=? AND period_end=?",
    )
    .bind(report_type)
    .bind(period_start)
    .bind(period_end)
    .fetch_optional(pool)
    .await?;
    if let Some(existing) = existing.as_ref().filter(|_| !overwrite) {
        let now = now_string();
        let task_id = sqlx::query(
            "INSERT INTO generation_tasks(report_type, period_start, period_end, status, message, report_id, completed_at, created_at, updated_at) \
             VALUES(?, ?, ?, 'skipped', 'Report already exists; existing draft was not overwritten.', ?, ?, ?, ?)",
        )
        .bind(report_type)
        .bind(period_start)
        .bind(period_end)
        .bind(existing.id)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?
        .last_insert_rowid();
        return Ok(GenerateResponse {
            report: existing.clone().into(),
            task_id,
            used_llm: false,
        });
    }

    let now = now_string();
    let task_id = sqlx::query(
        "INSERT INTO generation_tasks(report_type, period_start, period_end, status, created_at, updated_at) \
         VALUES(?, ?, ?, 'pending', ?, ?)",
    )
    .bind(report_type)
    .bind(period_start)
    .bind(period_end)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?
    .last_insert_rowid();

    let result: AppResult<GenerateResponse> = async {
        let template = get_template(pool, report_type, template_id).await?;
        validate_template(&template.content)?;
        let logs = sqlx::query_as::<_, WorkLog>(
            "SELECT id, work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, notes, created_at, updated_at \
             FROM work_logs WHERE end_date >= ? AND start_date <= ? ORDER BY start_date, end_date, id",
        )
        .bind(period_start)
        .bind(period_end)
        .fetch_all(pool)
        .await?;
        let config = active_config(pool, secrets).await?;
        let (content, used_llm, title) = if requires_llm_fill(&template.content) {
            let generated = fill_template(
                config.as_ref(),
                report_type,
                period_start,
                period_end,
                &logs,
                &template.content,
            )
            .await?;
            (
                generated.content,
                generated.used_llm,
                format!("{} 至 {} {}", period_start, period_end, report_title(report_type)),
            )
        } else {
            let generated = generate_report(
                config.as_ref(),
                report_type,
                period_start,
                period_end,
                &logs,
            )
            .await?;
            let context = build_context(
                report_type,
                period_start,
                period_end,
                &logs,
                generated.content,
                local_datetime(),
            );
            let content = render_template(&template.content, &context)?;
            (content, generated.used_llm, context.title)
        };
        let source_ids = serde_json::to_string(&logs.iter().map(|item| item.id).collect::<Vec<_>>())?;
        let timestamp = now_string();
        let report_id = if let Some(existing) = existing {
            sqlx::query(
                "UPDATE reports SET title=?, template_id=?, content_markdown=?, status='draft', source_log_ids=?, generated_at=?, updated_at=? WHERE id=?",
            )
            .bind(&title)
            .bind(template.id)
            .bind(&content)
            .bind(&source_ids)
            .bind(&timestamp)
            .bind(&timestamp)
            .bind(existing.id)
            .execute(pool)
            .await?;
            existing.id
        } else {
            sqlx::query(
                "INSERT INTO reports(report_type, title, period_start, period_end, template_id, content_markdown, status, source_log_ids, generated_at, created_at, updated_at) \
                 VALUES(?, ?, ?, ?, ?, ?, 'draft', ?, ?, ?, ?)",
            )
            .bind(report_type)
            .bind(&title)
            .bind(period_start)
            .bind(period_end)
            .bind(template.id)
            .bind(&content)
            .bind(&source_ids)
            .bind(&timestamp)
            .bind(&timestamp)
            .bind(&timestamp)
            .execute(pool)
            .await?
            .last_insert_rowid()
        };
        sqlx::query(
            "UPDATE generation_tasks SET status='success', report_id=?, completed_at=?, updated_at=? WHERE id=?",
        )
        .bind(report_id)
        .bind(&timestamp)
        .bind(&timestamp)
        .bind(task_id)
        .execute(pool)
        .await?;
        Ok(GenerateResponse {
            report: load_report(pool, report_id).await?,
            task_id,
            used_llm,
        })
    }
    .await;

    if let Err(error) = &result {
        let now = now_string();
        let _ = sqlx::query(
            "UPDATE generation_tasks SET status='failed', message=?, completed_at=?, updated_at=? WHERE id=?",
        )
        .bind(error.message.chars().take(1000).collect::<String>())
        .bind(&now)
        .bind(&now)
        .bind(task_id)
        .execute(pool)
        .await;
    }
    result
}

async fn get_template(
    pool: &SqlitePool,
    report_type: &str,
    template_id: Option<i64>,
) -> AppResult<Template> {
    let template = if let Some(id) = template_id {
        sqlx::query_as::<_, Template>(
            "SELECT id, name, template_type, content, is_default, created_at, updated_at FROM templates WHERE id=? AND template_type=?",
        )
        .bind(id)
        .bind(report_type)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_as::<_, Template>(
            "SELECT id, name, template_type, content, is_default, created_at, updated_at FROM templates WHERE template_type=? ORDER BY is_default DESC, updated_at DESC LIMIT 1",
        )
        .bind(report_type)
        .fetch_optional(pool)
        .await?
    };
    template
        .ok_or_else(|| AppError::new("template_not_found", "No template exists for report type"))
}

pub fn resolve_period(
    report_type: &str,
    anchor: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
) -> AppResult<(String, String)> {
    validate_report_type(report_type)?;
    if let (Some(start), Some(end)) = (start, end) {
        let start_date = parse_date(start, "period_start")?;
        let end_date = parse_date(end, "period_end")?;
        if end_date < start_date {
            return Err(AppError::validation(
                "period_end",
                "period_end must be on or after period_start",
            ));
        }
        return Ok((start.into(), end.into()));
    }
    let anchor = anchor
        .map(|value| parse_date(value, "anchor_date"))
        .transpose()?
        .unwrap_or_else(|| Local::now().date_naive());
    let (start, end) = if report_type == "weekly_report" {
        let offset = anchor.weekday().num_days_from_monday() as i64;
        let start = anchor - Duration::days(offset);
        (start, start + Duration::days(6))
    } else {
        let start = anchor.with_day(1).unwrap();
        let next_month = if anchor.month() == 12 {
            NaiveDate::from_ymd_opt(anchor.year() + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(anchor.year(), anchor.month() + 1, 1).unwrap()
        };
        (start, next_month - Duration::days(1))
    };
    Ok((start.to_string(), end.to_string()))
}

pub fn scheduled_period(report_type: &str, occurrence: NaiveDate) -> AppResult<(String, String)> {
    validate_report_type(report_type)?;
    if report_type == "weekly_report" {
        let start = occurrence - Duration::days(occurrence.weekday().num_days_from_monday() as i64);
        Ok((start.to_string(), occurrence.to_string()))
    } else {
        Ok((
            occurrence.with_day(1).unwrap().to_string(),
            occurrence.to_string(),
        ))
    }
}

pub fn validate_report_type(value: &str) -> AppResult<()> {
    match value {
        "weekly_report" | "monthly_report" | "performance_review" => Ok(()),
        _ => Err(AppError::validation(
            "report_type",
            "Unsupported report type",
        )),
    }
}

pub fn parse_date(value: &str, field: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::validation(field, "Date must use YYYY-MM-DD format"))
}

fn local_datetime() -> String {
    chrono::Utc::now()
        .with_timezone(&chrono_tz::Asia::Shanghai)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub fn weekday_from_code(value: &str) -> Option<Weekday> {
    match value {
        "mon" => Some(Weekday::Mon),
        "tue" => Some(Weekday::Tue),
        "wed" => Some(Weekday::Wed),
        "thu" => Some(Weekday::Thu),
        "fri" => Some(Weekday::Fri),
        "sat" => Some(Weekday::Sat),
        "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{now_string, Database};
    use crate::secrets::SecretStore;
    use std::collections::HashSet;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[test]
    fn resolves_week_and_month_periods() {
        assert_eq!(
            resolve_period("weekly_report", Some("2026-06-26"), None, None).unwrap(),
            ("2026-06-22".into(), "2026-06-28".into())
        );
        assert_eq!(
            resolve_period("monthly_report", Some("2026-06-12"), None, None).unwrap(),
            ("2026-06-01".into(), "2026-06-30".into())
        );
    }

    #[test]
    fn scheduled_week_ends_on_occurrence() {
        assert_eq!(
            scheduled_period(
                "weekly_report",
                NaiveDate::from_ymd_opt(2026, 6, 26).unwrap()
            )
            .unwrap(),
            ("2026-06-22".into(), "2026-06-26".into())
        );
    }

    #[tokio::test]
    async fn generates_a_fallback_report_without_an_llm_key() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "worklog-report-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let database = Database::open_for_test(directory, None).await.unwrap();
        let now = now_string();
        sqlx::query(
            "INSERT INTO work_logs(work_date, start_date, end_date, project, task, progress, result, hours, priority, created_at, updated_at) \
             VALUES('2026-06-23', '2026-06-23', '2026-06-23', 'Worklog', 'Rust desktop', '完成迁移', '可本地运行', 4, 'high', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&database.pool)
        .await
        .unwrap();
        let generated = generate(
            &database.pool,
            &SecretStore::memory(),
            &Arc::new(Mutex::new(HashSet::new())),
            ReportGenerateInput {
                report_type: "monthly_report".into(),
                anchor_date: Some("2026-06-23".into()),
                period_start: None,
                period_end: None,
                template_id: None,
                overwrite: false,
            },
        )
        .await
        .unwrap();
        assert!(!generated.used_llm);
        assert!(generated.report.content_markdown.contains("Rust desktop"));
        assert_eq!(generated.report.period_start, "2026-06-01");
        assert_eq!(generated.report.period_end, "2026-06-30");

        sqlx::query("UPDATE reports SET content_markdown='# 手工修改后不可覆盖' WHERE id=?")
            .bind(generated.report.id)
            .execute(&database.pool)
            .await
            .unwrap();
        let duplicate = generate(
            &database.pool,
            &SecretStore::memory(),
            &Arc::new(Mutex::new(HashSet::new())),
            ReportGenerateInput {
                report_type: "monthly_report".into(),
                anchor_date: Some("2026-06-23".into()),
                period_start: None,
                period_end: None,
                template_id: None,
                overwrite: false,
            },
        )
        .await
        .unwrap();
        let task_status: String =
            sqlx::query_scalar("SELECT status FROM generation_tasks WHERE id=?")
                .bind(duplicate.task_id)
                .fetch_one(&database.pool)
                .await
                .unwrap();
        assert_eq!(task_status, "skipped");
        assert_eq!(duplicate.report.content_markdown, "# 手工修改后不可覆盖");
    }
}
