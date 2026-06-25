from datetime import date, datetime, timezone

from sqlalchemy import Boolean, Date, DateTime, Float, ForeignKey, Index, Integer, String, Text
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.database import Base


def utcnow() -> datetime:
    return datetime.now(timezone.utc)


class TimestampMixin:
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), default=utcnow, onupdate=utcnow
    )


class WorkLog(TimestampMixin, Base):
    __tablename__ = "work_logs"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    work_date: Mapped[date] = mapped_column(Date, index=True)
    start_date: Mapped[date] = mapped_column(Date, index=True)
    end_date: Mapped[date] = mapped_column(Date, index=True)
    project: Mapped[str] = mapped_column(String(160), index=True)
    task: Mapped[str] = mapped_column(String(240))
    progress: Mapped[str] = mapped_column(Text)
    result: Mapped[str | None] = mapped_column(Text, default=None)
    blockers: Mapped[str | None] = mapped_column(Text, default=None)
    hours: Mapped[float | None] = mapped_column(Float, default=None)
    priority: Mapped[str] = mapped_column(String(32), default="medium")
    notes: Mapped[str | None] = mapped_column(Text, default=None)


class Template(TimestampMixin, Base):
    __tablename__ = "templates"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    name: Mapped[str] = mapped_column(String(160))
    template_type: Mapped[str] = mapped_column(String(48), index=True)
    content: Mapped[str] = mapped_column(Text)
    is_default: Mapped[bool] = mapped_column(Boolean, default=False)

    reports: Mapped[list["Report"]] = relationship("Report", back_populates="template")


class LLMSetting(TimestampMixin, Base):
    __tablename__ = "llm_settings"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    provider: Mapped[str] = mapped_column(String(48))
    base_url: Mapped[str] = mapped_column(String(500))
    model: Mapped[str] = mapped_column(String(160))
    api_key: Mapped[str | None] = mapped_column(Text, default=None)
    extra_headers: Mapped[str | None] = mapped_column(Text, default=None)
    is_active: Mapped[bool] = mapped_column(Boolean, default=True)


class EmailSetting(TimestampMixin, Base):
    __tablename__ = "email_settings"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    host: Mapped[str] = mapped_column(String(255))
    port: Mapped[int] = mapped_column(Integer)
    security: Mapped[str] = mapped_column(String(32), default="starttls")
    username: Mapped[str] = mapped_column(String(320))
    password: Mapped[str] = mapped_column(Text)
    sender_address: Mapped[str] = mapped_column(String(320))
    sender_name: Mapped[str | None] = mapped_column(String(160), default=None)
    is_active: Mapped[bool] = mapped_column(Boolean, default=True)


class Recipient(TimestampMixin, Base):
    __tablename__ = "recipients"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    name: Mapped[str] = mapped_column(String(160))
    email: Mapped[str] = mapped_column(String(320), unique=True, index=True)
    is_default: Mapped[bool] = mapped_column(Boolean, default=False)


class Report(TimestampMixin, Base):
    __tablename__ = "reports"
    __table_args__ = (
        Index("ix_reports_type_period", "report_type", "period_start", "period_end", unique=True),
    )

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    report_type: Mapped[str] = mapped_column(String(48))
    title: Mapped[str] = mapped_column(String(240))
    period_start: Mapped[date] = mapped_column(Date)
    period_end: Mapped[date] = mapped_column(Date)
    template_id: Mapped[int | None] = mapped_column(
        ForeignKey("templates.id", ondelete="SET NULL"), default=None
    )
    content_markdown: Mapped[str] = mapped_column(Text)
    status: Mapped[str] = mapped_column(String(32), default="draft")
    source_log_ids: Mapped[str] = mapped_column(Text, default="[]")
    generated_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), default=None)
    edited_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), default=None)

    template: Mapped[Template | None] = relationship("Template", back_populates="reports")


class ReportEmailDelivery(TimestampMixin, Base):
    __tablename__ = "report_email_deliveries"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    report_id: Mapped[int] = mapped_column(ForeignKey("reports.id", ondelete="CASCADE"), index=True)
    subject: Mapped[str] = mapped_column(String(240))
    recipients_json: Mapped[str] = mapped_column(Text)
    content_markdown: Mapped[str] = mapped_column(Text)
    status: Mapped[str] = mapped_column(String(32), default="pending")
    error_message: Mapped[str | None] = mapped_column(Text, default=None)
    sent_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), default=None)

    report: Mapped["Report"] = relationship("Report")


class GenerationTask(TimestampMixin, Base):
    __tablename__ = "generation_tasks"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    report_type: Mapped[str] = mapped_column(String(48))
    period_start: Mapped[date] = mapped_column(Date)
    period_end: Mapped[date] = mapped_column(Date)
    status: Mapped[str] = mapped_column(String(32), default="pending")
    message: Mapped[str | None] = mapped_column(Text, default=None)
    report_id: Mapped[int | None] = mapped_column(
        ForeignKey("reports.id", ondelete="SET NULL"), default=None
    )
    completed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), default=None)
