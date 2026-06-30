use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct WorkLog {
    pub id: i64,
    pub work_date: String,
    pub start_date: String,
    pub end_date: String,
    pub project: String,
    pub task: String,
    pub progress: String,
    pub result: Option<String>,
    pub blockers: Option<String>,
    pub hours: Option<f64>,
    pub priority: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkLogInput {
    pub work_date: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub project: Option<String>,
    pub task: Option<String>,
    pub progress: Option<String>,
    pub result: Option<String>,
    pub blockers: Option<String>,
    pub hours: Option<f64>,
    pub priority: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaginatedWorkLogs {
    pub items: Vec<WorkLog>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub template_type: String,
    pub content: String,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateInput {
    pub name: Option<String>,
    pub template_type: Option<String>,
    pub content: Option<String>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateImportInput {
    pub template_type: String,
    pub example_content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateOptimizeInput {
    pub template_type: String,
    pub content: String,
    pub optimization_request: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateTransformResponse {
    pub template_type: String,
    pub content: String,
    pub used_llm: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct LlmSettingRow {
    pub id: i64,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub extra_headers: Option<String>,
    pub timeout_seconds: i64,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmSetting {
    pub id: i64,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub extra_headers: BTreeMap<String, String>,
    pub timeout_seconds: i64,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmSettingInput {
    pub provider: String,
    pub base_url: Option<String>,
    pub model: String,
    pub api_key: Option<String>,
    #[serde(default)]
    pub extra_headers: BTreeMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: i64,
}

fn default_timeout() -> i64 {
    60
}

#[derive(Debug, Clone, FromRow)]
pub struct EmailSettingRow {
    pub id: i64,
    pub host: String,
    pub port: i64,
    pub security: String,
    pub username: String,
    pub password: String,
    pub sender_address: String,
    pub sender_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmailSetting {
    pub host: String,
    pub port: i64,
    pub security: String,
    pub username: String,
    pub password: Option<String>,
    pub sender_address: String,
    pub sender_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailSettingInput {
    pub host: String,
    pub port: i64,
    pub security: String,
    pub username: String,
    pub password: Option<String>,
    pub sender_address: String,
    pub sender_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Recipient {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecipientInput {
    pub name: Option<String>,
    pub email: Option<String>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Report {
    pub id: i64,
    pub report_type: String,
    pub title: String,
    pub period_start: String,
    pub period_end: String,
    pub template_id: Option<i64>,
    pub content_markdown: String,
    pub status: String,
    #[sqlx(skip)]
    pub source_log_ids: Vec<i64>,
    pub generated_at: Option<String>,
    pub edited_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct ReportRow {
    pub id: i64,
    pub report_type: String,
    pub title: String,
    pub period_start: String,
    pub period_end: String,
    pub template_id: Option<i64>,
    pub content_markdown: String,
    pub status: String,
    pub source_log_ids: String,
    pub generated_at: Option<String>,
    pub edited_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ReportRow> for Report {
    fn from(value: ReportRow) -> Self {
        let source_log_ids = serde_json::from_str(&value.source_log_ids).unwrap_or_default();
        Self {
            id: value.id,
            report_type: value.report_type,
            title: value.title,
            period_start: value.period_start,
            period_end: value.period_end,
            template_id: value.template_id,
            content_markdown: value.content_markdown,
            status: value.status,
            source_log_ids,
            generated_at: value.generated_at,
            edited_at: value.edited_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportUpdateInput {
    pub title: Option<String>,
    pub content_markdown: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportOptimizeInput {
    pub content: String,
    pub optimization_request: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportOptimizeResponse {
    pub report_id: i64,
    pub content: String,
    pub used_llm: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportGenerateInput {
    pub report_type: String,
    pub anchor_date: Option<String>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub template_id: Option<i64>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerateResponse {
    pub report: Report,
    pub task_id: i64,
    pub used_llm: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportSchedule {
    pub id: i64,
    pub report_type: String,
    pub enabled: bool,
    pub weekday: Option<String>,
    pub day_of_month: Option<i64>,
    pub template_id: Option<i64>,
    pub run_time: String,
    pub auto_send: bool,
    pub recipient_ids: Vec<i64>,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct ReportScheduleRow {
    pub id: i64,
    pub report_type: String,
    pub enabled: bool,
    pub weekday: Option<String>,
    pub day_of_month: Option<i64>,
    pub template_id: Option<i64>,
    pub run_time: String,
    pub auto_send: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportScheduleInput {
    pub enabled: bool,
    pub weekday: Option<String>,
    pub day_of_month: Option<i64>,
    pub template_id: Option<i64>,
    pub run_time: String,
    pub auto_send: bool,
    #[serde(default)]
    pub recipient_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryRecipient {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportEmailDelivery {
    pub id: i64,
    pub report_id: i64,
    pub subject: String,
    pub recipients: Vec<DeliveryRecipient>,
    pub status: String,
    pub error_message: Option<String>,
    pub sent_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct ReportEmailDeliveryRow {
    pub id: i64,
    pub report_id: i64,
    pub subject: String,
    pub recipients_json: String,
    pub status: String,
    pub error_message: Option<String>,
    pub sent_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ReportEmailDeliveryRow> for ReportEmailDelivery {
    fn from(value: ReportEmailDeliveryRow) -> Self {
        Self {
            id: value.id,
            report_id: value.report_id,
            subject: value.subject,
            recipients: serde_json::from_str(&value.recipients_json).unwrap_or_default(),
            status: value.status,
            error_message: value.error_message,
            sent_at: value.sent_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportEmailSendInput {
    #[serde(default)]
    pub recipient_ids: Vec<i64>,
    #[serde(default)]
    pub additional_recipients: Vec<String>,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopPreferences {
    pub launch_at_login: bool,
    pub database_path: String,
    pub legacy_database_path: Option<String>,
    pub migrated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationResult {
    pub imported: bool,
    pub source_path: Option<String>,
    pub database_path: String,
    pub message: String,
}
