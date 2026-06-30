use crate::error::{AppError, AppResult};
use crate::mail;
use crate::models::{ReportGenerateInput, ReportSchedule, ReportScheduleRow};
use crate::reports::{self, ActiveGenerations};
use crate::secrets::SecretStore;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::{Asia::Shanghai, Tz};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Notify;

pub async fn list_schedules(pool: &SqlitePool) -> AppResult<Vec<ReportSchedule>> {
    let rows = sqlx::query_as::<_, ReportScheduleRow>(
        "SELECT id, report_type, enabled, weekday, day_of_month, template_id, run_time, auto_send, created_at, updated_at \
         FROM report_schedules ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        result.push(expand_schedule(pool, row).await?);
    }
    Ok(result)
}

pub async fn load_schedule(pool: &SqlitePool, report_type: &str) -> AppResult<ReportSchedule> {
    let row = sqlx::query_as::<_, ReportScheduleRow>(
        "SELECT id, report_type, enabled, weekday, day_of_month, template_id, run_time, auto_send, created_at, updated_at \
         FROM report_schedules WHERE report_type=?",
    )
    .bind(report_type)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("Report schedule"))?;
    expand_schedule(pool, row).await
}

async fn expand_schedule(pool: &SqlitePool, row: ReportScheduleRow) -> AppResult<ReportSchedule> {
    let recipient_ids = sqlx::query_scalar::<_, i64>(
        "SELECT recipient_id FROM report_schedule_recipients WHERE schedule_id=? ORDER BY recipient_id",
    )
    .bind(row.id)
    .fetch_all(pool)
    .await?;
    let next_run_at = if row.enabled {
        Some(next_occurrence(&row, Utc::now().with_timezone(&Shanghai))?.to_rfc3339())
    } else {
        None
    };
    Ok(ReportSchedule {
        id: row.id,
        report_type: row.report_type,
        enabled: row.enabled,
        weekday: row.weekday,
        day_of_month: row.day_of_month,
        template_id: row.template_id,
        run_time: row.run_time,
        auto_send: row.auto_send,
        recipient_ids,
        next_run_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub fn start(
    pool: SqlitePool,
    secrets: SecretStore,
    active: ActiveGenerations,
    notify: Arc<Notify>,
) {
    tauri::async_runtime::spawn(async move {
        run_catchups(&pool, &secrets, &active).await;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                    run_due(&pool, &secrets, &active).await;
                    run_catchups(&pool, &secrets, &active).await;
                }
                _ = notify.notified() => {
                    run_catchups(&pool, &secrets, &active).await;
                }
            }
        }
    });
}

async fn run_due(pool: &SqlitePool, secrets: &SecretStore, active: &ActiveGenerations) {
    let Ok(rows) = sqlx::query_as::<_, ReportScheduleRow>(
        "SELECT id, report_type, enabled, weekday, day_of_month, template_id, run_time, auto_send, created_at, updated_at \
         FROM report_schedules WHERE enabled=1 ORDER BY id",
    )
    .fetch_all(pool)
    .await else {
        return;
    };
    let now = Utc::now().with_timezone(&Shanghai);
    for row in rows {
        if is_due(&row, now) {
            execute(pool, secrets, active, &row, now.date_naive(), true).await;
        }
    }
}

async fn run_catchups(pool: &SqlitePool, secrets: &SecretStore, active: &ActiveGenerations) {
    let Ok(rows) = sqlx::query_as::<_, ReportScheduleRow>(
        "SELECT id, report_type, enabled, weekday, day_of_month, template_id, run_time, auto_send, created_at, updated_at \
         FROM report_schedules WHERE enabled=1 ORDER BY id",
    )
    .fetch_all(pool)
    .await else {
        return;
    };
    let now = Utc::now().with_timezone(&Shanghai);
    for row in rows {
        let Ok(occurrence) = latest_occurrence(&row, now) else {
            continue;
        };
        let updated_at = DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|value| value.with_timezone(&Shanghai))
            .unwrap_or(now - Duration::days(3650));
        if occurrence < updated_at {
            continue;
        }
        let Ok((start, end)) = reports::scheduled_period(&row.report_type, occurrence.date_naive())
        else {
            continue;
        };
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM reports WHERE report_type=? AND period_start=? AND period_end=?",
        )
        .bind(&row.report_type)
        .bind(start)
        .bind(end)
        .fetch_one(pool)
        .await
        .unwrap_or(1);
        if exists == 0 {
            execute(pool, secrets, active, &row, occurrence.date_naive(), false).await;
        }
    }
}

async fn execute(
    pool: &SqlitePool,
    secrets: &SecretStore,
    active: &ActiveGenerations,
    schedule: &ReportScheduleRow,
    occurrence: NaiveDate,
    allow_email: bool,
) {
    let Ok((start, end)) = reports::scheduled_period(&schedule.report_type, occurrence) else {
        return;
    };
    let generated = reports::generate(
        pool,
        secrets,
        active,
        ReportGenerateInput {
            report_type: schedule.report_type.clone(),
            anchor_date: None,
            period_start: Some(start),
            period_end: Some(end),
            template_id: schedule.template_id,
            overwrite: false,
        },
    )
    .await;
    let Ok(generated) = generated else {
        return;
    };
    let status: String = sqlx::query_scalar("SELECT status FROM generation_tasks WHERE id=?")
        .bind(generated.task_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| "failed".into());
    if status != "success" || !allow_email || !schedule.auto_send {
        return;
    }
    let recipient_ids = sqlx::query_scalar::<_, i64>(
        "SELECT recipient_id FROM report_schedule_recipients WHERE schedule_id=? ORDER BY recipient_id",
    )
    .bind(schedule.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let _ = mail::deliver_report(
        pool,
        secrets,
        &generated.report,
        &recipient_ids,
        &[],
        &generated.report.title,
    )
    .await;
}

fn is_due(schedule: &ReportScheduleRow, now: DateTime<Tz>) -> bool {
    let Ok(time) = parse_time(&schedule.run_time) else {
        return false;
    };
    if now.hour() != time.hour() || now.minute() != time.minute() {
        return false;
    }
    if schedule.report_type == "weekly_report" {
        return schedule
            .weekday
            .as_deref()
            .and_then(reports::weekday_from_code)
            == Some(now.weekday());
    }
    schedule
        .day_of_month
        .map(|day| day as u32)
        .unwrap_or_else(|| last_day(now.year(), now.month()))
        == now.day()
}

fn next_occurrence(schedule: &ReportScheduleRow, now: DateTime<Tz>) -> AppResult<DateTime<Tz>> {
    let time = parse_time(&schedule.run_time)?;
    if schedule.report_type == "weekly_report" {
        let weekday = schedule
            .weekday
            .as_deref()
            .and_then(reports::weekday_from_code)
            .ok_or_else(|| AppError::validation("weekday", "Invalid weekday"))?;
        for offset in 0..=7 {
            let date = now.date_naive() + Duration::days(offset);
            if date.weekday() == weekday {
                let candidate = local_datetime(date, time)?;
                if candidate > now {
                    return Ok(candidate);
                }
            }
        }
    } else {
        for month_offset in 0..=1 {
            let (year, month) = add_month(now.year(), now.month(), month_offset);
            let day = schedule
                .day_of_month
                .map(|value| value as u32)
                .unwrap_or_else(|| last_day(year, month));
            let date = NaiveDate::from_ymd_opt(year, month, day)
                .ok_or_else(|| AppError::validation("day_of_month", "Invalid schedule date"))?;
            let candidate = local_datetime(date, time)?;
            if candidate > now {
                return Ok(candidate);
            }
        }
    }
    Err(AppError::new(
        "schedule_error",
        "Unable to compute next schedule occurrence",
    ))
}

fn latest_occurrence(schedule: &ReportScheduleRow, now: DateTime<Tz>) -> AppResult<DateTime<Tz>> {
    let time = parse_time(&schedule.run_time)?;
    if schedule.report_type == "weekly_report" {
        let weekday = schedule
            .weekday
            .as_deref()
            .and_then(reports::weekday_from_code)
            .ok_or_else(|| AppError::validation("weekday", "Invalid weekday"))?;
        for offset in 0..=7 {
            let date = now.date_naive() - Duration::days(offset);
            if date.weekday() == weekday {
                let candidate = local_datetime(date, time)?;
                if candidate <= now {
                    return Ok(candidate);
                }
            }
        }
    } else {
        for month_offset in 0..=1 {
            let (year, month) = subtract_month(now.year(), now.month(), month_offset);
            let day = schedule
                .day_of_month
                .map(|value| value as u32)
                .unwrap_or_else(|| last_day(year, month));
            let candidate =
                local_datetime(NaiveDate::from_ymd_opt(year, month, day).unwrap(), time)?;
            if candidate <= now {
                return Ok(candidate);
            }
        }
    }
    Err(AppError::new(
        "schedule_error",
        "Unable to compute latest schedule occurrence",
    ))
}

fn parse_time(value: &str) -> AppResult<NaiveTime> {
    NaiveTime::parse_from_str(value, "%H:%M:%S%.f")
        .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M"))
        .map_err(|_| AppError::validation("run_time", "Invalid run time"))
}

fn local_datetime(date: NaiveDate, time: NaiveTime) -> AppResult<DateTime<Tz>> {
    Shanghai
        .from_local_datetime(&date.and_time(time))
        .single()
        .ok_or_else(|| AppError::new("schedule_error", "Invalid local schedule time"))
}

fn last_day(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    (NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap() - Duration::days(1)).day()
}

fn add_month(year: i32, month: u32, offset: i32) -> (i32, u32) {
    let zero = year * 12 + month as i32 - 1 + offset;
    (zero.div_euclid(12), zero.rem_euclid(12) as u32 + 1)
}

fn subtract_month(year: i32, month: u32, offset: i32) -> (i32, u32) {
    add_month(year, month, -offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weekly() -> ReportScheduleRow {
        ReportScheduleRow {
            id: 1,
            report_type: "weekly_report".into(),
            enabled: true,
            weekday: Some("fri".into()),
            day_of_month: None,
            template_id: None,
            run_time: "15:00:00".into(),
            auto_send: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn computes_next_weekly_occurrence() {
        let now = Shanghai.with_ymd_and_hms(2026, 6, 25, 16, 0, 0).unwrap();
        let next = next_occurrence(&weekly(), now).unwrap();
        assert_eq!(next.to_rfc3339(), "2026-06-26T15:00:00+08:00");
    }

    #[test]
    fn computes_last_day_monthly_occurrence() {
        let mut schedule = weekly();
        schedule.report_type = "monthly_report".into();
        schedule.weekday = None;
        let now = Shanghai.with_ymd_and_hms(2026, 6, 12, 16, 0, 0).unwrap();
        let next = next_occurrence(&schedule, now).unwrap();
        assert_eq!(next.to_rfc3339(), "2026-06-30T15:00:00+08:00");
    }
}
