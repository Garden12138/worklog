from datetime import date

import pytest

from app.constants import ReportType
from app.models import LLMSetting, WorkLog
from app.services.llm import (
    LLMClient,
    LLMProviderError,
    _extract_chat_content,
    _prepare_provider_payload,
    _provider_timeout,
    build_chat_payload,
)


def test_build_chat_payload_is_openai_compatible():
    payload = build_chat_payload(
        "gpt-test",
        "周报",
        (date(2026, 6, 22), date(2026, 6, 28)),
        "- 记录",
    )
    assert payload["model"] == "gpt-test"
    assert payload["messages"][0]["role"] == "system"
    assert payload["messages"][1]["role"] == "user"
    assert "2026-06-22" in payload["messages"][1]["content"]


def test_extract_chat_content_reports_provider_error_without_choices():
    data = {"error": {"message": "Model does not exist"}}

    with pytest.raises(LLMProviderError, match="Model does not exist"):
        _extract_chat_content(data)


def test_nvidia_deepseek_uses_non_thinking_payload_and_longer_timeout():
    setting = LLMSetting(
        provider="nvidia",
        base_url="https://integrate.api.nvidia.com/v1",
        model="deepseek-ai/deepseek-v4-pro",
        api_key="nvapi-test",
        timeout_seconds=240,
    )

    payload = _prepare_provider_payload(setting, {"model": setting.model, "messages": [], "temperature": 0.2})

    assert payload["temperature"] == 1
    assert payload["top_p"] == 0.95
    assert payload["chat_template_kwargs"] == {"thinking": False}
    assert _provider_timeout(setting) == 240


def test_fill_template_requests_completed_markdown(monkeypatch):
    setting = LLMSetting(
        provider="openrouter",
        base_url="https://openrouter.ai/api/v1",
        model="test-model",
        api_key="sk-test",
        timeout_seconds=60,
    )
    log = WorkLog(
        work_date=date(2026, 6, 25),
        start_date=date(2026, 6, 25),
        end_date=date(2026, 6, 25),
        project="Worklog",
        task="生成绩效草稿",
        progress="已完成",
        priority="high",
    )
    captured: dict = {}

    def fake_chat(self, current_setting, payload):
        captured.update(payload)
        return "```markdown\n| 指标名称 | 权重 |\n| --- | --- |\n| 生成绩效草稿 | 待确认 |\n```"

    monkeypatch.setattr("app.services.llm.LLMClient._chat_completion", fake_chat)

    result = LLMClient().fill_template(
        setting,
        "绩效考核表",
        (date(2026, 6, 1), date(2026, 6, 30)),
        [log],
        "| 指标名称 | 权重 |\n| --- | --- |\n| 【填写核心任务】 | ____ |",
    )

    assert result.used_llm is True
    assert result.content.startswith("| 指标名称 | 权重 |")
    assert "生成绩效草稿" in captured["messages"][1]["content"]
    assert "【填写核心任务】" in captured["messages"][1]["content"]


def test_optimize_template_preserves_template_constraints(monkeypatch):
    setting = LLMSetting(
        provider="openai",
        base_url="https://api.openai.com/v1",
        model="test-model",
        api_key="sk-test",
        timeout_seconds=60,
    )
    captured: dict = {}

    def fake_chat(self, current_setting, payload):
        captured.update(payload)
        return "```markdown\n# {{ title }}\n\n## 总结\n\n{{ ai_content }}\n```"

    monkeypatch.setattr("app.services.llm.LLMClient._chat_completion", fake_chat)

    result = LLMClient().optimize_template(
        setting,
        template_type=ReportType.WEEKLY,
        template_content="# {{ title }}\n\n{{ ai_content }}",
        optimization_request="增加总结章节并精简表达",
    )

    assert result.used_llm is True
    assert result.content.startswith("# {{ title }}")
    prompt = captured["messages"][1]["content"]
    assert "增加总结章节并精简表达" in prompt
    assert "以用户优化需求为修改目标" in prompt
    assert "--- REQUEST START ---" in prompt
    assert "--- TEMPLATE START ---" in prompt


def test_optimize_report_uses_current_markdown_and_forbids_invented_facts(monkeypatch):
    setting = LLMSetting(
        provider="openai",
        base_url="https://api.openai.com/v1",
        model="test-model",
        api_key="sk-test",
        timeout_seconds=60,
    )
    captured: dict = {}

    def fake_chat(self, current_setting, payload):
        captured.update(payload)
        return "```markdown\n# 周报\n\n## 关键成果\n\n- 完成接口联调\n```"

    monkeypatch.setattr("app.services.llm.LLMClient._chat_completion", fake_chat)

    result = LLMClient().optimize_report(
        setting,
        report_kind="周报",
        period=(date(2026, 6, 22), date(2026, 6, 28)),
        report_content="# 周报\n\n- 完成接口联调",
        optimization_request="突出关键成果并精简表达",
    )

    assert result.used_llm is True
    assert result.content.startswith("# 周报")
    prompt = captured["messages"][1]["content"]
    assert "突出关键成果并精简表达" in prompt
    assert "不要添加草稿中没有的" in prompt
    assert "--- REPORT START ---" in prompt
    assert "完成接口联调" in prompt
