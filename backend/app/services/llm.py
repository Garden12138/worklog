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
        with httpx.Client(timeout=60) as client:
            response = client.post(url, headers=headers, json=payload)
            response.raise_for_status()
            data = response.json()
        return data["choices"][0]["message"]["content"]

    def generate(self, setting: LLMSetting | None, report_kind: str, period: tuple[date, date], logs: list[WorkLog]) -> LLMResult:
        logs_text = format_logs_for_prompt(logs)
        if not setting or not setting.api_key:
            return LLMResult(fallback_report_content(report_kind, period, logs), used_llm=False)

        payload = build_chat_payload(setting.model, report_kind, period, logs_text)
        content = self._chat_completion(setting, payload)
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
