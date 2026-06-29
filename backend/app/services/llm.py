import json
from dataclasses import dataclass
from datetime import date

import httpx

from app.constants import ReportType
from app.models import LLMSetting, WorkLog


@dataclass
class LLMResult:
    content: str
    used_llm: bool


class LLMProviderError(RuntimeError):
    """Raised when an OpenAI-compatible provider cannot return chat content."""


def _prepare_provider_payload(setting: LLMSetting, payload: dict) -> dict:
    prepared = dict(payload)
    if setting.provider == "nvidia" and setting.model == "deepseek-ai/deepseek-v4-pro":
        prepared["temperature"] = 1
        prepared.setdefault("top_p", 0.95)
        prepared.setdefault("max_tokens", 4096)
        chat_template_kwargs = dict(prepared.get("chat_template_kwargs", {}))
        chat_template_kwargs.setdefault("thinking", False)
        prepared["chat_template_kwargs"] = chat_template_kwargs
    return prepared


def _provider_timeout(setting: LLMSetting) -> float:
    return float(setting.timeout_seconds or (180 if setting.provider == "nvidia" else 60))


def build_chat_payload(model: str, report_kind: str, period: tuple[date, date], logs_text: str) -> dict:
    system = (
        "你是一个严谨的工作总结助手。请根据用户提供的每日工作记录，"
        "生成适合直接放入工作报告的中文 Markdown 内容。"
    )
    user = f"""报告类型：{report_kind}
周期：{period[0]} 至 {period[1]}

每日工作记录：
{logs_text or "无记录"}

请输出结构清晰的 Markdown，包含总结、关键成果、风险/阻塞、下阶段计划。"""
    return {
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "temperature": 0.2,
    }


def format_logs_for_prompt(work_logs: list[WorkLog]) -> str:
    if not work_logs:
        return ""
    lines: list[str] = []
    for item in work_logs:
        date_label = (
            str(item.start_date)
            if item.start_date == item.end_date
            else f"{item.start_date} 至 {item.end_date}"
        )
        hours = f"，工时 {item.hours:g}h" if item.hours is not None else ""
        blockers = f"，阻塞：{item.blockers}" if item.blockers else ""
        result = f"，结果：{item.result}" if item.result else ""
        notes = f"，备注：{item.notes}" if item.notes else ""
        lines.append(
            f"- {date_label} [{item.project}] {item.task}：{item.progress}"
            f"{result}{blockers}{hours}{notes}"
        )
    return "\n".join(lines)


def fallback_report_content(report_kind: str, period: tuple[date, date], work_logs: list[WorkLog]) -> str:
    logs_text = format_logs_for_prompt(work_logs)
    if not work_logs:
        return (
            "## 总结\n\n本周期暂无工作记录。\n\n"
            "## 关键成果\n\n- 暂无\n\n## 风险与阻塞\n\n- 暂无\n\n## 下阶段计划\n\n- 补充工作记录后重新生成。"
        )

    projects = sorted({item.project for item in work_logs})
    blockers = [item.blockers for item in work_logs if item.blockers]
    results = [item.result for item in work_logs if item.result]
    total_hours = sum(item.hours or 0 for item in work_logs)
    return f"""## 总结

{period[0]} 至 {period[1]} 共记录 {len(work_logs)} 条工作事项，覆盖 {", ".join(projects)}。记录工时合计 {total_hours:g} 小时。

## 关键成果

{_bullet_lines(results) if results else "- 本周期主要完成了记录中的推进事项。"}

## 风险与阻塞

{_bullet_lines(blockers) if blockers else "- 暂无明确阻塞。"}

## 下阶段计划

- 基于本周期进展继续推进未完成事项。

## 明细摘要

{logs_text}
"""


def _bullet_lines(items: list[str]) -> str:
    return "\n".join(f"- {item}" for item in items)


class LLMClient:
    def _chat_completion(self, setting: LLMSetting, payload: dict) -> str:
        headers = {"Authorization": f"Bearer {setting.api_key}", "Content-Type": "application/json"}
        if setting.extra_headers:
            headers.update(json.loads(setting.extra_headers))

        url = f"{setting.base_url.rstrip('/')}/chat/completions"
        with httpx.Client(timeout=_provider_timeout(setting)) as client:
            try:
                response = client.post(url, headers=headers, json=_prepare_provider_payload(setting, payload))
                response.raise_for_status()
            except httpx.HTTPStatusError as exc:
                raise LLMProviderError(_format_http_error(exc.response)) from exc
            except httpx.RequestError as exc:
                raise LLMProviderError(f"Unable to reach LLM provider: {exc}") from exc
            try:
                data = response.json()
            except ValueError as exc:
                raise LLMProviderError("LLM provider returned invalid JSON") from exc
        return _extract_chat_content(data)

    def generate(self, setting: LLMSetting | None, report_kind: str, period: tuple[date, date], logs: list[WorkLog]) -> LLMResult:
        logs_text = format_logs_for_prompt(logs)
        if not setting or not setting.api_key:
            return LLMResult(fallback_report_content(report_kind, period, logs), used_llm=False)

        payload = build_chat_payload(setting.model, report_kind, period, logs_text)
        content = self._chat_completion(setting, payload)
        return LLMResult(content=content, used_llm=True)

    def fill_template(
        self,
        setting: LLMSetting | None,
        report_kind: str,
        period: tuple[date, date],
        logs: list[WorkLog],
        template_content: str,
    ) -> LLMResult:
        if not setting or not setting.api_key:
            raise ValueError("LLM API key is required to fill this report template")

        logs_text = format_logs_for_prompt(logs)
        payload = {
            "model": setting.model,
            "messages": [
                {
                    "role": "system",
                    "content": (
                        "你是严谨的工作绩效材料填写助手。请仅根据工作记录填写用户提供的 Markdown 模板，"
                        "保持模板结构，不虚构事实。"
                    ),
                },
                {
                    "role": "user",
                    "content": f"""报告类型：{report_kind}
考核周期：{period[0]} 至 {period[1]}

工作记录：
{logs_text or "无工作记录"}

待填写模板：
{template_content}

要求：
1. 只输出填写完成后的 Markdown 正文，不要解释，不要使用代码围栏。
2. 保留模板的标题、表格列、模块、行数和顺序。
3. 替换所有【填写...】、【说明...】和连续下划线占位符，不要原样保留。
4. 考核月份根据考核周期填写；填报人、所属部门等工作记录中没有的信息填写“待补充”。
5. 指标名称、考核标准和指标说明必须来自工作记录；同一事项不要机械重复。
6. 权重没有依据时填写“待确认”，不得擅自编造数字。
""",
                },
            ],
            "temperature": 0.1,
        }
        content = strip_markdown_fence(self._chat_completion(setting, payload)).strip()
        return LLMResult(content=content, used_llm=True)

    def template_from_example(
        self,
        setting: LLMSetting | None,
        template_type: ReportType,
        example_content: str,
    ) -> LLMResult:
        if not setting or not setting.api_key:
            raise ValueError("LLM API key is required to import a template from an example")

        allowed_variables = (
            "{{ title }}, {{ report_type }}, {{ period_start }}, {{ period_end }}, "
            "{{ generated_at }}, {{ ai_content }}, {{ summary }}, {{ work_items }}, "
            "{{ highlights }}, {{ blockers }}, {{ next_steps }}, {{ raw_llm_content }}"
        )
        payload = {
            "model": setting.model,
            "messages": [
                {
                    "role": "system",
                    "content": (
                        "你是一个工作报告模板工程师。你的任务是把用户提供的示例文档"
                        "抽象成可复用的 Markdown + Jinja 模板。"
                    ),
                },
                {
                    "role": "user",
                    "content": f"""模板类型：{template_type.value}

允许使用的变量只有：
{allowed_variables}

要求：
1. 只输出模板正文，不要解释，不要包裹 Markdown 代码围栏。
2. 保留示例中的标题层级、表格结构、章节顺序和中文表达风格。
3. 把日期、周期、标题、AI 生成主体、工作明细、亮点、阻塞、计划等可变内容替换为上面的变量。
4. 不要引入未列出的变量。
5. 输出必须是 Markdown，可直接保存为本系统模板。
6. work_items、highlights、blockers、next_steps 既可直接输出，也可使用 Jinja for 循环。
7. 遍历 work_items 时，item 仅支持 date、start_date、end_date、project、task、status、content、progress、conclusion、result、blockers、hours、priority、notes 字段。

示例文档：
{example_content}
""",
                },
            ],
            "temperature": 0.1,
        }
        content = strip_markdown_fence(self._chat_completion(setting, payload)).strip()
        return LLMResult(content=content, used_llm=True)


def strip_markdown_fence(content: str) -> str:
    stripped = content.strip()
    if not stripped.startswith("```"):
        return stripped
    lines = stripped.splitlines()
    if lines and lines[0].startswith("```"):
        lines = lines[1:]
    if lines and lines[-1].strip() == "```":
        lines = lines[:-1]
    return "\n".join(lines).strip()


def _extract_chat_content(data: object) -> str:
    if not isinstance(data, dict):
        raise LLMProviderError("LLM provider returned a non-object JSON response")

    choices = data.get("choices")
    if not isinstance(choices, list) or not choices:
        detail = _provider_error_detail(data)
        suffix = f": {detail}" if detail else ""
        raise LLMProviderError(f"LLM provider response did not include chat choices{suffix}")

    first_choice = choices[0]
    if not isinstance(first_choice, dict):
        raise LLMProviderError("LLM provider returned an invalid chat choice")

    message = first_choice.get("message")
    content = message.get("content") if isinstance(message, dict) else None
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        text = "".join(
            part.get("text", "") if isinstance(part, dict) else str(part)
            for part in content
        )
        if text:
            return text

    raise LLMProviderError("LLM provider returned a chat choice without message content")


def _format_http_error(response: httpx.Response) -> str:
    detail: str | None = None
    try:
        detail = _provider_error_detail(response.json())
    except ValueError:
        body = response.text.strip()
        if body:
            detail = _truncate_message(body)

    message = f"LLM provider returned HTTP {response.status_code}"
    if detail:
        message = f"{message}: {detail}"
    return message


def _provider_error_detail(data: object) -> str | None:
    if not isinstance(data, dict):
        return None

    error = data.get("error")
    if isinstance(error, dict):
        for key in ("message", "detail", "code", "type"):
            value = error.get(key)
            if isinstance(value, str) and value.strip():
                return _truncate_message(value)
        return _truncate_message(json.dumps(error, ensure_ascii=False))
    if isinstance(error, str) and error.strip():
        return _truncate_message(error)

    for key in ("message", "detail"):
        value = data.get(key)
        if isinstance(value, str) and value.strip():
            return _truncate_message(value)
    return None


def _truncate_message(message: str, limit: int = 500) -> str:
    compact = " ".join(message.split())
    if len(compact) <= limit:
        return compact
    return f"{compact[: limit - 1]}..."
