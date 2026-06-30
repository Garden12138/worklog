use crate::db::now_string;
use crate::documents::markdown_to_docx;
use crate::error::{AppError, AppResult};
use crate::models::{
    DeliveryRecipient, EmailSettingRow, Report, ReportEmailDelivery, ReportEmailDeliveryRow,
};
use crate::secrets::{email_secret_key, SecretStore};
use lettre::message::{header::ContentType, Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use regex::Regex;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use std::collections::HashSet;

pub async fn active_email_setting(pool: &SqlitePool) -> AppResult<Option<EmailSettingRow>> {
    Ok(sqlx::query_as::<_, EmailSettingRow>(
        "SELECT id, host, port, security, username, password, sender_address, sender_name, is_active, created_at, updated_at \
         FROM email_settings WHERE is_active=1 ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn deliver_report(
    pool: &SqlitePool,
    secrets: &SecretStore,
    report: &Report,
    recipient_ids: &[i64],
    additional_recipients: &[String],
    subject: &str,
) -> AppResult<ReportEmailDelivery> {
    let setting = active_email_setting(pool)
        .await?
        .ok_or_else(|| AppError::new("email_not_configured", "请先在设置中完成 SMTP 邮箱配置"))?;
    let password = secrets
        .get(&email_secret_key(setting.id))
        .or_else(|| (!setting.password.is_empty()).then_some(setting.password.clone()))
        .ok_or_else(|| AppError::new("email_not_configured", "SMTP password is required"))?;

    let mut snapshots = Vec::new();
    let mut seen = HashSet::new();
    if !recipient_ids.is_empty() {
        let mut query =
            QueryBuilder::<Sqlite>::new("SELECT id, name, email FROM recipients WHERE id IN (");
        let mut separated = query.separated(", ");
        for id in recipient_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(") ORDER BY id");
        let rows = query.build().fetch_all(pool).await?;
        if rows.len() != recipient_ids.iter().collect::<HashSet<_>>().len() {
            return Err(AppError::new(
                "invalid_recipient",
                "存在已删除或无效的收件人",
            ));
        }
        use sqlx::Row;
        for row in rows {
            let email: String = row.try_get("email")?;
            if seen.insert(email.clone()) {
                snapshots.push(DeliveryRecipient {
                    name: Some(row.try_get("name")?),
                    email,
                });
            }
        }
    }
    for raw in additional_recipients {
        let email = normalize_email(raw)?;
        if seen.insert(email.clone()) {
            snapshots.push(DeliveryRecipient { name: None, email });
        }
    }
    if snapshots.is_empty() {
        return Err(AppError::new("invalid_recipient", "至少需要一位有效收件人"));
    }

    let now = now_string();
    let recipients_json = serde_json::to_string(&snapshots)?;
    let delivery_id = sqlx::query(
        "INSERT INTO report_email_deliveries(report_id, subject, recipients_json, content_markdown, status, created_at, updated_at) \
         VALUES(?, ?, ?, ?, 'pending', ?, ?)",
    )
    .bind(report.id)
    .bind(subject)
    .bind(&recipients_json)
    .bind(&report.content_markdown)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?
    .last_insert_rowid();

    let addresses = snapshots
        .iter()
        .map(|item| item.email.clone())
        .collect::<Vec<_>>();
    let filename = format!(
        "worklog-{}-{}-{}.docx",
        report.report_type, report.period_start, report.period_end
    );
    let send_result = send_email(
        &setting,
        &password,
        &addresses,
        subject,
        &report.content_markdown,
        Some((&filename, markdown_to_docx(&report.content_markdown)?)),
    )
    .await;
    match send_result {
        Ok(()) => {
            let sent_at = now_string();
            sqlx::query("UPDATE report_email_deliveries SET status='sent', sent_at=?, updated_at=? WHERE id=?")
                .bind(&sent_at)
                .bind(&sent_at)
                .bind(delivery_id)
                .execute(pool)
                .await?;
        }
        Err(error) => {
            sqlx::query("UPDATE report_email_deliveries SET status='failed', error_message=?, updated_at=? WHERE id=?")
                .bind(&error.message)
                .bind(now_string())
                .bind(delivery_id)
                .execute(pool)
                .await?;
            return Err(error);
        }
    }
    load_delivery(pool, delivery_id).await
}

pub async fn send_test_email(
    setting: &EmailSettingRow,
    password: &str,
    recipient: &str,
) -> AppResult<()> {
    let recipient = normalize_email(recipient)?;
    send_email(
        setting,
        password,
        &[recipient],
        "Worklog SMTP 测试邮件",
        "这是一封来自 Worklog 的 SMTP 测试邮件。",
        None,
    )
    .await
}

async fn send_email(
    setting: &EmailSettingRow,
    password: &str,
    recipients: &[String],
    subject: &str,
    markdown: &str,
    attachment: Option<(&str, Vec<u8>)>,
) -> AppResult<()> {
    let sender: Mailbox = format!(
        "{} <{}>",
        setting
            .sender_name
            .as_deref()
            .unwrap_or(&setting.sender_address),
        setting.sender_address
    )
    .parse()
    .map_err(|error| {
        AppError::new(
            "email_configuration_error",
            format!("Invalid sender: {error}"),
        )
    })?;
    let mut builder = Message::builder().from(sender).subject(subject);
    for address in recipients {
        let mailbox: Mailbox = address.parse().map_err(|error| {
            AppError::new("invalid_recipient", format!("Invalid recipient: {error}"))
        })?;
        builder = builder.to(mailbox);
    }
    let mut multipart = MultiPart::alternative()
        .singlepart(
            SinglePart::builder()
                .header(ContentType::TEXT_PLAIN)
                .body(markdown.to_string()),
        )
        .singlepart(
            SinglePart::builder()
                .header(ContentType::TEXT_HTML)
                .body(markdown_to_html(markdown)),
        );
    if let Some((filename, bytes)) = attachment {
        let content_type = ContentType::parse(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )
        .map_err(|error| AppError::new("email_configuration_error", error.to_string()))?;
        multipart =
            multipart.singlepart(Attachment::new(filename.to_string()).body(bytes, content_type));
    }
    let message = builder
        .multipart(multipart)
        .map_err(|error| AppError::new("email_configuration_error", error.to_string()))?;
    let credentials = Credentials::new(setting.username.clone(), password.into());
    let transport = if setting.security == "ssl" {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&setting.host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&setting.host)
    }
    .map_err(|error| AppError::new("email_configuration_error", error.to_string()))?
    .port(setting.port as u16)
    .credentials(credentials)
    .timeout(Some(std::time::Duration::from_secs(20)))
    .build();
    transport.send(message).await.map_err(|_| {
        AppError::new(
            "email_delivery_error",
            "邮件发送失败，请检查 SMTP 设置或稍后重试。",
        )
    })?;
    Ok(())
}

pub async fn load_delivery(pool: &SqlitePool, id: i64) -> AppResult<ReportEmailDelivery> {
    let row = sqlx::query_as::<_, ReportEmailDeliveryRow>(
        "SELECT id, report_id, subject, recipients_json, status, error_message, sent_at, created_at, updated_at \
         FROM report_email_deliveries WHERE id=?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("Email delivery"))?;
    Ok(row.into())
}

pub fn normalize_email(value: &str) -> AppResult<String> {
    let value = value.trim().to_lowercase();
    let parts = value.split('@').collect::<Vec<_>>();
    if parts.len() != 2
        || parts[0].is_empty()
        || !parts[1].contains('.')
        || value.chars().any(char::is_whitespace)
    {
        return Err(AppError::validation(
            "email",
            "a valid email address is required",
        ));
    }
    Ok(value)
}

pub fn markdown_to_html(markdown: &str) -> String {
    let heading = Regex::new(r"^(#{1,4})\s+(.+)$").unwrap();
    let ordered = Regex::new(r"^\d+\.\s+(.+)$").unwrap();
    let mut blocks = Vec::new();
    let mut list: Vec<String> = Vec::new();
    let mut list_tag = "";
    let flush = |blocks: &mut Vec<String>, list: &mut Vec<String>, tag: &mut &str| {
        if !list.is_empty() {
            blocks.push(format!(
                "<{0}>{1}</{0}>",
                *tag,
                list.iter()
                    .map(|item| format!("<li>{item}</li>"))
                    .collect::<String>()
            ));
        }
        list.clear();
        *tag = "";
    };
    for raw in markdown.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut blocks, &mut list, &mut list_tag);
        } else if let Some(captures) = heading.captures(line) {
            flush(&mut blocks, &mut list, &mut list_tag);
            let level = captures[1].len();
            blocks.push(format!(
                "<h{level}>{}</h{level}>",
                inline_html(&captures[2])
            ));
        } else if line.starts_with("- ") || line.starts_with("* ") {
            if !list_tag.is_empty() && list_tag != "ul" {
                flush(&mut blocks, &mut list, &mut list_tag);
            }
            list_tag = "ul";
            list.push(inline_html(&line[2..]));
        } else if let Some(captures) = ordered.captures(line) {
            if !list_tag.is_empty() && list_tag != "ol" {
                flush(&mut blocks, &mut list, &mut list_tag);
            }
            list_tag = "ol";
            list.push(inline_html(&captures[1]));
        } else {
            flush(&mut blocks, &mut list, &mut list_tag);
            blocks.push(format!("<p>{}</p>", inline_html(line)));
        }
    }
    flush(&mut blocks, &mut list, &mut list_tag);
    format!("<!doctype html><html><head><meta charset=\"utf-8\"></head><body style=\"font-family:Arial,'Microsoft YaHei',sans-serif;line-height:1.65;color:#172033;max-width:760px;margin:auto\">{}</body></html>", blocks.join("\n"))
}

fn inline_html(value: &str) -> String {
    let escaped = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let code = Regex::new(r"`([^`]+)`")
        .unwrap()
        .replace_all(&escaped, "<code>$1</code>");
    let strong = Regex::new(r"\*\*([^*]+)\*\*")
        .unwrap()
        .replace_all(&code, "<strong>$1</strong>");
    Regex::new(r"\*([^*]+)\*")
        .unwrap()
        .replace_all(&strong, "<em>$1</em>")
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_normalizes_email_addresses() {
        assert_eq!(
            normalize_email(" Manager@Example.Test ").unwrap(),
            "manager@example.test"
        );
        assert!(normalize_email("invalid").is_err());
    }

    #[test]
    fn renders_safe_markdown_html() {
        let html = markdown_to_html("# 标题\n\n- **成果** <script>");
        assert!(html.contains("<h1>标题</h1>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
