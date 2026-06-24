from datetime import date

from app.constants import ReportType
from app.services.periods import month_period, resolve_period, week_period


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
