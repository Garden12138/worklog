from jinja2 import Environment, StrictUndefined, meta

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


def render_template(content: str, context: dict[str, object]) -> str:
    validate_template_content(content)
    return jinja_env().from_string(content).render(**context)
