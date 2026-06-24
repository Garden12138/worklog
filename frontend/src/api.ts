import type {
  GenerateResponse,
  LlmSetting,
  PaginatedWorkLogs,
  Report,
  ReportType,
  Template,
  TemplateImportExampleResponse,
  WorkLog
} from "./types";

const API_BASE = import.meta.env.VITE_API_BASE ?? "";

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...(options.headers ?? {})
    }
  });
  if (!response.ok) {
    let message = response.statusText;
    try {
      const data = await response.json();
      message = data.detail ?? message;
    } catch {
      // Response is not JSON; keep status text.
    }
    throw new Error(typeof message === "string" ? message : JSON.stringify(message));
  }
  if (response.status === 204) {
    return undefined as T;
  }
  return response.json() as Promise<T>;
}

export const api = {
  listWorkLogs: (page = 1, pageSize = 10) => request<PaginatedWorkLogs>(`/api/work-logs?page=${page}&page_size=${pageSize}`),
  createWorkLog: (payload: Partial<WorkLog>) =>
    request<WorkLog>("/api/work-logs", { method: "POST", body: JSON.stringify(payload) }),
  updateWorkLog: (id: number, payload: Partial<WorkLog>) =>
    request<WorkLog>(`/api/work-logs/${id}`, { method: "PUT", body: JSON.stringify(payload) }),
  deleteWorkLog: (id: number) => request<void>(`/api/work-logs/${id}`, { method: "DELETE" }),

  listTemplates: (type?: ReportType) =>
    request<Template[]>(`/api/templates${type ? `?template_type=${type}` : ""}`),
  createTemplate: (payload: Partial<Template>) =>
    request<Template>("/api/templates", { method: "POST", body: JSON.stringify(payload) }),
  importTemplateExample: (payload: { template_type: ReportType; example_content: string }) =>
    request<TemplateImportExampleResponse>("/api/templates/import-example", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  updateTemplate: (id: number, payload: Partial<Template>) =>
    request<Template>(`/api/templates/${id}`, { method: "PUT", body: JSON.stringify(payload) }),
  deleteTemplate: (id: number) => request<void>(`/api/templates/${id}`, { method: "DELETE" }),

  listReports: () => request<Report[]>("/api/reports"),
  updateReport: (id: number, payload: Partial<Report>) =>
    request<Report>(`/api/reports/${id}`, { method: "PUT", body: JSON.stringify(payload) }),
  deleteReport: (id: number) => request<void>(`/api/reports/${id}`, { method: "DELETE" }),
  generateReport: (payload: {
    report_type: ReportType;
    anchor_date?: string;
    period_start?: string;
    period_end?: string;
    template_id?: number;
    overwrite?: boolean;
  }) =>
    request<GenerateResponse>("/api/reports/generate", {
      method: "POST",
      body: JSON.stringify(payload)
    }),

  getLlmSetting: () => request<LlmSetting | null>("/api/settings/llm"),
  updateLlmSetting: (payload: LlmSetting) =>
    request<LlmSetting>("/api/settings/llm", { method: "PUT", body: JSON.stringify(payload) })
};

export function docxUrl(reportId: number): string {
  return `${API_BASE}/api/reports/${reportId}/export/docx`;
}
