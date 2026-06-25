import html
import re
import smtplib
import ssl
from email.message import EmailMessage
from email.utils import formataddr

from app.models import EmailSetting


class EmailDeliveryError(Exception):
    """An SMTP delivery failure that is safe to show to the user."""


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
