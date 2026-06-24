import React from "react";
import ReactDOM from "react-dom/client";
import {
  CalendarDays,
  Download,
  FileText,
  NotebookTabs,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Settings,
  Sparkles,
  Trash2,
  X
} from "lucide-react";

import { api, docxUrl } from "./api";
import type { LlmSetting, Priority, Provider, Report, ReportType, Template, WorkLog } from "./types";
import "./styles.css";

type Tab = "logs" | "reports" | "templates" | "settings";

const reportLabels: Record<ReportType, string> = {
  weekly_report: "周报",
  monthly_report: "月报",
  performance_review: "绩效考核表"
};

const providerDefaults: Record<Provider, { base_url: string; model: string }> = {
  openai: { base_url: "https://api.openai.com/v1", model: "gpt-4.1-mini" },
  nvidia: { base_url: "https://integrate.api.nvidia.com/v1", model: "meta/llama-3.1-70b-instruct" },
  openrouter: { base_url: "https://openrouter.ai/api/v1", model: "openai/gpt-4.1-mini" }
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
  const [notice, setNotice] = React.useState("");
  const [error, setError] = React.useState("");
  const [loading, setLoading] = React.useState(true);

  const refresh = React.useCallback(async (pageOverride = workLogPage, pageSizeOverride = workLogPageSize) => {
    setLoading(true);
    setError("");
    try {
      const [logsPage, reportList, templateList, setting] = await Promise.all([
        api.listWorkLogs(pageOverride, pageSizeOverride),
        api.listReports(),
        api.listTemplates(),
        api.getLlmSetting()
      ]);
      setWorkLogs(logsPage.items);
      setWorkLogMeta(logsPage);
      setReports(reportList);
      setTemplates(templateList);
      setLlmSetting(setting);
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
    <main className="app-shell">
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
            LLM 设置
          </TabButton>
        </nav>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{loading ? "同步中" : "本地自托管"}</p>
            <h1>{tabTitle(tab)}</h1>
          </div>
          <button className="icon-button" type="button" title="刷新" onClick={() => void refresh()}>
            <RefreshCw size={18} />
          </button>
        </header>

        {notice && <div className="notice">{notice}</div>}
        {error && <div className="error">{error}</div>}

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
            onGenerate={(payload) => run(() => api.generateReport(payload).then(() => undefined), "报告草稿已生成")}
            onSave={(id, payload) => run(() => api.updateReport(id, payload).then(() => undefined), "报告草稿已保存到本地数据库")}
            onDelete={(id) => run(() => api.deleteReport(id), "报告草稿已删除")}
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
                setNotice("已根据示例生成模板草稿");
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
            onSave={(payload) => run(() => api.updateLlmSetting(payload).then(() => undefined), "LLM 设置已保存")}
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
    settings: "LLM 设置"
  }[tab];
}

function TabButton(props: { active: boolean; icon: React.ReactNode; onClick: () => void; children: React.ReactNode }) {
  return (
    <button type="button" className={props.active ? "tab active" : "tab"} onClick={props.onClick}>
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

  return (
    <div className="two-column">
      <form className="panel form-grid" onSubmit={submit}>
        <h2>{editingId ? "编辑记录" : "新增记录"}</h2>
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
          项目
          <input value={form.project} onChange={(event) => setField("project", event.target.value)} required />
        </label>
        <label>
          事项
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
          进展
          <textarea value={form.progress} onChange={(event) => setField("progress", event.target.value)} required />
        </label>
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
        {props.items.map((item) => (
          <article className="row-card" key={item.id}>
            <div>
              <div className="row-meta">
                <span>{dateRangeLabel(item)}</span>
                <span>{item.project}</span>
                <span>{priorityLabel(item.priority)}</span>
                {item.hours != null && <span>{item.hours}h</span>}
              </div>
              <h3>{item.task}</h3>
              <p>{item.progress}</p>
              {item.result && <p className="muted">结果：{item.result}</p>}
              {item.blockers && <p className="warning">阻塞：{item.blockers}</p>}
            </div>
            <div className="row-actions">
              <button className="icon-button" type="button" title="编辑" onClick={() => startEdit(item)}>
                <Pencil size={16} />
              </button>
              <button className="icon-button danger" type="button" title="删除" onClick={() => void props.onDelete(item.id)}>
                <Trash2 size={16} />
              </button>
            </div>
          </article>
        ))}
        {props.items.length === 0 && <p className="empty">暂无工作记录。</p>}
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
  onGenerate: (payload: {
    report_type: ReportType;
    anchor_date?: string;
    template_id?: number;
    overwrite?: boolean;
  }) => Promise<void>;
  onSave: (id: number, payload: Partial<Report>) => Promise<void>;
  onDelete: (id: number) => Promise<void>;
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

  return (
    <div className="reports-layout">
      <section className="panel generate-bar">
        <select value={type} disabled={isGenerating} onChange={(event) => setType(event.target.value as ReportType)}>
          {Object.entries(reportLabels).map(([key, label]) => (
            <option key={key} value={key}>
              {label}
            </option>
          ))}
        </select>
        <input type="date" value={anchor} disabled={isGenerating} onChange={(event) => setAnchor(event.target.value)} />
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
        <button
          className="primary"
          type="button"
          disabled={isGenerating}
          onClick={() => void generateDraft()}
        >
          <RefreshCw className={isGenerating ? "spin" : undefined} size={18} />
          {isGenerating ? "生成中" : "生成草稿"}
        </button>
      </section>

      <div className="report-grid">
        <section className="list-panel">
          {props.reports.map((item) => (
            <button
              type="button"
              className={selected?.id === item.id ? "report-item active" : "report-item"}
              key={item.id}
              onClick={() => setSelectedId(item.id)}
            >
              <strong>{item.title}</strong>
              <span>
                {reportLabels[item.report_type]} · {item.period_start} 至 {item.period_end}
              </span>
            </button>
          ))}
          {props.reports.length === 0 && <p className="empty">暂无报告草稿。</p>}
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
              <a className="icon-button" title="导出 DOCX" href={docxUrl(selected.id)}>
                <Download size={18} />
              </a>
              <button
                className="icon-button"
                type="button"
                title="保存草稿到本地数据库"
                onClick={() => void props.onSave(selected.id, { title: draftTitle, content_markdown: draft })}
              >
                <Save size={18} />
              </button>
              <button
                className="icon-button danger"
                type="button"
                title="删除草稿"
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
      </div>
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

  React.useEffect(() => {
    if (selectedId === "new") {
      setForm(emptyTemplateForm());
      setShowExampleImport(false);
      setExampleContent("");
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
      setShowExampleImport(false);
    } finally {
      setIsImporting(false);
    }
  }

  return (
    <div className="report-grid">
      <section className="list-panel">
        <button className={selectedId === "new" ? "report-item active" : "report-item"} type="button" onClick={() => setSelectedId("new")}>
          <strong>新建模板</strong>
          <span>Markdown + Jinja 变量</span>
        </button>
        {props.templates.map((item) => (
          <button className={selected?.id === item.id ? "report-item active" : "report-item"} type="button" key={item.id} onClick={() => setSelectedId(item.id)}>
            <strong>{item.name}</strong>
            <span>
              {reportLabels[item.template_type]}
              {item.is_default ? " · 默认" : ""}
            </span>
          </button>
        ))}
      </section>

      <section className="template-editor form-grid">
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
        <label className="full">
          模板内容
          <textarea className="template-markdown-editor" value={form.content} onChange={(event) => setField("content", event.target.value)} />
        </label>
        <div className="button-row">
          <button
            className="primary"
            type="button"
            onClick={() => void (selected ? props.onSave(selected.id, form) : props.onCreate(form))}
          >
            <Save size={18} />
            保存模板
          </button>
          {selected && (
            <button className="secondary danger" type="button" onClick={() => void props.onDelete(selected.id)}>
              <Trash2 size={18} />
              删除模板
            </button>
          )}
        </div>
      </section>
    </div>
  );
}

function SettingsPage(props: { setting: LlmSetting | null; onSave: (payload: LlmSetting) => Promise<void> }) {
  const [form, setForm] = React.useState<LlmSetting>({
    provider: "openai",
    base_url: providerDefaults.openai.base_url,
    model: providerDefaults.openai.model,
    api_key: "",
    extra_headers: {}
  });
  const [headersText, setHeadersText] = React.useState("{}");

  React.useEffect(() => {
    if (props.setting) {
      setForm({ ...props.setting, api_key: "" });
      setHeadersText(JSON.stringify(props.setting.extra_headers ?? {}, null, 2));
    }
  }, [props.setting?.provider, props.setting?.base_url, props.setting?.model]);

  function setProvider(provider: Provider) {
    setForm((current) => ({
      ...current,
      provider,
      base_url: providerDefaults[provider].base_url,
      model: providerDefaults[provider].model
    }));
  }

  async function save() {
    let extra_headers: Record<string, string> = {};
    if (headersText.trim()) {
      extra_headers = JSON.parse(headersText);
    }
    await props.onSave({ ...form, extra_headers });
  }

  return (
    <section className="panel settings-panel">
      <label>
        Provider
        <select value={form.provider} onChange={(event) => setProvider(event.target.value as Provider)}>
          <option value="openai">OpenAI</option>
          <option value="nvidia">NVIDIA</option>
          <option value="openrouter">OpenRouter</option>
        </select>
      </label>
      <label>
        Base URL
        <input value={form.base_url} onChange={(event) => setForm({ ...form, base_url: event.target.value })} />
      </label>
      <label>
        Model
        <input value={form.model} onChange={(event) => setForm({ ...form, model: event.target.value })} />
      </label>
      <label>
        API Key
        <input
          type="password"
          placeholder={props.setting?.api_key ? `当前：${props.setting.api_key}` : ""}
          value={form.api_key ?? ""}
          onChange={(event) => setForm({ ...form, api_key: event.target.value })}
        />
      </label>
      <label>
        Extra Headers JSON
        <textarea value={headersText} onChange={(event) => setHeadersText(event.target.value)} />
      </label>
      <button className="primary" type="button" onClick={() => void save()}>
        <Save size={18} />
        保存设置
      </button>
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
