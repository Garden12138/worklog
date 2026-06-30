use crate::error::{AppError, AppResult};
use crate::models::WorkLog;
use minijinja::{context, Environment};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashSet;

const ALLOWED_VARIABLES: &[&str] = &[
    "title",
    "report_type",
    "period_start",
    "period_end",
    "generated_at",
    "ai_content",
    "summary",
    "work_items",
    "highlights",
    "blockers",
    "next_steps",
    "raw_llm_content",
];

pub struct ReportContext {
    pub title: String,
    pub report_type: String,
    pub period_start: String,
    pub period_end: String,
    pub generated_at: String,
    pub ai_content: String,
    pub work_items: Vec<Value>,
    pub work_items_markdown: String,
    pub highlights: Vec<String>,
    pub highlights_markdown: String,
    pub blockers: Vec<String>,
    pub blockers_markdown: String,
    pub next_steps: Vec<String>,
    pub next_steps_markdown: String,
}

pub fn validate_template(content: &str) -> AppResult<()> {
    if content.trim().is_empty() {
        return Err(AppError::validation(
            "content",
            "Template content is required",
        ));
    }
    let mut environment = Environment::new();
    environment.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    environment
        .add_template("candidate", content)
        .map_err(|error| {
            AppError::validation("content", format!("Template syntax error: {error}"))
        })?;

    let mut allowed: HashSet<String> = ALLOWED_VARIABLES
        .iter()
        .map(|value| value.to_string())
        .collect();
    allowed.insert("loop".into());
    let loop_declaration =
        Regex::new(r"\{%\s*for\s+([A-Za-z_][A-Za-z0-9_]*)\s+in\s+([A-Za-z_][A-Za-z0-9_]*)")
            .unwrap();
    for capture in loop_declaration.captures_iter(content) {
        allowed.insert(capture[1].to_string());
    }
    let expression = Regex::new(r"\{\{\s*([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    let statement = Regex::new(r"\{%\s*(?:for\s+\w+\s+in|if)\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    let invalid = expression
        .captures_iter(content)
        .chain(statement.captures_iter(content))
        .filter_map(|capture| capture.get(1).map(|item| item.as_str()))
        .filter(|name| !allowed.contains(*name))
        .collect::<HashSet<_>>();
    if !invalid.is_empty() {
        let mut invalid = invalid.into_iter().collect::<Vec<_>>();
        invalid.sort_unstable();
        return Err(AppError::validation(
            "content",
            format!("Unsupported template variable(s): {}", invalid.join(", ")),
        ));
    }
    Ok(())
}

pub fn requires_llm_fill(content: &str) -> bool {
    if content.contains("{{") || content.contains("{%") {
        return false;
    }
    Regex::new(r"【(?:填写|说明)[^】]*】|_{3,}")
        .unwrap()
        .is_match(content)
}

pub fn build_context(
    report_type: &str,
    period_start: &str,
    period_end: &str,
    logs: &[WorkLog],
    ai_content: String,
    generated_at: String,
) -> ReportContext {
    let title = format!(
        "{} 至 {} {}",
        period_start,
        period_end,
        report_title(report_type)
    );
    let work_items = logs
        .iter()
        .map(|item| {
            let date = if item.start_date == item.end_date {
                item.start_date.clone()
            } else {
                format!("{} 至 {}", item.start_date, item.end_date)
            };
            json!({
                "date": date,
                "start_date": item.start_date,
                "end_date": item.end_date,
                "project": item.project,
                "task": item.task,
                "status": item.progress,
                "content": format!("[{}] {}", item.project, item.task),
                "progress": item.progress,
                "conclusion": item.result.as_deref().or(item.notes.as_deref()).unwrap_or("暂无"),
                "result": item.result.as_deref().unwrap_or(""),
                "blockers": item.blockers.as_deref().unwrap_or(""),
                "hours": item.hours,
                "priority": item.priority,
                "notes": item.notes.as_deref().unwrap_or("")
            })
        })
        .collect::<Vec<_>>();
    let highlights = logs
        .iter()
        .filter_map(|item| item.result.clone())
        .collect::<Vec<_>>();
    let blockers = logs
        .iter()
        .filter_map(|item| item.blockers.clone())
        .collect::<Vec<_>>();
    let next_steps = vec!["继续推进未完成事项".to_string()];
    ReportContext {
        title,
        report_type: report_type.into(),
        period_start: period_start.into(),
        period_end: period_end.into(),
        generated_at,
        ai_content,
        work_items,
        work_items_markdown: format_logs(logs).unwrap_or_else(|| "- 暂无工作记录".into()),
        highlights_markdown: bullets(&highlights, "- 暂无"),
        highlights,
        blockers_markdown: bullets(&blockers, "- 暂无"),
        blockers,
        next_steps_markdown: bullets(&next_steps, "- 暂无"),
        next_steps,
    }
}

pub fn render_template(content: &str, report: &ReportContext) -> AppResult<String> {
    validate_template(content)?;
    let prepared = content
        .replace("{{ work_items }}", &report.work_items_markdown)
        .replace("{{work_items}}", &report.work_items_markdown)
        .replace("{{ highlights }}", &report.highlights_markdown)
        .replace("{{highlights}}", &report.highlights_markdown)
        .replace("{{ blockers }}", &report.blockers_markdown)
        .replace("{{blockers}}", &report.blockers_markdown)
        .replace("{{ next_steps }}", &report.next_steps_markdown)
        .replace("{{next_steps}}", &report.next_steps_markdown);
    let mut environment = Environment::new();
    environment.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    environment
        .add_template("report", &prepared)
        .map_err(|error| {
            AppError::validation("content", format!("Template syntax error: {error}"))
        })?;
    environment
        .get_template("report")
        .map_err(|error| AppError::validation("content", error.to_string()))?
        .render(context! {
            title => report.title,
            report_type => report.report_type,
            period_start => report.period_start,
            period_end => report.period_end,
            generated_at => report.generated_at,
            ai_content => report.ai_content,
            summary => report.ai_content,
            work_items => report.work_items,
            highlights => report.highlights,
            blockers => report.blockers,
            next_steps => report.next_steps,
            raw_llm_content => report.ai_content,
        })
        .map_err(|error| AppError::validation("content", format!("Template render error: {error}")))
}

pub fn format_logs(logs: &[WorkLog]) -> Option<String> {
    if logs.is_empty() {
        return None;
    }
    Some(
        logs.iter()
            .map(|item| {
                let date = if item.start_date == item.end_date {
                    item.start_date.clone()
                } else {
                    format!("{} 至 {}", item.start_date, item.end_date)
                };
                let result = item
                    .result
                    .as_ref()
                    .map(|value| format!("，结果：{value}"))
                    .unwrap_or_default();
                let blockers = item
                    .blockers
                    .as_ref()
                    .map(|value| format!("，阻塞：{value}"))
                    .unwrap_or_default();
                let hours = item
                    .hours
                    .map(|value| format!("，工时 {}h", format_number(value)))
                    .unwrap_or_default();
                let notes = item
                    .notes
                    .as_ref()
                    .map(|value| format!("，备注：{value}"))
                    .unwrap_or_default();
                format!(
                    "- {date} [{}] {}：{}{}{}{}{}",
                    item.project, item.task, item.progress, result, blockers, hours, notes
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn bullets(values: &[String], empty: &str) -> String {
    if values.is_empty() {
        empty.into()
    } else {
        values
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        let rendered = format!("{value:.2}");
        rendered
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

pub fn report_title(report_type: &str) -> &'static str {
    match report_type {
        "weekly_report" => "周报",
        "monthly_report" => "月报",
        "performance_review" => "绩效考核表",
        _ => "工作报告",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_allowed_variables_and_rejects_unknowns() {
        validate_template("# {{ title }}\n{{ ai_content }}").unwrap();
        let error = validate_template("{{ employee_name }}").unwrap_err();
        assert_eq!(error.code, "validation_error");
    }

    #[test]
    fn recognizes_plain_placeholder_templates() {
        assert!(requires_llm_fill("月份：____\n【填写核心任务】"));
        assert!(!requires_llm_fill("# {{ title }}"));
    }
}
