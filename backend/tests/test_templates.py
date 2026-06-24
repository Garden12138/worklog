import pytest

from app.services.templates import TemplateValidationError, render_template, validate_template_content


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
