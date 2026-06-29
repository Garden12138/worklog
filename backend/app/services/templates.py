import re

from jinja2 import Environment, StrictUndefined, TemplateError, meta

ALLOWED_TEMPLATE_VARIABLES = {
    "title",
    "report_type",
    "period_start",
    "period_end",
    "generated_at",
    "ai_content",
    "summary",
    "work_items",
    "highlights",
    "blockers",
    "next_steps",
    "raw_llm_content",
}


class TemplateValidationError(ValueError):
    pass


def jinja_env() -> Environment:
    return Environment(autoescape=False, undefined=StrictUndefined)


def validate_template_content(content: str) -> None:
    env = jinja_env()
    try:
        parsed = env.parse(content)
    except Exception as exc:
        raise TemplateValidationError(f"Template syntax error: {exc}") from exc

    undeclared = meta.find_undeclared_variables(parsed)
    invalid = sorted(undeclared - ALLOWED_TEMPLATE_VARIABLES)
    if invalid:
        allowed = ", ".join(sorted(ALLOWED_TEMPLATE_VARIABLES))
        raise TemplateValidationError(
            f"Unsupported template variable(s): {', '.join(invalid)}. Allowed: {allowed}"
        )


def requires_llm_template_fill(content: str) -> bool:
    parsed = jinja_env().parse(content)
    if meta.find_undeclared_variables(parsed):
        return False
    return bool(re.search(r"【(?:填写|说明)[^】]*】|_{3,}", content))


def render_template(content: str, context: dict[str, object]) -> str:
    validate_template_content(content)
    try:
        return jinja_env().from_string(content).render(**context)
    except TemplateError as exc:
        raise TemplateValidationError(f"Template render error: {exc}") from exc
