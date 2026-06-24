from datetime import date, timedelta
from zoneinfo import ZoneInfo

from app.config import settings
from app.constants import ReportType


def app_timezone() -> ZoneInfo:
    return ZoneInfo(settings.timezone)


def week_period(anchor: date) -> tuple[date, date]:
    start = anchor - timedelta(days=anchor.weekday())
    return start, start + timedelta(days=6)


def month_period(anchor: date) -> tuple[date, date]:
    start = anchor.replace(day=1)
    next_month = (anchor.replace(day=28) + timedelta(days=4)).replace(day=1)
    return start, next_month - timedelta(days=1)


def resolve_period(
    report_type: ReportType,
    anchor_date: date | None = None,
    period_start: date | None = None,
    period_end: date | None = None,
) -> tuple[date, date]:
    if period_start and period_end:
        if period_end < period_start:
            raise ValueError("period_end must be on or after period_start")
        return period_start, period_end

    anchor = anchor_date or date.today()
    if report_type == ReportType.WEEKLY:
        return week_period(anchor)
    return month_period(anchor)
