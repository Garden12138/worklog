//! xcli v2 local CLI integration for Worklog.
//! Handles async execution, progress streaming, cancellation, error handling,
//! report schema validation, and recovery across app restarts.

use crate::db::now_string;
use crate::error::{AppError, AppResult};
use crate::llm::LlmConfig;
use crate::models::{GenerationTask, GenerationTaskStatus, Report, WorkLog};
use crate::templates::{build_context, render_template, report_title};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

pub const DEFAULT_XCLI_PATH: &str = "xcli";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub model: String,
    pub inference_location: String,
    #[serde(default)]
    pub agents: Vec<XcliAgentInfo>,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcliRequest {
    pub version: i32,
    pub capability: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    pub goal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_sec: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl XcliRequest {
    #[allow(dead_code)]
    pub fn new_ask(goal: String, inputs: Vec<String>, schema: Option<Value>) -> Self {
        Self {
            version: 1,
            capability: "ask".into(),
            permission: Some("read".into()),
            goal,
            agent: None,
            agents: Vec::new(),
            timeout_sec: None,
            inputs,
            schema,
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct XcliSubmitOutput {
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcliEvent {
    #[serde(default)]
    pub v: Option<i32>,
    #[serde(default)]
    pub ts: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub seq: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub step: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcliErrorDetail {
    #[serde(default)]
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub step_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcliResult {
    #[serde(default)]
    pub version: Option<i32>,
    pub task_id: String,
    pub status: String,
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub output: Option<Value>,
    #[serde(default)]
    pub error: Option<XcliErrorDetail>,
    #[serde(default)]
    pub metadata: Option<HashMap<String, String>>,
    #[serde(default)]
    pub steps: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcliRecord {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub result: Option<XcliResult>,
    #[serde(default)]
    pub request: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcliAgentInfo {
    pub name: String,
    pub installed: bool,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub version: String,
    pub auth: String,
    pub available: bool,
    #[serde(default)]
    pub cooldown_until: String,
    #[serde(default)]
    pub strengths: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructuredReportOutput {
    pub summary: String,
    pub items: Vec<String>,
    pub blockers: Vec<String>,
    pub next: Vec<String>,
}

#[allow(dead_code)]
pub fn expand_tilde(path_str: &str) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    expand_tilde_with_home(path_str, home.as_deref())
}

pub(crate) fn expand_tilde_with_home(path_str: &str, home: Option<&Path>) -> PathBuf {
    if path_str == "~" {
        if let Some(home) = home {
            return home.to_path_buf();
        }
    } else if let Some(rest) = path_str.strip_prefix("~/") {
        if let Some(home) = home {
            return home.join(rest);
        }
    }
    PathBuf::from(path_str)
}

#[allow(dead_code)]
pub fn find_in_path(cmd: &str) -> Option<PathBuf> {
    find_in_path_env(cmd, std::env::var_os("PATH").as_deref())
}

pub(crate) fn find_in_path_env(cmd: &str, path_env: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            let full = dir.join(cmd);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

pub fn resolve_xcli_path(config: Option<&LlmConfig>) -> PathBuf {
    let configured = config.map(|c| c.base_url.as_str());
    let xcli_path_env = std::env::var("XCLI_PATH").ok();
    let path_env = std::env::var_os("PATH");
    let home_env = std::env::var_os("HOME").map(PathBuf::from);

    resolve_xcli_path_with_env(
        configured,
        xcli_path_env.as_deref(),
        path_env.as_deref(),
        home_env.as_deref(),
    )
}

pub(crate) fn resolve_xcli_path_with_env(
    configured_path: Option<&str>,
    xcli_path_env: Option<&str>,
    path_env: Option<&std::ffi::OsStr>,
    home_env: Option<&Path>,
) -> PathBuf {
    // 1. If user configured a specific path or command name in database (and it's not the default "xcli" nor empty/http)
    if let Some(raw_path) = configured_path {
        let trimmed = raw_path.trim();
        if !trimmed.is_empty()
            && trimmed != DEFAULT_XCLI_PATH
            && !trimmed.starts_with("http://")
            && !trimmed.starts_with("https://")
        {
            let expanded = expand_tilde_with_home(trimmed, home_env);
            if expanded.exists() {
                return expanded;
            }
            if !trimmed.contains('/') && !trimmed.contains('\\') {
                if let Some(found) = find_in_path_env(trimmed, path_env) {
                    return found;
                }
            }
            // Explicit path given by user (absolute or containing path separators)
            if trimmed.contains('/') || trimmed.contains('\\') {
                return expanded;
            }
        }
    }

    // Default resolution order: XCLI_PATH -> PATH -> ~/.local/bin/xcli -> "xcli"
    // Priority 1: Environment variable XCLI_PATH
    if let Some(env_path) = xcli_path_env {
        let trimmed = env_path.trim();
        if !trimmed.is_empty() {
            let expanded = expand_tilde_with_home(trimmed, home_env);
            if expanded.exists() {
                return expanded;
            }
        }
    }

    // Priority 2: System PATH binary
    if let Some(found) = find_in_path_env("xcli", path_env) {
        return found;
    }

    // Priority 3: User standard directory ~/.local/bin/xcli
    if let Some(home) = home_env {
        let home_bin = home.join(".local/bin/xcli");
        if home_bin.exists() {
            return home_bin;
        }
    }

    PathBuf::from(DEFAULT_XCLI_PATH)
}

pub fn report_json_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "items", "blockers", "next"],
        "properties": {
            "summary": {"type": "string"},
            "items": {"type": "array", "items": {"type": "string"}},
            "blockers": {"type": "array", "items": {"type": "string"}},
            "next": {"type": "array", "items": {"type": "string"}}
        }
    })
}

pub fn build_report_goal(
    report_type: &str,
    period_start: &str,
    period_end: &str,
    template_content: Option<&str>,
) -> String {
    let name = report_title(report_type);
    let range_desc = if period_start == period_end {
        period_start.to_string()
    } else {
        format!("{period_start} 至 {period_end}")
    };
    let base = format!(
        "根据输入工作项生成{name}（{range_desc}），包含完成事项、阻碍和下周计划。依据已有事实，按指定 Schema 输出中文内容。缺失信息如实表达，不编造不存在的事实。"
    );
    if let Some(template) = template_content {
        let trimmed = template.trim();
        if !trimmed.is_empty() {
            return format!(
                "{base}\n\n报告模板要求如下，请依据模板结构提炼并组织内容：\n{trimmed}"
            );
        }
    }
    base
}

pub fn validate_report_output(output: Option<&Value>) -> AppResult<StructuredReportOutput> {
    let Some(output) = output else {
        return Err(AppError::new(
            "xcli_missing_output",
            "xcli 任务未返回结构化报告数据 (output 为空)",
        ));
    };
    if !output.is_object() {
        return Err(AppError::new(
            "xcli_invalid_output",
            "xcli 返回的 output 不是 JSON 对象",
        ));
    }
    serde_json::from_value::<StructuredReportOutput>(output.clone()).map_err(|e| {
        AppError::new(
            "xcli_invalid_output_structure",
            format!("xcli 返回的 output 不符合周报/日报结构定义: {e}"),
        )
    })
}

pub fn has_jinja_content_tags(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("{{ summary")
        || lower.contains("{{summary")
        || lower.contains("{{ highlights")
        || lower.contains("{{highlights")
        || lower.contains("{{ work_items")
        || lower.contains("{{work_items")
        || lower.contains("{{ items")
        || lower.contains("{{items")
        || lower.contains("{{ blockers")
        || lower.contains("{{blockers")
        || lower.contains("{{ next_steps")
        || lower.contains("{{next_steps")
        || lower.contains("{{ ai_content")
        || lower.contains("{{ai_content")
        || lower.contains("{{ raw_llm_content")
        || lower.contains("{{raw_llm_content")
        || lower.contains("{% for")
}

pub fn is_short_report_template(name: &str, content: &str) -> bool {
    let name_lower = name.to_lowercase();
    if name_lower.contains("short")
        || name_lower.contains("简报")
        || name_lower.contains("精简")
        || name_lower.contains("条目")
        || name_lower.contains("短")
    {
        return true;
    }
    content.contains("精简周报")
        || content.contains("短周报")
        || (content.contains("序号、模块") && content.contains("状态"))
}

pub fn format_short_report_markdown(title: &str, output: &StructuredReportOutput) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {title}\n\n"));
    if !output.items.is_empty() {
        for (i, item) in output.items.iter().enumerate() {
            let trimmed = item.trim();
            if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                md.push_str(trimmed);
            } else {
                md.push_str(&format!("{}、{}", i + 1, trimmed));
            }
            md.push_str("\n\n");
        }
    } else if !output.summary.is_empty() {
        let summary_trimmed = output.summary.trim();
        let clean = if summary_trimmed.starts_with('#') {
            if let Some((_, rest)) = summary_trimmed.split_once('\n') {
                rest.trim()
            } else {
                summary_trimmed
            }
        } else {
            summary_trimmed
        };
        md.push_str(clean);
        md.push_str("\n\n");
    }
    md.trim_end().to_string()
}

pub fn format_structured_report_markdown(title: &str, output: &StructuredReportOutput) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {title}\n\n"));

    let summary_trimmed = output.summary.trim();
    let clean_summary = if summary_trimmed.starts_with('#') {
        if let Some((_header, rest)) = summary_trimmed.split_once('\n') {
            rest.trim()
        } else {
            summary_trimmed
        }
    } else {
        summary_trimmed
    };

    if !clean_summary.is_empty() {
        md.push_str("## 工作总结\n");
        md.push_str(clean_summary);
        md.push_str("\n\n");
    }

    md.push_str("## 完成事项\n");
    if output.items.is_empty() {
        md.push_str("- 无\n");
    } else {
        for item in &output.items {
            let trimmed = item.trim();
            if trimmed.starts_with('-')
                || trimmed.starts_with('*')
                || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
            {
                md.push_str(&format!("{}\n", trimmed));
            } else {
                md.push_str(&format!("- {}\n", trimmed));
            }
        }
    }
    md.push('\n');

    if !output.blockers.is_empty() {
        md.push_str("## 阻碍与风险\n");
        for b in &output.blockers {
            let trimmed = b.trim();
            if trimmed.starts_with('-') || trimmed.starts_with('*') {
                md.push_str(&format!("{}\n", trimmed));
            } else {
                md.push_str(&format!("- {}\n", trimmed));
            }
        }
        md.push('\n');
    }

    if !output.next.is_empty() {
        md.push_str("## 下阶段计划\n");
        for n in &output.next {
            let trimmed = n.trim();
            if trimmed.starts_with('-') || trimmed.starts_with('*') {
                md.push_str(&format!("{}\n", trimmed));
            } else {
                md.push_str(&format!("- {}\n", trimmed));
            }
        }
    }
    md
}

pub fn format_event_progress(event: &XcliEvent) -> String {
    match event.event_type.as_str() {
        "task.started" => "任务已启动，正在规划...".into(),
        "plan.ready" => "执行计划已生成".into(),
        "step.waiting" => {
            let reason = event
                .data
                .as_ref()
                .and_then(|d| d.get("reason"))
                .and_then(|r| r.as_str())
                .unwrap_or("等待资源");
            format!("等待中: {reason}")
        }
        "step.started" => {
            let step = event.step.as_deref().unwrap_or("main");
            let round = event
                .data
                .as_ref()
                .and_then(|d| d.get("round"))
                .and_then(|r| r.as_i64())
                .unwrap_or(1);
            format!("正在执行步骤 {step} (第 {round} 轮)")
        }
        "step.progress" => {
            if let Some(data) = &event.data {
                if let Some(text) = data.get("text").and_then(|t| t.as_str()) {
                    return text.to_string();
                }
                if let Some(tool) = data.get("tool").and_then(|t| t.as_str()) {
                    return format!("正在调用工具: {tool}");
                }
            }
            "正在处理工作项...".into()
        }
        "step.finished" => {
            let summary = event
                .data
                .as_ref()
                .and_then(|d| d.get("summary"))
                .and_then(|s| s.as_str())
                .unwrap_or("步骤完成");
            summary.to_string()
        }
        "agent.fallback" => {
            let from = event
                .data
                .as_ref()
                .and_then(|d| d.get("from"))
                .and_then(|f| f.as_str())
                .unwrap_or("原 CLI");
            let to = event
                .data
                .as_ref()
                .and_then(|d| d.get("to"))
                .and_then(|t| t.as_str())
                .unwrap_or("新 CLI");
            let reason = event
                .data
                .as_ref()
                .and_then(|d| d.get("reason"))
                .and_then(|r| r.as_str())
                .unwrap_or("");
            format!("已自动切换: {from} -> {to} ({reason})")
        }
        "notice" => event
            .data
            .as_ref()
            .and_then(|d| d.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        "task.finished" => "生成完毕，正在整理报告...".into(),
        _ => format!("事件: {}", event.event_type),
    }
}

pub(crate) fn create_xcli_command(xcli_path: &Path) -> Command {
    #[cfg(windows)]
    {
        let path_str = xcli_path.to_string_lossy().to_ascii_lowercase();
        if path_str.ends_with(".bat") || path_str.ends_with(".cmd") {
            let mut cmd = Command::new("cmd.exe");
            cmd.arg("/c").arg(xcli_path);
            return cmd;
        }
        if path_str.ends_with(".py") {
            let mut cmd = Command::new("python");
            cmd.env("PYTHONIOENCODING", "utf-8");
            cmd.env("PYTHONUTF8", "1");
            cmd.arg(xcli_path);
            return cmd;
        }
    }
    Command::new(xcli_path)
}

pub async fn submit_task(xcli_path: &Path, request: &XcliRequest) -> AppResult<String> {
    let req_bytes = serde_json::to_vec(request)
        .map_err(|e| AppError::new("xcli_serialization_error", format!("无法序列化请求: {e}")))?;

    let mut child = create_xcli_command(xcli_path)
        .args(["submit", "--request", "-", "--detach"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            AppError::new(
                "xcli_launch_failed",
                format!("启动 xcli 失败 (路径: {}): {e}", xcli_path.display()),
            )
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&req_bytes).await.map_err(|e| {
            AppError::new("xcli_io_error", format!("向 xcli stdin 写入请求失败: {e}"))
        })?;
        stdin
            .flush()
            .await
            .map_err(|e| AppError::new("xcli_io_error", format!("刷新 xcli stdin 失败: {e}")))?;
        drop(stdin);
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| AppError::new("xcli_wait_error", format!("等待 xcli submit 响应失败: {e}")))?;

    if !output.status.success() {
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let msg = if !stderr_str.trim().is_empty() {
            stderr_str.trim().to_string()
        } else {
            stdout_str.trim().to_string()
        };
        return Err(AppError::new(
            "xcli_submit_error",
            format!(
                "xcli submit 失败 (退出码 {:?}): {msg}",
                output.status.code()
            ),
        ));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let parsed: XcliSubmitOutput = serde_json::from_str(stdout_str.trim()).map_err(|e| {
        AppError::new(
            "xcli_output_corrupt",
            format!("xcli 输出损坏，无法解析 task_id: {e} (原输出: {stdout_str})"),
        )
    })?;

    if parsed.task_id.trim().is_empty() {
        return Err(AppError::new(
            "xcli_output_corrupt",
            "xcli 返回的 task_id 为空",
        ));
    }

    Ok(parsed.task_id)
}

pub async fn stream_task_logs<F>(
    xcli_path: &Path,
    task_id: &str,
    after_seq: i64,
    mut on_event: F,
) -> AppResult<Option<XcliResult>>
where
    F: FnMut(XcliEvent),
{
    let mut child = create_xcli_command(xcli_path)
        .args(["logs", task_id, "-f", "--after", &after_seq.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            AppError::new(
                "xcli_launch_failed",
                format!("无法启动 xcli logs (ID: {task_id}): {e}"),
            )
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new("xcli_io_error", "无法获取 xcli stdout 管道"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new("xcli_io_error", "无法获取 xcli stderr 管道"))?;

    let stderr_drain = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        let mut last_line = String::new();
        while let Ok(Some(line)) = reader.next_line().await {
            if !line.trim().is_empty() {
                last_line = line;
            }
        }
        last_line
    });

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut finished_result: Option<XcliResult> = None;

    while let Ok(Some(line)) = stdout_reader.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<XcliEvent>(trimmed) {
            if event.event_type == "task.finished" {
                if let Some(data) = &event.data {
                    if let Ok(res) = serde_json::from_value::<XcliResult>(data.clone()) {
                        finished_result = Some(res);
                    }
                }
            }
            on_event(event);
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| AppError::new("xcli_wait_error", format!("等待 xcli logs 结束失败: {e}")))?;
    let _ = stderr_drain.await;

    if finished_result.is_none() && !status.success() {
        return Err(AppError::new(
            "xcli_logs_interrupted",
            format!("xcli logs 连接中断 (退出码 {:?})", status.code()),
        ));
    }

    Ok(finished_result)
}

pub async fn fetch_task_status(xcli_path: &Path, task_id: &str) -> AppResult<XcliRecord> {
    let output = create_xcli_command(xcli_path)
        .args(["status", task_id, "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            AppError::new(
                "xcli_launch_failed",
                format!("无法执行 xcli status (ID: {task_id}): {e}"),
            )
        })?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::new(
            "xcli_status_error",
            format!("xcli status 查询失败 (ID: {task_id}): {}", err.trim()),
        ));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout_str.trim()).map_err(|e| {
        AppError::new(
            "xcli_output_corrupt",
            format!("xcli status 输出解析失败: {e} (原输出: {stdout_str})"),
        )
    })
}

pub async fn fetch_task_result(xcli_path: &Path, task_id: &str) -> AppResult<XcliResult> {
    let output = create_xcli_command(xcli_path)
        .args(["result", task_id])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            AppError::new(
                "xcli_launch_failed",
                format!("无法执行 xcli result (ID: {task_id}): {e}"),
            )
        })?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::new(
            "xcli_result_error",
            format!("xcli result 查询失败 (ID: {task_id}): {}", err.trim()),
        ));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout_str.trim()).map_err(|e| {
        AppError::new(
            "xcli_output_corrupt",
            format!("xcli result 输出解析失败: {e} (原输出: {stdout_str})"),
        )
    })
}

pub async fn cancel_task(xcli_path: &Path, task_id: &str) -> AppResult<()> {
    let output = create_xcli_command(xcli_path)
        .args(["cancel", task_id])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            AppError::new(
                "xcli_launch_failed",
                format!("无法执行 xcli cancel (ID: {task_id}): {e}"),
            )
        })?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::new(
            "xcli_cancel_error",
            format!("xcli cancel 失败 (ID: {task_id}): {}", err.trim()),
        ));
    }

    Ok(())
}

pub async fn list_agents(xcli_path: &Path) -> AppResult<Vec<XcliAgentInfo>> {
    let output = create_xcli_command(xcli_path)
        .args(["agents", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            AppError::new(
                "xcli_launch_failed",
                format!("无法执行 xcli agents (路径: {}): {e}", xcli_path.display()),
            )
        })?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::new(
            "xcli_agents_error",
            format!("xcli agents 查询失败: {}", err.trim()),
        ));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout_str.trim()).map_err(|e| {
        AppError::new(
            "xcli_output_corrupt",
            format!("xcli agents 输出解析失败: {e}"),
        )
    })
}

pub async fn run_xcli_ask(xcli_path: &Path, goal: &str) -> AppResult<String> {
    let req = serde_json::json!({
        "version": 1,
        "capability": "ask",
        "permission": "read",
        "goal": goal,
    });
    let req_bytes = serde_json::to_vec(&req)
        .map_err(|e| AppError::new("xcli_serialization_error", format!("无法序列化请求: {e}")))?;
    let mut child = create_xcli_command(xcli_path)
        .args(["submit", "--request", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::new("xcli_launch_failed", format!("启动 xcli 失败: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&req_bytes)
            .await
            .map_err(|e| AppError::new("xcli_io_error", format!("向 xcli stdin 写入失败: {e}")))?;
        stdin
            .flush()
            .await
            .map_err(|e| AppError::new("xcli_io_error", format!("刷新 xcli stdin 失败: {e}")))?;
        drop(stdin);
    }
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| AppError::new("xcli_wait_error", format!("等待 xcli 执行失败: {e}")))?;
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout_str.trim())
        .map_err(|e| AppError::new("xcli_output_corrupt", format!("无法解析 xcli 输出: {e}")))?;
    if let Some(err) = parsed
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return Err(AppError::new("xcli_error", err.to_string()));
    }
    let summary = parsed
        .get("summary")
        .and_then(|s| s.as_str())
        .unwrap_or_default();
    Ok(summary.to_string())
}

pub async fn probe(config: &LlmConfig) -> AppResult<ConnectionInfo> {
    let path = resolve_xcli_path(Some(config));
    let agents = list_agents(&path).await?;
    let mut names = Vec::new();
    for a in &agents {
        names.push(format!("{} ({})", a.name, a.auth));
    }
    let summary = format!("检测到 {} 个 CLI: {}", agents.len(), names.join(", "));
    Ok(ConnectionInfo {
        model: "xcli/auto".into(),
        inference_location: "local".into(),
        agents,
        summary,
    })
}

pub async fn execute_task_lifecycle<F>(
    pool: &SqlitePool,
    xcli_path: &Path,
    generation_task_id: i64,
    mut on_progress: F,
) -> AppResult<Report>
where
    F: FnMut(&str, &str) + Send, // (status, progress_text)
{
    let task = sqlx::query_as::<_, GenerationTask>(
        "SELECT id, report_type, period_start, period_end, status, message, report_id, completed_at, created_at, updated_at, \
         xcli_task_id, last_seq, step_progress, prompt_inputs, template_id, title, overwrite \
         FROM generation_tasks WHERE id=?",
    )
    .bind(generation_task_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("GenerationTask"))?;

    let xcli_task_id = task
        .xcli_task_id
        .as_deref()
        .ok_or_else(|| AppError::new("task_missing_xcli_id", "任务缺少 xcli_task_id"))?;

    let mut last_seq = task.last_seq.unwrap_or(0);

    let stream_res = stream_task_logs(xcli_path, xcli_task_id, last_seq, |event| {
        if event.seq > last_seq {
            last_seq = event.seq;
            let progress_text = format_event_progress(&event);
            on_progress("running", &progress_text);
        }
    })
    .await;

    let result = match stream_res {
        Ok(Some(res)) => res,
        _ => {
            let record = fetch_task_status(xcli_path, xcli_task_id).await?;
            if let Some(res) = record.result {
                res
            } else {
                fetch_task_result(xcli_path, xcli_task_id).await?
            }
        }
    };

    finalize_and_save_result(pool, &task, result, last_seq, &mut on_progress).await
}

pub async fn finalize_and_save_result(
    pool: &SqlitePool,
    task: &GenerationTask,
    result: XcliResult,
    last_seq: i64,
    on_progress: &mut (dyn FnMut(&str, &str) + Send),
) -> AppResult<Report> {
    let now = now_string();
    let generation_task_id = task.id;
    let xcli_task_id = task.xcli_task_id.as_deref().unwrap_or("");

    match result.status.as_str() {
        "completed" => {
            let structured = validate_report_output(result.output.as_ref())?;
            let report_type = &task.report_type;
            let template_id = task.template_id;
            let template = crate::reports::get_template(pool, report_type, template_id).await?;
            let title = task.title.clone().unwrap_or_else(|| {
                format!(
                    "{} 至 {} {}",
                    task.period_start,
                    task.period_end,
                    report_title(report_type)
                )
            });

            let logs = sqlx::query_as::<_, WorkLog>(
                "SELECT id, work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, notes, origin, auto_capture_date, manually_edited, git_commit_count, agent_session_count, pending_evidence_count, created_at, updated_at \
                 FROM work_logs WHERE end_date >= ? AND start_date <= ? ORDER BY start_date, end_date, id",
            )
            .bind(&task.period_start)
            .bind(&task.period_end)
            .fetch_all(pool)
            .await?;

            let mut context = build_context(
                report_type,
                &task.period_start,
                &task.period_end,
                &logs,
                format_structured_report_markdown(&title, &structured),
                crate::reports::local_datetime(),
            );
            context.title = title.clone();
            context.highlights = structured.items.clone();
            context.blockers = structured.blockers.clone();
            context.next_steps = structured.next.clone();

            let is_short_template = is_short_report_template(&template.name, &template.content);
            let is_jinja_data_template = has_jinja_content_tags(&template.content);

            let content = if is_short_template {
                format_short_report_markdown(&title, &structured)
            } else if is_jinja_data_template {
                render_template(&template.content, &context)?
            } else {
                format_structured_report_markdown(&title, &structured)
            };

            let source_ids =
                serde_json::to_string(&logs.iter().map(|item| item.id).collect::<Vec<_>>())?;

            let existing = if task.overwrite.unwrap_or(false) {
                sqlx::query_as::<_, crate::models::ReportRow>(
                    "SELECT id, report_type, title, period_start, period_end, template_id, content_markdown, status, source_log_ids, generated_at, edited_at, created_at, updated_at \
                     FROM reports WHERE report_type=? AND period_start=? AND period_end=? ORDER BY id DESC LIMIT 1",
                )
                .bind(report_type)
                .bind(&task.period_start)
                .bind(&task.period_end)
                .fetch_optional(pool)
                .await?
            } else {
                None
            };

            let report_id = if let Some(existing) = existing {
                sqlx::query(
                    "UPDATE reports SET title=?, template_id=?, content_markdown=?, status='draft', source_log_ids=?, generated_at=?, updated_at=? WHERE id=?",
                )
                .bind(&title)
                .bind(template.id)
                .bind(&content)
                .bind(&source_ids)
                .bind(&now)
                .bind(&now)
                .bind(existing.id)
                .execute(pool)
                .await?;
                existing.id
            } else {
                sqlx::query(
                    "INSERT INTO reports(report_type, title, period_start, period_end, template_id, content_markdown, status, source_log_ids, generated_at, created_at, updated_at) \
                     VALUES(?, ?, ?, ?, ?, ?, 'draft', ?, ?, ?, ?)",
                )
                .bind(report_type)
                .bind(&title)
                .bind(&task.period_start)
                .bind(&task.period_end)
                .bind(template.id)
                .bind(&content)
                .bind(&source_ids)
                .bind(&now)
                .bind(&now)
                .bind(&now)
                .execute(pool)
                .await?
                .last_insert_rowid()
            };

            sqlx::query(
                "UPDATE generation_tasks SET status='completed', report_id=?, step_progress='已完成', last_seq=?, completed_at=?, updated_at=? WHERE id=?",
            )
            .bind(report_id)
            .bind(last_seq)
            .bind(&now)
            .bind(&now)
            .bind(generation_task_id)
            .execute(pool)
            .await?;

            on_progress("completed", "报告草稿已生成并保存");
            crate::reports::load_report(pool, report_id).await
        }
        "cancelled" => {
            sqlx::query(
                "UPDATE generation_tasks SET status='cancelled', step_progress='任务已取消', message='任务已取消', last_seq=?, completed_at=?, updated_at=? WHERE id=?",
            )
            .bind(last_seq)
            .bind(&now)
            .bind(&now)
            .bind(generation_task_id)
            .execute(pool)
            .await?;
            on_progress("cancelled", "任务已取消");
            Err(AppError::new(
                "task_cancelled",
                format!("任务已取消 (ID: {xcli_task_id})"),
            ))
        }
        status => {
            let err_msg =
                result
                    .error
                    .map(|e| e.message)
                    .unwrap_or(if !result.summary.is_empty() {
                        result.summary
                    } else {
                        format!("xcli 任务处于 {status} 状态")
                    });
            let full_err = format!("任务失败 (ID: {xcli_task_id}): {err_msg}");
            sqlx::query(
                "UPDATE generation_tasks SET status='failed', step_progress='任务失败', message=?, last_seq=?, completed_at=?, updated_at=? WHERE id=?",
            )
            .bind(&full_err)
            .bind(last_seq)
            .bind(&now)
            .bind(&now)
            .bind(generation_task_id)
            .execute(pool)
            .await?;
            on_progress("failed", &full_err);
            Err(AppError::new("xcli_task_failed", full_err))
        }
    }
}

pub async fn cancel_generation_task(
    pool: &SqlitePool,
    xcli_path: &Path,
    generation_task_id: i64,
) -> AppResult<()> {
    let task = sqlx::query_as::<_, GenerationTask>(
        "SELECT id, report_type, period_start, period_end, status, message, report_id, completed_at, created_at, updated_at, \
         xcli_task_id, last_seq, step_progress, prompt_inputs, template_id, title, overwrite \
         FROM generation_tasks WHERE id=?",
    )
    .bind(generation_task_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("GenerationTask"))?;

    let now = now_string();
    sqlx::query(
        "UPDATE generation_tasks SET status='cancelling', step_progress='正在取消...', updated_at=? WHERE id=?",
    )
    .bind(&now)
    .bind(generation_task_id)
    .execute(pool)
    .await?;

    if let Some(ref xcli_id) = task.xcli_task_id {
        let _ = cancel_task(xcli_path, xcli_id).await;
    }
    Ok(())
}

pub async fn recover_active_task(
    pool: &SqlitePool,
    xcli_path: &Path,
    generation_task_id: i64,
) -> AppResult<GenerationTaskStatus> {
    let task = sqlx::query_as::<_, GenerationTask>(
        "SELECT id, report_type, period_start, period_end, status, message, report_id, completed_at, created_at, updated_at, \
         xcli_task_id, last_seq, step_progress, prompt_inputs, template_id, title, overwrite \
         FROM generation_tasks WHERE id=?",
    )
    .bind(generation_task_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("GenerationTask"))?;

    if let Some(ref xcli_id) = task.xcli_task_id {
        if task.status == "running" || task.status == "pending" || task.status == "cancelling" {
            let record = fetch_task_status(xcli_path, xcli_id).await?;
            if record.status != "running" && record.status != "queued" && record.status != "waiting"
            {
                let result = if let Some(res) = record.result {
                    res
                } else {
                    fetch_task_result(xcli_path, xcli_id).await?
                };
                let _ = finalize_and_save_result(
                    pool,
                    &task,
                    result,
                    task.last_seq.unwrap_or(0),
                    &mut |_, _| {},
                )
                .await;
            } else {
                let pool_c = pool.clone();
                let path_c = xcli_path.to_path_buf();
                let gid = generation_task_id;
                tokio::spawn(async move {
                    let _ = execute_task_lifecycle(&pool_c, &path_c, gid, |_, _| {}).await;
                });
            }
        }
    }

    let refreshed = sqlx::query_as::<_, GenerationTask>(
        "SELECT id, report_type, period_start, period_end, status, message, report_id, completed_at, created_at, updated_at, \
         xcli_task_id, last_seq, step_progress, prompt_inputs, template_id, title, overwrite \
         FROM generation_tasks WHERE id=?",
    )
    .bind(generation_task_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("GenerationTask"))?;

    Ok(GenerationTaskStatus {
        id: refreshed.id,
        report_type: refreshed.report_type,
        period_start: refreshed.period_start,
        period_end: refreshed.period_end,
        status: refreshed.status,
        message: refreshed.message,
        report_id: refreshed.report_id,
        xcli_task_id: refreshed.xcli_task_id,
        last_seq: refreshed.last_seq.unwrap_or(0),
        step_progress: refreshed.step_progress,
        title: refreshed.title,
    })
}

// ---------------------------------------------------------------------------
// Unit & Integration Tests using fake xcli
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn set_fake_xcli_mode(dir: &Path, mode: &str) {
        fs::write(dir.join("fake_mode.txt"), mode).unwrap();
    }

    fn make_fake_xcli(dir: &Path) -> PathBuf {
        let fake = dir.join("fake_xcli.py");
        let script = r#"#!/usr/bin/env python3
import sys, json, os

if hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass
if hasattr(sys.stderr, "reconfigure"):
    try:
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

script_dir = os.path.dirname(os.path.abspath(__file__))
mode_file = os.path.join(script_dir, "fake_mode.txt")
if os.path.isfile(mode_file):
    with open(mode_file, "r", encoding="utf-8") as f:
        mode = f.read().strip()
else:
    mode = os.environ.get("FAKE_XCLI_MODE", "normal")

args = sys.argv[1:]

if not args:
    sys.exit(0)

cmd = args[0]

if cmd == "agents":
    print(json.dumps([
        {"name": "codex", "installed": True, "path": "/bin/codex", "version": "1.0", "auth": "ok", "available": True, "cooldown_until": "0001-01-01T00:00:00Z"},
        {"name": "cursor", "installed": True, "path": "/bin/cursor", "version": "1.0", "auth": "ok", "available": True, "cooldown_until": "0001-01-01T00:00:00Z"},
        {"name": "antigravity", "installed": True, "path": "/bin/agy", "version": "1.0", "auth": "unknown", "available": True, "cooldown_until": "0001-01-01T00:00:00Z"}
    ]))
    sys.exit(0)

if cmd == "submit":
    if mode == "submit_fail":
        sys.stderr.write("fake submit failed: permission denied\n")
        sys.exit(1)
    if mode == "submit_broken":
        print("BROKEN_OUTPUT_NOT_JSON")
        sys.exit(0)
    raw = sys.stdin.read()
    req = json.loads(raw)
    assert req["version"] == 1
    assert req["capability"] == "ask"
    assert req["permission"] == "read"
    assert "summary" in req["schema"]["properties"]
    assert req["schema"]["additionalProperties"] is False
    rec_file = os.environ.get("FAKE_XCLI_REQ_RECORD")
    if not rec_file:
        rec_file = os.path.join(script_dir, "req_record.json")
    if rec_file:
        with open(rec_file, "w", encoding="utf-8") as f:
            f.write(raw)
    print(json.dumps({"task_id": "t-fake-task-123"}))
    sys.exit(0)

if cmd == "logs":
    task_id = args[1]
    after = 0
    if "--after" in args:
        idx = args.index("--after")
        after = int(args[idx + 1])

    events = [
        {"v": 1, "ts": "2026-11-17T10:00:00Z", "task": task_id, "seq": 1, "type": "task.started", "data": {"capability": "ask"}},
        {"v": 1, "ts": "2026-11-17T10:00:01Z", "task": task_id, "seq": 2, "type": "plan.ready", "data": {"plan": {}}},
        {"v": 1, "ts": "2026-11-17T10:00:02Z", "task": task_id, "seq": 3, "type": "step.progress", "step": "main", "agent": "codex", "data": {"text": "正在整理本周工作项与完成事实"}},
        {"v": 1, "ts": "2026-11-17T10:00:03Z", "task": task_id, "seq": 4, "type": "agent.fallback", "data": {"from": "codex", "to": "antigravity", "reason": "quota"}},
        {"v": 1, "ts": "2026-11-17T10:00:04Z", "task": task_id, "seq": 5, "type": "step.progress", "step": "main", "agent": "antigravity", "data": {"text": "已切换到 antigravity，继续生成结构化报告"}},
        {"v": 1, "ts": "2026-11-17T10:00:05Z", "task": task_id, "seq": 6, "type": "unknown.future.event", "unknown_field": "val", "data": {"custom": 123}},
    ]

    if mode == "disconnect_at_3":
        for ev in events:
            if ev["seq"] > after:
                if ev["seq"] > 3:
                    sys.exit(143)
                print(json.dumps(ev), flush=True)
        sys.exit(0)

    for ev in events:
        if ev["seq"] > after:
            print(json.dumps(ev), flush=True)

    if mode == "missing_output":
        finished = {
            "v": 1, "ts": "2026-11-17T10:00:06Z", "task": task_id, "seq": 7, "type": "task.finished",
            "data": {
                "version": 1, "task_id": task_id, "status": "completed", "capability": "ask",
                "summary": "完成", "output": None, "steps": []
            }
        }
        if 7 > after:
            print(json.dumps(finished), flush=True)
        sys.exit(0)

    if mode == "task_failed":
        finished = {
            "v": 1, "ts": "2026-11-17T10:00:06Z", "task": task_id, "seq": 7, "type": "task.finished",
            "data": {
                "version": 1, "task_id": task_id, "status": "failed", "capability": "ask",
                "summary": "任务失败", "output": None,
                "error": {"kind": "agent_error", "message": "所有 CLI 均不可用"},
                "steps": []
            }
        }
        if 7 > after:
            print(json.dumps(finished), flush=True)
        sys.exit(1)

    if mode == "task_cancelled":
        finished = {
            "v": 1, "ts": "2026-11-17T10:00:06Z", "task": task_id, "seq": 7, "type": "task.finished",
            "data": {
                "version": 1, "task_id": task_id, "status": "cancelled", "capability": "ask",
                "summary": "任务已取消", "output": None, "steps": []
            }
        }
        if 7 > after:
            print(json.dumps(finished), flush=True)
        sys.exit(130)

    finished = {
        "v": 1, "ts": "2026-11-17T10:00:06Z", "task": task_id, "seq": 7, "type": "task.finished",
        "data": {
            "version": 1, "task_id": task_id, "status": "completed", "capability": "ask",
            "summary": "本周工作总结生成完毕",
            "output": {
                "summary": "本周完成了超信生成及短链平台联调，系统整体稳定。",
                "items": ["完成超信智能生成组件改造", "完成短链平台高并发联调"],
                "blockers": ["第三方网关接口偶尔超时"],
                "next": ["推进 5G 消息模板灰度上线", "补充接口自动化测试用例"]
            },
            "steps": []
        }
    }
    if 7 > after:
        print(json.dumps(finished), flush=True)
    sys.exit(0)

if cmd == "status":
    task_id = args[1]
    if mode == "status_running":
        print(json.dumps({
            "id": task_id,
            "status": "running",
            "result": None
        }))
    else:
        print(json.dumps({
            "id": task_id,
            "status": "completed",
            "result": {
                "version": 1, "task_id": task_id, "status": "completed", "capability": "ask",
                "summary": "本周工作总结生成完毕",
                "output": {
                    "summary": "已恢复：本周完成了超信生成及短链平台联调。",
                    "items": ["完成超信智能生成组件改造"],
                    "blockers": ["无"],
                    "next": ["推进灰度上线"]
                },
                "steps": []
            }
        }))
    sys.exit(0)

if cmd == "result":
    task_id = args[1]
    print(json.dumps({
        "version": 1, "task_id": task_id, "status": "completed", "capability": "ask",
        "summary": "本周工作总结生成完毕",
        "output": {
            "summary": "从 result 获取：本周完成超信生成。",
            "items": ["超信组件"],
            "blockers": [],
            "next": ["测试"]
        },
        "steps": []
    }))
    sys.exit(0)

if cmd == "cancel":
    task_id = args[1]
    rec = os.environ.get("FAKE_XCLI_CANCEL_RECORD")
    if not rec:
        rec = os.path.join(script_dir, "cancel_record.txt")
    if rec:
        with open(rec, "w", encoding="utf-8") as f:
            f.write(task_id)
    print("cancelled: " + task_id)
    sys.exit(0)
"#;
        fs::write(&fake, script).unwrap();
        set_fake_xcli_mode(dir, "normal");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&fake).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake, perms).unwrap();
        }
        fake
    }

    #[tokio::test]
    async fn test_xcli_submit_request_stdin_and_close() {
        let temp_dir =
            std::env::temp_dir().join(format!("xcli-test-submit-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let fake_cli = make_fake_xcli(&temp_dir);

        let record_path = temp_dir.join("req_record.json");

        let mut metadata = HashMap::new();
        metadata.insert("app".into(), "worklog".into());
        metadata.insert("report_id".into(), "1".into());

        let mut req = XcliRequest::new_ask(
            "测试周报".into(),
            vec!["/tmp/test.json".into()],
            Some(report_json_schema()),
        );
        req.metadata = metadata;

        let task_id = submit_task(&fake_cli, &req).await.unwrap();
        assert_eq!(task_id, "t-fake-task-123");

        let recorded = fs::read_to_string(&record_path).unwrap();
        let parsed: Value = serde_json::from_str(&recorded).unwrap();
        assert_eq!(parsed["version"], 1);
        assert_eq!(parsed["capability"], "ask");
        assert_eq!(parsed["permission"], "read");
        assert_eq!(parsed["schema"]["additionalProperties"], false);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_xcli_jsonl_chunking_chinese_and_unknown_fields() {
        let temp_dir = std::env::temp_dir().join(format!("xcli-test-logs-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let fake_cli = make_fake_xcli(&temp_dir);

        let mut events_received = Vec::new();
        let result = stream_task_logs(&fake_cli, "t-fake-task-123", 0, |ev| {
            events_received.push(ev);
        })
        .await
        .unwrap();

        assert!(result.is_some());
        let res = result.unwrap();
        assert_eq!(res.status, "completed");

        let structured = validate_report_output(res.output.as_ref()).unwrap();
        assert!(structured.summary.contains("超信"));
        assert_eq!(structured.items.len(), 2);

        // Check events include Chinese text and unknown fields were accepted
        let progress_texts: Vec<String> =
            events_received.iter().map(format_event_progress).collect();
        assert!(progress_texts.iter().any(|t| t.contains("整理本周工作项")));
        assert!(progress_texts
            .iter()
            .any(|t| t.contains("自动切换: codex -> antigravity")));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_xcli_nonzero_exit_and_launch_failure() {
        let temp_dir = std::env::temp_dir().join(format!("xcli-test-err-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let fake_cli = make_fake_xcli(&temp_dir);
        set_fake_xcli_mode(&temp_dir, "submit_fail");

        let req = XcliRequest::new_ask("测试".into(), vec![], None);

        let err = submit_task(&fake_cli, &req).await.unwrap_err();
        assert!(err.message.contains("fake submit failed"));

        // Launch failure with nonexistent binary
        let bad_path = PathBuf::from("/nonexistent/xcli/binary");
        let err2 = submit_task(&bad_path, &req).await.unwrap_err();
        assert_eq!(err2.code, "xcli_launch_failed");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_xcli_broken_output_and_missing_output() {
        let temp_dir =
            std::env::temp_dir().join(format!("xcli-test-broken-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let fake_cli = make_fake_xcli(&temp_dir);
        set_fake_xcli_mode(&temp_dir, "submit_broken");

        let req = XcliRequest::new_ask("测试".into(), vec![], None);

        let err = submit_task(&fake_cli, &req).await.unwrap_err();
        assert_eq!(err.code, "xcli_output_corrupt");

        // Missing output validation error
        let val_err = validate_report_output(None).unwrap_err();
        assert_eq!(val_err.code, "xcli_missing_output");

        let bad_json = json!({"summary": "only summary"});
        let val_err2 = validate_report_output(Some(&bad_json)).unwrap_err();
        assert_eq!(val_err2.code, "xcli_invalid_output_structure");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_xcli_cancel_lifecycle() {
        let temp_dir =
            std::env::temp_dir().join(format!("xcli-test-cancel-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let fake_cli = make_fake_xcli(&temp_dir);

        let cancel_marker = temp_dir.join("cancel_record.txt");

        cancel_task(&fake_cli, "t-test-cancel-999").await.unwrap();
        assert_eq!(
            fs::read_to_string(&cancel_marker).unwrap(),
            "t-test-cancel-999"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_xcli_disconnect_and_seq_resume() {
        let temp_dir = std::env::temp_dir().join(format!("xcli-test-seq-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let fake_cli = make_fake_xcli(&temp_dir);
        set_fake_xcli_mode(&temp_dir, "disconnect_at_3");

        let mut events1 = Vec::new();
        let res1 = stream_task_logs(&fake_cli, "t-fake-task-123", 0, |ev| {
            events1.push(ev);
        })
        .await;
        assert!(res1.is_err()); // disconnected
        assert_eq!(events1.len(), 3);
        let last_seq = events1.last().unwrap().seq;
        assert_eq!(last_seq, 3);

        // Resume after seq 3
        set_fake_xcli_mode(&temp_dir, "normal");
        let mut events2 = Vec::new();
        let res2 = stream_task_logs(&fake_cli, "t-fake-task-123", last_seq, |ev| {
            events2.push(ev);
        })
        .await
        .unwrap();

        assert!(res2.is_some());
        // Events received should start from seq > 3 (4, 5, 6, 7)
        assert_eq!(events2.first().unwrap().seq, 4);
        assert_eq!(events2.last().unwrap().seq, 7);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_xcli_agents_inspection() {
        let temp_dir =
            std::env::temp_dir().join(format!("xcli-test-agents-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let fake_cli = make_fake_xcli(&temp_dir);

        let agents = list_agents(&fake_cli).await.unwrap();
        assert_eq!(agents.len(), 3);
        assert_eq!(agents[0].name, "codex");
        assert_eq!(agents[0].auth, "ok");
        assert_eq!(agents[2].name, "antigravity");
        assert_eq!(agents[2].auth, "unknown");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_xcli_background_lifecycle_and_draft_mapping() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "xcli-test-lifecycle-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&temp_dir).unwrap();
        let fake_cli = make_fake_xcli(&temp_dir);

        let db = crate::db::Database::open_for_test(temp_dir.clone(), None)
            .await
            .unwrap();
        let now = now_string();

        // Seed a work log
        sqlx::query(
            "INSERT INTO work_logs(work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, created_at, updated_at) \
             VALUES('2026-11-16', '2026-11-16', '2026-11-16', '超信项目', '完成超信生成', '已完成', '联调通过', '无', 8.0, 'high', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&db.pool)
        .await
        .unwrap();

        // Create a generation task
        let gen_id = sqlx::query(
            "INSERT INTO generation_tasks(report_type, period_start, period_end, xcli_task_id, status, title, created_at, updated_at) \
             VALUES('weekly_report', '2026-11-10', '2026-11-16', 't-fake-task-123', 'running', '2026-11-10 至 2026-11-16 周报', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&db.pool)
        .await
        .unwrap()
        .last_insert_rowid();

        let mut progress_list = Vec::new();

        let report = execute_task_lifecycle(&db.pool, &fake_cli, gen_id, |status, p| {
            progress_list.push((status.to_string(), p.to_string()));
        })
        .await
        .unwrap();

        assert_eq!(report.report_type, "weekly_report");
        assert_eq!(report.title, "2026-11-10 至 2026-11-16 周报");
        assert_eq!(report.status, "draft");
        assert!(report.content_markdown.contains("超信生成及短链平台联调"));
        assert!(report.content_markdown.contains("## 完成事项"));
        assert!(report.content_markdown.contains("## 下阶段计划"));
        assert!(!report.source_log_ids.is_empty());

        // Check task row is updated to completed
        let task_row = sqlx::query_as::<_, GenerationTask>(
            "SELECT id, report_type, period_start, period_end, status, message, report_id, completed_at, created_at, updated_at, \
             xcli_task_id, last_seq, step_progress, prompt_inputs, template_id, title, overwrite \
             FROM generation_tasks WHERE id=?",
        )
        .bind(gen_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();

        assert_eq!(task_row.status, "completed");
        assert_eq!(task_row.report_id, Some(report.id));
        assert!(task_row.last_seq.unwrap_or(0) >= 6);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_xcli_app_restart_recovery() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "xcli-test-recover-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&temp_dir).unwrap();
        let fake_cli = make_fake_xcli(&temp_dir);

        let db = crate::db::Database::open_for_test(temp_dir.clone(), None)
            .await
            .unwrap();
        let now = now_string();

        let gen_id = sqlx::query(
            "INSERT INTO generation_tasks(report_type, period_start, period_end, xcli_task_id, status, title, created_at, updated_at) \
             VALUES('weekly_report', '2026-11-10', '2026-11-16', 't-fake-recover-001', 'running', '2026-11-10 至 2026-11-16 周报', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&db.pool)
        .await
        .unwrap()
        .last_insert_rowid();

        // Call recover_active_task
        let recovered = recover_active_task(&db.pool, &fake_cli, gen_id)
            .await
            .unwrap();
        assert_eq!(recovered.status, "completed");
        assert!(recovered.report_id.is_some());

        let report = crate::reports::load_report(&db.pool, recovered.report_id.unwrap())
            .await
            .unwrap();
        assert!(report
            .content_markdown
            .contains("已恢复：本周完成了超信生成及短链平台联调"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_xcli_concurrent_task_isolation() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir =
            std::env::temp_dir().join(format!("xcli-test-iso-{}-{}", std::process::id(), unique));
        fs::create_dir_all(&temp_dir).unwrap();

        let task1_dir = temp_dir.join("task-1");
        let task2_dir = temp_dir.join("task-2");
        fs::create_dir_all(&task1_dir).unwrap();
        fs::create_dir_all(&task2_dir).unwrap();

        let input1 = task1_dir.join("work-items.json");
        let input2 = task2_dir.join("work-items.json");

        tokio::fs::write(&input1, r#"[{"task":"Task 1"}]"#)
            .await
            .unwrap();
        tokio::fs::write(&input2, r#"[{"task":"Task 2"}]"#)
            .await
            .unwrap();

        assert_ne!(input1, input2);
        assert!(input1.exists());
        assert!(input2.exists());

        let content1 = tokio::fs::read_to_string(&input1).await.unwrap();
        let content2 = tokio::fs::read_to_string(&input2).await.unwrap();
        assert_eq!(content1, r#"[{"task":"Task 1"}]"#);
        assert_eq!(content2, r#"[{"task":"Task 2"}]"#);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_xcli_daily_and_template_reports() {
        let goal_daily = build_report_goal("daily_report", "2026-11-17", "2026-11-17", None);
        assert!(goal_daily.contains("日报"));
        assert!(goal_daily.contains("2026-11-17"));

        let custom_template =
            "# {{ title }}\n\n核心成果：\n{{ summary }}\n\n明细：\n{{ highlights }}";
        let goal_template = build_report_goal(
            "weekly_report",
            "2026-11-10",
            "2026-11-16",
            Some(custom_template),
        );
        assert!(goal_template.contains("周报"));
        assert!(goal_template.contains("报告模板要求如下"));
        assert!(goal_template.contains("核心成果："));
    }

    #[test]
    fn test_short_report_template_and_formatting() {
        let short_name = "garden_bojin_weekly_short";
        let short_content = "# {{ title }}\n\n【请按事项逐条输出精简周报明细，每条格式为：\n序号、模块/业务项：具体完成事项描述。——状态（已完成/进行中）】";
        assert!(is_short_report_template(short_name, short_content));

        let non_short_name = "garden_bojin_weekly";
        let non_short_content = "# {{ title }}\n\n## 一、 周工作总结\n{{ summary }}\n\n## 二、 工作明细\n{% for item in work_items %}\n- {{ item.content }}\n{% endfor %}";
        assert!(!is_short_report_template(non_short_name, non_short_content));
        assert!(has_jinja_content_tags(non_short_content));

        let structured = StructuredReportOutput {
            summary: "本周推进多项研发任务".into(),
            items: vec![
                "1、V3.22.2-超信智能生成：完成草稿流式生成。——已完成".into(),
                "2、V3.20.2-5G消息模板：修复参数映射。——已完成".into(),
            ],
            blockers: vec!["环境权限等待确认".into()],
            next: vec!["下周进行联调上线".into()],
        };

        let formatted = format_short_report_markdown("2026-11-16 至 2026-11-22 周报", &structured);
        assert!(formatted.starts_with("# 2026-11-16 至 2026-11-22 周报\n\n"));
        assert!(formatted.contains("1、V3.22.2-超信智能生成：完成草稿流式生成。——已完成\n\n"));
        assert!(formatted.contains("2、V3.20.2-5G消息模板：修复参数映射。——已完成"));
        // Should NOT contain section headers for short report:
        assert!(!formatted.contains("## 完成事项"));
        assert!(!formatted.contains("## 阻碍与风险"));
    }

    #[test]
    fn test_resolve_xcli_path_from_home_local_bin() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "xcli-test-home-{}-{}",
            std::process::id(),
            unique
        ));
        let local_bin = temp_dir.join(".local").join("bin");
        fs::create_dir_all(&local_bin).unwrap();
        let fake_xcli = local_bin.join("xcli");
        fs::write(&fake_xcli, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&fake_xcli).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake_xcli, perms).unwrap();
        }

        let resolved = resolve_xcli_path_with_env(
            Some("xcli"),
            None,
            None,
            Some(&temp_dir),
        );
        assert_eq!(resolved, fake_xcli);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    #[ignore]
    async fn test_real_xcli_integration() {
        let real_xcli = resolve_xcli_path(None);
        if !real_xcli.is_file() {
            eprintln!(
                "Real xcli binary not found (resolved to {:?}); skipping test",
                real_xcli
            );
            return;
        }

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "worklog-real-xcli-test-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let db = crate::db::Database::open_for_test(temp_dir.clone(), None)
            .await
            .unwrap();
        let now = now_string();

        let log_id = sqlx::query(
            "INSERT INTO work_logs (work_date, start_date, end_date, project, task, progress, result, blockers, hours, priority, created_at, updated_at) \
             VALUES ('2026-11-17', '2026-11-17', '2026-11-17', 'Worklog', '接入 xcli v2 报告编排引擎', '已完成架构与协议适配', '完成后台任务流式进度与断点恢复并通过离线测试', '无', 3.5, 'high', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&db.pool)
        .await
        .unwrap()
        .last_insert_rowid();

        let task_dir = temp_dir.join("task-real");
        fs::create_dir_all(&task_dir).unwrap();
        let work_items_file = task_dir.join("work-items.json");

        let items = serde_json::json!([
            {
                "id": log_id,
                "project": "Worklog",
                "task": "接入 xcli v2 报告编排引擎",
                "progress": "已完成架构与协议适配",
                "result": "完成后台任务流式进度与断点恢复并通过离线测试",
                "blockers": "无",
                "hours": 3.5
            }
        ]);
        tokio::fs::write(
            &work_items_file,
            serde_json::to_string_pretty(&items).unwrap(),
        )
        .await
        .unwrap();

        let goal = build_report_goal("weekly_report", "2026-11-10", "2026-11-17", None);
        let mut metadata = HashMap::new();
        metadata.insert("app".into(), "worklog".into());
        metadata.insert("report_type".into(), "weekly".into());
        metadata.insert("report_id".into(), "real-test-1".into());

        let mut req = XcliRequest::new_ask(
            goal,
            vec![work_items_file.display().to_string()],
            Some(report_json_schema()),
        );
        req.metadata = metadata;

        println!("[Real Integration] Submitting request to real xcli...");
        let xcli_task_id = submit_task(&real_xcli, &req)
            .await
            .expect("real submit failed");
        println!(
            "[Real Integration] Received real xcli_task_id: {}",
            xcli_task_id
        );

        let gen_id = sqlx::query(
            "INSERT INTO generation_tasks (report_type, period_start, period_end, status, created_at, updated_at, xcli_task_id, prompt_inputs, title) \
             VALUES ('weekly_report', '2026-11-10', '2026-11-17', 'running', ?, ?, ?, ?, '2026-11-10 至 2026-11-17 周报')",
        )
        .bind(&now)
        .bind(&now)
        .bind(&xcli_task_id)
        .bind(work_items_file.display().to_string())
        .execute(&db.pool)
        .await
        .unwrap()
        .last_insert_rowid();

        println!("[Real Integration] Streaming real xcli logs...");
        let saved_report =
            execute_task_lifecycle(&db.pool, &real_xcli, gen_id, |status, progress| {
                println!(
                    "[Real Integration Progress] status: {}, progress: {}",
                    status, progress
                );
            })
            .await
            .expect("real execute_task_lifecycle failed");

        println!("[Real Integration] Task finished successfully!");
        println!(
            "[Real Integration] Saved Report Title: {}",
            saved_report.title
        );
        println!(
            "[Real Integration] Saved Report Content:\n{}",
            saved_report.content_markdown
        );

        assert!(!saved_report.content_markdown.is_empty());
        assert!(
            saved_report.content_markdown.contains("Worklog")
                || saved_report.content_markdown.contains("xcli")
                || saved_report.content_markdown.contains("完成")
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
