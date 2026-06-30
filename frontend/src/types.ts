export type ReportType = "weekly_report" | "monthly_report" | "performance_review";
export type Priority = "low" | "medium" | "high" | "urgent";
export type Provider = "openai" | "nvidia" | "openrouter";
export type EmailSecurity = "starttls" | "ssl";
export type EmailDeliveryStatus = "pending" | "sent" | "failed";
export type ScheduleWeekday = "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun";

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

export interface TemplateOptimizeResponse {
  template_type: ReportType;
  content: string;
  used_llm: boolean;
}

export interface ReportOptimizeResponse {
  report_id: number;
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
  id?: number;
  provider: Provider;
  base_url: string;
  model: string;
  api_key?: string | null;
  extra_headers: Record<string, string>;
  timeout_seconds: number;
  is_active?: boolean;
  created_at?: string;
  updated_at?: string;
}

export interface EmailSetting {
  host: string;
  port: number;
  security: EmailSecurity;
  username: string;
  password?: string | null;
  sender_address: string;
  sender_name?: string | null;
}

export interface Recipient {
  id: number;
  name: string;
  email: string;
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

export interface ReportSchedule {
  id: number;
  report_type: ReportType;
  enabled: boolean;
  weekday?: ScheduleWeekday | null;
  day_of_month?: number | null;
  template_id?: number | null;
  run_time: string;
  auto_send: boolean;
  recipient_ids: number[];
  next_run_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface DeliveryRecipient {
  name?: string | null;
  email: string;
}

export interface ReportEmailDelivery {
  id: number;
  report_id: number;
  subject: string;
  recipients: DeliveryRecipient[];
  status: EmailDeliveryStatus;
  error_message?: string | null;
  sent_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface GenerateResponse {
  report: Report;
  task_id: number;
  used_llm: boolean;
}
