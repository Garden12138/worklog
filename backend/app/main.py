import json
from contextlib import asynccontextmanager
from datetime import date

from fastapi import Depends, FastAPI, HTTPException, Query
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import StreamingResponse
from sqlalchemy import delete, func, select
from sqlalchemy.orm import Session

from app.config import settings
from app.constants import ReportType
from app.database import SessionLocal, get_db, init_db
from app.models import GenerationTask, LLMSetting, Report, Template, WorkLog, utcnow
from app.scheduler import start_scheduler, stop_scheduler
from app.schemas import (
    GenerateResponse,
    HealthResponse,
    LLMSettingRead,
    LLMSettingUpdate,
    PaginatedWorkLogs,
    ReportGenerateRequest,
    ReportRead,
    ReportUpdate,
    TemplateCreate,
    TemplateImportExampleRequest,
    TemplateImportExampleResponse,
    TemplateRead,
    TemplateUpdate,
    WorkLogCreate,
    WorkLogRead,
    WorkLogUpdate,
)
from app.services.docx_export import markdown_to_docx
from app.services.llm import LLMClient
from app.services.reports import create_report, report_to_dict_source_ids, seed_default_templates
from app.services.reports import active_llm_setting
from app.services.templates import TemplateValidationError, validate_template_content


@asynccontextmanager
async def lifespan(app: FastAPI):
    init_db()
    with SessionLocal() as db:
        seed_default_templates(db)
    start_scheduler()
    yield
    stop_scheduler()


app = FastAPI(title=settings.app_name, version="0.1.0", lifespan=lifespan)
app.add_middleware(
    CORSMiddleware,
    allow_origins=settings.cors_origins,
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


def report_read(report: Report) -> ReportRead:
    return ReportRead.model_validate(
        {
            **report.__dict__,
            "report_type": report.report_type,
            "source_log_ids": report_to_dict_source_ids(report),
        }
    )


@app.get("/health", response_model=HealthResponse)
def health() -> HealthResponse:
    return HealthResponse(ok=True, app=settings.app_name)


@app.get("/api/work-logs", response_model=PaginatedWorkLogs)
def list_work_logs(
    start: date | None = Query(default=None),
    end: date | None = Query(default=None),
    page: int = Query(default=1, ge=1),
    page_size: int = Query(default=10, ge=1, le=100),
    db: Session = Depends(get_db),
) -> PaginatedWorkLogs:
    stmt = select(WorkLog)
    if start:
        stmt = stmt.where(WorkLog.end_date >= start)
    if end:
        stmt = stmt.where(WorkLog.start_date <= end)
    total = db.scalar(select(func.count()).select_from(stmt.subquery())) or 0
    items = list(
        db.scalars(
            stmt.order_by(WorkLog.start_date.desc(), WorkLog.end_date.desc(), WorkLog.id.desc())
            .offset((page - 1) * page_size)
            .limit(page_size)
        )
    )
    total_pages = max(1, (total + page_size - 1) // page_size)
    return PaginatedWorkLogs(items=items, total=total, page=page, page_size=page_size, total_pages=total_pages)


@app.post("/api/work-logs", response_model=WorkLogRead, status_code=201)
def create_work_log(payload: WorkLogCreate, db: Session = Depends(get_db)) -> WorkLog:
    item = WorkLog(**payload.model_dump())
    db.add(item)
    db.commit()
    db.refresh(item)
    return item


@app.put("/api/work-logs/{work_log_id}", response_model=WorkLogRead)
def update_work_log(work_log_id: int, payload: WorkLogUpdate, db: Session = Depends(get_db)) -> WorkLog:
    item = db.get(WorkLog, work_log_id)
    if not item:
        raise HTTPException(status_code=404, detail="Work log not found")
    update_data = payload.model_dump(exclude_unset=True)
    if "work_date" in update_data and "start_date" not in update_data:
        update_data["start_date"] = update_data["work_date"]
    start_date = update_data.get("start_date", item.start_date)
    end_date = update_data.get("end_date", item.end_date)
    if "start_date" in update_data and "end_date" not in update_data:
        end_date = start_date
        update_data["end_date"] = end_date
    if end_date < start_date:
        raise HTTPException(status_code=422, detail="end_date must be on or after start_date")
    update_data["work_date"] = start_date
    for key, value in update_data.items():
        setattr(item, key, value)
    db.commit()
    db.refresh(item)
    return item


@app.delete("/api/work-logs/{work_log_id}", status_code=204)
def delete_work_log(work_log_id: int, db: Session = Depends(get_db)) -> None:
    item = db.get(WorkLog, work_log_id)
    if not item:
        raise HTTPException(status_code=404, detail="Work log not found")
    db.delete(item)
    db.commit()


@app.get("/api/templates", response_model=list[TemplateRead])
def list_templates(
    template_type: ReportType | None = Query(default=None),
    db: Session = Depends(get_db),
) -> list[Template]:
    stmt = select(Template)
    if template_type:
        stmt = stmt.where(Template.template_type == template_type.value)
    return list(db.scalars(stmt.order_by(Template.template_type.asc(), Template.is_default.desc())))


@app.post("/api/templates", response_model=TemplateRead, status_code=201)
def create_template(payload: TemplateCreate, db: Session = Depends(get_db)) -> Template:
    try:
        validate_template_content(payload.content)
    except TemplateValidationError as exc:
        raise HTTPException(status_code=422, detail=str(exc)) from exc
    if payload.is_default:
        db.query(Template).filter(Template.template_type == payload.template_type.value).update(
            {"is_default": False}
        )
    item = Template(
        name=payload.name,
        template_type=payload.template_type.value,
        content=payload.content,
        is_default=payload.is_default,
    )
    db.add(item)
    db.commit()
    db.refresh(item)
    return item


@app.post("/api/templates/import-example", response_model=TemplateImportExampleResponse)
def import_template_from_example(
    payload: TemplateImportExampleRequest,
    db: Session = Depends(get_db),
) -> TemplateImportExampleResponse:
    try:
        result = LLMClient().template_from_example(
            active_llm_setting(db),
            payload.template_type,
            payload.example_content,
        )
        validate_template_content(result.content)
    except TemplateValidationError as exc:
        raise HTTPException(status_code=422, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return TemplateImportExampleResponse(
        template_type=payload.template_type,
        content=result.content,
        used_llm=result.used_llm,
    )


@app.put("/api/templates/{template_id}", response_model=TemplateRead)
def update_template(template_id: int, payload: TemplateUpdate, db: Session = Depends(get_db)) -> Template:
    item = db.get(Template, template_id)
    if not item:
        raise HTTPException(status_code=404, detail="Template not found")
    update_data = payload.model_dump(exclude_unset=True)
    content = update_data.get("content", item.content)
    try:
        validate_template_content(content)
    except TemplateValidationError as exc:
        raise HTTPException(status_code=422, detail=str(exc)) from exc
    new_type = update_data.get("template_type", item.template_type)
    if hasattr(new_type, "value"):
        new_type = new_type.value
    if update_data.get("is_default"):
        db.query(Template).filter(Template.template_type == new_type).update({"is_default": False})
    for key, value in update_data.items():
        setattr(item, key, value.value if hasattr(value, "value") else value)
    db.commit()
    db.refresh(item)
    return item


@app.delete("/api/templates/{template_id}", status_code=204)
def delete_template(template_id: int, db: Session = Depends(get_db)) -> None:
    item = db.get(Template, template_id)
    if not item:
        raise HTTPException(status_code=404, detail="Template not found")
    same_type_count = db.scalar(
        select(func.count()).select_from(Template).where(Template.template_type == item.template_type)
    )
    if same_type_count == 1:
        raise HTTPException(status_code=400, detail="Cannot delete the last template for a report type")
    db.delete(item)
    db.commit()


@app.get("/api/reports", response_model=list[ReportRead])
def list_reports(
    report_type: ReportType | None = Query(default=None),
    db: Session = Depends(get_db),
) -> list[ReportRead]:
    stmt = select(Report)
    if report_type:
        stmt = stmt.where(Report.report_type == report_type.value)
    reports = db.scalars(stmt.order_by(Report.period_start.desc(), Report.id.desc()))
    return [report_read(item) for item in reports]


@app.get("/api/reports/{report_id}", response_model=ReportRead)
def get_report(report_id: int, db: Session = Depends(get_db)) -> ReportRead:
    item = db.get(Report, report_id)
    if not item:
        raise HTTPException(status_code=404, detail="Report not found")
    return report_read(item)


@app.put("/api/reports/{report_id}", response_model=ReportRead)
def update_report(report_id: int, payload: ReportUpdate, db: Session = Depends(get_db)) -> ReportRead:
    item = db.get(Report, report_id)
    if not item:
        raise HTTPException(status_code=404, detail="Report not found")
    for key, value in payload.model_dump(exclude_unset=True).items():
        setattr(item, key, value)
    item.edited_at = utcnow()
    db.commit()
    db.refresh(item)
    return report_read(item)


@app.delete("/api/reports/{report_id}", status_code=204)
def delete_report(report_id: int, db: Session = Depends(get_db)) -> None:
    if not db.get(Report, report_id):
        raise HTTPException(status_code=404, detail="Report not found")
    db.query(GenerationTask).filter(GenerationTask.report_id == report_id).update(
        {"report_id": None},
        synchronize_session=False,
    )
    result = db.execute(delete(Report).where(Report.id == report_id))
    db.commit()
    if result.rowcount == 0:
        raise HTTPException(status_code=404, detail="Report not found")


@app.post("/api/reports/generate", response_model=GenerateResponse)
def generate_report(payload: ReportGenerateRequest, db: Session = Depends(get_db)) -> GenerateResponse:
    try:
        report, task, used_llm = create_report(
            db,
            report_type=payload.report_type,
            anchor_date=payload.anchor_date,
            period_start=payload.period_start,
            period_end=payload.period_end,
            template_id=payload.template_id,
            overwrite=payload.overwrite,
        )
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return GenerateResponse(report=report_read(report), task_id=task.id, used_llm=used_llm)


@app.get("/api/reports/{report_id}/export/docx")
def export_report_docx(report_id: int, db: Session = Depends(get_db)) -> StreamingResponse:
    item = db.get(Report, report_id)
    if not item:
        raise HTTPException(status_code=404, detail="Report not found")
    buffer = markdown_to_docx(item.content_markdown)
    filename = f"worklog-{item.report_type}-{item.period_start}-{item.period_end}.docx"
    return StreamingResponse(
        buffer,
        media_type="application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        headers={"Content-Disposition": f'attachment; filename="{filename}"'},
    )


@app.get("/api/settings/llm", response_model=LLMSettingRead | None)
def get_llm_setting(db: Session = Depends(get_db)) -> LLMSettingRead | None:
    item = db.scalar(select(LLMSetting).where(LLMSetting.is_active.is_(True)).order_by(LLMSetting.id.desc()))
    if not item:
        return None
    return LLMSettingRead(
        provider=item.provider,
        base_url=item.base_url,
        model=item.model,
        api_key=item.api_key,
        extra_headers=json.loads(item.extra_headers or "{}"),
    )


@app.put("/api/settings/llm", response_model=LLMSettingRead)
def update_llm_setting(payload: LLMSettingUpdate, db: Session = Depends(get_db)) -> LLMSettingRead:
    previous = db.scalar(
        select(LLMSetting).where(LLMSetting.is_active.is_(True)).order_by(LLMSetting.id.desc())
    )
    db.query(LLMSetting).update({"is_active": False})
    base_url = payload.resolved_base_url()
    api_key = payload.api_key if payload.api_key else previous.api_key if previous else None
    item = LLMSetting(
        provider=payload.provider.value,
        base_url=base_url,
        model=payload.model,
        api_key=api_key,
        extra_headers=json.dumps(payload.extra_headers),
        is_active=True,
    )
    db.add(item)
    db.commit()
    db.refresh(item)
    return LLMSettingRead(
        provider=item.provider,
        base_url=item.base_url,
        model=item.model,
        api_key=item.api_key,
        extra_headers=json.loads(item.extra_headers or "{}"),
    )
