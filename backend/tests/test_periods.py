from datetime import date, datetime, timezone

from app.constants import ReportType
from app.services.periods import format_local_datetime, month_period, resolve_period, week_period


def test_week_period_uses_monday_to_sunday():
    assert week_period(date(2026, 6, 23)) == (date(2026, 6, 22), date(2026, 6, 28))


def test_month_period_uses_calendar_month():
    assert month_period(date(2026, 2, 10)) == (date(2026, 2, 1), date(2026, 2, 28))


def test_resolve_explicit_period_wins():
    assert resolve_period(
        ReportType.WEEKLY,
        period_start=date(2026, 6, 1),
        period_end=date(2026, 6, 3),
    ) == (date(2026, 6, 1), date(2026, 6, 3))


def test_format_local_datetime_uses_application_timezone():
    value = datetime(2026, 6, 29, 2, 26, 59, tzinfo=timezone.utc)

    assert format_local_datetime(value) == "2026-06-29 10:26:59"
