import calendar
from datetime import date, datetime, time, timedelta, timezone

from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.cron import CronTrigger
from sqlalchemy import select
from sqlalchemy.orm import Session

from app.config import settings
from app.constants import ReportType, TaskStatus
from app.database import SessionLocal
from app.models import Report, ReportSchedule
from app.services.email import (
    EmailConfigurationError,
    EmailDeliveryError,
    RecipientSelectionError,
    deliver_report_email,
)
from app.services.periods import app_timezone
from app.services.reports import create_report


DEFAULT_REPORT_SCHEDULES = {
    ReportType.WEEKLY: {"weekday": "fri", "day_of_month": None, "run_time": time(15, 0)},
    ReportType.MONTHLY: {"weekday": None, "day_of_month": None, "run_time": time(15, 0)},
    ReportType.PERFORMANCE: {"weekday": None, "day_of_month": None, "run_time": time(15, 0)},
}
WEEKDAYS = {"mon": 0, "tue": 1, "wed": 2, "thu": 3, "fri": 4, "sat": 5, "sun": 6}

scheduler: BackgroundScheduler | None = None


def seed_default_report_schedules(db: Session) -> None:
    for report_type, defaults in DEFAULT_REPORT_SCHEDULES.items():
        existing = db.scalar(
            select(ReportSchedule).where(ReportSchedule.report_type == report_type.value)
        )
        if existing:
            continue
        db.add(
            ReportSchedule(
                report_type=report_type.value,
                enabled=True,
                weekday=defaults["weekday"],
                day_of_month=defaults["day_of_month"],
                run_time=defaults["run_time"],
                auto_send=False,
            )
        )
    db.commit()


def scheduled_report_period(report_type: ReportType, occurrence_date: date) -> tuple[date, date]:
    if report_type == ReportType.WEEKLY:
        return occurrence_date - timedelta(days=occurrence_date.weekday()), occurrence_date
    return occurrence_date.replace(day=1), occurrence_date


def schedule_trigger(schedule: ReportSchedule) -> CronTrigger:
    trigger_kwargs = {
        "hour": schedule.run_time.hour,
        "minute": schedule.run_time.minute,
        "timezone": app_timezone(),
    }
    if schedule.report_type == ReportType.WEEKLY.value:
        return CronTrigger(day_of_week=schedule.weekday, **trigger_kwargs)
    return CronTrigger(day=schedule.day_of_month or "last", **trigger_kwargs)


def next_run_at(schedule: ReportSchedule, now: datetime | None = None) -> datetime | None:
    reference = now or datetime.now(app_timezone())
    return schedule_trigger(schedule).get_next_fire_time(None, reference)


def _run_report_schedule(
    report_type_value: str,
    allow_email: bool = True,
    occurrence_date_value: str | None = None,
) -> None:
    report_type = ReportType(report_type_value)
    with SessionLocal() as db:
        schedule = db.scalar(
            select(ReportSchedule).where(ReportSchedule.report_type == report_type.value)
        )
        if not schedule or not schedule.enabled:
            return
        occurrence_date = (
            date.fromisoformat(occurrence_date_value)
            if occurrence_date_value
            else datetime.now(app_timezone()).date()
        )
        period_start, period_end = scheduled_report_period(report_type, occurrence_date)
        report, task, _ = create_report(
            db,
            report_type=report_type,
            period_start=period_start,
            period_end=period_end,
            template_id=schedule.template_id,
            overwrite=False,
        )
        if task.status != TaskStatus.SUCCESS.value or not allow_email or not schedule.auto_send:
            return
        recipient_ids = [recipient.id for recipient in schedule.recipients]
        try:
            deliver_report_email(db, report, recipient_ids, report.title)
        except (EmailConfigurationError, RecipientSelectionError, EmailDeliveryError):
            # Delivery failures are persisted by the shared service and are intentionally not retried.
            return


def _latest_scheduled_occurrence(
    schedule: ReportSchedule,
    now: datetime,
) -> datetime:
    local_now = now.astimezone(app_timezone())
    if schedule.report_type == ReportType.WEEKLY.value:
        days_since = (local_now.weekday() - WEEKDAYS[schedule.weekday or "fri"]) % 7
        candidate_date = local_now.date() - timedelta(days=days_since)
        candidate = datetime.combine(candidate_date, schedule.run_time, tzinfo=app_timezone())
        if candidate > local_now:
            candidate -= timedelta(days=7)
        return candidate

    day = schedule.day_of_month or calendar.monthrange(local_now.year, local_now.month)[1]
    candidate_date = date(local_now.year, local_now.month, day)
    candidate = datetime.combine(candidate_date, schedule.run_time, tzinfo=app_timezone())
    if candidate <= local_now:
        return candidate
    previous_month_end = candidate_date.replace(day=1) - timedelta(days=1)
    previous_day = schedule.day_of_month or previous_month_end.day
    return datetime.combine(
        previous_month_end.replace(day=previous_day),
        schedule.run_time,
        tzinfo=app_timezone(),
    )


def catch_up_occurrence(
    db: Session,
    schedule: ReportSchedule,
    now: datetime | None = None,
) -> date | None:
    if not schedule.enabled:
        return None
    reference = now or datetime.now(app_timezone())
    occurrence = _latest_scheduled_occurrence(schedule, reference)
    effective_at = schedule.updated_at
    if effective_at.tzinfo is None:
        effective_at = effective_at.replace(tzinfo=timezone.utc)
    if occurrence < effective_at.astimezone(app_timezone()):
        return None
    report_type = ReportType(schedule.report_type)
    period_start, period_end = scheduled_report_period(report_type, occurrence.date())
    existing = db.scalar(
        select(Report).where(
            Report.report_type == report_type.value,
            Report.period_start == period_start,
            Report.period_end == period_end,
        )
    )
    return None if existing else occurrence.date()


def _job_id(report_type_value: str) -> str:
    return f"report-schedule-{report_type_value}"


def _configure_job(schedule: ReportSchedule) -> None:
    if not scheduler:
        return
    job_id = _job_id(schedule.report_type)
    if not schedule.enabled:
        if scheduler.get_job(job_id):
            scheduler.remove_job(job_id)
        return
    scheduler.add_job(
        _run_report_schedule,
        schedule_trigger(schedule),
        args=[schedule.report_type],
        id=job_id,
        replace_existing=True,
        coalesce=True,
        max_instances=1,
        misfire_grace_time=60,
    )


def sync_report_schedule(report_type_value: str) -> None:
    if not scheduler:
        return
    with SessionLocal() as db:
        schedule = db.scalar(
            select(ReportSchedule).where(ReportSchedule.report_type == report_type_value)
        )
        if schedule:
            _configure_job(schedule)


def start_scheduler() -> None:
    global scheduler
    if not settings.enable_scheduler or scheduler:
        return
    scheduler = BackgroundScheduler(timezone=app_timezone())
    catch_ups: list[tuple[str, date]] = []
    with SessionLocal() as db:
        schedules = list(db.scalars(select(ReportSchedule).order_by(ReportSchedule.id.asc())))
        for schedule in schedules:
            _configure_job(schedule)
            occurrence = catch_up_occurrence(db, schedule)
            if occurrence:
                catch_ups.append((schedule.report_type, occurrence))
    scheduler.start()
    for report_type_value, occurrence in catch_ups:
        scheduler.add_job(
            _run_report_schedule,
            trigger="date",
            args=[report_type_value, False, occurrence.isoformat()],
            id=f"report-schedule-catch-up-{report_type_value}",
            replace_existing=True,
        )


def stop_scheduler() -> None:
    global scheduler
    if scheduler:
        scheduler.shutdown(wait=False)
        scheduler = None
