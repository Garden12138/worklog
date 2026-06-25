from app.models import ReportEmailDelivery
from app.services.email import EmailDeliveryError


def configure_email(client, password: str = "smtp-app-password"):
    response = client.put(
        "/api/settings/email",
        json={
            "host": "smtp.example.test",
            "port": 587,
            "security": "starttls",
            "username": "sender@example.test",
            "password": password,
            "sender_address": "sender@example.test",
            "sender_name": "Worklog",
        },
    )
    assert response.status_code == 200
    return response


def create_report(client) -> int:
    created_log = client.post(
        "/api/work-logs",
        json={
            "work_date": "2026-06-23",
            "project": "Worklog",
            "task": "邮件发送能力",
            "progress": "完成 SMTP 和通讯录功能",
            "hours": 3,
            "priority": "high",
        },
    )
    assert created_log.status_code == 201
    generated = client.post(
        "/api/reports/generate",
        json={"report_type": "weekly_report", "anchor_date": "2026-06-23"},
    )
    assert generated.status_code == 200
    return generated.json()["report"]["id"]


def test_recipient_crud_validates_emails_and_defaults(client):
    invalid = client.post(
        "/api/recipients",
        json={"name": "上级", "email": "not-an-email", "is_default": True},
    )
    assert invalid.status_code == 422

    created = client.post(
        "/api/recipients",
        json={"name": "直属上级", "email": "Manager@Example.Test", "is_default": True},
    )
    assert created.status_code == 201
    contact = created.json()
    assert contact["email"] == "manager@example.test"
    assert contact["is_default"] is True

    duplicate = client.post(
        "/api/recipients",
        json={"name": "另一个名字", "email": "manager@example.test", "is_default": False},
    )
    assert duplicate.status_code == 409

    updated = client.put(f"/api/recipients/{contact['id']}", json={"is_default": False})
    assert updated.status_code == 200
    assert updated.json()["is_default"] is False

    listed = client.get("/api/recipients")
    assert listed.status_code == 200
    assert [item["id"] for item in listed.json()] == [contact["id"]]

    deleted = client.delete(f"/api/recipients/{contact['id']}")
    assert deleted.status_code == 204


def test_smtp_setting_masks_and_retains_password_and_sends_test_email(client, monkeypatch):
    saved = configure_email(client, password="smtp-secret-password")
    assert saved.json()["password"] == "smtp...word"

    retained = client.put(
        "/api/settings/email",
        json={
            "host": "smtp.example.test",
            "port": 465,
            "security": "ssl",
            "username": "sender@example.test",
            "password": "",
            "sender_address": "sender@example.test",
            "sender_name": "Worklog",
        },
    )
    assert retained.status_code == 200
    assert retained.json()["password"] == "smtp...word"

    captured = {}

    def fake_send(setting, recipients, subject, text_body, html_body, attachment_filename=None, attachment_content=None):
        captured.update(
            setting=setting,
            recipients=recipients,
            subject=subject,
            text_body=text_body,
            html_body=html_body,
            attachment_filename=attachment_filename,
        )

    monkeypatch.setattr("app.main.send_email", fake_send)
    tested = client.post("/api/settings/email/test", json={"recipient_email": "owner@example.test"})
    assert tested.status_code == 200
    assert captured["recipients"] == ["owner@example.test"]
    assert captured["subject"] == "Worklog SMTP 测试邮件"
    assert captured["attachment_filename"] is None


def test_report_email_sends_docx_and_records_success_or_failure(client, db_session, monkeypatch):
    report_id = create_report(client)
    unconfigured = client.post(
        f"/api/reports/{report_id}/send-email",
        json={"recipient_ids": [], "additional_recipients": ["leader@example.test"], "subject": "周报"},
    )
    assert unconfigured.status_code == 400

    configure_email(client)
    recipient = client.post(
        "/api/recipients",
        json={"name": "直属上级", "email": "manager@example.test", "is_default": True},
    ).json()
    captured = {}

    def fake_send(setting, recipients, subject, text_body, html_body, attachment_filename=None, attachment_content=None):
        captured.update(
            recipients=recipients,
            subject=subject,
            text_body=text_body,
            html_body=html_body,
            attachment_filename=attachment_filename,
            attachment_content=attachment_content,
        )

    monkeypatch.setattr("app.main.send_email", fake_send)
    delivered = client.post(
        f"/api/reports/{report_id}/send-email",
        json={
            "recipient_ids": [recipient["id"]],
            "additional_recipients": ["director@example.test", "manager@example.test"],
            "subject": "本周工作周报",
        },
    )
    assert delivered.status_code == 200
    payload = delivered.json()
    assert payload["status"] == "sent"
    assert payload["sent_at"] is not None
    assert [item["email"] for item in payload["recipients"]] == [
        "manager@example.test",
        "director@example.test",
    ]
    assert captured["recipients"] == ["manager@example.test", "director@example.test"]
    assert "邮件发送能力" in captured["text_body"]
    assert "<h1>" in captured["html_body"]
    assert captured["attachment_filename"].endswith(".docx")
    assert captured["attachment_content"].startswith(b"PK")

    def fail_send(*args, **kwargs):
        raise EmailDeliveryError("邮件发送失败，请检查 SMTP 设置或稍后重试。")

    monkeypatch.setattr("app.main.send_email", fail_send)
    failed = client.post(
        f"/api/reports/{report_id}/send-email",
        json={"recipient_ids": [recipient["id"]], "additional_recipients": [], "subject": "重发周报"},
    )
    assert failed.status_code == 502

    history = client.get(f"/api/reports/{report_id}/email-deliveries")
    assert history.status_code == 200
    assert [item["status"] for item in history.json()] == ["failed", "sent"]
    assert history.json()[0]["error_message"]

    db_session.expire_all()
    records = db_session.query(ReportEmailDelivery).filter_by(report_id=report_id).all()
    assert len(records) == 2
    assert records[0].content_markdown
