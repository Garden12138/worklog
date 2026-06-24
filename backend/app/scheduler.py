from datetime import datetime

from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.cron import CronTrigger

from app.config import settings
from app.constants import ReportType
from app.database import SessionLocal
from app.services.periods import app_timezone
from app.services.reports import create_missing_report

scheduler: BackgroundScheduler | None = None


def _generate_weekly() -> None:
    with SessionLocal() as db:
        create_missing_report(db, ReportType.WEEKLY, datetime.now(app_timezone()).date())


def _generate_monthly_and_performance() -> None:
    today = datetime.now(app_timezone()).date()
    with SessionLocal() as db:
        create_missing_report(db, ReportType.MONTHLY, today)
        create_missing_report(db, ReportType.PERFORMANCE, today)


def start_scheduler() -> None:
    global scheduler
    if not settings.enable_scheduler or scheduler:
        return
    tz = app_timezone()
    scheduler = BackgroundScheduler(timezone=tz)
    scheduler.add_job(
        _generate_weekly,
        CronTrigger(day_of_week="sun", hour=20, minute=0, timezone=tz),
        id="weekly-report",
        replace_existing=True,
    )
    scheduler.add_job(
        _generate_monthly_and_performance,
        CronTrigger(day="last", hour=20, minute=30, timezone=tz),
        id="monthly-and-performance-report",
        replace_existing=True,
    )
    scheduler.start()


def stop_scheduler() -> None:
    global scheduler
    if scheduler:
        scheduler.shutdown(wait=False)
        scheduler = None
