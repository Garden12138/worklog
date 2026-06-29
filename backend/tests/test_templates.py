from datetime import date

import pytest

from app.constants import ReportType
from app.models import WorkLog
from app.services.reports import build_report_context
from app.services.templates import (
    TemplateValidationError,
    render_template,
    requires_llm_template_fill,
    validate_template_content,
)


def test_template_accepts_known_variables():
    content = "# {{ title }}\n\n{{ ai_content }}\n{{ work_items }}"
    validate_template_content(content)
    assert "周报" in render_template(
        content,
        {"title": "周报", "ai_content": "总结", "work_items": "- A"},
    )


def test_template_rejects_unknown_variables():
    with pytest.raises(TemplateValidationError):
        validate_template_content("{{ unsafe_value }}")


def test_report_context_supports_direct_output_and_structured_loops():
    log = WorkLog(
        work_date=date(2026, 6, 25),
        start_date=date(2026, 6, 25),
        end_date=date(2026, 6, 26),
        project="Worklog",
        task="修复模板",
        progress="已完成",
        result="报告可生成",
        priority="high",
    )
    context = build_report_context(
        ReportType.WEEKLY,
        (date(2026, 6, 22), date(2026, 6, 28)),
        [log],
        "本周总结",
    )
    loop_template = """{% for item in work_items %}
{{ item.date }}|{{ item.status }}|{{ item.content }}|{{ item.conclusion }}
{% endfor %}"""

    loop_rendered = render_template(loop_template, context)
    direct_rendered = render_template("{{ work_items }}", context)

    assert "2026-06-25 至 2026-06-26|已完成|[Worklog] 修复模板|报告可生成" in loop_rendered
    assert "[Worklog] 修复模板：已完成" in direct_rendered
    assert "T" not in str(context["generated_at"])
    assert "+00:00" not in str(context["generated_at"])


def test_template_runtime_error_becomes_validation_error():
    with pytest.raises(TemplateValidationError, match="Template render error"):
        render_template("{{ work_items.missing }}", {"work_items": "记录"})


def test_placeholder_template_requires_llm_fill_but_jinja_template_does_not():
    placeholder_template = "| 指标名称 | 权重 |\n| --- | --- |\n| 【填写核心任务】 | ____ |"

    assert requires_llm_template_fill(placeholder_template) is True
    assert requires_llm_template_fill("# {{ title }}\n\n{{ ai_content }}") is False
