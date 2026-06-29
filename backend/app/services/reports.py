import json
from datetime import date
from threading import Lock

from sqlalchemy import select
from sqlalchemy.orm import Session

from app.constants import ReportStatus, ReportType, TaskStatus
from app.default_templates import DEFAULT_TEMPLATES
from app.models import GenerationTask, LLMSetting, Report, Template, WorkLog, utcnow
from app.services.llm import LLMClient, format_logs_for_prompt
from app.services.periods import format_local_datetime, resolve_period
from app.services.templates import render_template, requires_llm_template_fill, validate_template_content


REPORT_TITLES = {
    ReportType.WEEKLY: "周报",
    ReportType.MONTHLY: "月报",
    ReportType.PERFORMANCE: "绩效考核表",
}
_ACTIVE_GENERATION_KEYS: set[tuple[str, date, date]] = set()
_ACTIVE_GENERATION_LOCK = Lock()


class TemplateList(list):
    """A template value that supports both Markdown output and Jinja iteration."""

    def __init__(self, values: list[object], markdown: str):
        super().__init__(values)
        self.markdown = markdown

    def __str__(self) -> str:
        return self.markdown


def seed_default_templates(db: Session) -> None:
    for template_type, default in DEFAULT_TEMPLATES.items():
        existing = db.scalar(
            select(Template).where(
                Template.template_type == template_type.value,
                Template.is_default.is_(True),
            )
        )
        if existing:
            continue
        db.add(
            Template(
                name=default["name"],
                template_type=template_type.value,
                content=default["content"],
                is_default=True,
            )
        )
    db.commit()


def list_work_logs_for_period(db: Session, start: date, end: date) -> list[WorkLog]:
    return list(
        db.scalars(
            select(WorkLog)
            .where(WorkLog.end_date >= start, WorkLog.start_date <= end)
            .order_by(WorkLog.start_date.asc(), WorkLog.end_date.asc(), WorkLog.id.asc())
        )
    )


def get_template_for_report(db: Session, report_type: ReportType, template_id: int | None) -> Template:
    if template_id:
        template = db.get(Template, template_id)
        if not template or template.template_type != report_type.value:
            raise ValueError("Template not found for report type")
        return template

    template = db.scalar(
        select(Template)
        .where(Template.template_type == report_type.value)
        .order_by(Template.is_default.desc(), Template.updated_at.desc())
    )
    if not template:
        raise ValueError("No template exists for report type")
    return template


def active_llm_setting(db: Session) -> LLMSetting | None:
    return db.scalar(select(LLMSetting).where(LLMSetting.is_active.is_(True)).order_by(LLMSetting.id.desc()))


def report_to_dict_source_ids(report: Report) -> list[int]:
    try:
        return list(json.loads(report.source_log_ids or "[]"))
    except json.JSONDecodeError:
        return []


def build_report_context(
    report_type: ReportType,
    period: tuple[date, date],
    work_logs: list[WorkLog],
    ai_content: str,
) -> dict[str, object]:
    title = f"{period[0]} 至 {period[1]} {REPORT_TITLES[report_type]}"
    results = [item.result for item in work_logs if item.result]
    blockers = [item.blockers for item in work_logs if item.blockers]
    work_item_rows: list[object] = []
    for item in work_logs:
        date_label = (
            str(item.start_date)
            if item.start_date == item.end_date
            else f"{item.start_date} 至 {item.end_date}"
        )
        work_item_rows.append(
            {
                "date": date_label,
                "start_date": str(item.start_date),
                "end_date": str(item.end_date),
                "project": item.project,
                "task": item.task,
                "status": item.progress,
                "content": f"[{item.project}] {item.task}",
                "progress": item.progress,
                "conclusion": item.result or item.notes or "暂无",
                "result": item.result or "",
                "blockers": item.blockers or "",
                "hours": item.hours,
                "priority": item.priority,
                "notes": item.notes or "",
            }
        )
    work_items = TemplateList(
        work_item_rows,
        format_logs_for_prompt(work_logs) or "- 暂无工作记录",
    )
    highlights = TemplateList(results, "\n".join(f"- {item}" for item in results) or "- 暂无")
    blocker_items = TemplateList(blockers, "\n".join(f"- {item}" for item in blockers) or "- 暂无")
    next_step_items = ["继续推进未完成事项"]
    next_steps = TemplateList(next_step_items, "\n".join(f"- {item}" for item in next_step_items))
    return {
        "title": title,
        "report_type": report_type.value,
        "period_start": str(period[0]),
        "period_end": str(period[1]),
        "generated_at": format_local_datetime(utcnow()),
        "ai_content": ai_content,
        "summary": ai_content,
        "work_items": work_items,
        "highlights": highlights,
        "blockers": blocker_items,
        "next_steps": next_steps,
        "raw_llm_content": ai_content,
    }


def create_report(
    db: Session,
    report_type: ReportType,
    anchor_date: date | None = None,
    period_start: date | None = None,
    period_end: date | None = None,
    template_id: int | None = None,
    overwrite: bool = False,
    llm_client: LLMClient | None = None,
) -> tuple[Report, GenerationTask, bool]:
    period = resolve_period(report_type, anchor_date, period_start, period_end)
    existing = db.scalar(
        select(Report).where(
            Report.report_type == report_type.value,
            Report.period_start == period[0],
            Report.period_end == period[1],
        )
    )
    if existing and not overwrite:
        task = GenerationTask(
            report_type=report_type.value,
            period_start=period[0],
            period_end=period[1],
            status=TaskStatus.SKIPPED.value,
            message="Report already exists; existing draft was not overwritten.",
            report_id=existing.id,
            completed_at=utcnow(),
        )
        db.add(task)
        db.commit()
        db.refresh(task)
        return existing, task, False

    generation_key = (report_type.value, period[0], period[1])
    with _ACTIVE_GENERATION_LOCK:
        if generation_key in _ACTIVE_GENERATION_KEYS:
            raise ValueError("Report generation is already running for this period")
        _ACTIVE_GENERATION_KEYS.add(generation_key)

    task: GenerationTask | None = None
    try:
        pending_task = db.scalar(
            select(GenerationTask).where(
                GenerationTask.report_type == report_type.value,
                GenerationTask.period_start == period[0],
                GenerationTask.period_end == period[1],
                GenerationTask.status == TaskStatus.PENDING.value,
            )
        )
        if pending_task:
            raise ValueError("Report generation is already running for this period")

        task = GenerationTask(
            report_type=report_type.value,
            period_start=period[0],
            period_end=period[1],
            status=TaskStatus.PENDING.value,
        )
        db.add(task)
        db.commit()
        db.refresh(task)

        template = get_template_for_report(db, report_type, template_id)
        validate_template_content(template.content)
        logs = list_work_logs_for_period(db, period[0], period[1])
        client = llm_client or LLMClient()
        setting = active_llm_setting(db)
        if requires_llm_template_fill(template.content):
            result = client.fill_template(
                setting,
                REPORT_TITLES[report_type],
                period,
                logs,
                template.content,
            )
            rendered = result.content
            title = f"{period[0]} 至 {period[1]} {REPORT_TITLES[report_type]}"
        else:
            result = client.generate(setting, REPORT_TITLES[report_type], period, logs)
            context = build_report_context(report_type, period, logs, result.content)
            rendered = render_template(template.content, context)
            title = str(context["title"])
        source_log_ids = json.dumps([item.id for item in logs])

        if existing:
            existing.title = title
            existing.template_id = template.id
            existing.content_markdown = rendered
            existing.status = ReportStatus.DRAFT.value
            existing.source_log_ids = source_log_ids
            existing.generated_at = utcnow()
            report = existing
        else:
            report = Report(
                report_type=report_type.value,
                title=title,
                period_start=period[0],
                period_end=period[1],
                template_id=template.id,
                content_markdown=rendered,
                status=ReportStatus.DRAFT.value,
                source_log_ids=source_log_ids,
                generated_at=utcnow(),
            )
            db.add(report)

        db.flush()
        task.status = TaskStatus.SUCCESS.value
        task.report_id = report.id
        task.completed_at = utcnow()
        db.commit()
        db.refresh(report)
        db.refresh(task)
        return report, task, result.used_llm
    except Exception as exc:
        db.rollback()
        failed_task = db.get(GenerationTask, task.id) if task else None
        if failed_task and failed_task.status == TaskStatus.PENDING.value:
            failed_task.status = TaskStatus.FAILED.value
            failed_task.message = str(exc)[:1000]
            failed_task.completed_at = utcnow()
            db.commit()
        raise
    finally:
        with _ACTIVE_GENERATION_LOCK:
            _ACTIVE_GENERATION_KEYS.discard(generation_key)


def create_missing_report(db: Session, report_type: ReportType, anchor_date: date) -> Report | None:
    period = resolve_period(report_type, anchor_date)
    existing = db.scalar(
        select(Report).where(
            Report.report_type == report_type.value,
            Report.period_start == period[0],
            Report.period_end == period[1],
        )
    )
    if existing:
        return None
    report, _, _ = create_report(db, report_type=report_type, anchor_date=anchor_date, overwrite=False)
    return report
