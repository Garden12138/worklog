import json
from contextlib import asynccontextmanager
from datetime import date

from fastapi import Depends, FastAPI, HTTPException, Query
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import StreamingResponse
from sqlalchemy import delete, func, select
from sqlalchemy.orm import Session

from app.config import settings
from app.constants import EmailDeliveryStatus, ReportType
from app.database import SessionLocal, get_db, init_db
from app.models import (
    EmailSetting,
    GenerationTask,
    LLMSetting,
    Recipient,
    Report,
    ReportEmailDelivery,
    Template,
    WorkLog,
    utcnow,
)
from app.scheduler import start_scheduler, stop_scheduler
from app.schemas import (
    GenerateResponse,
    HealthResponse,
    EmailSettingRead,
    EmailSettingUpdate,
    EmailTestRequest,
    EmailTestResponse,
    LLMSettingRead,
    LLMSettingUpdate,
    PaginatedWorkLogs,
    RecipientCreate,
    RecipientRead,
    RecipientUpdate,
    ReportEmailDeliveryRead,
    ReportEmailSendRequest,
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
from app.services.email import EmailDeliveryError, markdown_to_email_html, send_email
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


def email_setting_read(setting: EmailSetting) -> EmailSettingRead:
    return EmailSettingRead(
        host=setting.host,
        port=setting.port,
        security=setting.security,
        username=setting.username,
        password=setting.password,
        sender_address=setting.sender_address,
        sender_name=setting.sender_name,
    )


def report_email_delivery_read(delivery: ReportEmailDelivery) -> ReportEmailDeliveryRead:
    try:
        recipients = json.loads(delivery.recipients_json)
    except json.JSONDecodeError:
        recipients = []
    return ReportEmailDeliveryRead(
        id=delivery.id,
        report_id=delivery.report_id,
        subject=delivery.subject,
        recipients=recipients,
        status=delivery.status,
        error_message=delivery.error_message,
        sent_at=delivery.sent_at,
        created_at=delivery.created_at,
        updated_at=delivery.updated_at,
    )


def active_email_setting(db: Session) -> EmailSetting | None:
    return db.scalar(select(EmailSetting).where(EmailSetting.is_active.is_(True)).order_by(EmailSetting.id.desc()))


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
    db.query(ReportEmailDelivery).filter(ReportEmailDelivery.report_id == report_id).delete(
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


@app.get("/api/reports/{report_id}/email-deliveries", response_model=list[ReportEmailDeliveryRead])
def list_report_email_deliveries(
    report_id: int,
    db: Session = Depends(get_db),
) -> list[ReportEmailDeliveryRead]:
    if not db.get(Report, report_id):
        raise HTTPException(status_code=404, detail="Report not found")
    deliveries = db.scalars(
        select(ReportEmailDelivery)
        .where(ReportEmailDelivery.report_id == report_id)
        .order_by(ReportEmailDelivery.created_at.desc(), ReportEmailDelivery.id.desc())
    )
    return [report_email_delivery_read(item) for item in deliveries]


@app.post("/api/reports/{report_id}/send-email", response_model=ReportEmailDeliveryRead)
def send_report_email(
    report_id: int,
    payload: ReportEmailSendRequest,
    db: Session = Depends(get_db),
) -> ReportEmailDeliveryRead:
    report = db.get(Report, report_id)
    if not report:
        raise HTTPException(status_code=404, detail="Report not found")
    setting = active_email_setting(db)
    if not setting:
        raise HTTPException(status_code=400, detail="请先在设置中完成 SMTP 邮箱配置")

    selected_ids = list(dict.fromkeys(payload.recipient_ids))
    contacts = list(db.scalars(select(Recipient).where(Recipient.id.in_(selected_ids)))) if selected_ids else []
    found_ids = {contact.id for contact in contacts}
    if found_ids != set(selected_ids):
        raise HTTPException(status_code=422, detail="存在已删除或无效的收件人")

    snapshots: list[dict[str, str | None]] = []
    recipient_addresses: list[str] = []
    seen_addresses: set[str] = set()
    for contact in contacts:
        if contact.email not in seen_addresses:
            snapshots.append({"name": contact.name, "email": contact.email})
            recipient_addresses.append(contact.email)
            seen_addresses.add(contact.email)
    for address in payload.additional_recipients:
        if address not in seen_addresses:
            snapshots.append({"name": None, "email": address})
            recipient_addresses.append(address)
            seen_addresses.add(address)
    if not recipient_addresses:
        raise HTTPException(status_code=422, detail="至少需要一位有效收件人")

    delivery = ReportEmailDelivery(
        report_id=report.id,
        subject=payload.subject,
        recipients_json=json.dumps(snapshots, ensure_ascii=False),
        content_markdown=report.content_markdown,
        status=EmailDeliveryStatus.PENDING.value,
    )
    db.add(delivery)
    db.commit()
    db.refresh(delivery)

    filename = f"worklog-{report.report_type}-{report.period_start}-{report.period_end}.docx"
    try:
        send_email(
            setting,
            recipient_addresses,
            payload.subject,
            report.content_markdown,
            markdown_to_email_html(report.content_markdown),
            filename,
            markdown_to_docx(report.content_markdown).getvalue(),
        )
    except EmailDeliveryError as exc:
        delivery.status = EmailDeliveryStatus.FAILED.value
        delivery.error_message = str(exc)
        db.commit()
        db.refresh(delivery)
        raise HTTPException(status_code=502, detail=str(exc)) from exc

    delivery.status = EmailDeliveryStatus.SENT.value
    delivery.sent_at = utcnow()
    db.commit()
    db.refresh(delivery)
    return report_email_delivery_read(delivery)


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


@app.get("/api/settings/email", response_model=EmailSettingRead | None)
def get_email_setting(db: Session = Depends(get_db)) -> EmailSettingRead | None:
    item = active_email_setting(db)
    return email_setting_read(item) if item else None


@app.put("/api/settings/email", response_model=EmailSettingRead)
def update_email_setting(payload: EmailSettingUpdate, db: Session = Depends(get_db)) -> EmailSettingRead:
    previous = active_email_setting(db)
    password = payload.password or (previous.password if previous else None)
    if not password:
        raise HTTPException(status_code=422, detail="SMTP 密码不能为空")
    db.query(EmailSetting).update({"is_active": False})
    item = EmailSetting(
        host=payload.host,
        port=payload.port,
        security=payload.security.value,
        username=payload.username,
        password=password,
        sender_address=payload.sender_address,
        sender_name=payload.sender_name or None,
        is_active=True,
    )
    db.add(item)
    db.commit()
    db.refresh(item)
    return email_setting_read(item)


@app.post("/api/settings/email/test", response_model=EmailTestResponse)
def send_email_test(payload: EmailTestRequest, db: Session = Depends(get_db)) -> EmailTestResponse:
    setting = active_email_setting(db)
    if not setting:
        raise HTTPException(status_code=400, detail="请先保存 SMTP 邮箱配置")
    try:
        send_email(
            setting,
            [payload.recipient_email],
            "Worklog SMTP 测试邮件",
            "这是一封来自 Worklog 的 SMTP 测试邮件。",
            "<p>这是一封来自 <strong>Worklog</strong> 的 SMTP 测试邮件。</p>",
        )
    except EmailDeliveryError as exc:
        raise HTTPException(status_code=502, detail=str(exc)) from exc
    return EmailTestResponse()


@app.get("/api/recipients", response_model=list[RecipientRead])
def list_recipients(db: Session = Depends(get_db)) -> list[Recipient]:
    return list(db.scalars(select(Recipient).order_by(Recipient.is_default.desc(), Recipient.name.asc(), Recipient.id.asc())))


@app.post("/api/recipients", response_model=RecipientRead, status_code=201)
def create_recipient(payload: RecipientCreate, db: Session = Depends(get_db)) -> Recipient:
    existing = db.scalar(select(Recipient).where(Recipient.email == payload.email))
    if existing:
        raise HTTPException(status_code=409, detail="该邮箱已在通讯录中")
    item = Recipient(**payload.model_dump())
    db.add(item)
    db.commit()
    db.refresh(item)
    return item


@app.put("/api/recipients/{recipient_id}", response_model=RecipientRead)
def update_recipient(
    recipient_id: int,
    payload: RecipientUpdate,
    db: Session = Depends(get_db),
) -> Recipient:
    item = db.get(Recipient, recipient_id)
    if not item:
        raise HTTPException(status_code=404, detail="Recipient not found")
    update_data = payload.model_dump(exclude_unset=True)
    email = update_data.get("email")
    if email and email != item.email:
        existing = db.scalar(select(Recipient).where(Recipient.email == email))
        if existing:
            raise HTTPException(status_code=409, detail="该邮箱已在通讯录中")
    for key, value in update_data.items():
        setattr(item, key, value)
    db.commit()
    db.refresh(item)
    return item


@app.delete("/api/recipients/{recipient_id}", status_code=204)
def delete_recipient(recipient_id: int, db: Session = Depends(get_db)) -> None:
    item = db.get(Recipient, recipient_id)
    if not item:
        raise HTTPException(status_code=404, detail="Recipient not found")
    db.delete(item)
    db.commit()
