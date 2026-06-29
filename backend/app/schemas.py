from datetime import date, datetime, time
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from app.constants import EmailDeliveryStatus, EmailSecurity, LLMProvider, PROVIDER_DEFAULT_BASE_URLS, ReportType


def normalize_email_address(value: str) -> str:
    normalized = value.strip().lower()
    local, separator, domain = normalized.partition("@")
    if (
        not separator
        or not local
        or not domain
        or "@" in domain
        or any(character.isspace() for character in normalized)
        or "." not in domain
    ):
        raise ValueError("a valid email address is required")
    return normalized


class WorkLogBase(BaseModel):
    start_date: date | None = None
    end_date: date | None = None
    work_date: date | None = None
    project: str = Field(min_length=1, max_length=160)
    task: str = Field(min_length=1, max_length=240)
    progress: str = Field(min_length=1)
    result: str | None = None
    blockers: str | None = None
    hours: float | None = Field(default=None, ge=0, le=24)
    priority: Literal["low", "medium", "high", "urgent"] = "medium"
    notes: str | None = None

    @model_validator(mode="after")
    def resolve_dates(self) -> "WorkLogBase":
        start = self.start_date or self.work_date
        if not start:
            raise ValueError("start_date is required")
        end = self.end_date or start
        if end < start:
            raise ValueError("end_date must be on or after start_date")
        self.start_date = start
        self.end_date = end
        self.work_date = start
        return self


class WorkLogCreate(WorkLogBase):
    pass


class WorkLogUpdate(BaseModel):
    start_date: date | None = None
    end_date: date | None = None
    work_date: date | None = None
    project: str | None = Field(default=None, min_length=1, max_length=160)
    task: str | None = Field(default=None, min_length=1, max_length=240)
    progress: str | None = Field(default=None, min_length=1)
    result: str | None = None
    blockers: str | None = None
    hours: float | None = Field(default=None, ge=0, le=24)
    priority: Literal["low", "medium", "high", "urgent"] | None = None
    notes: str | None = None


class WorkLogRead(WorkLogBase):
    id: int
    created_at: datetime
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)


class PaginatedWorkLogs(BaseModel):
    items: list[WorkLogRead]
    total: int
    page: int
    page_size: int
    total_pages: int


class TemplateBase(BaseModel):
    name: str = Field(min_length=1, max_length=160)
    template_type: ReportType
    content: str = Field(min_length=1)
    is_default: bool = False


class TemplateCreate(TemplateBase):
    pass


class TemplateUpdate(BaseModel):
    name: str | None = Field(default=None, min_length=1, max_length=160)
    template_type: ReportType | None = None
    content: str | None = Field(default=None, min_length=1)
    is_default: bool | None = None


class TemplateImportExampleRequest(BaseModel):
    template_type: ReportType
    example_content: str = Field(min_length=20)


class TemplateImportExampleResponse(BaseModel):
    template_type: ReportType
    content: str
    used_llm: bool = True


class TemplateOptimizeRequest(BaseModel):
    template_type: ReportType
    content: str = Field(min_length=1)
    optimization_request: str = Field(min_length=2, max_length=1000)

    @field_validator("optimization_request")
    @classmethod
    def normalize_optimization_request(cls, value: str) -> str:
        normalized = value.strip()
        if len(normalized) < 2:
            raise ValueError("optimization request must contain at least 2 characters")
        return normalized


class TemplateOptimizeResponse(BaseModel):
    template_type: ReportType
    content: str
    used_llm: bool = True


class TemplateRead(TemplateBase):
    id: int
    created_at: datetime
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)


class LLMSettingBase(BaseModel):
    provider: LLMProvider = LLMProvider.OPENAI
    base_url: str | None = None
    model: str = Field(default="gpt-4.1-mini", min_length=1, max_length=160)
    api_key: str | None = None
    extra_headers: dict[str, str] = Field(default_factory=dict)
    timeout_seconds: int = Field(default=60, ge=5, le=600)

    @field_validator("base_url")
    @classmethod
    def empty_base_url_to_none(cls, value: str | None) -> str | None:
        if value is not None and not value.strip():
            return None
        return value

    def resolved_base_url(self) -> str:
        return self.base_url or PROVIDER_DEFAULT_BASE_URLS[self.provider]


class LLMSettingUpdate(LLMSettingBase):
    pass


class LLMSettingRead(BaseModel):
    id: int
    provider: LLMProvider
    base_url: str
    model: str
    api_key: str | None
    extra_headers: dict[str, str] = Field(default_factory=dict)
    timeout_seconds: int
    is_active: bool
    created_at: datetime
    updated_at: datetime

    @field_validator("api_key")
    @classmethod
    def mask_key(cls, value: str | None) -> str | None:
        if not value:
            return None
        if len(value) <= 8:
            return "********"
        return f"{value[:4]}...{value[-4:]}"


class EmailSettingBase(BaseModel):
    host: str = Field(min_length=1, max_length=255)
    port: int = Field(default=587, ge=1, le=65535)
    security: EmailSecurity = EmailSecurity.STARTTLS
    username: str = Field(min_length=1, max_length=320)
    password: str | None = Field(default=None, max_length=1024)
    sender_address: str = Field(min_length=3, max_length=320)
    sender_name: str | None = Field(default=None, max_length=160)

    @field_validator("sender_address")
    @classmethod
    def validate_sender_address(cls, value: str) -> str:
        return normalize_email_address(value)

    @field_validator("host", "username", "sender_name")
    @classmethod
    def trim_email_setting_text(cls, value: str | None) -> str | None:
        if value is None:
            return None
        trimmed = value.strip()
        if not trimmed and value is not None:
            raise ValueError("a value is required")
        return trimmed

    @field_validator("password")
    @classmethod
    def empty_password_to_none(cls, value: str | None) -> str | None:
        if value is None or not value.strip():
            return None
        return value


class EmailSettingUpdate(EmailSettingBase):
    pass


class EmailSettingRead(BaseModel):
    host: str
    port: int
    security: EmailSecurity
    username: str
    password: str | None
    sender_address: str
    sender_name: str | None

    @field_validator("password")
    @classmethod
    def mask_password(cls, value: str | None) -> str | None:
        if not value:
            return None
        if len(value) <= 8:
            return "********"
        return f"{value[:4]}...{value[-4:]}"


class EmailTestRequest(BaseModel):
    recipient_email: str = Field(min_length=3, max_length=320)

    @field_validator("recipient_email")
    @classmethod
    def validate_recipient_email(cls, value: str) -> str:
        return normalize_email_address(value)


class EmailTestResponse(BaseModel):
    sent: bool = True


class RecipientBase(BaseModel):
    name: str = Field(min_length=1, max_length=160)
    email: str = Field(min_length=3, max_length=320)
    is_default: bool = False

    @field_validator("name")
    @classmethod
    def trim_name(cls, value: str) -> str:
        trimmed = value.strip()
        if not trimmed:
            raise ValueError("a name is required")
        return trimmed

    @field_validator("email")
    @classmethod
    def validate_contact_email(cls, value: str) -> str:
        return normalize_email_address(value)


class RecipientCreate(RecipientBase):
    pass


class RecipientUpdate(BaseModel):
    name: str | None = Field(default=None, min_length=1, max_length=160)
    email: str | None = Field(default=None, min_length=3, max_length=320)
    is_default: bool | None = None

    @field_validator("name")
    @classmethod
    def trim_updated_name(cls, value: str | None) -> str | None:
        if value is None:
            return None
        trimmed = value.strip()
        if not trimmed:
            raise ValueError("a name is required")
        return trimmed

    @field_validator("email")
    @classmethod
    def validate_updated_contact_email(cls, value: str | None) -> str | None:
        return normalize_email_address(value) if value is not None else None


class RecipientRead(RecipientBase):
    id: int
    created_at: datetime
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)


class ReportScheduleUpdate(BaseModel):
    enabled: bool
    weekday: Literal["mon", "tue", "wed", "thu", "fri", "sat", "sun"] | None = None
    day_of_month: int | None = Field(default=None, ge=1, le=28)
    template_id: int | None = Field(default=None, gt=0)
    run_time: time
    auto_send: bool = False
    recipient_ids: list[int] = Field(default_factory=list, max_length=50)

    @field_validator("recipient_ids")
    @classmethod
    def validate_recipient_ids(cls, values: list[int]) -> list[int]:
        if any(value <= 0 for value in values):
            raise ValueError("recipient IDs must be positive")
        return list(dict.fromkeys(values))


class ReportScheduleRead(ReportScheduleUpdate):
    id: int
    report_type: ReportType
    next_run_at: datetime | None
    created_at: datetime
    updated_at: datetime


class DeliveryRecipient(BaseModel):
    name: str | None = None
    email: str


class ReportEmailSendRequest(BaseModel):
    recipient_ids: list[int] = Field(default_factory=list, max_length=50)
    additional_recipients: list[str] = Field(default_factory=list, max_length=50)
    subject: str = Field(min_length=1, max_length=240)

    @field_validator("additional_recipients")
    @classmethod
    def validate_additional_recipients(cls, values: list[str]) -> list[str]:
        return [normalize_email_address(value) for value in values]

    @field_validator("subject")
    @classmethod
    def trim_subject(cls, value: str) -> str:
        trimmed = value.strip()
        if not trimmed:
            raise ValueError("a subject is required")
        return trimmed

    @model_validator(mode="after")
    def require_recipient(self) -> "ReportEmailSendRequest":
        if not self.recipient_ids and not self.additional_recipients:
            raise ValueError("at least one recipient is required")
        if any(item <= 0 for item in self.recipient_ids):
            raise ValueError("recipient IDs must be positive")
        return self


class ReportEmailDeliveryRead(BaseModel):
    id: int
    report_id: int
    subject: str
    recipients: list[DeliveryRecipient]
    status: EmailDeliveryStatus
    error_message: str | None
    sent_at: datetime | None
    created_at: datetime
    updated_at: datetime


class ReportRead(BaseModel):
    id: int
    report_type: ReportType
    title: str
    period_start: date
    period_end: date
    template_id: int | None
    content_markdown: str
    status: str
    source_log_ids: list[int]
    generated_at: datetime | None
    edited_at: datetime | None
    created_at: datetime
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)


class ReportUpdate(BaseModel):
    title: str | None = Field(default=None, min_length=1, max_length=240)
    content_markdown: str | None = Field(default=None, min_length=1)


class ReportGenerateRequest(BaseModel):
    report_type: ReportType
    anchor_date: date | None = None
    period_start: date | None = None
    period_end: date | None = None
    template_id: int | None = None
    overwrite: bool = False

    @field_validator("period_end")
    @classmethod
    def period_end_requires_start(cls, value: date | None, info: Any) -> date | None:
        if value and not info.data.get("period_start"):
            raise ValueError("period_start is required when period_end is provided")
        return value


class GenerateResponse(BaseModel):
    report: ReportRead
    task_id: int
    used_llm: bool


class HealthResponse(BaseModel):
    ok: bool
    app: str
