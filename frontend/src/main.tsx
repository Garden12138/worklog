import React from "react";
import ReactDOM from "react-dom/client";
import {
  CalendarDays,
  CheckCircle2,
  CircleAlert,
  ClipboardList,
  Download,
  FileText,
  LoaderCircle,
  Mail,
  NotebookTabs,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Send,
  Settings,
  Sparkles,
  Trash2,
  X
} from "lucide-react";

import { api, docxUrl } from "./api";
import type {
  EmailSetting,
  LlmSetting,
  Priority,
  Provider,
  Recipient,
  Report,
  ReportEmailDelivery,
  ReportType,
  Template,
  WorkLog
} from "./types";
import "./styles.css";

type Tab = "logs" | "reports" | "templates" | "settings";

const reportLabels: Record<ReportType, string> = {
  weekly_report: "周报",
  monthly_report: "月报",
  performance_review: "绩效考核表"
};

const providerLabels: Record<Provider, string> = {
  openai: "OpenAI",
  nvidia: "NVIDIA",
  openrouter: "OpenRouter"
};

const providerDefaults: Record<Provider, { base_url: string; model: string; timeout_seconds: number }> = {
  openai: { base_url: "https://api.openai.com/v1", model: "gpt-4.1-mini", timeout_seconds: 60 },
  nvidia: { base_url: "https://integrate.api.nvidia.com/v1", model: "meta/llama-3.1-70b-instruct", timeout_seconds: 180 },
  openrouter: { base_url: "https://openrouter.ai/api/v1", model: "openai/gpt-4.1-mini", timeout_seconds: 60 }
};

function today() {
  return new Date().toISOString().slice(0, 10);
}

function App() {
  const [tab, setTab] = React.useState<Tab>("logs");
  const [workLogs, setWorkLogs] = React.useState<WorkLog[]>([]);
  const [workLogPage, setWorkLogPage] = React.useState(1);
  const [workLogPageSize, setWorkLogPageSize] = React.useState(10);
  const [workLogMeta, setWorkLogMeta] = React.useState({
    total: 0,
    page: 1,
    page_size: 10,
    total_pages: 1
  });
  const [reports, setReports] = React.useState<Report[]>([]);
  const [templates, setTemplates] = React.useState<Template[]>([]);
  const [llmSetting, setLlmSetting] = React.useState<LlmSetting | null>(null);
  const [llmSettings, setLlmSettings] = React.useState<LlmSetting[]>([]);
  const [emailSetting, setEmailSetting] = React.useState<EmailSetting | null>(null);
  const [recipients, setRecipients] = React.useState<Recipient[]>([]);
  const [notice, setNotice] = React.useState("");
  const [error, setError] = React.useState("");
  const [loading, setLoading] = React.useState(true);

  const refresh = React.useCallback(async (pageOverride = workLogPage, pageSizeOverride = workLogPageSize) => {
    setLoading(true);
    setError("");
    try {
      const [
        logsPage,
        reportList,
        templateList,
        setting,
        savedSettings,
        savedEmailSetting,
        recipientList
      ] = await Promise.all([
        api.listWorkLogs(pageOverride, pageSizeOverride),
        api.listReports(),
        api.listTemplates(),
        api.getLlmSetting(),
        api.listLlmSettings(),
        api.getEmailSetting(),
        api.listRecipients()
      ]);
      setWorkLogs(logsPage.items);
      setWorkLogMeta(logsPage);
      setReports(reportList);
      setTemplates(templateList);
      setLlmSetting(setting);
      setLlmSettings(savedSettings);
      setEmailSetting(savedEmailSetting);
      setRecipients(recipientList);
    } catch (err) {
      setError(err instanceof Error ? err.message : "加载失败");
    } finally {
      setLoading(false);
    }
  }, [workLogPage, workLogPageSize]);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  async function run(action: () => Promise<void>, success: string, options?: { workLogPage?: number }) {
    setError("");
    setNotice("");
    try {
      await action();
      setNotice(success);
      if (options?.workLogPage) {
        setWorkLogPage(options.workLogPage);
      }
      await refresh(options?.workLogPage);
    } catch (err) {
      setError(err instanceof Error ? err.message : "操作失败");
      throw err;
    }
  }

  return (
    <main className={loading ? "app-shell is-loading" : "app-shell"}>
      <aside className="sidebar">
        <div className="brand">
          <NotebookTabs size={26} />
          <div>
            <strong>Worklog</strong>
            <span>工作记录与报告</span>
          </div>
        </div>
        <nav>
          <TabButton active={tab === "logs"} icon={<CalendarDays size={18} />} onClick={() => setTab("logs")}>
            每日记录
          </TabButton>
          <TabButton active={tab === "reports"} icon={<FileText size={18} />} onClick={() => setTab("reports")}>
            报告草稿
          </TabButton>
          <TabButton active={tab === "templates"} icon={<NotebookTabs size={18} />} onClick={() => setTab("templates")}>
            模板管理
          </TabButton>
          <TabButton active={tab === "settings"} icon={<Settings size={18} />} onClick={() => setTab("settings")}>
            系统设置
          </TabButton>
        </nav>
      </aside>

      <section className="workspace" aria-busy={loading}>
        <header className="topbar">
          <div>
            <p className="eyebrow status-line" aria-live="polite">
              {loading ? <LoaderCircle className="spin" size={14} /> : <CheckCircle2 size={14} />}
              {loading ? "正在同步本地数据" : "本地工作空间"}
            </p>
            <h1>{tabTitle(tab)}</h1>
          </div>
          <button className="icon-button" type="button" title="刷新数据" aria-label="刷新数据" disabled={loading} onClick={() => void refresh()}>
            <RefreshCw size={18} />
          </button>
        </header>

        {notice && <div className="notice" role="status"><CheckCircle2 size={17} />{notice}</div>}
        {error && <div className="error" role="alert"><CircleAlert size={17} />{error}</div>}

        {tab === "logs" && (
          <WorkLogsPage
            items={workLogs}
            meta={workLogMeta}
            pageSize={workLogPageSize}
            onCreate={(payload) => run(() => api.createWorkLog(payload).then(() => undefined), "工作记录已保存", { workLogPage: 1 })}
            onUpdate={(id, payload) => run(() => api.updateWorkLog(id, payload).then(() => undefined), "工作记录已更新")}
            onDelete={(id) => run(() => api.deleteWorkLog(id), "工作记录已删除")}
            onPageChange={setWorkLogPage}
            onPageSizeChange={(nextPageSize) => {
              setWorkLogPageSize(nextPageSize);
              setWorkLogPage(1);
            }}
          />
        )}
        {tab === "reports" && (
          <ReportsPage
            reports={reports}
            templates={templates}
            recipients={recipients}
            onGenerate={(payload) => run(() => api.generateReport(payload).then(() => undefined), "报告草稿已生成")}
            onSave={(id, payload) => run(() => api.updateReport(id, payload).then(() => undefined), "报告草稿已保存到本地数据库")}
            onDelete={(id) => run(() => api.deleteReport(id), "报告草稿已删除")}
            onSendEmail={(id, payload) => run(() => api.sendReportEmail(id, payload).then(() => undefined), "报告邮件已发送")}
            onListEmailDeliveries={(id) => api.listReportEmailDeliveries(id)}
          />
        )}
        {tab === "templates" && (
          <TemplatesPage
            templates={templates}
            onCreate={(payload) => run(() => api.createTemplate(payload).then(() => undefined), "模板已创建")}
            onImportExample={async (payload) => {
              setError("");
              setNotice("");
              try {
                const result = await api.importTemplateExample(payload);
                setNotice("已根据示例生成模板草稿，请检查并保存模板");
                return result.content;
              } catch (err) {
                setError(err instanceof Error ? err.message : "导入示例失败");
                throw err;
              }
            }}
            onSave={(id, payload) => run(() => api.updateTemplate(id, payload).then(() => undefined), "模板已保存")}
            onDelete={(id) => run(() => api.deleteTemplate(id), "模板已删除")}
          />
        )}
        {tab === "settings" && (
          <SettingsPage
            setting={llmSetting}
            settings={llmSettings}
            emailSetting={emailSetting}
            recipients={recipients}
            onSave={(payload) => run(
              () => (payload.id
                ? api.updateLlmSetting(payload.id, payload)
                : api.createLlmSetting(payload)
              ).then(() => undefined),
              payload.id ? "LLM 设置已更新并应用" : "LLM 新设置已保存并应用"
            )}
            onApplyLlmSetting={(id) => run(() => api.applyLlmSetting(id).then(() => undefined), "LLM 配置已应用")}
            onDeleteLlmSetting={(id) => run(() => api.deleteLlmSetting(id), "LLM 配置已删除")}
            onSaveEmail={(payload) => run(() => api.updateEmailSetting(payload).then(() => undefined), "SMTP 邮箱设置已保存")}
            onTestEmail={(address) => run(() => api.testEmailSetting(address).then(() => undefined), "测试邮件已发送")}
            onCreateRecipient={(payload) => run(() => api.createRecipient(payload).then(() => undefined), "收件人已添加")}
            onUpdateRecipient={(id, payload) => run(() => api.updateRecipient(id, payload).then(() => undefined), "收件人已更新")}
            onDeleteRecipient={(id) => run(() => api.deleteRecipient(id), "收件人已删除")}
          />
        )}
      </section>
    </main>
  );
}

function tabTitle(tab: Tab) {
  return {
    logs: "每日记录",
    reports: "报告草稿",
    templates: "模板管理",
    settings: "系统设置"
  }[tab];
}

function TabButton(props: { active: boolean; icon: React.ReactNode; onClick: () => void; children: React.ReactNode }) {
  return (
    <button type="button" className={props.active ? "tab active" : "tab"} aria-current={props.active ? "page" : undefined} onClick={props.onClick}>
      {props.icon}
      <span>{props.children}</span>
    </button>
  );
}

function WorkLogsPage(props: {
  items: WorkLog[];
  meta: { total: number; page: number; page_size: number; total_pages: number };
  pageSize: number;
  onCreate: (payload: Partial<WorkLog>) => Promise<void>;
  onUpdate: (id: number, payload: Partial<WorkLog>) => Promise<void>;
  onDelete: (id: number) => Promise<void>;
  onPageChange: (page: number) => void;
  onPageSizeChange: (pageSize: number) => void;
}) {
  const emptyForm = React.useCallback(
    () => ({
      start_date: today(),
      end_date: today(),
      project: "",
      task: "",
      progress: "",
      result: "",
      blockers: "",
      hours: "",
      priority: "medium" as Priority,
      notes: ""
    }),
    []
  );
  const [form, setForm] = React.useState({
    start_date: today(),
    end_date: today(),
    project: "",
    task: "",
    progress: "",
    result: "",
    blockers: "",
    hours: "",
    priority: "medium" as Priority,
    notes: ""
  });
  const [editingId, setEditingId] = React.useState<number | null>(null);

  function setField(name: string, value: string) {
    setForm((current) => ({ ...current, [name]: value }));
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    const payload = {
      ...form,
      work_date: form.start_date,
      hours: form.hours ? Number(form.hours) : null
    };
    if (editingId) {
      await props.onUpdate(editingId, payload);
    } else {
      await props.onCreate(payload);
    }
    setEditingId(null);
    setForm(emptyForm());
  }

  function startEdit(item: WorkLog) {
    setEditingId(item.id);
    setForm({
      start_date: item.start_date,
      end_date: item.end_date,
      project: item.project,
      task: item.task,
      progress: item.progress,
      result: item.result ?? "",
      blockers: item.blockers ?? "",
      hours: item.hours != null ? String(item.hours) : "",
      priority: item.priority,
      notes: item.notes ?? ""
    });
  }

  function cancelEdit() {
    setEditingId(null);
    setForm(emptyForm());
  }

  async function deleteItem(item: WorkLog) {
    const confirmed = window.confirm(`删除工作记录「${item.task}」？`);
    if (confirmed) {
      await props.onDelete(item.id);
    }
  }

  return (
    <div className="two-column">
      <form className="panel form-grid" onSubmit={submit}>
        <div className="panel-heading full">
          <div>
            <p className="section-eyebrow">每日工作日志</p>
            <h2>{editingId ? "编辑记录" : "新增记录"}</h2>
          </div>
          {editingId && <span className="status-badge">编辑中</span>}
        </div>
        <p className="form-hint full"><span aria-hidden="true">*</span> 为必填项</p>
        <p className="form-section-title full">基本信息</p>
        <label>
          开始日期
          <input
            type="date"
            value={form.start_date}
            onChange={(event) => {
              setForm((current) => ({
                ...current,
                start_date: event.target.value,
                end_date: current.end_date < event.target.value ? event.target.value : current.end_date
              }));
            }}
          />
        </label>
        <label>
          结束日期
          <input
            type="date"
            value={form.end_date}
            min={form.start_date}
            onChange={(event) => setField("end_date", event.target.value)}
          />
        </label>
        <label>
          项目 <em aria-hidden="true">*</em>
          <input value={form.project} onChange={(event) => setField("project", event.target.value)} required />
        </label>
        <label>
          事项 <em aria-hidden="true">*</em>
          <input value={form.task} onChange={(event) => setField("task", event.target.value)} required />
        </label>
        <label>
          优先级
          <select value={form.priority} onChange={(event) => setField("priority", event.target.value)}>
            <option value="low">低</option>
            <option value="medium">中</option>
            <option value="high">高</option>
            <option value="urgent">紧急</option>
          </select>
        </label>
        <label>
          工时
          <input type="number" min="0" max="24" step="0.25" value={form.hours} onChange={(event) => setField("hours", event.target.value)} />
        </label>
        <label className="full">
          进展 <em aria-hidden="true">*</em>
          <textarea value={form.progress} onChange={(event) => setField("progress", event.target.value)} required />
        </label>
        <p className="form-section-title full">复盘与补充</p>
        <label className="full">
          结果
          <textarea value={form.result} onChange={(event) => setField("result", event.target.value)} />
        </label>
        <label className="full">
          阻塞
          <textarea value={form.blockers} onChange={(event) => setField("blockers", event.target.value)} />
        </label>
        <label className="full">
          备注
          <textarea value={form.notes} onChange={(event) => setField("notes", event.target.value)} />
        </label>
        <div className="button-row">
          <button className="primary" type="submit">
            {editingId ? <Save size={18} /> : <Plus size={18} />}
            {editingId ? "保存修改" : "保存记录"}
          </button>
          {editingId && (
            <button className="secondary" type="button" onClick={cancelEdit}>
              <X size={18} />
              取消编辑
            </button>
          )}
        </div>
      </form>

      <section className="list-panel">
        <div className="list-panel-header">
          <div>
            <p className="section-eyebrow">历史记录</p>
            <h2>工作记录</h2>
          </div>
          <span className="count-badge">{props.meta.total} 条</span>
        </div>
        {props.items.map((item) => (
          <article className="row-card" key={item.id}>
            <div>
              <div className="row-meta">
                <span className="meta-date">{dateRangeLabel(item)}</span>
                <span className="meta-project">{item.project}</span>
                <span className={`priority-badge priority-${item.priority}`}>{priorityLabel(item.priority)}</span>
                {item.hours != null && <span className="hours-badge">{item.hours}h</span>}
              </div>
              <h3>{item.task}</h3>
              <p>{item.progress}</p>
              {item.result && <p className="muted">结果：{item.result}</p>}
              {item.blockers && <p className="warning">阻塞：{item.blockers}</p>}
            </div>
            <div className="row-actions">
              <button className="icon-button" type="button" title="编辑记录" aria-label={`编辑记录：${item.task}`} onClick={() => startEdit(item)}>
                <Pencil size={16} />
              </button>
              <button className="icon-button danger" type="button" title="删除记录" aria-label={`删除记录：${item.task}`} onClick={() => void deleteItem(item)}>
                <Trash2 size={16} />
              </button>
            </div>
          </article>
        ))}
        {props.items.length === 0 && (
          <div className="empty-state">
            <ClipboardList size={28} />
            <div>
              <strong>还没有工作记录</strong>
              <p>从左侧填写今天的进展，记录会保存在这里。</p>
            </div>
          </div>
        )}
        <div className="pagination">
          <span>
            共 {props.meta.total} 条 · 第 {props.meta.page} / {props.meta.total_pages} 页
          </span>
          <select value={props.pageSize} onChange={(event) => props.onPageSizeChange(Number(event.target.value))}>
            <option value={5}>5 条/页</option>
            <option value={10}>10 条/页</option>
            <option value={20}>20 条/页</option>
            <option value={50}>50 条/页</option>
          </select>
          <button
            className="secondary"
            type="button"
            disabled={props.meta.page <= 1}
            onClick={() => props.onPageChange(props.meta.page - 1)}
          >
            上一页
          </button>
          <button
            className="secondary"
            type="button"
            disabled={props.meta.page >= props.meta.total_pages}
            onClick={() => props.onPageChange(props.meta.page + 1)}
          >
            下一页
          </button>
        </div>
      </section>
    </div>
  );
}

function dateRangeLabel(item: WorkLog) {
  return item.start_date === item.end_date ? `${item.start_date} · 当天事项` : `${item.start_date} 至 ${item.end_date}`;
}

function ReportsPage(props: {
  reports: Report[];
  templates: Template[];
  recipients: Recipient[];
  onGenerate: (payload: {
    report_type: ReportType;
    anchor_date?: string;
    template_id?: number;
    overwrite?: boolean;
  }) => Promise<void>;
  onSave: (id: number, payload: Partial<Report>) => Promise<void>;
  onDelete: (id: number) => Promise<void>;
  onSendEmail: (id: number, payload: { recipient_ids: number[]; additional_recipients: string[]; subject: string }) => Promise<void>;
  onListEmailDeliveries: (id: number) => Promise<ReportEmailDelivery[]>;
}) {
  const [type, setType] = React.useState<ReportType>("weekly_report");
  const [anchor, setAnchor] = React.useState(today());
  const [templateId, setTemplateId] = React.useState("");
  const [selectedId, setSelectedId] = React.useState<number | null>(null);
  const selected = props.reports.find((item) => item.id === selectedId) ?? props.reports[0];
  const [draft, setDraft] = React.useState("");
  const [draftTitle, setDraftTitle] = React.useState("");
  const [viewMode, setViewMode] = React.useState<"edit" | "preview">("edit");
  const [isGenerating, setIsGenerating] = React.useState(false);
  const [showEmailDialog, setShowEmailDialog] = React.useState(false);
  const [emailSubject, setEmailSubject] = React.useState("");
  const [selectedRecipientIds, setSelectedRecipientIds] = React.useState<number[]>([]);
  const [additionalRecipients, setAdditionalRecipients] = React.useState("");
  const [emailDeliveries, setEmailDeliveries] = React.useState<ReportEmailDelivery[]>([]);
  const [isSendingEmail, setIsSendingEmail] = React.useState(false);
  const [emailError, setEmailError] = React.useState("");
  const generationLock = React.useRef(false);

  React.useEffect(() => {
    setDraft(selected?.content_markdown ?? "");
    setDraftTitle(selected?.title ?? "");
  }, [selected?.id]);

  async function generateDraft() {
    if (generationLock.current) {
      return;
    }
    generationLock.current = true;
    setIsGenerating(true);
    try {
      await props.onGenerate({
        report_type: type,
        anchor_date: anchor,
        template_id: templateId ? Number(templateId) : undefined,
        overwrite: false
      });
    } finally {
      generationLock.current = false;
      setIsGenerating(false);
    }
  }

  async function deleteSelectedReport() {
    if (!selected) {
      return;
    }
    const confirmed = window.confirm(`删除草稿「${selected.title}」？`);
    if (!confirmed) {
      return;
    }
    await props.onDelete(selected.id);
    setSelectedId(null);
  }

  async function openEmailDialog() {
    if (!selected) {
      return;
    }
    setEmailSubject(draftTitle || selected.title);
    setSelectedRecipientIds(props.recipients.filter((recipient) => recipient.is_default).map((recipient) => recipient.id));
    setAdditionalRecipients("");
    setEmailError("");
    setShowEmailDialog(true);
    try {
      setEmailDeliveries(await props.onListEmailDeliveries(selected.id));
    } catch (error) {
      setEmailError(error instanceof Error ? error.message : "发送记录加载失败");
    }
  }

  function toggleRecipient(recipientId: number) {
    setSelectedRecipientIds((current) =>
      current.includes(recipientId) ? current.filter((id) => id !== recipientId) : [...current, recipientId]
    );
  }

  async function sendEmail() {
    if (!selected || isSendingEmail) {
      return;
    }
    const transientRecipients = additionalRecipients
      .split(/[;,，\n]/)
      .map((item) => item.trim())
      .filter(Boolean);
    if (!emailSubject.trim()) {
      setEmailError("请填写邮件主题。");
      return;
    }
    if (selectedRecipientIds.length === 0 && transientRecipients.length === 0) {
      setEmailError("请至少选择一位收件人或填写一个邮箱。");
      return;
    }
    setIsSendingEmail(true);
    setEmailError("");
    try {
      await props.onSave(selected.id, { title: draftTitle, content_markdown: draft });
      await props.onSendEmail(selected.id, {
        recipient_ids: selectedRecipientIds,
        additional_recipients: transientRecipients,
        subject: emailSubject.trim()
      });
      setEmailDeliveries(await props.onListEmailDeliveries(selected.id));
      setShowEmailDialog(false);
    } catch (error) {
      setEmailError(error instanceof Error ? error.message : "邮件发送失败");
      try {
        setEmailDeliveries(await props.onListEmailDeliveries(selected.id));
      } catch {
        // Keep the original delivery error visible if the history refresh also fails.
      }
    } finally {
      setIsSendingEmail(false);
    }
  }

  return (
    <div className="reports-layout">
      <section className="panel generation-panel">
        <div className="generation-copy">
          <p className="section-eyebrow">AI 报告助手</p>
          <h2>生成新草稿</h2>
          <p>按周期汇总已有工作记录，并以选定模板生成可编辑内容。</p>
        </div>
        <div className="generate-bar">
          <label className="compact-field">
            报告类型
            <select value={type} disabled={isGenerating} onChange={(event) => setType(event.target.value as ReportType)}>
              {Object.entries(reportLabels).map(([key, label]) => (
                <option key={key} value={key}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="compact-field">
            参考日期
            <input type="date" value={anchor} disabled={isGenerating} onChange={(event) => setAnchor(event.target.value)} />
          </label>
          <label className="compact-field">
            使用模板
            <select value={templateId} disabled={isGenerating} onChange={(event) => setTemplateId(event.target.value)}>
              <option value="">默认模板</option>
              {props.templates
                .filter((template) => template.template_type === type)
                .map((template) => (
                  <option key={template.id} value={template.id}>
                    {template.name}
                  </option>
                ))}
            </select>
          </label>
          <button
            className="primary"
            type="button"
            disabled={isGenerating}
            onClick={() => void generateDraft()}
          >
            <RefreshCw className={isGenerating ? "spin" : undefined} size={18} />
            {isGenerating ? "生成中" : "生成草稿"}
          </button>
        </div>
      </section>

      <div className="report-grid">
        <section className="list-panel">
          <div className="list-panel-header">
            <div>
              <p className="section-eyebrow">已生成内容</p>
              <h2>报告草稿</h2>
            </div>
            <span className="count-badge">{props.reports.length} 份</span>
          </div>
          {props.reports.map((item) => (
            <button
              type="button"
              className={selected?.id === item.id ? "report-item active" : "report-item"}
              key={item.id}
              onClick={() => setSelectedId(item.id)}
            >
              <strong>{item.title}</strong>
              <span className="report-item-meta"><b>{reportLabels[item.report_type]}</b>{item.period_start} 至 {item.period_end}</span>
            </button>
          ))}
          {props.reports.length === 0 && (
            <div className="empty-state report-empty-state">
              <FileText size={28} />
              <div>
                <strong>还没有报告草稿</strong>
                <p>选择报告类型与参考日期，即可生成第一份草稿。</p>
              </div>
              <div className="empty-steps" aria-label="生成报告步骤">
                <span>选择类型</span>
                <span>确认日期</span>
                <span>生成草稿</span>
              </div>
            </div>
          )}
        </section>

        {selected && (
          <section className="editor-panel">
            <div className="editor-toolbar">
              <input
                value={draftTitle}
                onChange={(event) => setDraftTitle(event.target.value)}
              />
              <div className="mode-switch" role="group" aria-label="Markdown view mode">
                <button
                  type="button"
                  className={viewMode === "edit" ? "active" : ""}
                  onClick={() => setViewMode("edit")}
                >
                  <Pencil size={15} />
                  编辑
                </button>
                <button
                  type="button"
                  className={viewMode === "preview" ? "active" : ""}
                  onClick={() => setViewMode("preview")}
                >
                  <FileText size={15} />
                  预览
                </button>
              </div>
              <a className="icon-button" title="导出 DOCX" aria-label="导出 DOCX" href={docxUrl(selected.id)}>
                <Download size={18} />
              </a>
              <button
                className="icon-button"
                type="button"
                title="发送邮件"
                aria-label="发送邮件"
                onClick={() => void openEmailDialog()}
              >
                <Mail size={18} />
              </button>
              <button
                className="icon-button"
                type="button"
                title="保存草稿到本地数据库"
                aria-label="保存草稿"
                onClick={() => void props.onSave(selected.id, { title: draftTitle, content_markdown: draft })}
              >
                <Save size={18} />
              </button>
              <button
                className="icon-button danger"
                type="button"
                title="删除草稿"
                aria-label="删除草稿"
                onClick={() => void deleteSelectedReport()}
              >
                <Trash2 size={18} />
              </button>
            </div>
            {viewMode === "edit" ? (
              <div className="editor-body">
                <textarea className="markdown-editor" value={draft} onChange={(event) => setDraft(event.target.value)} />
              </div>
            ) : (
              <div className="editor-body">
                <MarkdownPreview content={draft} />
              </div>
            )}
          </section>
        )}
        {!selected && (
          <section className="editor-empty">
            <FileText size={32} />
            <span className="empty-kicker">准备就绪</span>
            <h2>报告将在这里编辑</h2>
            <p>生成或选择一份报告草稿后，可以在这里修改内容、预览 Markdown 并导出 DOCX。</p>
          </section>
        )}
      </div>
      {showEmailDialog && selected && (
        <section className="email-dialog-backdrop" role="presentation" onMouseDown={() => !isSendingEmail && setShowEmailDialog(false)}>
          <div
            className="email-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="email-dialog-title"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="dialog-heading">
              <div>
                <p className="section-eyebrow">发送报告</p>
                <h2 id="email-dialog-title">发送给收件人</h2>
                <p>将保存当前草稿，并发送正文与 DOCX 附件。</p>
              </div>
              <button className="icon-button" type="button" aria-label="关闭发送窗口" onClick={() => setShowEmailDialog(false)} disabled={isSendingEmail}>
                <X size={18} />
              </button>
            </div>
            <label>
              邮件主题
              <input value={emailSubject} onChange={(event) => setEmailSubject(event.target.value)} disabled={isSendingEmail} />
            </label>
            <div className="recipient-picker">
              <div className="dialog-field-label">通讯录收件人</div>
              {props.recipients.length === 0 ? (
                <p className="form-hint">通讯录暂无收件人，可在系统设置中添加，或直接填写下方临时邮箱。</p>
              ) : (
                <div className="recipient-options">
                  {props.recipients.map((recipient) => (
                    <label className="recipient-option" key={recipient.id}>
                      <input
                        type="checkbox"
                        checked={selectedRecipientIds.includes(recipient.id)}
                        disabled={isSendingEmail}
                        onChange={() => toggleRecipient(recipient.id)}
                      />
                      <span>
                        <strong>{recipient.name}</strong>
                        <small>{recipient.email}</small>
                      </span>
                      {recipient.is_default && <em>默认</em>}
                    </label>
                  ))}
                </div>
              )}
            </div>
            <label>
              临时邮箱
              <textarea
                className="recipient-input"
                placeholder="多个邮箱可使用逗号、分号或换行分隔；不会自动保存到通讯录"
                value={additionalRecipients}
                disabled={isSendingEmail}
                onChange={(event) => setAdditionalRecipients(event.target.value)}
              />
            </label>
            {emailError && <p className="field-error" role="alert">{emailError}</p>}
            <div className="delivery-history">
              <div className="dialog-field-label">最近发送记录</div>
              {emailDeliveries.length === 0 ? (
                <p className="form-hint">这份报告尚未发送过。</p>
              ) : (
                emailDeliveries.slice(0, 5).map((delivery) => (
                  <div className={`delivery-item ${delivery.status}`} key={delivery.id}>
                    <span>{delivery.status === "sent" ? "已发送" : delivery.status === "failed" ? "发送失败" : "发送中"}</span>
                    <p>{delivery.recipients.map((recipient) => recipient.email).join("、")}</p>
                    <small>{new Date(delivery.sent_at ?? delivery.created_at).toLocaleString("zh-CN")}</small>
                    {delivery.error_message && <small className="delivery-error">{delivery.error_message}</small>}
                  </div>
                ))
              )}
            </div>
            <div className="button-row dialog-actions">
              <button className="secondary" type="button" disabled={isSendingEmail} onClick={() => setShowEmailDialog(false)}>取消</button>
              <button className="primary" type="button" disabled={isSendingEmail} onClick={() => void sendEmail()}>
                <Send size={17} />
                {isSendingEmail ? "发送中" : "保存并发送"}
              </button>
            </div>
          </div>
        </section>
      )}
    </div>
  );
}

function MarkdownPreview(props: { content: string }) {
  const lines = props.content.split(/\r?\n/);
  const blocks: React.ReactNode[] = [];
  let listItems: string[] = [];
  let listType: "ul" | "ol" | null = null;
  let codeLines: string[] | null = null;

  function flushList() {
    if (!listType || listItems.length === 0) {
      return;
    }
    const items = listItems.map((item, index) => <li key={index}>{renderInlineMarkdown(item)}</li>);
    blocks.push(
      listType === "ul" ? (
        <ul key={`list-${blocks.length}`}>{items}</ul>
      ) : (
        <ol key={`list-${blocks.length}`}>{items}</ol>
      )
    );
    listItems = [];
    listType = null;
  }

  function flushCode() {
    if (!codeLines) {
      return;
    }
    blocks.push(
      <pre key={`code-${blocks.length}`}>
        <code>{codeLines.join("\n")}</code>
      </pre>
    );
    codeLines = null;
  }

  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    const trimmed = line.trim();
    if (trimmed.startsWith("```")) {
      if (codeLines) {
        flushCode();
      } else {
        flushList();
        codeLines = [];
      }
      index += 1;
      continue;
    }

    if (codeLines) {
      codeLines.push(line);
      index += 1;
      continue;
    }

    if (!trimmed) {
      flushList();
      index += 1;
      continue;
    }

    if (isMarkdownTableStart(lines, index)) {
      flushList();
      const tableRows = [trimmed];
      index += 2;
      while (index < lines.length && isMarkdownTableRow(lines[index])) {
        tableRows.push(lines[index].trim());
        index += 1;
      }
      blocks.push(renderMarkdownTable(tableRows, `table-${blocks.length}`));
      continue;
    }

    const heading = trimmed.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {
      flushList();
      const level = heading[1].length;
      const text = renderInlineMarkdown(heading[2]);
      if (level === 1) {
        blocks.push(<h1 key={`h-${blocks.length}`}>{text}</h1>);
      } else if (level === 2) {
        blocks.push(<h2 key={`h-${blocks.length}`}>{text}</h2>);
      } else if (level === 3) {
        blocks.push(<h3 key={`h-${blocks.length}`}>{text}</h3>);
      } else {
        blocks.push(<h4 key={`h-${blocks.length}`}>{text}</h4>);
      }
      index += 1;
      continue;
    }

    const bullet = trimmed.match(/^[-*]\s+(.+)$/);
    if (bullet) {
      if (listType && listType !== "ul") {
        flushList();
      }
      listType = "ul";
      listItems.push(bullet[1]);
      index += 1;
      continue;
    }

    const numbered = trimmed.match(/^\d+\.\s+(.+)$/);
    if (numbered) {
      if (listType && listType !== "ol") {
        flushList();
      }
      listType = "ol";
      listItems.push(numbered[1]);
      index += 1;
      continue;
    }

    flushList();
    blocks.push(<p key={`p-${blocks.length}`}>{renderInlineMarkdown(trimmed)}</p>);
    index += 1;
  }

  flushList();
  flushCode();

  return (
    <div className="markdown-preview">
      {blocks.length ? blocks : <p className="empty">暂无预览内容。</p>}
    </div>
  );
}

function isMarkdownTableStart(lines: string[], index: number) {
  return isMarkdownTableRow(lines[index]) && isMarkdownTableDivider(lines[index + 1] ?? "");
}

function isMarkdownTableRow(line: string) {
  const trimmed = line.trim();
  return trimmed.includes("|") && splitMarkdownTableRow(trimmed).length >= 2;
}

function isMarkdownTableDivider(line: string) {
  const cells = splitMarkdownTableRow(line);
  return cells.length >= 2 && cells.every((cell) => /^:?-{3,}:?$/.test(cell.trim()));
}

function splitMarkdownTableRow(row: string) {
  let trimmed = row.trim();
  if (trimmed.startsWith("|")) {
    trimmed = trimmed.slice(1);
  }
  if (trimmed.endsWith("|")) {
    trimmed = trimmed.slice(0, -1);
  }
  return trimmed.split("|").map((cell) => cell.trim());
}

function renderMarkdownTable(rows: string[], key: string) {
  const header = splitMarkdownTableRow(rows[0]);
  const body = rows.slice(1).map(splitMarkdownTableRow);
  return (
    <div className="markdown-table-wrap" key={key}>
      <table>
        <thead>
          <tr>
            {header.map((cell, index) => (
              <th key={index}>{renderInlineMarkdown(cell)}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {body.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {header.map((_, cellIndex) => (
                <td key={cellIndex}>{renderInlineMarkdown(row[cellIndex] ?? "")}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function renderInlineMarkdown(text: string) {
  const breakParts = text.split(/<br\s*\/?>/i);
  if (breakParts.length > 1) {
    return breakParts.flatMap((part, index) => {
      const rendered = renderInlineMarkdownTokens(part);
      return index === breakParts.length - 1 ? [rendered] : [rendered, <br key={`br-${index}`} />];
    });
  }
  return renderInlineMarkdownTokens(text);
}

function renderInlineMarkdownTokens(text: string) {
  const parts: React.ReactNode[] = [];
  const matcher = /(\*\*[^*]+\*\*|`[^`]+`)/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = matcher.exec(text)) !== null) {
    if (match.index > cursor) {
      parts.push(text.slice(cursor, match.index));
    }
    const token = match[0];
    if (token.startsWith("**")) {
      parts.push(<strong key={parts.length}>{token.slice(2, -2)}</strong>);
    } else {
      parts.push(<code key={parts.length}>{token.slice(1, -1)}</code>);
    }
    cursor = match.index + token.length;
  }

  if (cursor < text.length) {
    parts.push(text.slice(cursor));
  }

  return parts.length ? parts : text;
}

function TemplatesPage(props: {
  templates: Template[];
  onCreate: (payload: Partial<Template>) => Promise<void>;
  onImportExample: (payload: { template_type: ReportType; example_content: string }) => Promise<string>;
  onSave: (id: number, payload: Partial<Template>) => Promise<void>;
  onDelete: (id: number) => Promise<void>;
}) {
  const [selectedId, setSelectedId] = React.useState<number | "new">("new");
  const selected = selectedId === "new" ? null : props.templates.find((item) => item.id === selectedId) ?? null;
  const emptyTemplateForm = React.useCallback(
    () => ({
      name: "自定义模板",
      template_type: "weekly_report" as ReportType,
      content: "# {{ title }}\n\n周期：{{ period_start }} - {{ period_end }}\n\n{{ ai_content }}",
      is_default: false
    }),
    []
  );
  const [form, setForm] = React.useState(emptyTemplateForm());
  const [showExampleImport, setShowExampleImport] = React.useState(false);
  const [exampleContent, setExampleContent] = React.useState("");
  const [isImporting, setIsImporting] = React.useState(false);
  const [hasImportedDraft, setHasImportedDraft] = React.useState(false);
  const contentEditorRef = React.useRef<HTMLTextAreaElement | null>(null);

  React.useEffect(() => {
    if (selectedId === "new") {
      setForm(emptyTemplateForm());
      setShowExampleImport(false);
      setExampleContent("");
      setHasImportedDraft(false);
    }
  }, [emptyTemplateForm, selectedId]);

  React.useEffect(() => {
    if (selected) {
      setForm({
        name: selected.name,
        template_type: selected.template_type,
        content: selected.content,
        is_default: selected.is_default
      });
      setHasImportedDraft(false);
    }
  }, [selected?.id]);

  function setField(name: string, value: string | boolean) {
    setForm((current) => ({ ...current, [name]: value }));
  }

  async function importExample() {
    if (exampleContent.trim().length < 20) {
      window.alert("请先粘贴一段完整的模板示例，至少 20 个字符。");
      return;
    }
    setIsImporting(true);
    try {
      const content = await props.onImportExample({
        template_type: form.template_type,
        example_content: exampleContent,
      });
      setForm((current) => ({ ...current, content }));
      setHasImportedDraft(true);
      setShowExampleImport(false);
      window.requestAnimationFrame(() => {
        contentEditorRef.current?.scrollIntoView({ behavior: "smooth", block: "center" });
        contentEditorRef.current?.focus();
      });
    } finally {
      setIsImporting(false);
    }
  }

  async function saveTemplate() {
    if (selected) {
      await props.onSave(selected.id, form);
    } else {
      await props.onCreate(form);
    }
    setHasImportedDraft(false);
  }

  async function deleteTemplate() {
    if (!selected) {
      return;
    }
    const confirmed = window.confirm(`删除模板「${selected.name}」？`);
    if (confirmed) {
      await props.onDelete(selected.id);
    }
  }

  return (
    <div className="report-grid">
      <section className="list-panel">
        <div className="list-panel-header">
          <div>
            <p className="section-eyebrow">可复用格式</p>
            <h2>报告模板</h2>
          </div>
          <span className="count-badge">{props.templates.length} 个</span>
        </div>
        <button className={selectedId === "new" ? "report-item active" : "report-item"} type="button" onClick={() => setSelectedId("new")}>
          <strong>新建模板</strong>
          <span className="report-item-meta"><b>自定义</b>Markdown + Jinja 变量</span>
        </button>
        {props.templates.map((item) => (
          <button className={selected?.id === item.id ? "report-item active" : "report-item"} type="button" key={item.id} onClick={() => setSelectedId(item.id)}>
            <strong>{item.name}</strong>
            <span className="report-item-meta"><b>{reportLabels[item.template_type]}</b>{item.is_default ? "默认模板" : "自定义模板"}</span>
          </button>
        ))}
      </section>

      <section className="template-editor form-grid">
        <div className="panel-heading full">
          <div>
            <p className="section-eyebrow">模板编辑器</p>
            <h2>{selected ? "编辑模板" : "创建模板"}</h2>
          </div>
          {selected?.is_default && <span className="status-badge">默认模板</span>}
        </div>
        <label>
          模板名称
          <input value={form.name} onChange={(event) => setField("name", event.target.value)} />
        </label>
        <label>
          类型
          <select value={form.template_type} onChange={(event) => setField("template_type", event.target.value)}>
            {Object.entries(reportLabels).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label className="toggle">
          <input type="checkbox" checked={form.is_default} onChange={(event) => setField("is_default", event.target.checked)} />
          设为默认模板
        </label>
        {selectedId === "new" && (
          <div className="import-example full">
            <div className="import-example-header">
              <div>
                <strong>导入示例</strong>
                <span>粘贴一份已有周报/月报/绩效表示例，由 LLM 抽象成可复用模板。</span>
              </div>
              <button className="secondary" type="button" onClick={() => setShowExampleImport((value) => !value)}>
                <Sparkles size={16} />
                {showExampleImport ? "收起" : "导入示例"}
              </button>
            </div>
            {showExampleImport && (
              <div className="import-example-body">
                <textarea
                  value={exampleContent}
                  onChange={(event) => setExampleContent(event.target.value)}
                  placeholder="粘贴一份完整示例，例如公司现有周报格式、月报格式或绩效考核表格式..."
                />
                <button className="primary" type="button" disabled={isImporting} onClick={() => void importExample()}>
                  <Sparkles size={16} />
                  {isImporting ? "生成中" : "生成模板草稿"}
                </button>
              </div>
            )}
          </div>
        )}
        {hasImportedDraft && (
          <div className="draft-callout full" role="status">
            <CheckCircle2 size={16} />
            <span>模板草稿已填入下方“模板内容”，保存后会出现在左侧模板列表。</span>
          </div>
        )}
        <label className="full">
          模板内容
          <textarea
            ref={contentEditorRef}
            className="template-markdown-editor"
            value={form.content}
            onChange={(event) => setField("content", event.target.value)}
          />
        </label>
        <div className="button-row">
          <button
            className="primary"
            type="button"
            onClick={() => void saveTemplate()}
          >
            <Save size={18} />
            保存模板
          </button>
          {selected && (
            <button className="secondary danger" type="button" onClick={() => void deleteTemplate()}>
              <Trash2 size={18} />
              删除模板
            </button>
          )}
        </div>
      </section>
    </div>
  );
}

function SettingsPage(props: {
  setting: LlmSetting | null;
  settings: LlmSetting[];
  emailSetting: EmailSetting | null;
  recipients: Recipient[];
  onSave: (payload: LlmSetting) => Promise<void>;
  onApplyLlmSetting: (id: number) => Promise<void>;
  onDeleteLlmSetting: (id: number) => Promise<void>;
  onSaveEmail: (payload: EmailSetting) => Promise<void>;
  onTestEmail: (address: string) => Promise<void>;
  onCreateRecipient: (payload: Pick<Recipient, "name" | "email" | "is_default">) => Promise<void>;
  onUpdateRecipient: (id: number, payload: Partial<Pick<Recipient, "name" | "email" | "is_default">>) => Promise<void>;
  onDeleteRecipient: (id: number) => Promise<void>;
}) {
  const [form, setForm] = React.useState<LlmSetting>({
    provider: "openai",
    base_url: providerDefaults.openai.base_url,
    model: providerDefaults.openai.model,
    api_key: "",
    extra_headers: {},
    timeout_seconds: providerDefaults.openai.timeout_seconds
  });
  const [headersText, setHeadersText] = React.useState("{}");
  const [headersError, setHeadersError] = React.useState("");
  const activeProviderLabel = props.setting ? providerLabels[props.setting.provider] : "未配置";
  const formSetting = props.settings.find((item) => item.id === form.id);
  const apiKeyPlaceholder = formSetting?.api_key
    ? `当前：${formSetting.api_key}`
    : props.setting?.api_key
      ? `当前：${props.setting.api_key}`
      : "";

  React.useEffect(() => {
    if (props.setting) {
      setForm({ ...props.setting, api_key: "" });
      setHeadersText(JSON.stringify(props.setting.extra_headers ?? {}, null, 2));
    }
  }, [
    props.setting?.id,
    props.setting?.provider,
    props.setting?.base_url,
    props.setting?.model,
    props.setting?.api_key,
    props.setting?.timeout_seconds,
    props.setting?.updated_at
  ]);

  function loadSavedSetting(setting: LlmSetting) {
    setForm({ ...setting, api_key: "" });
    setHeadersText(JSON.stringify(setting.extra_headers ?? {}, null, 2));
    setHeadersError("");
  }

  function setProvider(provider: Provider) {
    const extra_headers: Record<string, string> = {};
    setForm((current) => ({
      ...current,
      id: undefined,
      is_active: undefined,
      created_at: undefined,
      updated_at: undefined,
      provider,
      base_url: providerDefaults[provider].base_url,
      model: providerDefaults[provider].model,
      api_key: "",
      extra_headers,
      timeout_seconds: providerDefaults[provider].timeout_seconds
    }));
    setHeadersText(JSON.stringify(extra_headers, null, 2));
    setHeadersError("");
  }

  function startNewSetting() {
    const provider = props.setting?.provider ?? "openai";
    const extra_headers: Record<string, string> = {};
    setForm({
      provider,
      base_url: providerDefaults[provider].base_url,
      model: providerDefaults[provider].model,
      api_key: "",
      extra_headers,
      timeout_seconds: providerDefaults[provider].timeout_seconds
    });
    setHeadersText(JSON.stringify(extra_headers, null, 2));
    setHeadersError("");
  }

  async function save(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    let extra_headers: Record<string, string> = {};
    try {
      if (headersText.trim()) {
        const parsed: unknown = JSON.parse(headersText);
        if (!parsed || Array.isArray(parsed) || typeof parsed !== "object" || Object.values(parsed).some((value) => typeof value !== "string")) {
          throw new Error("扩展请求头必须是一个键和值均为文本的 JSON 对象。");
        }
        extra_headers = parsed as Record<string, string>;
      }
    } catch (error) {
      setHeadersError(error instanceof Error ? error.message : "扩展请求头格式无效。");
      return;
    }
    setHeadersError("");
    await props.onSave({ ...form, extra_headers });
  }

  async function removeSavedSetting(setting: LlmSetting) {
    if (!setting.id || setting.is_active) {
      return;
    }
    const label = `${providerLabels[setting.provider]} / ${setting.model}`;
    if (!window.confirm(`删除 LLM 配置「${label}」？`)) {
      return;
    }
    await props.onDeleteLlmSetting(setting.id);
    if (form.id === setting.id && props.setting) {
      loadSavedSetting(props.setting);
    }
  }

  return (
    <div className="settings-stack">
      <form className="panel settings-panel" onSubmit={(event) => void save(event)}>
        <div className="settings-intro">
          <p className="section-eyebrow">本地配置</p>
          <h2>LLM 设置</h2>
          <p>配置一个兼容 OpenAI API 的服务，用于生成报告和从示例提炼模板。</p>
          <div className="settings-side-note">
            <span>当前应用</span>
            <strong>{activeProviderLabel}{props.setting ? ` / ${props.setting.model}` : ""}</strong>
            <p>保存并应用后，报告生成和模板提炼会立即使用这一组配置。各 Provider 的密钥独立保留。</p>
          </div>
        </div>

        <section className="settings-section saved-llm-section">
          <div className="saved-llm-header">
            <div>
              <h3>已保存 LLM 配置</h3>
              <p>这些配置会保留在本地；点击“应用”后，生成报告和模板会使用对应配置。</p>
            </div>
            <div className="saved-llm-header-actions">
              <span className="count-badge">{props.settings.length} 个</span>
              <button className="secondary" type="button" onClick={startNewSetting}>
                <Plus size={16} />
                新增配置
              </button>
            </div>
          </div>
          {props.settings.length ? (
            <div className="saved-llm-list">
              {props.settings.map((item) => (
                <div className={item.is_active ? "saved-llm-item active" : "saved-llm-item"} key={item.id}>
                  <button className="saved-llm-main" type="button" onClick={() => loadSavedSetting(item)}>
                    <strong>{providerLabels[item.provider]} / {item.model}</strong>
                    <span>{item.base_url}</span>
                    <small>{item.api_key ? `Key ${item.api_key}` : "未保存 API Key"}</small>
                    <small>请求超时 {item.timeout_seconds} 秒</small>
                  </button>
                  <div className="saved-llm-actions">
                    {item.is_active && <span className="status-badge">当前应用</span>}
                    <button className="secondary" type="button" onClick={() => loadSavedSetting(item)}>
                      编辑
                    </button>
                    <button
                      className="primary"
                      type="button"
                      disabled={Boolean(item.is_active) || !item.id}
                      onClick={() => item.id && void props.onApplyLlmSetting(item.id)}
                    >
                      应用
                    </button>
                    <button
                      className="icon-button danger"
                      type="button"
                      disabled={Boolean(item.is_active) || !item.id}
                      title={item.is_active ? "当前应用的配置不能删除，请先应用其他配置" : "删除配置"}
                      aria-label={`删除 ${providerLabels[item.provider]} ${item.model} 配置`}
                      onClick={() => void removeSavedSetting(item)}
                    >
                      <Trash2 size={16} />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <p className="empty-hint">还没有保存过 LLM 配置。</p>
          )}
        </section>

        <fieldset className="settings-section">
          <legend>模型服务</legend>
          <p>切换服务商会自动填入推荐的 Base URL 与模型名称，你仍可按实际环境调整。</p>
          <div className="settings-grid">
            <label>
              Provider
              <select value={form.provider} onChange={(event) => setProvider(event.target.value as Provider)}>
                <option value="openai">OpenAI</option>
                <option value="nvidia">NVIDIA</option>
                <option value="openrouter">OpenRouter</option>
              </select>
            </label>
            <label>
              Model
              <input value={form.model} onChange={(event) => setForm({ ...form, model: event.target.value })} />
            </label>
            <label className="full">
              Base URL
              <input value={form.base_url} onChange={(event) => setForm({ ...form, base_url: event.target.value })} />
            </label>
            <label>
              请求超时（秒）
              <input
                type="number"
                min={5}
                max={600}
                step={5}
                required
                value={form.timeout_seconds}
                onChange={(event) => setForm({ ...form, timeout_seconds: Number(event.target.value) })}
              />
            </label>
          </div>
        </fieldset>

        <fieldset className="settings-section">
          <legend>认证与扩展</legend>
          <p>API Key 仅在填写新值时更新；保留为空可继续使用当前已保存的密钥。</p>
          <p>切换 Provider 后留空 API Key 时，只会复用该 Provider 之前保存过的密钥。</p>
          <div className="settings-grid">
            <label className="full">
              API Key
              <input
                type="password"
                placeholder={apiKeyPlaceholder}
                value={form.api_key ?? ""}
                onChange={(event) => setForm({ ...form, api_key: event.target.value })}
              />
            </label>
            <label className="full">
              Extra Headers JSON
              <textarea
                aria-describedby={headersError ? "headers-error" : undefined}
                aria-invalid={Boolean(headersError)}
                value={headersText}
                onChange={(event) => {
                  setHeadersText(event.target.value);
                  setHeadersError("");
                }}
              />
            </label>
            {headersError && <p className="field-error full" id="headers-error" role="alert">{headersError}</p>}
          </div>
        </fieldset>

        <div className="button-row settings-actions">
          <button className="primary" type="submit">
            <Save size={18} />
            {form.id ? "保存并应用修改" : "保存并应用新配置"}
          </button>
        </div>
      </form>
      <EmailSettingsPanel setting={props.emailSetting} onSave={props.onSaveEmail} onTest={props.onTestEmail} />
      <RecipientDirectory
        recipients={props.recipients}
        onCreate={props.onCreateRecipient}
        onUpdate={props.onUpdateRecipient}
        onDelete={props.onDeleteRecipient}
      />
    </div>
  );
}

function EmailSettingsPanel(props: {
  setting: EmailSetting | null;
  onSave: (payload: EmailSetting) => Promise<void>;
  onTest: (address: string) => Promise<void>;
}) {
  const [form, setForm] = React.useState<EmailSetting>({
    host: "",
    port: 587,
    security: "starttls",
    username: "",
    password: "",
    sender_address: "",
    sender_name: ""
  });
  const [testAddress, setTestAddress] = React.useState("");

  React.useEffect(() => {
    if (props.setting) {
      setForm({ ...props.setting, password: "" });
      setTestAddress(props.setting.sender_address);
    }
  }, [props.setting?.host, props.setting?.port, props.setting?.username, props.setting?.sender_address]);

  async function save(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await props.onSave({ ...form, sender_name: form.sender_name?.trim() || null });
  }

  return (
    <form className="panel settings-panel" onSubmit={(event) => void save(event)}>
      <div className="settings-intro">
        <p className="section-eyebrow">邮件投递</p>
        <h2>SMTP 邮箱设置</h2>
        <p>使用你的企业邮箱或个人邮箱 SMTP 服务发送报告。密码只保存在本地后端，读取时会脱敏。</p>
        <div className="settings-side-note">
          <span>密码提示</span>
          <strong>留空即可保留当前密码</strong>
          <p>多数邮箱需要填写“应用专用密码”，而不是网页登录密码。</p>
        </div>
      </div>
      <fieldset className="settings-section">
        <legend>服务器与发件人</legend>
        <div className="settings-grid">
          <label>
            SMTP 主机
            <input required value={form.host} placeholder="smtp.example.com" onChange={(event) => setForm({ ...form, host: event.target.value })} />
          </label>
          <label>
            端口
            <input required type="number" min="1" max="65535" value={form.port} onChange={(event) => setForm({ ...form, port: Number(event.target.value) })} />
          </label>
          <label>
            加密方式
            <select value={form.security} onChange={(event) => setForm({ ...form, security: event.target.value as EmailSetting["security"] })}>
              <option value="starttls">STARTTLS（通常为 587）</option>
              <option value="ssl">SSL/TLS（通常为 465）</option>
            </select>
          </label>
          <label>
            SMTP 用户名
            <input required value={form.username} onChange={(event) => setForm({ ...form, username: event.target.value })} />
          </label>
          <label className="full">
            SMTP 密码 / 应用专用密码
            <input
              type="password"
              placeholder={props.setting?.password ? `当前：${props.setting.password}` : "首次保存时必填"}
              value={form.password ?? ""}
              onChange={(event) => setForm({ ...form, password: event.target.value })}
            />
          </label>
          <label>
            发件人邮箱
            <input required type="email" value={form.sender_address} onChange={(event) => setForm({ ...form, sender_address: event.target.value })} />
          </label>
          <label>
            发件人名称（可选）
            <input value={form.sender_name ?? ""} onChange={(event) => setForm({ ...form, sender_name: event.target.value })} />
          </label>
        </div>
      </fieldset>
      <div className="button-row settings-actions">
        <button className="primary" type="submit"><Save size={18} />保存 SMTP 设置</button>
      </div>
      <fieldset className="settings-section smtp-test-section">
        <legend>发送测试邮件</legend>
        <p>保存 SMTP 设置后，发送一封测试邮件以确认网络与鉴权正常。</p>
        <div className="test-email-row">
          <input type="email" value={testAddress} placeholder="your-email@example.com" onChange={(event) => setTestAddress(event.target.value)} />
          <button className="secondary" type="button" disabled={!testAddress.trim()} onClick={() => void props.onTest(testAddress.trim())}>
            <Mail size={17} />发送测试
          </button>
        </div>
      </fieldset>
    </form>
  );
}

function RecipientDirectory(props: {
  recipients: Recipient[];
  onCreate: (payload: Pick<Recipient, "name" | "email" | "is_default">) => Promise<void>;
  onUpdate: (id: number, payload: Partial<Pick<Recipient, "name" | "email" | "is_default">>) => Promise<void>;
  onDelete: (id: number) => Promise<void>;
}) {
  const emptyForm = { name: "", email: "", is_default: false };
  const [form, setForm] = React.useState(emptyForm);
  const [editingId, setEditingId] = React.useState<number | null>(null);

  function startEdit(recipient: Recipient) {
    setEditingId(recipient.id);
    setForm({ name: recipient.name, email: recipient.email, is_default: recipient.is_default });
  }

  function reset() {
    setEditingId(null);
    setForm(emptyForm);
  }

  async function save(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (editingId) {
      await props.onUpdate(editingId, form);
    } else {
      await props.onCreate(form);
    }
    reset();
  }

  async function remove(recipient: Recipient) {
    if (!window.confirm(`删除收件人「${recipient.name}」？`)) {
      return;
    }
    await props.onDelete(recipient.id);
    if (editingId === recipient.id) {
      reset();
    }
  }

  return (
    <section className="panel recipient-directory">
      <div className="panel-heading">
        <div>
          <p className="section-eyebrow">报告对象</p>
          <h2>收件人通讯录</h2>
          <p>默认收件人会在发送报告时自动勾选；可设置多个。</p>
        </div>
        <span className="count-badge">{props.recipients.length} 位</span>
      </div>
      <div className="recipient-directory-body">
        <div className="recipient-list">
          {props.recipients.length === 0 ? (
            <p className="form-hint">还没有收件人。添加上级邮箱后，发送报告时可一键选中。</p>
          ) : props.recipients.map((recipient) => (
            <div className={editingId === recipient.id ? "recipient-row active" : "recipient-row"} key={recipient.id}>
              <button type="button" onClick={() => startEdit(recipient)}>
                <strong>{recipient.name}</strong>
                <span>{recipient.email}</span>
              </button>
              {recipient.is_default && <em>默认</em>}
              <button className="icon-button danger" type="button" title="删除收件人" aria-label={`删除 ${recipient.name}`} onClick={() => void remove(recipient)}><Trash2 size={16} /></button>
            </div>
          ))}
        </div>
        <form className="recipient-form" onSubmit={(event) => void save(event)}>
          <h3>{editingId ? "编辑收件人" : "添加收件人"}</h3>
          <label>姓名<input required value={form.name} onChange={(event) => setForm({ ...form, name: event.target.value })} /></label>
          <label>邮箱<input required type="email" value={form.email} onChange={(event) => setForm({ ...form, email: event.target.value })} /></label>
          <label className="toggle"><input type="checkbox" checked={form.is_default} onChange={(event) => setForm({ ...form, is_default: event.target.checked })} />默认收件人</label>
          <div className="button-row">
            <button className="primary" type="submit"><Plus size={17} />{editingId ? "保存修改" : "添加收件人"}</button>
            {editingId && <button className="secondary" type="button" onClick={reset}>取消</button>}
          </div>
        </form>
      </div>
    </section>
  );
}

function priorityLabel(priority: Priority) {
  return {
    low: "低",
    medium: "中",
    high: "高",
    urgent: "紧急"
  }[priority];
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
