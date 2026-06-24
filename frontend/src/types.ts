export type ReportType = "weekly_report" | "monthly_report" | "performance_review";
export type Priority = "low" | "medium" | "high" | "urgent";
export type Provider = "openai" | "nvidia" | "openrouter";

export interface WorkLog {
  id: number;
  work_date: string;
  start_date: string;
  end_date: string;
  project: string;
  task: string;
  progress: string;
  result?: string | null;
  blockers?: string | null;
  hours?: number | null;
  priority: Priority;
  notes?: string | null;
  created_at: string;
  updated_at: string;
}

export interface PaginatedWorkLogs {
  items: WorkLog[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
}

export interface Template {
  id: number;
  name: string;
  template_type: ReportType;
  content: string;
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

export interface TemplateImportExampleResponse {
  template_type: ReportType;
  content: string;
  used_llm: boolean;
}

export interface Report {
  id: number;
  report_type: ReportType;
  title: string;
  period_start: string;
  period_end: string;
  template_id?: number | null;
  content_markdown: string;
  status: string;
  source_log_ids: number[];
  generated_at?: string | null;
  edited_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface LlmSetting {
  provider: Provider;
  base_url: string;
  model: string;
  api_key?: string | null;
  extra_headers: Record<string, string>;
}

export interface GenerateResponse {
  report: Report;
  task_id: number;
  used_llm: boolean;
}
