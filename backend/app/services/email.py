import html
import json
import re
import smtplib
import ssl
from collections.abc import Callable
from email.message import EmailMessage
from email.utils import formataddr

from sqlalchemy import select
from sqlalchemy.orm import Session

from app.constants import EmailDeliveryStatus
from app.models import EmailSetting, Recipient, Report, ReportEmailDelivery, utcnow
from app.services.docx_export import markdown_to_docx


class EmailDeliveryError(Exception):
    """An SMTP delivery failure that is safe to show to the user."""


class EmailConfigurationError(Exception):
    """The report cannot be delivered because SMTP is not configured."""


class RecipientSelectionError(Exception):
    """The requested recipient selection is empty or stale."""


def active_email_setting(db: Session) -> EmailSetting | None:
    return db.scalar(
        select(EmailSetting)
        .where(EmailSetting.is_active.is_(True))
        .order_by(EmailSetting.id.desc())
    )


def markdown_to_email_html(markdown: str) -> str:
    """Render the report's supported Markdown subset without trusting raw HTML."""
    blocks: list[str] = []
    list_items: list[str] = []
    list_tag: str | None = None

    def inline(value: str) -> str:
        escaped = html.escape(value, quote=False)
        escaped = re.sub(r"`([^`]+)`", r"<code>\1</code>", escaped)
        escaped = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", escaped)
        escaped = re.sub(r"\*([^*]+)\*", r"<em>\1</em>", escaped)
        return escaped

    def flush_list() -> None:
        nonlocal list_items, list_tag
        if list_tag and list_items:
            blocks.append(f"<{list_tag}>" + "".join(f"<li>{item}</li>" for item in list_items) + f"</{list_tag}>")
        list_items = []
        list_tag = None

    for raw_line in markdown.splitlines():
        line = raw_line.strip()
        if not line:
            flush_list()
            continue
        heading = re.match(r"^(#{1,4})\s+(.+)$", line)
        unordered = re.match(r"^[-*]\s+(.+)$", line)
        ordered = re.match(r"^\d+\.\s+(.+)$", line)
        if heading:
            flush_list()
            level = len(heading.group(1))
            blocks.append(f"<h{level}>{inline(heading.group(2))}</h{level}>")
        elif unordered or ordered:
            next_tag = "ul" if unordered else "ol"
            if list_tag and list_tag != next_tag:
                flush_list()
            list_tag = next_tag
            list_items.append(inline((unordered or ordered).group(1)))
        else:
            flush_list()
            blocks.append(f"<p>{inline(line)}</p>")
    flush_list()
    return """<!doctype html><html><head><meta charset=\"utf-8\"></head><body style=\"font-family:Arial,'Microsoft YaHei',sans-serif;line-height:1.65;color:#172033;max-width:760px;margin:auto\">""" + "\n".join(blocks) + "</body></html>"


def send_email(
    setting: EmailSetting,
    recipients: list[str],
    subject: str,
    text_body: str,
    html_body: str,
    attachment_filename: str | None = None,
    attachment_content: bytes | None = None,
) -> None:
    message = EmailMessage()
    message["Subject"] = subject
    message["From"] = formataddr((setting.sender_name or setting.sender_address, setting.sender_address))
    message["To"] = ", ".join(recipients)
    message.set_content(text_body)
    message.add_alternative(html_body, subtype="html")
    if attachment_filename and attachment_content is not None:
        message.add_attachment(
            attachment_content,
            maintype="application",
            subtype="vnd.openxmlformats-officedocument.wordprocessingml.document",
            filename=attachment_filename,
        )

    try:
        if setting.security == "ssl":
            with smtplib.SMTP_SSL(setting.host, setting.port, timeout=20, context=ssl.create_default_context()) as client:
                client.login(setting.username, setting.password)
                client.send_message(message)
        else:
            with smtplib.SMTP(setting.host, setting.port, timeout=20) as client:
                client.ehlo()
                client.starttls(context=ssl.create_default_context())
                client.ehlo()
                client.login(setting.username, setting.password)
                client.send_message(message)
    except (OSError, smtplib.SMTPException) as exc:
        raise EmailDeliveryError("邮件发送失败，请检查 SMTP 设置或稍后重试。") from exc


def deliver_report_email(
    db: Session,
    report: Report,
    recipient_ids: list[int],
    subject: str,
    additional_recipients: list[str] | None = None,
    sender: Callable[..., None] | None = None,
) -> ReportEmailDelivery:
    setting = active_email_setting(db)
    if not setting:
        raise EmailConfigurationError("请先在设置中完成 SMTP 邮箱配置")

    selected_ids = list(dict.fromkeys(recipient_ids))
    contacts = (
        list(db.scalars(select(Recipient).where(Recipient.id.in_(selected_ids))))
        if selected_ids
        else []
    )
    if {contact.id for contact in contacts} != set(selected_ids):
        raise RecipientSelectionError("存在已删除或无效的收件人")

    snapshots: list[dict[str, str | None]] = []
    recipient_addresses: list[str] = []
    seen_addresses: set[str] = set()
    for contact in contacts:
        if contact.email not in seen_addresses:
            snapshots.append({"name": contact.name, "email": contact.email})
            recipient_addresses.append(contact.email)
            seen_addresses.add(contact.email)
    for address in additional_recipients or []:
        if address not in seen_addresses:
            snapshots.append({"name": None, "email": address})
            recipient_addresses.append(address)
            seen_addresses.add(address)
    if not recipient_addresses:
        raise RecipientSelectionError("至少需要一位有效收件人")

    delivery = ReportEmailDelivery(
        report_id=report.id,
        subject=subject,
        recipients_json=json.dumps(snapshots, ensure_ascii=False),
        content_markdown=report.content_markdown,
        status=EmailDeliveryStatus.PENDING.value,
    )
    db.add(delivery)
    db.commit()
    db.refresh(delivery)

    filename = f"worklog-{report.report_type}-{report.period_start}-{report.period_end}.docx"
    try:
        (sender or send_email)(
            setting,
            recipient_addresses,
            subject,
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
        raise

    delivery.status = EmailDeliveryStatus.SENT.value
    delivery.sent_at = utcnow()
    db.commit()
    db.refresh(delivery)
    return delivery
