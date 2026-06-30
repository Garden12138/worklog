import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

import type {
  DesktopPreferences,
  EmailSetting,
  GenerateResponse,
  LlmSetting,
  MigrationResult,
  PaginatedWorkLogs,
  Recipient,
  Report,
  ReportEmailDelivery,
  ReportOptimizeResponse,
  ReportSchedule,
  ReportType,
  Template,
  TemplateImportExampleResponse,
  TemplateOptimizeResponse,
  WorkLog
} from "./types";

function asError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }
  if (typeof error === "object" && error && "message" in error) {
    return new Error(String((error as { message: unknown }).message));
  }
  return new Error(typeof error === "string" ? error : JSON.stringify(error));
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw asError(error);
  }
}

export const api = {
  listWorkLogs: (page = 1, pageSize = 10) =>
    call<PaginatedWorkLogs>("list_work_logs", { page, pageSize }),
  createWorkLog: (payload: Partial<WorkLog>) =>
    call<WorkLog>("create_work_log", { payload }),
  updateWorkLog: (id: number, payload: Partial<WorkLog>) =>
    call<WorkLog>("update_work_log", { id, payload }),
  deleteWorkLog: (id: number) => call<void>("delete_work_log", { id }),

  listTemplates: (templateType?: ReportType) =>
    call<Template[]>("list_templates", { templateType: templateType ?? null }),
  createTemplate: (payload: Partial<Template>) =>
    call<Template>("create_template", { payload }),
  importTemplateExample: (payload: { template_type: ReportType; example_content: string }) =>
    call<TemplateImportExampleResponse>("import_template_example", { payload }),
  optimizeTemplate: (payload: { template_type: ReportType; content: string; optimization_request: string }) =>
    call<TemplateOptimizeResponse>("optimize_template", { payload }),
  updateTemplate: (id: number, payload: Partial<Template>) =>
    call<Template>("update_template", { id, payload }),
  deleteTemplate: (id: number) => call<void>("delete_template", { id }),

  listReports: () => call<Report[]>("list_reports"),
  updateReport: (id: number, payload: Partial<Report>) =>
    call<Report>("update_report", { id, payload }),
  optimizeReport: (id: number, payload: { content: string; optimization_request: string }) =>
    call<ReportOptimizeResponse>("optimize_report", { id, payload }),
  deleteReport: (id: number) => call<void>("delete_report", { id }),
  generateReport: (payload: {
    report_type: ReportType;
    anchor_date?: string;
    period_start?: string;
    period_end?: string;
    template_id?: number;
    overwrite?: boolean;
  }) => call<GenerateResponse>("generate_report", { payload }),
  exportReportDocx: async (reportId: number) => {
    const path = await save({
      defaultPath: `worklog-report-${reportId}.docx`,
      filters: [{ name: "Word Document", extensions: ["docx"] }]
    });
    if (!path) {
      return null;
    }
    return call<string>("export_report_docx", { reportId, path });
  },
  listReportEmailDeliveries: (reportId: number) =>
    call<ReportEmailDelivery[]>("list_report_email_deliveries", { reportId }),
  sendReportEmail: (reportId: number, payload: {
    recipient_ids: number[];
    additional_recipients: string[];
    subject: string;
  }) => call<ReportEmailDelivery>("send_report_email", { reportId, payload }),

  listReportSchedules: () => call<ReportSchedule[]>("list_report_schedules"),
  updateReportSchedule: (
    reportType: ReportType,
    payload: Pick<ReportSchedule, "enabled" | "weekday" | "day_of_month" | "template_id" | "run_time" | "auto_send" | "recipient_ids">
  ) => call<ReportSchedule>("update_report_schedule", { reportType, payload }),

  getLlmSetting: () => call<LlmSetting | null>("get_llm_setting"),
  listLlmSettings: () => call<LlmSetting[]>("list_llm_settings"),
  createLlmSetting: (payload: LlmSetting) =>
    call<LlmSetting>("create_llm_setting", { payload }),
  updateLlmSetting: (id: number, payload: LlmSetting) =>
    call<LlmSetting>("update_llm_setting", { id, payload }),
  applyLlmSetting: (id: number) => call<LlmSetting>("apply_llm_setting", { id }),
  deleteLlmSetting: (id: number) => call<void>("delete_llm_setting", { id }),

  getEmailSetting: () => call<EmailSetting | null>("get_email_setting"),
  updateEmailSetting: (payload: EmailSetting) =>
    call<EmailSetting>("update_email_setting", { payload }),
  testEmailSetting: async (recipientEmail: string) => ({
    sent: await call<boolean>("send_email_test", { recipientEmail })
  }),

  listRecipients: () => call<Recipient[]>("list_recipients"),
  createRecipient: (payload: Pick<Recipient, "name" | "email" | "is_default">) =>
    call<Recipient>("create_recipient", { payload }),
  updateRecipient: (id: number, payload: Partial<Pick<Recipient, "name" | "email" | "is_default">>) =>
    call<Recipient>("update_recipient", { id, payload }),
  deleteRecipient: (id: number) => call<void>("delete_recipient", { id }),

  getDesktopPreferences: () => call<DesktopPreferences>("get_desktop_preferences"),
  setLaunchAtLogin: (enabled: boolean) =>
    call<DesktopPreferences>("set_launch_at_login", { enabled }),
  getStartupMigration: () => call<MigrationResult>("get_startup_migration"),
  importLegacyDatabase: async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "SQLite Database", extensions: ["db", "sqlite", "sqlite3"] }]
    });
    if (!path) {
      return null;
    }
    return call<MigrationResult>("import_legacy_database", { path });
  }
};
