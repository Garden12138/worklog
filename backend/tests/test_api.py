from datetime import date

from app.constants import ReportType, TaskStatus
from app.models import GenerationTask
from app.services.llm import LLMResult
from app.services.reports import create_missing_report


def test_create_work_log_generate_report_and_export_docx(client, db_session):
    log_response = client.post(
        "/api/work-logs",
        json={
            "work_date": "2026-06-23",
            "project": "Worklog",
            "task": "实现 V1",
            "progress": "完成后端和前端基础能力",
            "result": "可以生成报告草稿",
            "blockers": "",
            "hours": 6,
            "priority": "high",
            "notes": "测试记录",
        },
    )
    assert log_response.status_code == 201

    templates = client.get("/api/templates").json()
    assert {item["template_type"] for item in templates} >= {
        "weekly_report",
        "monthly_report",
        "performance_review",
    }

    generated = client.post(
        "/api/reports/generate",
        json={"report_type": "weekly_report", "anchor_date": "2026-06-23"},
    )
    assert generated.status_code == 200
    data = generated.json()
    assert data["used_llm"] is False
    assert data["report"]["period_start"] == "2026-06-22"
    assert "实现 V1" in data["report"]["content_markdown"]

    report_id = data["report"]["id"]
    saved = client.put(
        f"/api/reports/{report_id}",
        json={"content_markdown": data["report"]["content_markdown"] + "\n\n补充说明"},
    )
    assert saved.status_code == 200
    assert saved.json()["edited_at"] is not None

    exported = client.get(f"/api/reports/{report_id}/export/docx")
    assert exported.status_code == 200
    assert exported.content.startswith(b"PK")

    deleted = client.delete(f"/api/reports/{report_id}")
    assert deleted.status_code == 204
    missing = client.get(f"/api/reports/{report_id}")
    assert missing.status_code == 404
    db_session.expire_all()
    task = db_session.get(GenerationTask, data["task_id"])
    assert task is not None
    assert task.report_id is None


def test_work_logs_support_pagination_and_date_ranges(client):
    for index in range(12):
        response = client.post(
            "/api/work-logs",
            json={
                "start_date": "2026-06-22",
                "end_date": "2026-06-24" if index == 0 else "2026-06-22",
                "project": "Worklog",
                "task": f"事项 {index}",
                "progress": "推进分页和日期范围",
                "hours": 1,
                "priority": "medium",
            },
        )
        assert response.status_code == 201
        assert response.json()["work_date"] == "2026-06-22"

    first_page = client.get("/api/work-logs?page=1&page_size=5")
    assert first_page.status_code == 200
    first_data = first_page.json()
    assert first_data["total"] == 12
    assert first_data["page"] == 1
    assert first_data["page_size"] == 5
    assert first_data["total_pages"] == 3
    assert len(first_data["items"]) == 5

    second_page = client.get("/api/work-logs?page=2&page_size=5")
    assert second_page.status_code == 200
    assert len(second_page.json()["items"]) == 5

    generated = client.post(
        "/api/reports/generate",
        json={"report_type": "weekly_report", "anchor_date": "2026-06-25"},
    )
    assert generated.status_code == 200
    assert "2026-06-22 至 2026-06-24" in generated.json()["report"]["content_markdown"]


def test_llm_setting_masks_api_key(client):
    saved = client.put(
        "/api/settings/llm",
        json={
            "provider": "openrouter",
            "base_url": "",
            "model": "openai/gpt-4.1-mini",
            "api_key": "sk-test-secret",
            "extra_headers": {"HTTP-Referer": "http://127.0.0.1"},
        },
    )
    assert saved.status_code == 200
    assert saved.json()["base_url"] == "https://openrouter.ai/api/v1"
    assert saved.json()["api_key"] == "sk-t...cret"

    loaded = client.get("/api/settings/llm")
    assert loaded.json()["api_key"] == "sk-t...cret"


def test_import_template_from_example_uses_llm(client, monkeypatch):
    client.put(
        "/api/settings/llm",
        json={
            "provider": "openai",
            "base_url": "https://example.test/v1",
            "model": "template-model",
            "api_key": "sk-template-secret",
            "extra_headers": {},
        },
    )

    def fake_template_from_example(self, setting, template_type, example_content):
        assert setting.api_key == "sk-template-secret"
        assert template_type == ReportType.WEEKLY
        assert "示例周报" in example_content
        return LLMResult(
            content="# {{ title }}\n\n周期：{{ period_start }} - {{ period_end }}\n\n{{ ai_content }}",
            used_llm=True,
        )

    monkeypatch.setattr("app.main.LLMClient.template_from_example", fake_template_from_example)
    response = client.post(
        "/api/templates/import-example",
        json={
            "template_type": "weekly_report",
            "example_content": "# 示例周报\n\n这里是一段足够长的周报示例内容，用于生成模板。",
        },
    )

    assert response.status_code == 200
    data = response.json()
    assert data["used_llm"] is True
    assert data["content"].startswith("# {{ title }}")


def test_generate_report_rejects_duplicate_pending_task(client, db_session):
    db_session.add(
        GenerationTask(
            report_type=ReportType.WEEKLY.value,
            period_start=date(2026, 6, 22),
            period_end=date(2026, 6, 28),
            status=TaskStatus.PENDING.value,
        )
    )
    db_session.commit()

    response = client.post(
        "/api/reports/generate",
        json={"report_type": "weekly_report", "anchor_date": "2026-06-23"},
    )

    assert response.status_code == 400
    assert response.json()["detail"] == "Report generation is already running for this period"


def test_scheduled_generation_skips_existing_draft(db_session):
    first = create_missing_report(db_session, ReportType.MONTHLY, date(2026, 6, 30))
    second = create_missing_report(db_session, ReportType.MONTHLY, date(2026, 6, 30))

    assert first is not None
    assert second is None
