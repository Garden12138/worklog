from datetime import date

from sqlalchemy import select

from app.constants import ReportType, TaskStatus
from app.models import GenerationTask, LLMSetting
from app.services.llm import LLMProviderError, LLMResult
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
            "timeout_seconds": 75,
        },
    )
    assert saved.status_code == 200
    assert saved.json()["base_url"] == "https://openrouter.ai/api/v1"
    assert saved.json()["api_key"] == "sk-t...cret"
    assert saved.json()["timeout_seconds"] == 75

    loaded = client.get("/api/settings/llm")
    assert loaded.json()["api_key"] == "sk-t...cret"
    assert loaded.json()["timeout_seconds"] == 75


def test_llm_setting_rejects_timeout_outside_allowed_range(client):
    payload = {
        "provider": "openrouter",
        "base_url": "",
        "model": "openai/gpt-4.1-mini",
        "api_key": "sk-test-secret",
        "extra_headers": {},
    }

    too_short = client.put("/api/settings/llm", json={**payload, "timeout_seconds": 4})
    too_long = client.put("/api/settings/llm", json={**payload, "timeout_seconds": 601})

    assert too_short.status_code == 422
    assert too_long.status_code == 422


def test_llm_setting_reuses_api_key_only_for_same_provider(client, db_session):
    client.put(
        "/api/settings/llm",
        json={
            "provider": "openrouter",
            "base_url": "",
            "model": "openai/gpt-4.1-mini",
            "api_key": "sk-openrouter-secret",
            "extra_headers": {},
            "timeout_seconds": 45,
        },
    )
    client.put(
        "/api/settings/llm",
        json={
            "provider": "nvidia",
            "base_url": "",
            "model": "deepseek-ai/deepseek-v4-pro",
            "api_key": "nvapi-nvidia-secret",
            "extra_headers": {},
            "timeout_seconds": 180,
        },
    )

    switched_back = client.put(
        "/api/settings/llm",
        json={
            "provider": "openrouter",
            "base_url": "",
            "model": "openai/gpt-4.1-mini",
            "api_key": "",
            "extra_headers": {},
        },
    )

    assert switched_back.status_code == 200
    assert switched_back.json()["api_key"] == "sk-o...cret"

    db_session.expire_all()
    active = db_session.scalar(select(LLMSetting).where(LLMSetting.is_active.is_(True)))
    assert active is not None
    assert active.provider == "openrouter"
    assert active.api_key == "sk-openrouter-secret"


def test_llm_settings_can_be_listed_and_reapplied(client):
    first = client.put(
        "/api/settings/llm",
        json={
            "provider": "openrouter",
            "base_url": "",
            "model": "openai/gpt-4.1-mini",
            "api_key": "sk-openrouter-secret",
            "extra_headers": {},
            "timeout_seconds": 45,
        },
    ).json()
    second = client.put(
        "/api/settings/llm",
        json={
            "provider": "nvidia",
            "base_url": "",
            "model": "deepseek-ai/deepseek-v4-pro",
            "api_key": "nvapi-nvidia-secret",
            "extra_headers": {},
            "timeout_seconds": 180,
        },
    ).json()

    listed = client.get("/api/settings/llm/all")

    assert listed.status_code == 200
    assert [item["id"] for item in listed.json()] == [second["id"], first["id"]]
    assert listed.json()[0]["is_active"] is True
    assert listed.json()[1]["is_active"] is False
    assert listed.json()[0]["timeout_seconds"] == 180
    assert listed.json()[1]["timeout_seconds"] == 45

    applied = client.post(f"/api/settings/llm/{first['id']}/apply")

    assert applied.status_code == 200
    assert applied.json()["provider"] == "openrouter"
    assert applied.json()["is_active"] is True
    assert client.get("/api/settings/llm").json()["id"] == first["id"]


def test_llm_setting_is_updated_in_place_and_applied(client, db_session):
    first = client.put(
        "/api/settings/llm",
        json={
            "provider": "openrouter",
            "base_url": "",
            "model": "openai/gpt-4.1-mini",
            "api_key": "sk-openrouter-secret",
            "extra_headers": {},
            "timeout_seconds": 60,
        },
    ).json()
    second = client.put(
        "/api/settings/llm",
        json={
            "provider": "nvidia",
            "base_url": "",
            "model": "deepseek-ai/deepseek-v4-pro",
            "api_key": "nvapi-nvidia-secret",
            "extra_headers": {},
            "timeout_seconds": 180,
        },
    ).json()

    updated = client.put(
        f"/api/settings/llm/{first['id']}",
        json={
            "provider": "openrouter",
            "base_url": "https://openrouter.ai/api/v1",
            "model": "nvidia/nemotron-3-ultra-550b-a55b:free",
            "api_key": "",
            "extra_headers": {"HTTP-Referer": "http://127.0.0.1"},
            "timeout_seconds": 95,
        },
    )

    assert updated.status_code == 200
    assert updated.json()["id"] == first["id"]
    assert updated.json()["timeout_seconds"] == 95
    assert updated.json()["is_active"] is True
    listed = client.get("/api/settings/llm/all").json()
    assert len(listed) == 2
    assert [item["id"] for item in listed] == [first["id"], second["id"]]

    db_session.expire_all()
    saved = db_session.get(LLMSetting, first["id"])
    assert saved is not None
    assert saved.api_key == "sk-openrouter-secret"
    assert saved.timeout_seconds == 95


def test_llm_setting_can_be_deleted_except_when_active(client):
    first = client.put(
        "/api/settings/llm",
        json={
            "provider": "openrouter",
            "base_url": "",
            "model": "openai/gpt-4.1-mini",
            "api_key": "sk-openrouter-secret",
            "extra_headers": {},
        },
    ).json()
    second = client.put(
        "/api/settings/llm",
        json={
            "provider": "nvidia",
            "base_url": "",
            "model": "deepseek-ai/deepseek-v4-pro",
            "api_key": "nvapi-nvidia-secret",
            "extra_headers": {},
        },
    ).json()

    deleted = client.delete(f"/api/settings/llm/{first['id']}")

    assert deleted.status_code == 204
    assert [item["id"] for item in client.get("/api/settings/llm/all").json()] == [second["id"]]

    active_delete = client.delete(f"/api/settings/llm/{second['id']}")

    assert active_delete.status_code == 409
    assert active_delete.json()["detail"] == "当前应用的 LLM 配置不能删除，请先应用其他配置"
    assert client.get("/api/settings/llm").json()["id"] == second["id"]


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


def test_import_template_from_example_returns_provider_error(client, monkeypatch):
    client.put(
        "/api/settings/llm",
        json={
            "provider": "openai",
            "base_url": "https://example.test/v1",
            "model": "missing-model",
            "api_key": "sk-template-secret",
            "extra_headers": {},
        },
    )

    def fake_template_from_example(self, setting, template_type, example_content):
        raise LLMProviderError(
            "LLM provider response did not include chat choices: Model not found"
        )

    monkeypatch.setattr("app.main.LLMClient.template_from_example", fake_template_from_example)
    response = client.post(
        "/api/templates/import-example",
        json={
            "template_type": "weekly_report",
            "example_content": "# 示例周报\n\n这里是一段足够长的周报示例内容，用于生成模板。",
        },
    )

    assert response.status_code == 502
    assert response.json()["detail"].endswith("Model not found")


def test_generate_performance_report_fills_placeholder_template(client, monkeypatch):
    client.put(
        "/api/settings/llm",
        json={
            "provider": "openrouter",
            "base_url": "https://example.test/v1",
            "model": "performance-model",
            "api_key": "sk-performance-secret",
            "extra_headers": {},
        },
    )
    template = client.post(
        "/api/templates",
        json={
            "name": "绩效填写模板",
            "template_type": "performance_review",
            "content": (
                "# 月度绩效考核填写模板\n\n"
                "**考核月份：** ______年____月\n\n"
                "| 指标名称 | 考核标准 | 权重 |\n"
                "| --- | --- | ---: |\n"
                "| 【填写核心任务】 | 【填写交付结果】 | ____ |"
            ),
            "is_default": False,
        },
    ).json()

    def fake_fill_template(self, setting, report_kind, period, logs, template_content):
        assert setting.model == "performance-model"
        assert report_kind == "绩效考核表"
        assert period == (date(2026, 6, 1), date(2026, 6, 30))
        assert "【填写核心任务】" in template_content
        return LLMResult(
            content=(
                "# 月度绩效考核填写模板\n\n"
                "**考核月份：** 2026年06月\n\n"
                "| 指标名称 | 考核标准 | 权重 |\n"
                "| --- | --- | ---: |\n"
                "| 完成 Worklog | 报告生成通过 | 待确认 |"
            ),
            used_llm=True,
        )

    monkeypatch.setattr("app.services.llm.LLMClient.fill_template", fake_fill_template)
    response = client.post(
        "/api/reports/generate",
        json={
            "report_type": "performance_review",
            "anchor_date": "2026-06-29",
            "template_id": template["id"],
        },
    )

    assert response.status_code == 200
    assert response.json()["used_llm"] is True
    content = response.json()["report"]["content_markdown"]
    assert "2026年06月" in content
    assert "完成 Worklog" in content
    assert "【填写" not in content


def test_generate_report_returns_provider_error(client, monkeypatch):
    client.put(
        "/api/settings/llm",
        json={
            "provider": "openai",
            "base_url": "https://example.test/v1",
            "model": "missing-model",
            "api_key": "sk-report-secret",
            "extra_headers": {},
        },
    )

    def fake_generate(self, setting, report_kind, period, logs):
        raise LLMProviderError("LLM provider returned HTTP 401: invalid api key")

    monkeypatch.setattr("app.services.reports.LLMClient.generate", fake_generate)
    response = client.post(
        "/api/reports/generate",
        json={"report_type": "weekly_report", "anchor_date": "2026-06-23"},
    )

    assert response.status_code == 502
    assert response.json()["detail"].endswith("invalid api key")


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
