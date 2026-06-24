from datetime import date, datetime
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from app.constants import LLMProvider, PROVIDER_DEFAULT_BASE_URLS, ReportType


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
    provider: LLMProvider
    base_url: str
    model: str
    api_key: str | None
    extra_headers: dict[str, str] = Field(default_factory=dict)

    @field_validator("api_key")
    @classmethod
    def mask_key(cls, value: str | None) -> str | None:
        if not value:
            return None
        if len(value) <= 8:
            return "********"
        return f"{value[:4]}...{value[-4:]}"


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
