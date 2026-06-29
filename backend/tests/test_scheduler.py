from datetime import date, datetime, timezone

from sqlalchemy import select

from app.constants import ReportType
from app.models import Report, ReportEmailDelivery, ReportSchedule
from app.scheduler import _run_report_schedule, catch_up_occurrence, scheduled_report_period
from app.services.email import EmailDeliveryError


def configure_email(client) -> None:
    response = client.put(
        "/api/settings/email",
        json={
            "host": "smtp.example.test",
            "port": 587,
            "security": "starttls",
            "username": "sender@example.test",
            "password": "smtp-app-password",
            "sender_address": "sender@example.test",
            "sender_name": "Worklog",
        },
    )
    assert response.status_code == 200


def add_recipient(client, email: str = "manager@example.test") -> int:
    response = client.post(
        "/api/recipients",
        json={"name": "直属上级", "email": email, "is_default": True},
    )
    assert response.status_code == 201
    return response.json()["id"]


def update_schedule(client, report_type: str, **overrides):
    schedule = next(
        item
        for item in client.get("/api/settings/report-schedules").json()
        if item["report_type"] == report_type
    )
    payload = {
        "enabled": schedule["enabled"],
        "weekday": schedule["weekday"],
        "day_of_month": schedule["day_of_month"],
        "template_id": schedule["template_id"],
        "run_time": schedule["run_time"],
        "auto_send": schedule["auto_send"],
        "recipient_ids": schedule["recipient_ids"],
        **overrides,
    }
    return client.put(f"/api/settings/report-schedules/{report_type}", json=payload)


def test_default_report_schedules_and_periods(client):
    response = client.get("/api/settings/report-schedules")

    assert response.status_code == 200
    schedules = response.json()
    assert [item["report_type"] for item in schedules] == [
        "weekly_report",
        "monthly_report",
        "performance_review",
    ]
    assert schedules[0]["enabled"] is True
    assert schedules[0]["weekday"] == "fri"
    assert schedules[0]["run_time"] == "15:00:00"
    assert schedules[0]["template_id"] is None
    assert schedules[1]["day_of_month"] is None
    assert schedules[1]["run_time"] == "15:00:00"
    assert schedules[2]["run_time"] == "15:00:00"
    assert all(item["auto_send"] is False for item in schedules)
    assert all(item["next_run_at"] for item in schedules)

    assert scheduled_report_period(ReportType.WEEKLY, date(2026, 6, 26)) == (
        date(2026, 6, 22),
        date(2026, 6, 26),
    )
    assert scheduled_report_period(ReportType.MONTHLY, date(2026, 6, 12)) == (
        date(2026, 6, 1),
        date(2026, 6, 12),
    )


def test_schedule_validation_resync_and_recipient_delete_guard(client, monkeypatch):
    synced = []
    monkeypatch.setattr("app.main.sync_report_schedule", lambda report_type: synced.append(report_type))

    updated = update_schedule(
        client,
        "weekly_report",
        weekday="wed",
        run_time="16:45",
        enabled=False,
    )
    assert updated.status_code == 200
    assert updated.json()["weekday"] == "wed"
    assert updated.json()["run_time"] == "16:45:00"
    assert updated.json()["next_run_at"] is None
    assert synced == ["weekly_report"]

    invalid_weekly = update_schedule(client, "weekly_report", weekday=None)
    assert invalid_weekly.status_code == 422
    invalid_monthly = update_schedule(client, "monthly_report", weekday="fri")
    assert invalid_monthly.status_code == 422
    invalid_month_day = update_schedule(client, "monthly_report", day_of_month=29)
    assert invalid_month_day.status_code == 422
    monthly_template = next(
        item
        for item in client.get("/api/templates").json()
        if item["template_type"] == "monthly_report"
    )
    invalid_template = update_schedule(
        client,
        "weekly_report",
        template_id=monthly_template["id"],
    )
    assert invalid_template.status_code == 422

    no_smtp = update_schedule(client, "monthly_report", auto_send=True)
    assert no_smtp.status_code == 422
    configure_email(client)
    no_recipient = update_schedule(client, "monthly_report", auto_send=True)
    assert no_recipient.status_code == 422

    recipient_id = add_recipient(client)
    enabled = update_schedule(
        client,
        "monthly_report",
        auto_send=True,
        recipient_ids=[recipient_id],
        day_of_month=12,
    )
    assert enabled.status_code == 200
    assert enabled.json()["recipient_ids"] == [recipient_id]
    assert client.delete(f"/api/recipients/{recipient_id}").status_code == 409

    removed = update_schedule(
        client,
        "monthly_report",
        auto_send=False,
        recipient_ids=[],
        day_of_month=12,
    )
    assert removed.status_code == 200
    assert client.delete(f"/api/recipients/{recipient_id}").status_code == 204


def test_scheduled_report_sends_once_and_never_overwrites(client, db_session, monkeypatch):
    configure_email(client)
    recipient_id = add_recipient(client)
    template_response = client.post(
        "/api/templates",
        json={
            "name": "周报定时专用模板",
            "template_type": "weekly_report",
            "content": "# {{ title }}\n\n定时专用模板\n\n{{ work_items }}",
            "is_default": False,
        },
    )
    assert template_response.status_code == 201
    template_id = template_response.json()["id"]
    response = update_schedule(
        client,
        "weekly_report",
        enabled=True,
        weekday="fri",
        run_time="15:00",
        template_id=template_id,
        auto_send=True,
        recipient_ids=[recipient_id],
    )
    assert response.status_code == 200
    blocked_type_change = client.put(
        f"/api/templates/{template_id}",
        json={"template_type": "monthly_report"},
    )
    assert blocked_type_change.status_code == 409

    sent = []

    def fake_send(*args, **kwargs):
        sent.append((args, kwargs))

    monkeypatch.setattr("app.services.email.send_email", fake_send)
    _run_report_schedule("weekly_report", True, "2026-06-26")

    report = db_session.scalar(
        select(Report).where(
            Report.report_type == "weekly_report",
            Report.period_start == date(2026, 6, 22),
            Report.period_end == date(2026, 6, 26),
        )
    )
    assert report is not None
    assert report.template_id == template_id
    assert "定时专用模板" in report.content_markdown
    deliveries = list(
        db_session.scalars(
            select(ReportEmailDelivery).where(ReportEmailDelivery.report_id == report.id)
        )
    )
    assert len(sent) == 1
    assert len(deliveries) == 1
    assert deliveries[0].status == "sent"
    assert deliveries[0].subject == report.title

    report.content_markdown = "# 手工修改后不可覆盖"
    db_session.commit()
    _run_report_schedule("weekly_report", True, "2026-06-26")
    db_session.refresh(report)
    assert report.content_markdown == "# 手工修改后不可覆盖"
    assert len(sent) == 1
    assert db_session.scalar(select(ReportEmailDelivery).where(ReportEmailDelivery.report_id == report.id))

    deleted_template = client.delete(f"/api/templates/{template_id}")
    assert deleted_template.status_code == 204
    weekly_schedule = next(
        item
        for item in client.get("/api/settings/report-schedules").json()
        if item["report_type"] == "weekly_report"
    )
    assert weekly_schedule["template_id"] is None


def test_automatic_email_failure_is_recorded_without_retry(client, db_session, monkeypatch):
    configure_email(client)
    recipient_id = add_recipient(client, "director@example.test")
    response = update_schedule(
        client,
        "monthly_report",
        enabled=True,
        day_of_month=None,
        run_time="20:30",
        auto_send=True,
        recipient_ids=[recipient_id],
    )
    assert response.status_code == 200

    attempts = []

    def fail_send(*args, **kwargs):
        attempts.append(1)
        raise EmailDeliveryError("模拟 SMTP 失败")

    monkeypatch.setattr("app.services.email.send_email", fail_send)
    _run_report_schedule("monthly_report", True, "2026-06-30")
    _run_report_schedule("monthly_report", True, "2026-06-30")

    report = db_session.scalar(
        select(Report).where(
            Report.report_type == "monthly_report",
            Report.period_start == date(2026, 6, 1),
            Report.period_end == date(2026, 6, 30),
        )
    )
    assert report is not None
    deliveries = list(
        db_session.scalars(
            select(ReportEmailDelivery).where(ReportEmailDelivery.report_id == report.id)
        )
    )
    assert attempts == [1]
    assert len(deliveries) == 1
    assert deliveries[0].status == "failed"
    assert deliveries[0].error_message == "模拟 SMTP 失败"


def test_startup_catch_up_only_generates_latest_draft(client, db_session, monkeypatch):
    schedule = db_session.scalar(
        select(ReportSchedule).where(ReportSchedule.report_type == "weekly_report")
    )
    schedule.updated_at = datetime(2026, 6, 20, 0, 0, tzinfo=timezone.utc)
    schedule.auto_send = True
    db_session.commit()

    now = datetime(2026, 6, 27, 10, 0, tzinfo=timezone.utc)
    occurrence = catch_up_occurrence(db_session, schedule, now)
    assert occurrence == date(2026, 6, 26)

    sent = []
    monkeypatch.setattr("app.services.email.send_email", lambda *args, **kwargs: sent.append(1))
    _run_report_schedule("weekly_report", False, occurrence.isoformat())

    report = db_session.scalar(
        select(Report).where(
            Report.report_type == "weekly_report",
            Report.period_start == date(2026, 6, 22),
            Report.period_end == date(2026, 6, 26),
        )
    )
    assert report is not None
    assert sent == []
    assert catch_up_occurrence(db_session, schedule, now) is None
