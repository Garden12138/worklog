import React from "react";
import ReactDOM from "react-dom/client";
import {
  Activity,
  ArrowRight,
  Award,
  Bot,
  CalendarDays,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  CircleAlert,
  ClipboardList,
  Clock,
  Code2,
  Copy,
  Cpu,
  Database,
  Download,
  FileText,
  Flame,
  FolderPlus,
  GitCommit,
  LoaderCircle,
  Mail,
  MessageSquare,
  NotebookTabs,
  Pencil,
  PieChart,
  Play,
  Plus,
  Radio,
  RefreshCw,
  RotateCcw,
  Save,
  Send,
  Settings,
  ShieldCheck,
  Sparkles,
  Square,
  Target,
  Trash2,
  TrendingUp,
  Users,
  X
} from "lucide-react";

import { api } from "./api";
import type {
  ActivityEvidence,
  ActivitySource,
  ActivitySourceCandidate,
  ActivitySourceType,
  ChatMessage,
  DailyCaptureRun,
  DailyCaptureSettings,
  DesktopPreferences,
  EmailSetting,
  FieldChange,
  LlmSetting,
  MigrationResult,
  Priority,
  Provider,
  Recipient,
  Report,
  ReportEmailDelivery,
  ReportSchedule,
  ReportType,
  ScheduleWeekday,
  Template,
  WorkLog,
  WorkLogDialogueResponse,
  WorkLogModificationAction,
  DailyWorkStat,
  WorkReportSummary
} from "./types";
import "./styles.css";

type Tab = "logs" | "reports" | "templates" | "settings";

const reportLabels: Record<ReportType, string> = {
  weekly_report: "周报",
  monthly_report: "月报",
  performance_review: "绩效考核表"
};

const providerLabels: Record<Provider, string> = {
  xcli: "xcli 本机网关（推理位置取决于上游）",
  openai: "OpenAI",
  nvidia: "NVIDIA",
  openrouter: "OpenRouter",
  minimax: "MiniMax",
  bailian: "阿里云百炼"
};

const providerDefaults: Record<Provider, { base_url: string; model: string; timeout_seconds: number }> = {
  xcli: { base_url: "http://127.0.0.1:18741/v1", model: "xcli/text-default", timeout_seconds: 180 },
  openai: { base_url: "https://api.openai.com/v1", model: "gpt-4.1-mini", timeout_seconds: 60 },
  nvidia: { base_url: "https://integrate.api.nvidia.com/v1", model: "meta/llama-3.1-70b-instruct", timeout_seconds: 180 },
  openrouter: { base_url: "https://openrouter.ai/api/v1", model: "openai/gpt-4.1-mini", timeout_seconds: 60 },
  minimax: { base_url: "https://api.minimax.cn/v1", model: "MiniMax-M3", timeout_seconds: 180 },
  bailian: { base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "qwen-plus", timeout_seconds: 60 }
};

function today() {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit"
  }).formatToParts(new Date());
  const value = Object.fromEntries(parts.map((part) => [part.type, part.value]));
  return `${value.year}-${value.month}-${value.day}`;
}

interface CalendarDay {
  dateStr: string;
  dayNumber: number;
  isCurrentMonth: boolean;
  isToday: boolean;
}

function getCalendarGrid(year: number, month: number, todayStr: string): CalendarDay[] {
  const firstDayOfMonth = new Date(year, month - 1, 1);
  const totalDaysInMonth = new Date(year, month, 0).getDate();
  const startDayOfWeek = (firstDayOfMonth.getDay() + 6) % 7;
  const prevMonthTotalDays = new Date(year, month - 1, 0).getDate();

  const days: CalendarDay[] = [];

  // Previous month padding
  for (let i = startDayOfWeek - 1; i >= 0; i--) {
    const day = prevMonthTotalDays - i;
    const prevMonth = month === 1 ? 12 : month - 1;
    const prevYear = month === 1 ? year - 1 : year;
    const dateStr = `${prevYear}-${String(prevMonth).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
    days.push({
      dateStr,
      dayNumber: day,
      isCurrentMonth: false,
      isToday: dateStr === todayStr
    });
  }

  // Current month
  for (let day = 1; day <= totalDaysInMonth; day++) {
    const dateStr = `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
    days.push({
      dateStr,
      dayNumber: day,
      isCurrentMonth: true,
      isToday: dateStr === todayStr
    });
  }

  // Next month padding to fill row
  const remaining = (7 - (days.length % 7)) % 7;
  for (let day = 1; day <= remaining; day++) {
    const nextMonth = month === 12 ? 1 : month + 1;
    const nextYear = month === 12 ? year + 1 : year;
    const dateStr = `${nextYear}-${String(nextMonth).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
    days.push({
      dateStr,
      dayNumber: day,
      isCurrentMonth: false,
      isToday: dateStr === todayStr
    });
  }

  return days;
}

function SidebarClockAndCalendar(props: {
  todayStr: string;
  todayCount: number;
  todayHours: number;
  onOpenToday: () => void;
  onSelectDate: (date: string) => void;
  refreshTrigger: number;
}) {
  const [now, setNow] = React.useState(() => new Date());
  const [viewYear, setViewYear] = React.useState(() => {
    const parts = props.todayStr.split("-");
    return Number(parts[0]) || new Date().getFullYear();
  });
  const [viewMonth, setViewMonth] = React.useState(() => {
    const parts = props.todayStr.split("-");
    return Number(parts[1]) || (new Date().getMonth() + 1);
  });
  const [activeDates, setActiveDates] = React.useState<Set<string>>(new Set());

  // Real-time ticking every second
  React.useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(timer);
  }, []);

  // Fetch dates that have logs for viewYear-viewMonth
  React.useEffect(() => {
    let cancelled = false;
    const monthStr = `${viewYear}-${String(viewMonth).padStart(2, "0")}`;
    api
      .getMonthWorkLogDates(monthStr)
      .then((dates) => {
        if (!cancelled) setActiveDates(new Set(dates));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [viewYear, viewMonth, props.refreshTrigger]);

  const timeStr = React.useMemo(() => {
    return now.toTimeString().slice(0, 8); // "HH:mm:ss"
  }, [now]);

  const [hoursMin, sec] = React.useMemo(() => {
    const parts = timeStr.split(":");
    return [`${parts[0]}:${parts[1]}`, parts[2] || "00"];
  }, [timeStr]);

  const calendarDays = React.useMemo(() => {
    return getCalendarGrid(viewYear, viewMonth, props.todayStr);
  }, [viewYear, viewMonth, props.todayStr]);

  const todayYear = Number(props.todayStr.split("-")[0]) || new Date().getFullYear();
  const todayMonth = Number(props.todayStr.split("-")[1]) || (new Date().getMonth() + 1);
  const isViewingOtherMonth = viewYear !== todayYear || viewMonth !== todayMonth;

  function prevMonth() {
    if (viewMonth === 1) {
      setViewYear((y) => y - 1);
      setViewMonth(12);
    } else {
      setViewMonth((m) => m - 1);
    }
  }

  function nextMonth() {
    if (viewMonth === 12) {
      setViewYear((y) => y + 1);
      setViewMonth(1);
    } else {
      setViewMonth((m) => m + 1);
    }
  }

  function goTodayMonth() {
    setViewYear(todayYear);
    setViewMonth(todayMonth);
  }

  return (
    <div className="sidebar-minical-card">
      <div className="minical-header">
        <div
          className="minical-clock-wrap"
          onClick={props.onOpenToday}
          title={`今日已记录 ${props.todayCount} 项 (${props.todayHours.toFixed(1)}h)，点击查看`}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") props.onOpenToday();
          }}
        >
          <span className="clock-live-dot" />
          <span className="minical-clock-hm">{hoursMin}</span>
          <span className="clock-colon">:</span>
          <span className="minical-clock-sec">{sec}</span>
          <span className="minical-today-pill-tag">今日{props.todayCount}项</span>
        </div>

        <div className="minical-title-wrap">
          <button
            type="button"
            className="minical-nav-btn"
            onClick={prevMonth}
            title="上个月"
            aria-label="上个月"
          >
            <ChevronLeft size={12} />
          </button>
          <span className="minical-title">
            {viewYear}年{viewMonth}月
          </span>
          {isViewingOtherMonth && (
            <button
              type="button"
              className="minical-today-pill"
              onClick={goTodayMonth}
              title="返回本月"
            >
              今
            </button>
          )}
          <button
            type="button"
            className="minical-nav-btn"
            onClick={nextMonth}
            title="下个月"
            aria-label="下个月"
          >
            <ChevronRight size={12} />
          </button>
        </div>
      </div>

      <div className="minical-weekdays">
        <span>一</span>
        <span>二</span>
        <span>三</span>
        <span>四</span>
        <span>五</span>
        <span>六</span>
        <span>日</span>
      </div>

      <div className="minical-grid">
        {calendarDays.map((d) => {
          const hasRec = activeDates.has(d.dateStr);
          return (
            <button
              key={d.dateStr}
              type="button"
              className={`minical-cell ${d.isCurrentMonth ? "in-month" : "out-month"} ${
                d.isToday ? "is-today" : ""
              } ${hasRec ? "has-records" : ""}`}
              onClick={() => props.onSelectDate(d.dateStr)}
              title={`${d.dateStr}${hasRec ? " · 有记录" : ""}，点击查看详情`}
            >
              <span className="cell-day-num">{d.dayNumber}</span>
              {hasRec && <span className="cell-active-dot" />}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function SidebarWorkReportWidget(props: {
  todayStr: string;
  refreshTrigger: number;
  onSelectDate: (date: string) => void;
}) {
  const [activeTab, setActiveTab] = React.useState<"trend" | "projects">("trend");
  const [report, setReport] = React.useState<WorkReportSummary | null>(null);
  const [hoveredIdx, setHoveredIdx] = React.useState<number | null>(null);

  const loadReport = React.useCallback(() => {
    let cancelled = false;
    api
      .getWorkReportSummary(props.todayStr)
      .then((data) => {
        if (!cancelled) setReport(data);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [props.todayStr]);

  React.useEffect(() => {
    loadReport();
    const timer = setInterval(() => {
      loadReport();
    }, 60000);

    const onFocus = () => {
      loadReport();
    };
    window.addEventListener("focus", onFocus);

    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [loadReport, props.refreshTrigger]);

  const stats = report?.daily_stats ?? [];
  const maxHours = Math.max(...stats.map((s) => s.total_hours), 4);

  const maxDayStat = React.useMemo(() => {
    return stats.reduce<DailyWorkStat | null>((best, curr) => {
      if (!best || curr.total_hours > best.total_hours) {
        return curr.total_hours > 0 ? curr : best;
      }
      return best;
    }, null);
  }, [stats]);

  const goalPercent = Math.min(Math.round(((report?.week_total_hours ?? 0) / 40) * 100), 100);

  const svgWidth = 216;
  const svgHeight = 52;
  const padLeft = 14;
  const padRight = 14;
  const padTop = 6;
  const padBottom = 8;
  const usableWidth = svgWidth - padLeft - padRight;
  const usableHeight = svgHeight - padTop - padBottom;
  const baselineY = svgHeight - padBottom;

  const points = stats.map((s, i) => {
    const x = padLeft + (stats.length > 1 ? i * (usableWidth / (stats.length - 1)) : usableWidth / 2);
    const y = padTop + (1 - s.total_hours / maxHours) * usableHeight;
    return { x, y, stat: s };
  });

  const { pathD, areaD } = React.useMemo(() => {
    if (points.length === 0) return { pathD: "", areaD: "" };
    if (points.length === 1) {
      return {
        pathD: `M ${points[0].x} ${points[0].y}`,
        areaD: `M ${points[0].x} ${points[0].y} L ${points[0].x} ${baselineY} Z`
      };
    }
    let pD = `M ${points[0].x.toFixed(1)} ${points[0].y.toFixed(1)}`;
    for (let i = 0; i < points.length - 1; i++) {
      const p0 = points[i === 0 ? 0 : i - 1];
      const p1 = points[i];
      const p2 = points[i + 1];
      const p3 = points[i + 2 >= points.length ? i + 1 : i + 2];

      const cp1x = p1.x + (p2.x - p0.x) / 6;
      const cp1y = p1.y + (p2.y - p0.y) / 6;
      const cp2x = p2.x - (p3.x - p1.x) / 6;
      const cp2y = p2.y - (p3.y - p1.y) / 6;

      pD += ` C ${cp1x.toFixed(1)} ${cp1y.toFixed(1)}, ${cp2x.toFixed(1)} ${cp2y.toFixed(1)}, ${p2.x.toFixed(1)} ${p2.y.toFixed(1)}`;
    }
    const lastX = points[points.length - 1].x.toFixed(1);
    const firstX = points[0].x.toFixed(1);
    const aD = `${pD} L ${lastX} ${baselineY} L ${firstX} ${baselineY} Z`;
    return { pathD: pD, areaD: aD };
  }, [points, baselineY]);

  return (
    <div className="sidebar-work-report-card">
      <div className="report-card-header">
        <div className="report-header-left">
          <div className="report-icon-dot">
            <Activity size={11} />
          </div>
          <span className="report-card-title">工作情况分析</span>
          <span
            className="report-period-badge"
            onClick={loadReport}
            title="点击刷新工作分析数据"
            style={{ cursor: "pointer" }}
          >
            本周
          </span>
        </div>
        <div className="report-tab-pills">
          <button
            type="button"
            className={`report-tab-pill ${activeTab === "trend" ? "active" : ""}`}
            onClick={() => setActiveTab("trend")}
            title="工时趋势曲线"
          >
            <TrendingUp size={10} /> 趋势
          </button>
          <button
            type="button"
            className={`report-tab-pill ${activeTab === "projects" ? "active" : ""}`}
            onClick={() => setActiveTab("projects")}
            title="项目分布"
          >
            <PieChart size={10} /> 项目
          </button>
        </div>
      </div>

      {/* Metric Quick Stats */}
      <div className="report-metrics-bar">
        <div className="report-metric-col">
          <span className="report-metric-num">{report?.week_total_hours.toFixed(1) ?? "0.0"}h</span>
          <span className="report-metric-lbl">本周工时</span>
        </div>
        <div className="report-metric-sep" />
        <div className="report-metric-col">
          <span className="report-metric-num">{report?.week_avg_hours.toFixed(1) ?? "0.0"}h</span>
          <span className="report-metric-lbl">日均工时</span>
        </div>
        <div className="report-metric-sep" />
        <div className="report-metric-col">
          <span className="report-metric-num">{report?.total_tasks ?? 0} 项</span>
          <span className="report-metric-lbl">总任务数</span>
        </div>
        <div className="report-metric-sep" />
        <div
          className="report-metric-col"
          title={
            report?.agent_stats && report.agent_stats.length > 0
              ? report.agent_stats
                  .map((s) => `${s.agent}: ${s.formatted_duration || s.duration_hours + "h"} (${s.session_count}次)`)
                  .join(" · ")
              : "暂无 Agent 对话记录"
          }
        >
          <span className="report-metric-num" style={{ color: "#7c3aed" }}>
            {report?.agent_total_hours !== undefined && report.agent_total_hours > 0
              ? `${report.agent_total_hours.toFixed(1)}h`
              : "0h"}
          </span>
          <span className="report-metric-lbl">AI对话</span>
        </div>
      </div>

      <div className="report-tab-body">
        {activeTab === "trend" ? (
          <div className="report-trend-content">
            <div className="report-chart-container">
              <div className="report-chart-svg-wrap">
                <svg
                  className="report-curve-svg"
                  viewBox={`0 0 ${svgWidth} ${svgHeight}`}
                  preserveAspectRatio="none"
                >
                  <defs>
                    <linearGradient id="curveLineGradient" x1="0%" y1="0%" x2="100%" y2="0%">
                      <stop offset="0%" stopColor="#2563eb" />
                      <stop offset="50%" stopColor="#4f46e5" />
                      <stop offset="100%" stopColor="#7c3aed" />
                    </linearGradient>
                    <linearGradient id="curveAreaGradient" x1="0%" y1="0%" x2="0%" y2="100%">
                      <stop offset="0%" stopColor="#4f46e5" stopOpacity="0.28" />
                      <stop offset="65%" stopColor="#3b82f6" stopOpacity="0.06" />
                      <stop offset="100%" stopColor="#3b82f6" stopOpacity="0.0" />
                    </linearGradient>
                  </defs>

                  {/* Baseline dashed line */}
                  <line
                    x1={padLeft}
                    y1={baselineY}
                    x2={svgWidth - padRight}
                    y2={baselineY}
                    stroke="#e2e8f0"
                    strokeWidth="1"
                    strokeDasharray="2 2"
                  />

                  {/* Area Fill */}
                  {areaD && <path d={areaD} fill="url(#curveAreaGradient)" />}

                  {/* Line Stroke */}
                  {pathD && (
                    <path
                      d={pathD}
                      fill="none"
                      stroke="url(#curveLineGradient)"
                      strokeWidth="2.2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  )}

                  {/* Data points */}
                  {points.map((p, idx) => {
                    const isHovered = hoveredIdx === idx;
                    const isToday = p.stat.is_today;
                    const hasHours = p.stat.total_hours > 0;

                    return (
                      <g
                        key={p.stat.date}
                        className="curve-point-group"
                        onClick={() => props.onSelectDate(p.stat.date)}
                        onMouseEnter={() => setHoveredIdx(idx)}
                        onMouseLeave={() => setHoveredIdx(null)}
                        style={{ cursor: "pointer" }}
                      >
                        <title>{`${p.stat.day_name} (${p.stat.date}): ${p.stat.total_hours}h · ${p.stat.task_count}项`}</title>
                        <circle cx={p.x} cy={p.y} r="10" fill="transparent" />
                        {isToday && (
                          <circle
                            cx={p.x}
                            cy={p.y}
                            r="5"
                            fill="none"
                            stroke="#3b82f6"
                            strokeWidth="1.2"
                            className="curve-point-pulse"
                          />
                        )}
                        <circle
                          cx={p.x}
                          cy={p.y}
                          r={isHovered ? 4 : isToday ? 3.2 : hasHours ? 2.8 : 2}
                          fill={isHovered ? "#2563eb" : isToday ? "#3b82f6" : hasHours ? "#4f46e5" : "#cbd5e1"}
                          stroke="#ffffff"
                          strokeWidth={hasHours || isToday ? 1.2 : 0.8}
                        />
                      </g>
                    );
                  })}
                </svg>
              </div>

              <div className="report-chart-weekdays">
                {stats.map((s, idx) => (
                  <span
                    key={s.date}
                    className={`chart-weekday-label ${s.is_today ? "is-today" : ""} ${
                      hoveredIdx === idx ? "is-hovered" : ""
                    }`}
                    onClick={() => props.onSelectDate(s.date)}
                    title={`${s.day_name} (${s.date}): ${s.total_hours}h · 点击查看`}
                  >
                    {s.day_name.replace("周", "")}
                  </span>
                ))}
              </div>
            </div>

            {/* Weekly Target Goal */}
            <div className="report-goal-box">
              <div className="report-goal-header">
                <div className="report-goal-title">
                  <Target size={11} className="goal-icon" />
                  <span>周目标 (40h)</span>
                </div>
                <span className="report-goal-badge">{goalPercent}%</span>
              </div>
              <div className="report-goal-track">
                <div
                  className="report-goal-fill"
                  style={{ width: `${goalPercent}%` }}
                />
              </div>
              <div className="report-goal-footer">
                <span>已达成 {report?.week_total_hours.toFixed(1) ?? "0.0"}h</span>
                <span>
                  {report && report.week_total_hours >= 40
                    ? "已达标 🎉"
                    : `还差 ${(Math.max(40 - (report?.week_total_hours ?? 0), 0)).toFixed(1)}h`}
                </span>
              </div>
            </div>

            {/* Highlights Grid */}
            <div className="report-highlights-grid">
              <div className="highlight-card">
                <div className="highlight-card-top">
                  <Award size={11} color="#d97706" />
                  <span className="highlight-card-label">最佳产出日</span>
                </div>
                <span className="highlight-card-val" title={maxDayStat ? `${maxDayStat.day_name} (${maxDayStat.total_hours}h)` : "暂无"}>
                  {maxDayStat ? `${maxDayStat.day_name} · ${maxDayStat.total_hours}h` : "暂无"}
                </span>
              </div>

              <div className="highlight-card">
                <div className="highlight-card-top">
                  <Flame size={11} color="#ea580c" />
                  <span className="highlight-card-label">本周活跃</span>
                </div>
                <span className="highlight-card-val">
                  {report?.active_days ?? 0} / 7 天
                </span>
              </div>
            </div>

            {/* Subtle Motivational Status */}
            <div className="report-status-tip">
              <Sparkles size={11} className="tip-sparkle" />
              <span>
                {report && report.week_total_hours >= 35
                  ? "本周工作饱满，产出高效！"
                  : report && report.week_total_hours >= 20
                  ? "保持良好节奏，稳步推进目标。"
                  : "记录每日工作，清晰沉淀成果。"}
              </span>
            </div>
          </div>
        ) : (
          <div className="report-projects-content">
            <div className="report-projects-container">
              {(!report?.top_projects || report.top_projects.length === 0) ? (
                <div className="report-projects-empty">本周暂无项目数据</div>
              ) : (
                <div className="report-projects-list">
                  {report.top_projects.map((p, idx) => {
                    const colorGradients = [
                      "linear-gradient(90deg, #2563eb, #4f46e5)",
                      "linear-gradient(90deg, #059669, #0d9488)",
                      "linear-gradient(90deg, #d97706, #ea580c)",
                      "linear-gradient(90deg, #8b5cf6, #6366f1)",
                      "linear-gradient(90deg, #ec4899, #f43f5e)"
                    ];
                    const grad = colorGradients[idx % colorGradients.length];

                    return (
                      <div key={p.project} className="project-stat-row">
                        <div className="project-stat-top">
                          <span className="project-name" title={p.project}>
                            {p.project}
                          </span>
                          <span className="project-hours">
                            {p.hours}h · {p.percentage.toFixed(0)}%
                          </span>
                        </div>
                        <div className="project-progress-track">
                          <div
                            className="project-progress-fill"
                            style={{
                              width: `${Math.min(p.percentage, 100)}%`,
                              background: grad
                            }}
                          />
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            {report?.top_projects && report.top_projects.length > 0 && (
              <div className="report-projects-summary">
                <span>共投入 {report.top_projects.length} 个项目</span>
                <span title={report.top_projects[0].project}>
                  主要: {report.top_projects[0].project}
                </span>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Dev Activity & Commit Breakdown Box */}
      <div className="report-dev-activity-box">
        <div className="dev-activity-header">
          <div className="dev-activity-title">
            <Code2 size={11} className="dev-activity-icon" />
            <span>研发活动与 AI 对话</span>
          </div>
          <div className="dev-activity-pills">
            <span
              className="dev-pill agent-pill"
              title={`本周与 AI Agent 会话共 ${report?.agent_sessions ?? 0} 次，累计 ${report?.agent_total_hours?.toFixed(1) ?? "0"}h`}
            >
              <Bot size={10} />
              <span>AI <strong>{report?.agent_total_hours?.toFixed(1) ?? "0"}h</strong></span>
            </span>
            <span
              className="dev-pill git-pill"
              title={`本周代码提交共 ${report?.git_commits?.total ?? 0} 次`}
            >
              <GitCommit size={10} />
              <span>Git <strong>{report?.git_commits?.total ?? 0}</strong> 次</span>
            </span>
          </div>
        </div>

        {/* Agent 对话时长分项统计（每个 Agent 分开） */}
        {report?.agent_stats && report.agent_stats.length > 0 && (
          <div className="agent-breakdown-section">
            <div className="agent-breakdown-subhead">
              <span className="agent-subhead-title">Agent 对话时长明细</span>
              <span className="agent-subhead-total">共 {report?.agent_total_hours?.toFixed(1) ?? "0"}h · {report?.agent_sessions ?? 0} 次</span>
            </div>
            <div className="agent-chips-row">
              {report.agent_stats.map((s) => (
                <div
                  key={s.agent}
                  className="agent-stat-chip"
                  title={`${s.agent}：本周累计对话 ${s.formatted_duration || s.duration_hours + 'h'}，共 ${s.session_count} 次会话`}
                >
                  <div className="agent-chip-left">
                    <span className={`agent-dot ${s.agent.toLowerCase()}-dot`} />
                    <span className="agent-chip-name">{s.agent}</span>
                  </div>
                  <div className="agent-chip-right">
                    <span className="agent-chip-hours">{s.formatted_duration || `${s.duration_hours}h`}</span>
                    <span className="agent-chip-sessions">({s.session_count}次)</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {(report?.git_commits?.total ?? 0) > 0 ? (
          <>
            <div
              className="commit-segment-bar"
              title={`Bug修复: ${report?.git_commits?.fixes ?? 0} · 功能新增: ${report?.git_commits?.features ?? 0} · 优化升级: ${report?.git_commits?.optimizations ?? 0} · 其他: ${report?.git_commits?.others ?? 0}`}
            >
              {(report?.git_commits?.fixes ?? 0) > 0 && (
                <div
                  className="commit-segment fix-segment"
                  style={{
                    width: `${((report!.git_commits!.fixes / report!.git_commits!.total) * 100).toFixed(1)}%`
                  }}
                  title={`Bug修复: ${report!.git_commits!.fixes} (${Math.round((report!.git_commits!.fixes / report!.git_commits!.total) * 100)}%)`}
                />
              )}
              {(report?.git_commits?.features ?? 0) > 0 && (
                <div
                  className="commit-segment feat-segment"
                  style={{
                    width: `${((report!.git_commits!.features / report!.git_commits!.total) * 100).toFixed(1)}%`
                  }}
                  title={`功能新增: ${report!.git_commits!.features} (${Math.round((report!.git_commits!.features / report!.git_commits!.total) * 100)}%)`}
                />
              )}
              {(report?.git_commits?.optimizations ?? 0) > 0 && (
                <div
                  className="commit-segment opt-segment"
                  style={{
                    width: `${((report!.git_commits!.optimizations / report!.git_commits!.total) * 100).toFixed(1)}%`
                  }}
                  title={`优化升级: ${report!.git_commits!.optimizations} (${Math.round((report!.git_commits!.optimizations / report!.git_commits!.total) * 100)}%)`}
                />
              )}
              {(report?.git_commits?.others ?? 0) > 0 && (
                <div
                  className="commit-segment other-segment"
                  style={{
                    width: `${((report!.git_commits!.others / report!.git_commits!.total) * 100).toFixed(1)}%`
                  }}
                  title={`其他提交: ${report!.git_commits!.others} (${Math.round((report!.git_commits!.others / report!.git_commits!.total) * 100)}%)`}
                />
              )}
            </div>

            <div className="commit-types-grid">
              <div className="commit-type-chip fix-chip" title="包括 fix, bug, hotfix, 修复等">
                <span className="commit-chip-dot fix-dot" />
                <span className="commit-chip-name">Bug修复</span>
                <span className="commit-chip-val">
                  {report?.git_commits?.fixes ?? 0}
                  <small>({Math.round(((report?.git_commits?.fixes ?? 0) / (report?.git_commits?.total || 1)) * 100)}%)</small>
                </span>
              </div>
              <div className="commit-type-chip feat-chip" title="包括 feat, feature, add, 新增等">
                <span className="commit-chip-dot feat-dot" />
                <span className="commit-chip-name">功能新增</span>
                <span className="commit-chip-val">
                  {report?.git_commits?.features ?? 0}
                  <small>({Math.round(((report?.git_commits?.features ?? 0) / (report?.git_commits?.total || 1)) * 100)}%)</small>
                </span>
              </div>
              <div className="commit-type-chip opt-chip" title="包括 refactor, perf, opt, 优化, 升级等">
                <span className="commit-chip-dot opt-dot" />
                <span className="commit-chip-name">优化升级</span>
                <span className="commit-chip-val">
                  {report?.git_commits?.optimizations ?? 0}
                  <small>({Math.round(((report?.git_commits?.optimizations ?? 0) / (report?.git_commits?.total || 1)) * 100)}%)</small>
                </span>
              </div>
              <div className="commit-type-chip other-chip" title="包括 docs, chore, style, build 等">
                <span className="commit-chip-dot other-dot" />
                <span className="commit-chip-name">其他提交</span>
                <span className="commit-chip-val">
                  {report?.git_commits?.others ?? 0}
                  <small>({Math.round(((report?.git_commits?.others ?? 0) / (report?.git_commits?.total || 1)) * 100)}%)</small>
                </span>
              </div>
            </div>
          </>
        ) : (
          <div className="commit-empty-hint">本周暂无代码提交记录</div>
        )}
      </div>
    </div>
  );
}

function DayDetailModal(props: {
  date: string | null;
  onClose: () => void;
  onEditLog: (log: WorkLog) => void;
  onCreateForDate: (date: string) => void;
}) {
  const [logs, setLogs] = React.useState<WorkLog[]>([]);
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState("");

  const targetDate = props.date;

  React.useEffect(() => {
    if (!targetDate) return;
    setLoading(true);
    setError("");
    api
      .listWorkLogsByDate(targetDate)
      .then((items) => setLogs(items))
      .catch((err) => setError(err instanceof Error ? err.message : "加载该日工作记录失败"))
      .finally(() => setLoading(false));
  }, [targetDate]);

  React.useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        props.onClose();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [props.onClose]);

  if (!targetDate) return null;

  const totalHours = logs.reduce(
    (acc, curr) => acc + (curr.hours ?? (curr.origin === "auto" || (curr.git_commit_count ?? 0) > 0 ? 8 : 0)),
    0
  );

  const dateObj = new Date(targetDate + "T00:00:00");
  const weekdays = ["星期日", "星期一", "星期二", "星期三", "星期四", "星期五", "星期六"];
  const weekdayLabel = isNaN(dateObj.getDay()) ? "" : weekdays[dateObj.getDay()];
  const isWeekend = dateObj.getDay() === 0 || dateObj.getDay() === 6;

  return (
    <div
      className="email-dialog-backdrop day-detail-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) props.onClose();
      }}
    >
      <div
        className="email-dialog day-detail-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="day-detail-title"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="dialog-heading day-detail-header">
          <div className="day-detail-title-wrap">
            <div className="day-detail-icon-circle">
              <CalendarDays size={20} />
            </div>
            <div>
              <div className="day-detail-date-row">
                <h2 id="day-detail-title">{targetDate}</h2>
                <span className={`day-weekday-tag ${isWeekend ? "weekend" : "workday"}`}>
                  {weekdayLabel}
                </span>
              </div>
              <p className="day-detail-subtitle">
                当天共 <strong>{logs.length}</strong> 条记录 · 合计工时{" "}
                <strong>{totalHours.toFixed(1)}</strong> 小时
              </p>
            </div>
          </div>
          <div className="day-detail-header-actions">
            <button
              type="button"
              className="primary compact new-log-btn"
              onClick={() => props.onCreateForDate(targetDate)}
            >
              <Plus size={14} /> 新增该日记录
            </button>
            <button
              className="icon-button"
              type="button"
              aria-label="关闭详情窗口"
              onClick={props.onClose}
            >
              <X size={18} />
            </button>
          </div>
        </div>

        <div className="day-detail-body">
          {loading ? (
            <div className="day-detail-loading">
              <LoaderCircle className="spin" size={28} />
              <span>正在获取该日工作记录...</span>
            </div>
          ) : error ? (
            <div className="error" role="alert">
              <CircleAlert size={16} />
              <span>{error}</span>
            </div>
          ) : logs.length === 0 ? (
            <div className="day-detail-empty">
              <div className="empty-icon-circle">
                <CalendarDays size={32} />
              </div>
              <h3>当天暂无工作记录</h3>
              <p>记录当天的具体工作、推进进展与成果产出，轻松沉淀工作足迹</p>
              <button
                type="button"
                className="primary"
                onClick={() => props.onCreateForDate(targetDate)}
              >
                <Plus size={15} /> 立即创建该日记录
              </button>
            </div>
          ) : (
            <div className="day-detail-list">
              {logs.map((item) => {
                const priorityLabels: Record<string, string> = {
                  high: "高优",
                  medium: "中优",
                  low: "低优"
                };
                return (
                  <div key={item.id} className="day-log-card">
                    <div className="day-log-card-header">
                      <div className="day-log-meta-left">
                        <span className={`day-priority-pill ${item.priority}`}>
                          {priorityLabels[item.priority] || item.priority}
                        </span>
                        <span className="day-project-badge">{item.project}</span>
                        {item.origin === "auto" && (
                          <span className="day-origin-badge" title="系统根据开发活动自动捕获">
                            自动捕获
                          </span>
                        )}
                      </div>
                      <div className="day-log-meta-right">
                        {item.hours != null && (
                          <span className="day-hours-badge">
                            <Clock size={12} /> {item.hours}h
                          </span>
                        )}
                        <button
                          type="button"
                          className="secondary compact day-edit-btn"
                          onClick={() => props.onEditLog(item)}
                        >
                          <Pencil size={13} /> 前往编辑
                        </button>
                      </div>
                    </div>

                    <div className="day-log-task-title">{item.task}</div>

                    {item.progress && (
                      <div className="day-log-field-row">
                        <span className="field-lbl">进展：</span>
                        <span className="field-val">{item.progress}</span>
                      </div>
                    )}

                    {item.result && (
                      <div className="day-log-field-row">
                        <span className="field-lbl">成果：</span>
                        <span className="field-val">{item.result}</span>
                      </div>
                    )}

                    {item.blockers && (
                      <div className="day-log-field-row warning">
                        <span className="field-lbl">阻塞：</span>
                        <span className="field-val">{item.blockers}</span>
                      </div>
                    )}

                    {item.notes && (
                      <div className="day-log-field-row notes">
                        <span className="field-lbl">备注：</span>
                        <span className="field-val">{item.notes}</span>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <div className="day-detail-footer">
          <span className="day-footer-hint">
            提示：点击「前往编辑」可直接跳转到每日记录面板并载入该条数据进行修改
          </span>
          <button type="button" className="secondary" onClick={props.onClose}>
            关闭
          </button>
        </div>
      </div>
    </div>
  );
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
  const [reportSchedules, setReportSchedules] = React.useState<ReportSchedule[]>([]);
  const [desktopPreferences, setDesktopPreferences] = React.useState<DesktopPreferences | null>(null);
  const [migrationResult, setMigrationResult] = React.useState<MigrationResult | null>(null);
  const [activitySources, setActivitySources] = React.useState<ActivitySource[]>([]);
  const [activityCandidates, setActivityCandidates] = React.useState<ActivitySourceCandidate[]>([]);
  const [dailyCaptureSettings, setDailyCaptureSettings] = React.useState<DailyCaptureSettings | null>(null);
  const [dailyCaptureRuns, setDailyCaptureRuns] = React.useState<DailyCaptureRun[]>([]);
  const [inspectedDate, setInspectedDate] = React.useState<string | null>(null);
  const [targetEditLog, setTargetEditLog] = React.useState<WorkLog | null>(null);
  const [targetCreateDate, setTargetCreateDate] = React.useState<string | null>(null);
  const [notice, setNotice] = React.useState("");
  const [error, setError] = React.useState("");
  const [loading, setLoading] = React.useState(true);
  const [refreshSeq, setRefreshSeq] = React.useState(0);

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
        recipientList,
        scheduleList,
        desktop,
        migration,
        sources,
        candidates,
        captureSettings,
        captureRuns
      ] = await Promise.all([
        api.listWorkLogs(pageOverride, pageSizeOverride),
        api.listReports(),
        api.listTemplates(),
        api.getLlmSetting(),
        api.listLlmSettings(),
        api.getEmailSetting(),
        api.listRecipients(),
        api.listReportSchedules(),
        api.getDesktopPreferences(),
        api.getStartupMigration(),
        api.listActivitySources(),
        api.discoverActivitySources(),
        api.getDailyCaptureSettings(),
        api.listDailyCaptureRuns()
      ]);
      setWorkLogs(logsPage.items);
      setWorkLogMeta(logsPage);
      setReports(reportList);
      setTemplates(templateList);
      setLlmSetting(setting);
      setLlmSettings(savedSettings);
      setEmailSetting(savedEmailSetting);
      setRecipients(recipientList);
      setReportSchedules(scheduleList);
      setDesktopPreferences(desktop);
      setMigrationResult(migration);
      setActivitySources(sources);
      setActivityCandidates(candidates);
      setDailyCaptureSettings(captureSettings);
      setDailyCaptureRuns(captureRuns);
    } catch (err) {
      setError(err instanceof Error ? err.message : "加载失败");
    } finally {
      setLoading(false);
      setRefreshSeq((s) => s + 1);
    }
  }, [workLogPage, workLogPageSize]);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  React.useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => {
      setNotice("");
    }, 3500);
    return () => clearTimeout(timer);
  }, [notice]);

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

  const [todayStr, setTodayStr] = React.useState(today());

  React.useEffect(() => {
    const timer = setInterval(() => {
      const nowToday = today();
      setTodayStr((prev) => (prev !== nowToday ? nowToday : prev));
    }, 60000);

    const onFocus = () => {
      const nowToday = today();
      setTodayStr((prev) => (prev !== nowToday ? nowToday : prev));
      refresh().catch(() => {});
    };
    window.addEventListener("focus", onFocus);

    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, []);
  const todayItems = React.useMemo(() => {
    return workLogs.filter((item) => item.start_date === todayStr || item.work_date === todayStr);
  }, [workLogs, todayStr]);
  const todayHours = React.useMemo(() => {
    return todayItems.reduce(
      (acc, curr) => acc + (curr.hours ?? (curr.origin === "auto" || (curr.git_commit_count ?? 0) > 0 ? 8 : 0)),
      0
    );
  }, [todayItems]);

  return (
    <main className={loading ? "app-shell is-loading" : "app-shell"}>
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-icon-wrapper">
            <NotebookTabs size={20} />
          </div>
          <div className="brand-text">
            <div className="brand-title-row">
              <strong>Worklog</strong>
              <span className="brand-version-badge">v2.0.8</span>
            </div>
            <span>智能工作记录与报告</span>
          </div>
        </div>

        <nav className="sidebar-nav">
          <TabButton
            active={tab === "logs"}
            icon={<CalendarDays size={17} />}
            onClick={() => setTab("logs")}
            badge={workLogMeta.total > 0 ? `${workLogMeta.total}` : undefined}
          >
            每日记录
          </TabButton>
          <TabButton
            active={tab === "reports"}
            icon={<FileText size={17} />}
            onClick={() => setTab("reports")}
            badge={reports.length > 0 ? `${reports.length}` : undefined}
          >
            报告草稿
          </TabButton>
          <TabButton
            active={tab === "templates"}
            icon={<NotebookTabs size={17} />}
            onClick={() => setTab("templates")}
            badge={templates.length > 0 ? `${templates.length}` : undefined}
          >
            模板管理
          </TabButton>
          <TabButton
            active={tab === "settings"}
            icon={<Settings size={17} />}
            onClick={() => setTab("settings")}
          >
            系统设置
          </TabButton>
        </nav>

        <SidebarClockAndCalendar
          todayStr={todayStr}
          todayCount={todayItems.length}
          todayHours={todayHours}
          onOpenToday={() => setInspectedDate(todayStr)}
          onSelectDate={(d) => setInspectedDate(d)}
          refreshTrigger={refreshSeq}
        />

        <SidebarWorkReportWidget
          todayStr={todayStr}
          refreshTrigger={refreshSeq}
          onSelectDate={(d) => setInspectedDate(d)}
        />

        <div className="sidebar-footer">
          <button
            type="button"
            className="sidebar-model-pill"
            title="点击前往配置 AI 模型"
            onClick={() => setTab("settings")}
          >
            <span className={`status-indicator-dot ${llmSetting ? "online" : "offline"}`} />
            <span className="model-pill-text">
              {llmSetting ? `${providerLabels[llmSetting.provider] || llmSetting.provider}` : "点击配置 AI 模型"}
            </span>
            <Settings size={13} className="model-pill-icon" />
          </button>
        </div>
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

        <div className="page-content-wrapper">

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
            onOpenSettings={() => setTab("settings")}
            targetEditLog={targetEditLog}
            onClearTargetEditLog={() => setTargetEditLog(null)}
            targetCreateDate={targetCreateDate}
            onClearTargetCreateDate={() => setTargetCreateDate(null)}
          />
        )}
        {tab === "reports" && (
          <ReportsPage
            reports={reports}
            templates={templates}
            recipients={recipients}
            onGenerate={(payload) => run(() => api.generateReport(payload).then(() => undefined), "报告草稿已生成")}
            onOptimize={async (id, payload) => {
              setError("");
              setNotice("");
              try {
                const result = await api.optimizeReport(id, payload);
                return result.content;
              } catch (err) {
                setError(err instanceof Error ? err.message : "优化报告草稿失败");
                throw err;
              }
            }}
            onSave={(id, payload) => run(() => api.updateReport(id, payload).then(() => undefined), "报告草稿已保存到本地数据库")}
            onDelete={(id) => run(() => api.deleteReport(id), "报告草稿已删除")}
            onExport={async (id) => {
              setError("");
              const exported = await api.exportReportDocx(id);
              if (exported) {
                setNotice(`DOCX 已保存到 ${exported}`);
              }
            }}
            onSendEmail={(id, payload) => run(() => api.sendReportEmail(id, payload).then(() => undefined), "报告邮件已发送")}
            onListEmailDeliveries={(id) => api.listReportEmailDeliveries(id)}
            onNotice={(message) => setNotice(message)}
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
            onOptimize={async (payload) => {
              setError("");
              setNotice("");
              try {
                const result = await api.optimizeTemplate(payload);
                return result.content;
              } catch (err) {
                setError(err instanceof Error ? err.message : "优化模板失败");
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
            templates={templates}
            emailSetting={emailSetting}
            recipients={recipients}
            reportSchedules={reportSchedules}
            desktopPreferences={desktopPreferences}
            migrationResult={migrationResult}
            activitySources={activitySources}
            activityCandidates={activityCandidates}
            dailyCaptureSettings={dailyCaptureSettings}
            dailyCaptureRuns={dailyCaptureRuns}
            onSave={(payload) => run(
              () => (payload.id
                ? api.updateLlmSetting(payload.id, payload)
                : api.createLlmSetting(payload)
              ).then(() => undefined),
              payload.id ? "LLM 设置已更新并应用" : "LLM 新设置已保存并应用"
            )}
            onApplyLlmSetting={(id) => run(() => api.applyLlmSetting(id).then(() => undefined), "LLM 配置已应用")}
            onDeleteLlmSetting={(id) => run(() => api.deleteLlmSetting(id), "LLM 配置已删除")}
            onSaveReportSchedule={(reportType, payload) => run(
              () => api.updateReportSchedule(reportType, payload).then(() => undefined),
              `${reportLabels[reportType]}定时设置已保存，当前${payload.enabled ? "已启用" : "已停用"}`
            )}
            onSaveEmail={(payload) => run(() => api.updateEmailSetting(payload).then(() => undefined), "SMTP 邮箱设置已保存")}
            onTestEmail={(address) => run(() => api.testEmailSetting(address).then(() => undefined), "测试邮件已发送")}
            onCreateRecipient={(payload) => run(() => api.createRecipient(payload).then(() => undefined), "收件人已添加")}
            onUpdateRecipient={(id, payload) => run(() => api.updateRecipient(id, payload).then(() => undefined), "收件人已更新")}
            onDeleteRecipient={(id) => run(() => api.deleteRecipient(id), "收件人已删除")}
            onCreateActivitySource={(payload) => run(
              () => api.createActivitySource(payload).then(() => undefined),
              "采集目录已添加"
            )}
            onUpdateActivitySource={(id, payload) => run(
              () => api.updateActivitySource(id, payload).then(() => undefined),
              "采集目录已更新"
            )}
            onDeleteActivitySource={(id) => run(
              () => api.deleteActivitySource(id),
              "采集目录已删除"
            )}
            onSaveDailyCaptureSettings={(payload) => run(
              () => api.updateDailyCaptureSettings(payload).then(() => undefined),
              payload.enabled ? "每日自动记录已启用" : "每日自动记录设置已保存"
            )}
            onRunDailyCapture={async (date, forceOverwrite) => {
              setError("");
              setNotice("");
              try {
                const result = await api.runDailyCapture(date, forceOverwrite);
                setNotice(result.status === "skipped" ? "当天没有发现完成事项" : "每日工作记录扫描完成");
                await refresh();
                return result;
              } catch (err) {
                setError(err instanceof Error ? err.message : "自动记录扫描失败");
                throw err;
              }
            }}
            onSetLaunchAtLogin={async (enabled) => {
              const next = await api.setLaunchAtLogin(enabled);
              setDesktopPreferences(next);
              setNotice(enabled ? "已启用开机启动" : "已关闭开机启动");
            }}
            onImportLegacy={async () => {
              const result = await api.importLegacyDatabase();
              if (result) {
                setMigrationResult(result);
                setNotice("旧版数据库导入完成");
                await refresh();
              }
            }}
          />
        )}
        </div>
      </section>

      {inspectedDate && (
        <DayDetailModal
          date={inspectedDate}
          onClose={() => setInspectedDate(null)}
          onEditLog={(log) => {
            setTab("logs");
            setTargetEditLog(log);
            setInspectedDate(null);
          }}
          onCreateForDate={(date) => {
            setTab("logs");
            setTargetCreateDate(date);
            setInspectedDate(null);
          }}
        />
      )}
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

function TabButton(props: {
  active: boolean;
  icon: React.ReactNode;
  onClick: () => void;
  children: React.ReactNode;
  badge?: string | number;
}) {
  return (
    <button
      type="button"
      className={props.active ? "tab active" : "tab"}
      aria-current={props.active ? "page" : undefined}
      onClick={props.onClick}
    >
      {props.icon}
      <span className="tab-label">{props.children}</span>
      {props.badge !== undefined && (
        <span className="tab-badge">{props.badge}</span>
      )}
    </button>
  );
}

interface WorkLogAiDialogueDialogProps {
  isOpen: boolean;
  onClose: () => void;
  items: WorkLog[];
  currentEditingId: number | null;
  onApplyModification: (logId: number, payload: Partial<WorkLog>) => Promise<void>;
  onLoadIntoForm: (payload: Partial<WorkLog>, logId: number) => void;
  onOpenSettings?: () => void;
}

interface DialogueMessageItem {
  id: string;
  role: "user" | "assistant";
  content: string;
  actions?: WorkLogModificationAction[];
  appliedMap?: Record<number, boolean>;
  undoneMap?: Record<number, boolean>;
}

function buildWelcomeMessage(selectedIds: number[]): DialogueMessageItem {
  if (selectedIds.length > 0) {
    return {
      id: "welcome",
      role: "assistant",
      content: `已就绪，当前已选 ${selectedIds.length} 条目标记录。请直接告诉我如何调整。`
    };
  }
  return {
    id: "welcome",
    role: "assistant",
    content: "你好！请描述需要修改的工作记录及具体调整要求。"
  };
}

function WorkLogAiDialogueDialog(props: WorkLogAiDialogueDialogProps) {
  const initialLogIds = React.useMemo(() => {
    if (props.currentEditingId && props.items.some((i) => i.id === props.currentEditingId)) {
      return [props.currentEditingId];
    }
    return [];
  }, [props.currentEditingId, props.items]);

  const [selectedLogIds, setSelectedLogIds] = React.useState<number[]>(initialLogIds);
  const [selectorOpen, setSelectorOpen] = React.useState(false);
  const [autoApply, setAutoApply] = React.useState(false);
  const [messages, setMessages] = React.useState<DialogueMessageItem[]>(() => [
    buildWelcomeMessage(initialLogIds)
  ]);
  const [input, setInput] = React.useState("");
  const [loading, setLoading] = React.useState(false);
  const [actionLoading, setActionLoading] = React.useState(false);
  const [llmNotConfigured, setLlmNotConfigured] = React.useState(false);
  const messagesEndRef = React.useRef<HTMLDivElement | null>(null);
  const textareaRef = React.useRef<HTMLTextAreaElement | null>(null);
  const dropdownRef = React.useRef<HTMLDivElement | null>(null);
  const abortRequestIdRef = React.useRef(0);

  React.useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, loading]);

  React.useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setSelectorOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  function handleToggleSelectAll() {
    const allIds = props.items.map((i) => i.id);
    if (selectedLogIds.length === allIds.length) {
      setSelectedLogIds([]);
    } else {
      setSelectedLogIds(allIds);
    }
  }

  function handleClearHistory() {
    setMessages([buildWelcomeMessage(selectedLogIds)]);
    setLlmNotConfigured(false);
  }

  function handleStop() {
    abortRequestIdRef.current = 0;
    setLoading(false);
    setMessages((curr) => [
      ...curr,
      {
        id: "stopped-" + Date.now(),
        role: "assistant",
        content: "（已停止生成。可继续输入消息继续对话）"
      }
    ]);
    setTimeout(() => {
      textareaRef.current?.focus();
    }, 50);
  }

  async function handleSend(customText?: string) {
    const text = (customText ?? input).trim();
    if (!text || loading) return;

    setInput("");
    const userMsgId = "user-" + Date.now();
    const newMsg: DialogueMessageItem = {
      id: userMsgId,
      role: "user",
      content: text
    };

    const nextHistory = [...messages, newMsg];
    setMessages(nextHistory);
    setLoading(true);

    const reqId = ++abortRequestIdRef.current;

    try {
      const apiMessages: ChatMessage[] = nextHistory
        .filter((m) => m.id !== "welcome" && !m.id.startsWith("switch-") && !m.id.startsWith("error-") && !m.id.startsWith("stopped-"))
        .map((m) => {
          let content = m.content;
          if (m.role === "assistant" && m.actions && m.actions.length > 0) {
            const actionSummaries = m.actions
              .map((act) => {
                const diffStr = (act.changes_summary ?? [])
                  .map((d) => `${d.label || d.field}: ${d.old_value ?? "空"} -> ${d.new_value ?? "空"}`)
                  .join(", ");
                return `【已针对记录 #${act.log_id} 调整: ${diffStr}】`;
              })
              .join("; ");
            content = `${content}\n${actionSummaries}`;
          }
          return {
            role: m.role,
            content
          };
        });

      const res = await api.chatModifyWorkLog({
        messages: apiMessages,
        current_editing_id: props.currentEditingId,
        target_log_ids: selectedLogIds.length > 0 ? selectedLogIds : null
      });

      if (abortRequestIdRef.current !== reqId) {
        return;
      }

      const responseActions = res.actions && res.actions.length > 0
        ? res.actions
        : (res.action ? [res.action] : []);

      const initialAppliedMap: Record<number, boolean> = {};
      if (responseActions.length > 0 && autoApply) {
        try {
          await Promise.all(
            responseActions.map((act) => props.onApplyModification(act.log_id, act.updated))
          );
          for (const act of responseActions) {
            initialAppliedMap[act.log_id] = true;
          }
        } catch (applyErr) {
          console.error("Auto-apply failed:", applyErr);
        }
      }

      if (abortRequestIdRef.current !== reqId) {
        return;
      }

      setMessages((curr) => [
        ...curr,
        {
          id: "assistant-" + Date.now(),
          role: "assistant",
          content: responseActions.length > 0 && autoApply
            ? `${res.reply}\n\n（已自动保存修改到本地数据库）`
            : res.reply,
          actions: responseActions,
          appliedMap: initialAppliedMap
        }
      ]);
    } catch (err) {
      if (abortRequestIdRef.current !== reqId) {
        return;
      }
      const errMsg = err instanceof Error ? err.message : String(err);
      if (errMsg.includes("llm_not_configured") || errMsg.includes("not configured") || errMsg.includes("未配置")) {
        setLlmNotConfigured(true);
        setMessages((curr) => [
          ...curr,
          {
            id: "error-" + Date.now(),
            role: "assistant",
            content: "当前尚未配置 LLM 模型，请先前往系统设置添加 API Key 后再使用自然语言修改功能。"
          }
        ]);
      } else {
        setMessages((curr) => [
          ...curr,
          {
            id: "error-" + Date.now(),
            role: "assistant",
            content: `处理失败：${errMsg}`
          }
        ]);
      }
    } finally {
      if (abortRequestIdRef.current === reqId) {
        setLoading(false);
      }
    }
  }

  async function handleApplySingle(msgId: string, action: WorkLogModificationAction) {
    setActionLoading(true);
    try {
      await props.onApplyModification(action.log_id, action.updated);
      setMessages((curr) =>
        curr.map((m) => {
          if (m.id !== msgId) return m;
          return {
            ...m,
            appliedMap: { ...(m.appliedMap ?? {}), [action.log_id]: true },
            undoneMap: { ...(m.undoneMap ?? {}), [action.log_id]: false }
          };
        })
      );
    } catch (err) {
      window.alert(err instanceof Error ? err.message : "应用修改失败");
    } finally {
      setActionLoading(false);
    }
  }

  async function handleUndoSingle(msgId: string, action: WorkLogModificationAction) {
    setActionLoading(true);
    try {
      await props.onApplyModification(action.log_id, action.original);
      setMessages((curr) =>
        curr.map((m) => {
          if (m.id !== msgId) return m;
          return {
            ...m,
            appliedMap: { ...(m.appliedMap ?? {}), [action.log_id]: false },
            undoneMap: { ...(m.undoneMap ?? {}), [action.log_id]: true }
          };
        })
      );
    } catch (err) {
      window.alert(err instanceof Error ? err.message : "撤销修改失败");
    } finally {
      setActionLoading(false);
    }
  }

  async function handleApplyAll(msgId: string, actions: WorkLogModificationAction[]) {
    setActionLoading(true);
    try {
      await Promise.all(
        actions.map((act) => props.onApplyModification(act.log_id, act.updated))
      );
      setMessages((curr) =>
        curr.map((m) => {
          if (m.id !== msgId) return m;
          const nextApplied = { ...(m.appliedMap ?? {}) };
          const nextUndone = { ...(m.undoneMap ?? {}) };
          for (const act of actions) {
            nextApplied[act.log_id] = true;
            nextUndone[act.log_id] = false;
          }
          return { ...m, appliedMap: nextApplied, undoneMap: nextUndone };
        })
      );
    } catch (err) {
      window.alert(err instanceof Error ? err.message : "批量应用修改失败");
    } finally {
      setActionLoading(false);
    }
  }

  async function handleUndoAll(msgId: string, actions: WorkLogModificationAction[]) {
    setActionLoading(true);
    try {
      await Promise.all(
        actions.map((act) => props.onApplyModification(act.log_id, act.original))
      );
      setMessages((curr) =>
        curr.map((m) => {
          if (m.id !== msgId) return m;
          const nextApplied = { ...(m.appliedMap ?? {}) };
          const nextUndone = { ...(m.undoneMap ?? {}) };
          for (const act of actions) {
            nextApplied[act.log_id] = false;
            nextUndone[act.log_id] = true;
          }
          return { ...m, appliedMap: nextApplied, undoneMap: nextUndone };
        })
      );
    } catch (err) {
      window.alert(err instanceof Error ? err.message : "批量撤销修改失败");
    } finally {
      setActionLoading(false);
    }
  }

  function handleLoadIntoForm(action: WorkLogModificationAction) {
    props.onLoadIntoForm(action.updated, action.log_id);
    props.onClose();
  }

  return (
    <div
      className="email-dialog-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) props.onClose();
      }}
    >
      <div
        className="email-dialog worklog-ai-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="worklog-ai-dialog-title"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="dialog-heading">
          <div>
            <p className="section-eyebrow">AI 记录助手</p>
            <h2 id="worklog-ai-dialog-title">AI 对话修改记录</h2>
            <p>智能对话调整工作记录，支持多轮对话与多选批量修改</p>
          </div>
          <button
            className="icon-button"
            type="button"
            aria-label="关闭对话窗口"
            onClick={props.onClose}
          >
            <X size={18} />
          </button>
        </div>

        <div className="worklog-ai-toolbar">
          <div className="toolbar-left" ref={dropdownRef}>
            <div className="target-multiselect-container">
              <button
                type="button"
                className={`target-multiselect-trigger ${selectorOpen ? "is-open" : ""}`}
                onClick={() => setSelectorOpen((v) => !v)}
              >
                <span className="multiselect-label">目标记录：</span>
                {selectedLogIds.length === 0 ? (
                  <span className="multiselect-val placeholder">智能识别（由 AI 自动匹配）</span>
                ) : (
                  <span className="multiselect-val active">已选 {selectedLogIds.length} 条记录</span>
                )}
                <ChevronDown size={14} className={`dropdown-arrow ${selectorOpen ? "rotated" : ""}`} />
              </button>

              {selectorOpen && (
                <div className="target-multiselect-dropdown">
                  <div className="multiselect-dropdown-header">
                    <span>选择目标记录</span>
                    <div className="multiselect-header-actions">
                      <button
                        type="button"
                        className="text-btn"
                        onClick={handleToggleSelectAll}
                      >
                        {selectedLogIds.length === props.items.length ? "取消全选" : "全选当前页"}
                      </button>
                      {selectedLogIds.length > 0 && (
                        <button
                          type="button"
                          className="text-btn danger"
                          onClick={() => setSelectedLogIds([])}
                        >
                          清空
                        </button>
                      )}
                    </div>
                  </div>

                  <div className="multiselect-options-list">
                    {props.items.length === 0 ? (
                      <div className="multiselect-empty">暂无可选记录</div>
                    ) : (
                      props.items.map((item) => {
                        const isChecked = selectedLogIds.includes(item.id);
                        return (
                          <label
                            key={item.id}
                            className={`multiselect-option ${isChecked ? "checked" : ""}`}
                          >
                            <input
                              type="checkbox"
                              checked={isChecked}
                              onChange={(e) => {
                                if (e.target.checked) {
                                  setSelectedLogIds((curr) => [...curr, item.id]);
                                } else {
                                  setSelectedLogIds((curr) => curr.filter((id) => id !== item.id));
                                }
                              }}
                            />
                            <div className="option-info">
                              <div className="option-title-row">
                                <span className="option-date">{item.start_date}</span>
                                <span className="option-project">{item.project}</span>
                                {item.hours != null && <span className="option-hours">{item.hours}h</span>}
                              </div>
                              <div className="option-task">{item.task}</div>
                            </div>
                          </label>
                        );
                      })
                    )}
                  </div>
                </div>
              )}
            </div>
          </div>

          <div className="toolbar-right">
            <label className="auto-apply-label" title="开启后，AI 返回的修改将自动保存到数据库">
              <input
                type="checkbox"
                checked={autoApply}
                onChange={(e) => setAutoApply(e.target.checked)}
              />
              <span>自动应用修改</span>
            </label>
            <button
              type="button"
              className="icon-button compact-btn"
              title="清空对话历史"
              onClick={handleClearHistory}
            >
              <RefreshCw size={14} />
            </button>
          </div>
        </div>

        {selectedLogIds.length > 0 && (
          <div className="selected-targets-pill-row">
            <span className="pills-label">已选 {selectedLogIds.length} 条：</span>
            <div className="pills-list">
              {selectedLogIds.map((id) => {
                const item = props.items.find((i) => i.id === id);
                if (!item) return null;
                return (
                  <span key={id} className="target-item-pill">
                    <span className="pill-text">[{item.start_date}] {item.project} · {item.task}</span>
                    <button
                      type="button"
                      className="pill-remove-btn"
                      title="移除此记录"
                      onClick={() => setSelectedLogIds((curr) => curr.filter((i) => i !== id))}
                    >
                      <X size={12} />
                    </button>
                  </span>
                );
              })}
            </div>
            <button
              type="button"
              className="clear-all-targets-btn"
              onClick={() => setSelectedLogIds([])}
            >
              清空
            </button>
          </div>
        )}

        {llmNotConfigured && (
          <div className="llm-warning-card">
            <CircleAlert size={18} />
            <div>
              <strong>未配置 LLM 模型</strong>
              <p>自然语言对话修改记录需要配置并启用大模型。请前往系统设置添加 API Key。</p>
              {props.onOpenSettings && (
                <button
                  type="button"
                  className="secondary compact"
                  onClick={() => {
                    props.onClose();
                    props.onOpenSettings?.();
                  }}
                >
                  前往配置 LLM
                </button>
              )}
            </div>
          </div>
        )}

        <div className="worklog-chat-messages">
          {messages.map((m) => {
            const hasActions = Boolean(m.actions && m.actions.length > 0);
            const actionsList = m.actions ?? [];
            const allApplied = hasActions && actionsList.every((act) => Boolean(m.appliedMap?.[act.log_id]));
            const someApplied = hasActions && actionsList.some((act) => Boolean(m.appliedMap?.[act.log_id]));

            return (
              <div key={m.id} className={`worklog-chat-row ${m.role}`}>
                <div className={`chat-avatar ${m.role}`}>
                  {m.role === "user" ? "我" : <Sparkles size={14} />}
                </div>
                <div className="chat-bubble-container">
                  <div className={`chat-bubble ${m.role}`}>{m.content}</div>

                  {hasActions && (
                    <div className="worklog-multi-actions-container">
                      {actionsList.length > 1 && (
                        <div className="worklog-batch-actions-header">
                          <span className="batch-header-title">
                            共修改 <strong>{actionsList.length}</strong> 条记录
                          </span>
                          <div className="batch-header-buttons">
                            {allApplied ? (
                              <>
                                <span className="applied-pill">
                                  <Check size={14} /> 全部已保存
                                </span>
                                <button
                                  type="button"
                                  className="secondary compact diff-undo-btn"
                                  disabled={actionLoading}
                                  onClick={() => handleUndoAll(m.id, actionsList)}
                                >
                                  <RotateCcw size={13} /> 一键撤销全部
                                </button>
                              </>
                            ) : (
                              <>
                                <button
                                  type="button"
                                  className="primary compact"
                                  disabled={actionLoading}
                                  onClick={() => handleApplyAll(m.id, actionsList)}
                                >
                                  <Check size={14} /> 一键应用全部修改 ({actionsList.length} 条)
                                </button>
                                {someApplied && (
                                  <button
                                    type="button"
                                    className="secondary compact diff-undo-btn"
                                    disabled={actionLoading}
                                    onClick={() => handleUndoAll(m.id, actionsList)}
                                  >
                                    <RotateCcw size={13} /> 撤销已修改项
                                  </button>
                                )}
                              </>
                            )}
                          </div>
                        </div>
                      )}

                      <div className="worklog-diff-cards-grid">
                        {actionsList.map((action) => {
                          const isApplied = Boolean(m.appliedMap?.[action.log_id]);

                          return (
                            <div key={action.log_id} className="worklog-diff-card">
                              <div className="worklog-diff-card-header">
                                <div className="diff-target-badge">
                                  <span>{action.updated.start_date}</span>
                                  <span>·</span>
                                  <span>{action.updated.project}</span>
                                </div>
                                <div className="diff-task-title">{action.updated.task}</div>
                              </div>

                              <div className="worklog-diff-changes">
                                {action.changes_summary.length > 0 ? (
                                  action.changes_summary.map((change, idx) => (
                                    <div key={idx} className="diff-change-item">
                                      <span className="diff-field-badge">{change.label}</span>
                                      <div className="diff-change-values">
                                        <span className="diff-old-val">{change.old_value || "（空）"}</span>
                                        <ArrowRight size={13} className="diff-arrow" />
                                        <span className="diff-new-val">{change.new_value || "（空）"}</span>
                                      </div>
                                    </div>
                                  ))
                                ) : (
                                  <div className="diff-change-item">
                                    <span className="diff-field-badge">状态</span>
                                    <span className="diff-new-val">字段内容保持一致，无变更</span>
                                  </div>
                                )}
                              </div>

                              <div className="worklog-diff-actions">
                                {isApplied ? (
                                  <div className="worklog-diff-applied-row">
                                    <span className="applied-pill">
                                      <Check size={14} /> 已保存到数据库
                                    </span>
                                    <button
                                      type="button"
                                      className="secondary compact diff-undo-btn"
                                      disabled={actionLoading}
                                      onClick={() => handleUndoSingle(m.id, action)}
                                    >
                                      <RotateCcw size={13} /> 撤销修改
                                    </button>
                                  </div>
                                ) : (
                                  <div className="worklog-diff-pending-row">
                                    <button
                                      type="button"
                                      className="primary compact"
                                      disabled={actionLoading}
                                      onClick={() => handleApplySingle(m.id, action)}
                                    >
                                      <Check size={14} /> 确认应用修改
                                    </button>
                                    <button
                                      type="button"
                                      className="secondary compact"
                                      disabled={actionLoading}
                                      onClick={() => handleLoadIntoForm(action)}
                                    >
                                      <Pencil size={13} /> 载入左侧表单
                                    </button>
                                  </div>
                                )}
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            );
          })}

          {loading && (
            <div className="worklog-chat-row assistant">
              <div className="chat-avatar assistant">
                <Sparkles size={14} />
              </div>
              <div className="chat-bubble-container">
                <div className="chat-bubble assistant loading">
                  <LoaderCircle className="spin" size={14} />
                  <span>AI 正在思考并调整记录...</span>
                  <button
                    type="button"
                    className="chat-bubble-stop-link"
                    onClick={handleStop}
                    title="停止生成"
                  >
                    停止
                  </button>
                </div>
              </div>
            </div>
          )}
          <div ref={messagesEndRef} />
        </div>

        <form
          className="worklog-chat-input-box"
          onSubmit={(e) => {
            e.preventDefault();
            if (loading) {
              handleStop();
            } else {
              void handleSend();
            }
          }}
        >
          <textarea
            ref={textareaRef}
            autoFocus
            rows={2}
            value={input}
            placeholder={
              loading
                ? "正在生成中，可点击停止或继续输入追加内容..."
                : "输入修改需求，按 Enter 发送，Shift+Enter 换行..."
            }
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                if (!loading) {
                  void handleSend();
                }
              }
            }}
          />
          {loading ? (
            <button
              type="button"
              className="danger worklog-chat-stop-btn"
              onClick={handleStop}
              title="停止当前生成"
            >
              <Square size={14} fill="currentColor" />
              <span>停止</span>
            </button>
          ) : (
            <button
              type="submit"
              className="primary worklog-chat-send-btn"
              disabled={!input.trim()}
              title="发送 (Enter)"
            >
              <Send size={18} />
            </button>
          )}
        </form>
      </div>
    </div>
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
  onOpenSettings?: () => void;
  targetEditLog?: WorkLog | null;
  onClearTargetEditLog?: () => void;
  targetCreateDate?: string | null;
  onClearTargetCreateDate?: () => void;
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
  const [aiDialogOpen, setAiDialogOpen] = React.useState(false);

  React.useEffect(() => {
    if (props.targetEditLog) {
      startEdit(props.targetEditLog);
      props.onClearTargetEditLog?.();
    }
  }, [props.targetEditLog, props.onClearTargetEditLog]);

  React.useEffect(() => {
    if (props.targetCreateDate) {
      setEditingId(null);
      setForm({
        ...emptyForm(),
        start_date: props.targetCreateDate,
        end_date: props.targetCreateDate
      });
      props.onClearTargetCreateDate?.();
    }
  }, [props.targetCreateDate, props.onClearTargetCreateDate, emptyForm]);

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
      <form className="panel form-panel-container" onSubmit={submit}>
        <div className="panel-heading">
          <div>
            <p className="section-eyebrow">每日工作日志</p>
            <h2>{editingId ? "编辑记录" : "新增记录"}</h2>
          </div>
          {editingId && <span className="status-badge">编辑中</span>}
        </div>
        <div className="form-scrollable-content form-grid">
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
        </div>
        <div className="button-row form-fixed-actions">
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

      <section className="list-panel list-panel-container">
        <div className="list-panel-header">
          <div>
            <p className="section-eyebrow">历史记录</p>
            <h2>工作记录</h2>
          </div>
          <div className="list-panel-header-actions">
            <button
              className="secondary compact ai-dialog-trigger"
              type="button"
              onClick={() => setAiDialogOpen(true)}
            >
              <Sparkles size={15} />
              AI 对话修改记录
            </button>
            <span className="count-badge">{props.meta.total} 条</span>
          </div>
        </div>
        <div className="list-panel-scrollable-items">
          {props.items.map((item) => (
            <article className="row-card" key={item.id}>
              <div>
                <div className="row-meta">
                  <span className="meta-date">{dateRangeLabel(item)}</span>
                  <span className="meta-project">{item.project}</span>
                  {item.origin === "auto" && <span className="auto-capture-badge">自动采集</span>}
                  <span className={`priority-badge priority-${item.priority}`}>{priorityLabel(item.priority)}</span>
                  {item.hours != null && <span className="hours-badge">{item.hours}h</span>}
                  {item.origin === "auto" && (
                    <span className="source-count-badge">Git {item.git_commit_count} · Agent {item.agent_session_count}</span>
                  )}
                  {item.pending_evidence_count > 0 && (
                    <span className="pending-evidence-badge">{item.pending_evidence_count} 条待合并</span>
                  )}
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
        </div>
        <div className="pagination fixed-pagination">
          <span>
            共 {props.meta.total} 条 · 第 {props.meta.page} / {props.meta.total_pages} 页
          </span>
          <div className="pagination-controls">
            <select value={props.pageSize} onChange={(event) => props.onPageSizeChange(Number(event.target.value))}>
              <option value={5}>5 条/页</option>
              <option value={10}>10 条/页</option>
              <option value={20}>20 条/页</option>
              <option value={50}>50 条/页</option>
            </select>
            <button
              className="secondary compact"
              type="button"
              disabled={props.meta.page <= 1}
              onClick={() => props.onPageChange(props.meta.page - 1)}
            >
              上一页
            </button>
            <button
              className="secondary compact"
              type="button"
              disabled={props.meta.page >= props.meta.total_pages}
              onClick={() => props.onPageChange(props.meta.page + 1)}
            >
              下一页
            </button>
          </div>
        </div>
      </section>

      {aiDialogOpen && (
        <WorkLogAiDialogueDialog
          isOpen={aiDialogOpen}
          onClose={() => setAiDialogOpen(false)}
          items={props.items}
          currentEditingId={editingId}
          onApplyModification={async (logId, payload) => {
            await props.onUpdate(logId, payload);
            if (editingId === logId) {
              setForm((current) => ({
                ...current,
                start_date: payload.start_date ?? current.start_date,
                end_date: payload.end_date ?? current.end_date,
                project: payload.project ?? current.project,
                task: payload.task ?? current.task,
                progress: payload.progress ?? current.progress,
                result: payload.result !== undefined ? (payload.result ?? "") : current.result,
                blockers: payload.blockers !== undefined ? (payload.blockers ?? "") : current.blockers,
                hours: payload.hours !== undefined ? (payload.hours != null ? String(payload.hours) : "") : current.hours,
                priority: (payload.priority as Priority) ?? current.priority,
                notes: payload.notes !== undefined ? (payload.notes ?? "") : current.notes,
              }));
            }
          }}
          onLoadIntoForm={(payload, logId) => {
            setEditingId(logId);
            setForm({
              start_date: payload.start_date ?? today(),
              end_date: payload.end_date ?? today(),
              project: payload.project ?? "",
              task: payload.task ?? "",
              progress: payload.progress ?? "",
              result: payload.result ?? "",
              blockers: payload.blockers ?? "",
              hours: payload.hours != null ? String(payload.hours) : "",
              priority: (payload.priority as Priority) ?? "medium",
              notes: payload.notes ?? "",
            });
          }}
          onOpenSettings={props.onOpenSettings}
        />
      )}
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
  onOptimize: (
    id: number,
    payload: { content: string; optimization_request: string }
  ) => Promise<string>;
  onSave: (id: number, payload: Partial<Report>) => Promise<void>;
  onDelete: (id: number) => Promise<void>;
  onExport: (id: number) => Promise<void>;
  onSendEmail: (id: number, payload: { recipient_ids: number[]; additional_recipients: string[]; subject: string }) => Promise<void>;
  onListEmailDeliveries: (id: number) => Promise<ReportEmailDelivery[]>;
  onNotice?: (message: string) => void;
}) {
  const [type, setType] = React.useState<ReportType>("weekly_report");
  const [anchor, setAnchor] = React.useState(today());
  const [templateId, setTemplateId] = React.useState("");
  const [selectedId, setSelectedId] = React.useState<number | null>(null);
  const selected = props.reports.find((item) => item.id === selectedId) ?? props.reports[0];
  const [draft, setDraft] = React.useState("");
  const [draftTitle, setDraftTitle] = React.useState("");
  const [viewMode, setViewMode] = React.useState<"edit" | "preview">("edit");
  const [copied, setCopied] = React.useState(false);
  const previewRef = React.useRef<HTMLDivElement>(null);
  const [isGenerating, setIsGenerating] = React.useState(false);
  const [showOptimizationDialog, setShowOptimizationDialog] = React.useState(false);
  const [optimizationRequest, setOptimizationRequest] = React.useState("");
  const [isOptimizing, setIsOptimizing] = React.useState(false);
  const [optimizationComparison, setOptimizationComparison] = React.useState<{
    original: string;
    optimized: string;
  } | null>(null);
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
    setShowOptimizationDialog(false);
    setOptimizationRequest("");
    setOptimizationComparison(null);
    setCopied(false);
  }, [selected?.id]);

  async function handleCopy() {
    if (!draft.trim()) {
      props.onNotice?.("草稿内容为空");
      return;
    }

    if (viewMode === "edit") {
      const success = await copyPlainText(draft);
      if (success) {
        setCopied(true);
        props.onNotice?.("已复制原文本到剪贴板");
        setTimeout(() => setCopied(false), 2000);
      }
    } else {
      if (!previewRef.current) {
        const success = await copyPlainText(draft);
        if (success) {
          setCopied(true);
          props.onNotice?.("已复制预览内容到剪贴板");
          setTimeout(() => setCopied(false), 2000);
        }
        return;
      }

      const selection = window.getSelection();
      let targetElement: HTMLElement = previewRef.current;
      let isPartialSelection = false;

      if (
        selection &&
        !selection.isCollapsed &&
        selection.rangeCount > 0 &&
        previewRef.current.contains(selection.anchorNode) &&
        previewRef.current.contains(selection.focusNode)
      ) {
        const range = selection.getRangeAt(0);
        const container = document.createElement("div");
        container.appendChild(range.cloneContents());
        targetElement = container;
        isPartialSelection = true;
      }

      const success = await copyPreviewElement(targetElement, previewRef.current);
      if (success) {
        setCopied(true);
        props.onNotice?.(isPartialSelection ? "已复制选中的预览内容" : "已复制预览内容到剪贴板");
        setTimeout(() => setCopied(false), 2000);
      }
    }
  }

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

  async function optimizeDraft() {
    if (!selected || isOptimizing) {
      return;
    }
    const request = optimizationRequest.trim();
    if (!draft.trim()) {
      window.alert("请先填写需要优化的草稿内容。");
      return;
    }
    if (request.length < 2) {
      window.alert("请先填写具体的优化需求。");
      return;
    }
    setIsOptimizing(true);
    try {
      const original = draft;
      const optimized = await props.onOptimize(selected.id, {
        content: original,
        optimization_request: request,
      });
      setOptimizationComparison({ original, optimized });
      setShowOptimizationDialog(false);
    } finally {
      setIsOptimizing(false);
    }
  }

  function applyOptimizedDraft() {
    if (!optimizationComparison) {
      return;
    }
    setDraft(optimizationComparison.optimized);
    setOptimizationComparison(null);
    setViewMode("edit");
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
        <section className="list-panel report-list-container">
          <div className="list-panel-header">
            <div>
              <p className="section-eyebrow">已生成内容</p>
              <h2>报告草稿</h2>
            </div>
            <span className="count-badge">{props.reports.length} 份</span>
          </div>
          <div className="report-items-scroll">
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
          </div>
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
                  onClick={() => {
                    setViewMode("edit");
                    setCopied(false);
                  }}
                >
                  <Pencil size={15} />
                  编辑
                </button>
                <button
                  type="button"
                  className={viewMode === "preview" ? "active" : ""}
                  onClick={() => {
                    setViewMode("preview");
                    setCopied(false);
                  }}
                >
                  <FileText size={15} />
                  预览
                </button>
              </div>
              <button
                className="secondary report-optimize-trigger"
                type="button"
                title="AI 优化当前草稿"
                disabled={isOptimizing || !draft.trim()}
                onClick={() => setShowOptimizationDialog(true)}
              >
                <Sparkles size={16} />
                AI 优化
              </button>
              <button
                className={copied ? "icon-button success" : "icon-button"}
                type="button"
                title={
                  copied
                    ? (viewMode === "edit" ? "已复制原文本" : "已复制预览内容")
                    : (viewMode === "edit" ? "复制原文本" : "复制预览内容")
                }
                aria-label={
                  copied
                    ? (viewMode === "edit" ? "已复制原文本" : "已复制预览内容")
                    : (viewMode === "edit" ? "复制原文本" : "复制预览内容")
                }
                disabled={!draft.trim()}
                onClick={() => void handleCopy()}
              >
                {copied ? <Check size={18} /> : <Copy size={18} />}
              </button>
              <button className="icon-button" type="button" title="导出 DOCX" aria-label="导出 DOCX" onClick={() => void props.onExport(selected.id)}>
                <Download size={18} />
              </button>
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
                <MarkdownPreview content={draft} previewRef={previewRef} onNotice={props.onNotice} />
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
      {showOptimizationDialog && selected && (
        <section
          className="email-dialog-backdrop"
          role="presentation"
          onMouseDown={() => !isOptimizing && setShowOptimizationDialog(false)}
        >
          <div
            className="email-dialog report-optimize-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="report-optimize-title"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="dialog-heading">
              <div>
                <p className="section-eyebrow">AI 报告助手</p>
                <h2 id="report-optimize-title">优化当前草稿</h2>
                <p>描述想要的调整，AI 将生成候选版本供你对比确认。</p>
              </div>
              <button
                className="icon-button"
                type="button"
                aria-label="关闭优化窗口"
                disabled={isOptimizing}
                onClick={() => setShowOptimizationDialog(false)}
              >
                <X size={18} />
              </button>
            </div>
            <label htmlFor="report-optimization-request">
              优化需求
              <textarea
                id="report-optimization-request"
                autoFocus
                value={optimizationRequest}
                disabled={isOptimizing}
                maxLength={1000}
                placeholder="例如：精简重复表述，突出关键成果，并让下阶段计划更清晰。"
                onChange={(event) => setOptimizationRequest(event.target.value)}
              />
            </label>
            <small className="report-optimize-hint">AI 只会基于当前草稿调整结构与表达，不会直接覆盖或保存内容。</small>
            <div className="dialog-actions">
              <button
                className="secondary"
                type="button"
                disabled={isOptimizing}
                onClick={() => setShowOptimizationDialog(false)}
              >
                取消
              </button>
              <button
                className="primary"
                type="button"
                disabled={isOptimizing || optimizationRequest.trim().length < 2}
                onClick={() => void optimizeDraft()}
              >
                {isOptimizing ? <LoaderCircle className="spin" size={17} /> : <Sparkles size={17} />}
                {isOptimizing ? "优化中" : "生成优化版本"}
              </button>
            </div>
          </div>
        </section>
      )}
      {optimizationComparison && (
        <OptimizationDiffDialog
          comparison={optimizationComparison}
          eyebrow="AI 优化结果"
          title="对比草稿修改"
          originalLabel="当前草稿"
          optimizedLabel="优化后"
          applyNote="应用后仅替换当前编辑内容，仍需点击保存按钮才会写入数据库。"
          onDiscard={() => setOptimizationComparison(null)}
          onApply={applyOptimizedDraft}
        />
      )}
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
              <div className="dialog-field-label">
                <span>通讯录收件人</span>
                <em className={selectedRecipientIds.length ? "selection-count-badge selected" : "selection-count-badge"}>
                  {selectedRecipientIds.length ? `已选择 ${selectedRecipientIds.length} 位` : "尚未选择"}
                </em>
              </div>
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

function splitMarkdownTableRow(row: string): string[] {
  let trimmed = row.trim();
  if (trimmed.startsWith("|")) {
    trimmed = trimmed.slice(1);
  }
  if (trimmed.endsWith("|")) {
    trimmed = trimmed.slice(0, -1);
  }
  return trimmed.split("|").map((cell) => cell.trim());
}

function MarkdownPreview(props: {
  content: string;
  previewRef?: React.RefObject<HTMLDivElement>;
  onNotice?: (msg: string) => void;
}) {
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
      blocks.push(
        <MarkdownTable
          key={`table-${blocks.length}`}
          rows={tableRows}
          onNotice={props.onNotice}
        />
      );
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

  function handlePreviewCopy(event: React.ClipboardEvent<HTMLDivElement>) {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed) {
      return;
    }
    const range = selection.getRangeAt(0);
    const container = document.createElement("div");
    container.appendChild(range.cloneContents());

    if (!container.textContent && !container.querySelector("table, img")) {
      return;
    }

    const html = buildPreviewHtml(container);
    const text = buildPreviewPlainText(container);

    event.clipboardData.setData("text/html", html);
    event.clipboardData.setData("text/plain", text);
    event.preventDefault();
    props.onNotice?.("已复制选中的预览内容");
  }

  return (
    <div
      ref={props.previewRef}
      className="markdown-preview"
      onCopy={handlePreviewCopy}
    >
      {blocks.length ? blocks : <p className="empty">暂无预览内容。</p>}
    </div>
  );
}

function MarkdownTable(props: {
  rows: string[];
  onNotice?: (msg: string) => void;
}) {
  const [copied, setCopied] = React.useState(false);
  const tableRef = React.useRef<HTMLTableElement>(null);
  const header = splitMarkdownTableRow(props.rows[0]);
  const body = props.rows.slice(1).map(splitMarkdownTableRow);

  async function handleCopyTable(event: React.MouseEvent) {
    event.stopPropagation();
    if (!tableRef.current) return;
    const success = await copyTableElement(tableRef.current);
    if (success) {
      setCopied(true);
      props.onNotice?.("已复制表格内容到剪贴板");
      setTimeout(() => setCopied(false), 2000);
    }
  }

  return (
    <div className="markdown-table-wrap">
      <button
        type="button"
        className={copied ? "table-copy-button copied" : "table-copy-button"}
        title={copied ? "已复制表格" : "复制此表格"}
        onClick={handleCopyTable}
      >
        {copied ? <Check size={13} /> : <Copy size={13} />}
        <span>{copied ? "已复制" : "复制表格"}</span>
      </button>
      <table ref={tableRef}>
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

function getElementFormattedText(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) {
    return node.textContent || "";
  }
  if (node.nodeType === Node.ELEMENT_NODE) {
    const el = node as HTMLElement;
    const tag = el.tagName.toLowerCase();
    if (tag === "br") {
      return "\n";
    }
    let res = "";
    for (const child of Array.from(el.childNodes)) {
      res += getElementFormattedText(child);
    }
    return res;
  }
  return "";
}

function formatTsvCell(text: string): string {
  const trimmed = text.trim();
  if (trimmed.includes("\t") || trimmed.includes("\n") || trimmed.includes('"')) {
    return `"${trimmed.replace(/"/g, '""')}"`;
  }
  return trimmed;
}

function buildTableHtml(table: HTMLTableElement): string {
  const clone = table.cloneNode(true) as HTMLTableElement;
  clone.querySelectorAll(".table-copy-button").forEach((el) => el.remove());
  clone.setAttribute("border", "1");
  clone.style.borderCollapse = "collapse";
  clone.style.width = "100%";
  clone.style.fontSize = "14px";
  clone.style.fontFamily = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif";
  clone.style.color = "#333333";

  clone.querySelectorAll<HTMLTableCellElement>("th").forEach((th) => {
    th.style.border = "1px solid #d0d7de";
    th.style.padding = "8px 12px";
    th.style.backgroundColor = "#f6f8fa";
    th.style.fontWeight = "bold";
    th.style.textAlign = "left";
    th.style.verticalAlign = "top";
  });

  clone.querySelectorAll<HTMLTableCellElement>("td").forEach((td) => {
    td.style.border = "1px solid #d0d7de";
    td.style.padding = "8px 12px";
    td.style.textAlign = "left";
    td.style.verticalAlign = "top";
  });

  return clone.outerHTML;
}

function buildTablePlainText(table: HTMLTableElement): string {
  const rows: string[] = [];
  table.querySelectorAll<HTMLTableRowElement>("tr").forEach((tr) => {
    const cells: string[] = [];
    tr.querySelectorAll<HTMLTableCellElement>("th, td").forEach((cell) => {
      cells.push(formatTsvCell(getElementFormattedText(cell)));
    });
    rows.push(cells.join("\t"));
  });
  return rows.join("\n");
}

function buildPreviewHtml(element: HTMLElement): string {
  const clone = element.cloneNode(true) as HTMLElement;
  clone.querySelectorAll(".table-copy-button").forEach((el) => el.remove());

  clone.querySelectorAll<HTMLTableElement>("table").forEach((table) => {
    table.setAttribute("border", "1");
    table.style.borderCollapse = "collapse";
    table.style.width = "100%";
    table.style.margin = "12px 0";
    table.style.fontSize = "14px";
    table.style.fontFamily = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif";
    table.style.color = "#333333";
  });

  clone.querySelectorAll<HTMLTableCellElement>("th").forEach((th) => {
    th.style.border = "1px solid #d0d7de";
    th.style.padding = "8px 12px";
    th.style.backgroundColor = "#f6f8fa";
    th.style.fontWeight = "bold";
    th.style.textAlign = "left";
    th.style.verticalAlign = "top";
  });

  clone.querySelectorAll<HTMLTableCellElement>("td").forEach((td) => {
    td.style.border = "1px solid #d0d7de";
    td.style.padding = "8px 12px";
    td.style.textAlign = "left";
    td.style.verticalAlign = "top";
  });

  clone.querySelectorAll<HTMLHeadingElement>("h1").forEach((h1) => {
    h1.style.fontSize = "22px";
    h1.style.fontWeight = "bold";
    h1.style.margin = "16px 0 10px";
    h1.style.color = "#111827";
  });
  clone.querySelectorAll<HTMLHeadingElement>("h2").forEach((h2) => {
    h2.style.fontSize = "18px";
    h2.style.fontWeight = "bold";
    h2.style.margin = "20px 0 10px";
    h2.style.borderTop = "1px solid #edf1f5";
    h2.style.paddingTop = "14px";
    h2.style.color = "#111827";
  });
  clone.querySelectorAll<HTMLHeadingElement>("h3").forEach((h3) => {
    h3.style.fontSize = "16px";
    h3.style.fontWeight = "bold";
    h3.style.margin = "16px 0 8px";
    h3.style.color = "#111827";
  });
  clone.querySelectorAll<HTMLHeadingElement>("h4").forEach((h4) => {
    h4.style.fontSize = "14px";
    h4.style.fontWeight = "bold";
    h4.style.margin = "12px 0 6px";
    h4.style.color = "#374151";
  });

  clone.querySelectorAll<HTMLParagraphElement>("p").forEach((p) => {
    p.style.fontSize = "14px";
    p.style.lineHeight = "1.6";
    p.style.margin = "0 0 12px";
    p.style.color = "#374151";
  });
  clone.querySelectorAll<HTMLElement>("ul, ol").forEach((list) => {
    list.style.fontSize = "14px";
    list.style.lineHeight = "1.6";
    list.style.margin = "0 0 14px";
    list.style.paddingLeft = "24px";
    list.style.color = "#374151";
  });
  clone.querySelectorAll<HTMLLIElement>("li").forEach((li) => {
    li.style.marginBottom = "4px";
  });

  clone.querySelectorAll<HTMLElement>("code").forEach((code) => {
    if (code.parentElement?.tagName.toLowerCase() !== "pre") {
      code.style.backgroundColor = "#f0f2f5";
      code.style.borderRadius = "4px";
      code.style.padding = "2px 5px";
      code.style.fontFamily = "SFMono-Regular, Consolas, monospace";
      code.style.fontSize = "0.9em";
      code.style.color = "#244b9b";
    }
  });
  clone.querySelectorAll<HTMLPreElement>("pre").forEach((pre) => {
    pre.style.backgroundColor = "#f6f8fa";
    pre.style.border = "1px solid #e2e8f1";
    pre.style.borderRadius = "6px";
    pre.style.padding = "12px";
    pre.style.overflow = "auto";
  });

  return `<div style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; font-size: 14px; line-height: 1.6; color: #333333;">${clone.innerHTML}</div>`;
}

function buildPreviewPlainText(element: HTMLElement): string {
  const clone = element.cloneNode(true) as HTMLElement;
  clone.querySelectorAll(".table-copy-button").forEach((el) => el.remove());

  const blocks: string[] = [];
  for (const child of Array.from(clone.children)) {
    const tagName = child.tagName.toLowerCase();

    if (child.classList.contains("markdown-table-wrap") || tagName === "table") {
      const table = tagName === "table" ? (child as HTMLTableElement) : child.querySelector("table");
      if (table) {
        blocks.push(buildTablePlainText(table));
        continue;
      }
    }

    if (/^h[1-6]$/.test(tagName)) {
      const text = getElementFormattedText(child).trim();
      if (text) {
        const level = parseInt(tagName.slice(1), 10);
        blocks.push(`${"#".repeat(level)} ${text}`);
      }
      continue;
    }

    if (tagName === "ul" || tagName === "ol") {
      const isOl = tagName === "ol";
      const lis = child.querySelectorAll("li");
      const listLines: string[] = [];
      lis.forEach((li, idx) => {
        const prefix = isOl ? `${idx + 1}. ` : "- ";
        listLines.push(`${prefix}${getElementFormattedText(li).trim()}`);
      });
      if (listLines.length) {
        blocks.push(listLines.join("\n"));
      }
      continue;
    }

    if (tagName === "pre") {
      blocks.push(child.textContent || "");
      continue;
    }

    const text = getElementFormattedText(child).trim();
    if (text) {
      blocks.push(text);
    }
  }

  if (blocks.length === 0) {
    return (clone.textContent || "").trim();
  }

  return blocks.join("\n\n");
}

function fallbackCopyText(text: string): boolean {
  try {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.left = "-9999px";
    textarea.style.top = "-9999px";
    textarea.style.opacity = "0";
    document.body.appendChild(textarea);
    textarea.focus();
    textarea.select();
    const success = document.execCommand("copy");
    document.body.removeChild(textarea);
    return success;
  } catch (err) {
    console.error("All copy strategies failed", err);
    return false;
  }
}

async function writeClipboard(
  html: string,
  plainText: string,
  fallbackElement?: HTMLElement
): Promise<boolean> {
  if (navigator.clipboard && typeof ClipboardItem !== "undefined") {
    try {
      const clipboardRecord: Record<string, Blob> = {
        "text/plain": new Blob([plainText], { type: "text/plain" }),
      };
      if (html) {
        clipboardRecord["text/html"] = new Blob([html], { type: "text/html" });
      }
      await navigator.clipboard.write([new ClipboardItem(clipboardRecord)]);
      return true;
    } catch (err) {
      console.warn("navigator.clipboard.write failed, trying selection fallback", err);
    }
  }

  if (fallbackElement && html && document.body.contains(fallbackElement)) {
    try {
      const selection = window.getSelection();
      if (selection) {
        const originalRanges: Range[] = [];
        for (let i = 0; i < selection.rangeCount; i++) {
          originalRanges.push(selection.getRangeAt(i));
        }

        const range = document.createRange();
        range.selectNodeContents(fallbackElement);
        selection.removeAllRanges();
        selection.addRange(range);

        const success = document.execCommand("copy");
        selection.removeAllRanges();
        for (const r of originalRanges) {
          selection.addRange(r);
        }

        if (success) {
          return true;
        }
      }
    } catch (err) {
      console.warn("execCommand fallback failed", err);
    }
  }

  if (navigator.clipboard && navigator.clipboard.writeText) {
    try {
      await navigator.clipboard.writeText(plainText);
      return true;
    } catch (err) {
      console.warn("navigator.clipboard.writeText failed", err);
    }
  }

  return fallbackCopyText(plainText);
}

async function copyPlainText(text: string): Promise<boolean> {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (err) {
      console.warn("navigator.clipboard.writeText failed, trying fallback", err);
    }
  }
  return fallbackCopyText(text);
}

async function copyPreviewElement(
  targetElement: HTMLElement,
  fallbackElement?: HTMLElement
): Promise<boolean> {
  const html = buildPreviewHtml(targetElement);
  const text = buildPreviewPlainText(targetElement);
  return writeClipboard(html, text, fallbackElement ?? targetElement);
}

async function copyTableElement(table: HTMLTableElement): Promise<boolean> {
  const html = buildTableHtml(table);
  const text = buildTablePlainText(table);
  return writeClipboard(html, text, table);
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

type TemplateDiffRow = {
  kind: "unchanged" | "removed" | "added";
  oldLineNumber?: number;
  newLineNumber?: number;
  oldText?: string;
  newText?: string;
};

function buildTemplateDiff(original: string, optimized: string): TemplateDiffRow[] {
  const oldLines = original.split("\n");
  const newLines = optimized.split("\n");
  if (oldLines.length * newLines.length > 250_000) {
    let prefixLength = 0;
    while (
      prefixLength < oldLines.length
      && prefixLength < newLines.length
      && oldLines[prefixLength] === newLines[prefixLength]
    ) {
      prefixLength += 1;
    }

    let suffixLength = 0;
    while (
      suffixLength < oldLines.length - prefixLength
      && suffixLength < newLines.length - prefixLength
      && oldLines[oldLines.length - suffixLength - 1]
        === newLines[newLines.length - suffixLength - 1]
    ) {
      suffixLength += 1;
    }

    return [
      ...oldLines.slice(0, prefixLength).map((text, index) => ({
        kind: "unchanged" as const,
        oldLineNumber: index + 1,
        newLineNumber: index + 1,
        oldText: text,
        newText: text,
      })),
      ...oldLines.slice(prefixLength, oldLines.length - suffixLength).map((text, index) => ({
        kind: "removed" as const,
        oldLineNumber: prefixLength + index + 1,
        oldText: text,
      })),
      ...newLines.slice(prefixLength, newLines.length - suffixLength).map((text, index) => ({
        kind: "added" as const,
        newLineNumber: prefixLength + index + 1,
        newText: text,
      })),
      ...oldLines.slice(oldLines.length - suffixLength).map((text, index) => ({
        kind: "unchanged" as const,
        oldLineNumber: oldLines.length - suffixLength + index + 1,
        newLineNumber: newLines.length - suffixLength + index + 1,
        oldText: text,
        newText: text,
      })),
    ];
  }
  const commonLines = Array.from(
    { length: oldLines.length + 1 },
    () => new Uint32Array(newLines.length + 1)
  );

  for (let oldIndex = 1; oldIndex <= oldLines.length; oldIndex += 1) {
    for (let newIndex = 1; newIndex <= newLines.length; newIndex += 1) {
      commonLines[oldIndex][newIndex] = oldLines[oldIndex - 1] === newLines[newIndex - 1]
        ? commonLines[oldIndex - 1][newIndex - 1] + 1
        : Math.max(commonLines[oldIndex - 1][newIndex], commonLines[oldIndex][newIndex - 1]);
    }
  }

  const rows: TemplateDiffRow[] = [];
  let oldIndex = oldLines.length;
  let newIndex = newLines.length;
  while (oldIndex > 0 || newIndex > 0) {
    if (
      oldIndex > 0
      && newIndex > 0
      && oldLines[oldIndex - 1] === newLines[newIndex - 1]
    ) {
      rows.push({
        kind: "unchanged",
        oldLineNumber: oldIndex,
        newLineNumber: newIndex,
        oldText: oldLines[oldIndex - 1],
        newText: newLines[newIndex - 1],
      });
      oldIndex -= 1;
      newIndex -= 1;
    } else if (
      newIndex > 0
      && (oldIndex === 0 || commonLines[oldIndex][newIndex - 1] >= commonLines[oldIndex - 1][newIndex])
    ) {
      rows.push({
        kind: "added",
        newLineNumber: newIndex,
        newText: newLines[newIndex - 1],
      });
      newIndex -= 1;
    } else {
      rows.push({
        kind: "removed",
        oldLineNumber: oldIndex,
        oldText: oldLines[oldIndex - 1],
      });
      oldIndex -= 1;
    }
  }

  return rows.reverse();
}

function OptimizationDiffDialog(props: {
  comparison: { original: string; optimized: string };
  eyebrow: string;
  title: string;
  originalLabel: string;
  optimizedLabel: string;
  applyNote: string;
  onDiscard: () => void;
  onApply: () => void;
}) {
  const rows = React.useMemo(
    () => buildTemplateDiff(props.comparison.original, props.comparison.optimized),
    [props.comparison]
  );

  return (
    <section className="template-diff-backdrop" role="presentation">
      <div
        className="template-diff-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="optimization-diff-title"
      >
        <div className="dialog-heading">
          <div>
            <p className="section-eyebrow">{props.eyebrow}</p>
            <h2 id="optimization-diff-title">{props.title}</h2>
            <p>红色为原内容中删除的行，绿色为优化版本新增的行。</p>
          </div>
        </div>
        <div className="template-diff-summary" aria-label="差异统计">
          <span className="removed">
            删除 {rows.filter((row) => row.kind === "removed").length} 行
          </span>
          <span className="added">
            新增 {rows.filter((row) => row.kind === "added").length} 行
          </span>
        </div>
        <div className="template-diff-view">
          <div className="template-diff-column-heading">{props.originalLabel}</div>
          <div className="template-diff-column-heading optimized">{props.optimizedLabel}</div>
          <div className="template-diff-rows">
            {rows.map((row, index) => (
              <div className={`template-diff-row ${row.kind}`} key={`${row.kind}-${index}`}>
                <div className="template-diff-cell old">
                  <span className="template-diff-line-number">{row.oldLineNumber ?? ""}</span>
                  <pre>{row.oldText ?? " "}</pre>
                </div>
                <div className="template-diff-cell new">
                  <span className="template-diff-line-number">{row.newLineNumber ?? ""}</span>
                  <pre>{row.newText ?? " "}</pre>
                </div>
              </div>
            ))}
          </div>
        </div>
        <div className="template-diff-actions">
          <p>{props.applyNote}</p>
          <div>
            <button className="secondary" type="button" onClick={props.onDiscard}>
              不应用
            </button>
            <button className="primary" type="button" onClick={props.onApply}>
              <CheckCircle2 size={17} />
              应用优化
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}

function TemplatesPage(props: {
  templates: Template[];
  onCreate: (payload: Partial<Template>) => Promise<void>;
  onImportExample: (payload: { template_type: ReportType; example_content: string }) => Promise<string>;
  onOptimize: (payload: {
    template_type: ReportType;
    content: string;
    optimization_request: string;
  }) => Promise<string>;
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
  const [isOptimizing, setIsOptimizing] = React.useState(false);
  const [hasOptimizedDraft, setHasOptimizedDraft] = React.useState(false);
  const [optimizationRequest, setOptimizationRequest] = React.useState("");
  const [optimizationComparison, setOptimizationComparison] = React.useState<{
    original: string;
    optimized: string;
  } | null>(null);
  const contentEditorRef = React.useRef<HTMLTextAreaElement | null>(null);
  const comparisonRows = React.useMemo(
    () => optimizationComparison
      ? buildTemplateDiff(optimizationComparison.original, optimizationComparison.optimized)
      : [],
    [optimizationComparison]
  );

  React.useEffect(() => {
    if (selectedId === "new") {
      setForm(emptyTemplateForm());
      setShowExampleImport(false);
      setExampleContent("");
      setHasImportedDraft(false);
      setHasOptimizedDraft(false);
      setOptimizationRequest("");
      setOptimizationComparison(null);
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
      setHasOptimizedDraft(false);
      setOptimizationRequest("");
      setOptimizationComparison(null);
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
      setHasOptimizedDraft(false);
      setShowExampleImport(false);
      window.requestAnimationFrame(() => {
        contentEditorRef.current?.scrollIntoView({ behavior: "smooth", block: "center" });
        contentEditorRef.current?.focus();
      });
    } finally {
      setIsImporting(false);
    }
  }

  async function optimizeTemplate() {
    if (!form.content.trim()) {
      window.alert("请先填写需要优化的模板内容。");
      return;
    }
    if (optimizationRequest.trim().length < 2) {
      window.alert("请先填写具体的优化需求。");
      return;
    }
    setIsOptimizing(true);
    try {
      const original = form.content;
      const content = await props.onOptimize({
        template_type: form.template_type,
        content: original,
        optimization_request: optimizationRequest.trim(),
      });
      setOptimizationComparison({ original, optimized: content });
    } finally {
      setIsOptimizing(false);
    }
  }

  function applyOptimizedTemplate() {
    if (!optimizationComparison) {
      return;
    }
    setForm((current) => ({ ...current, content: optimizationComparison.optimized }));
    setOptimizationComparison(null);
    setHasImportedDraft(false);
    setHasOptimizedDraft(true);
    window.requestAnimationFrame(() => contentEditorRef.current?.focus());
  }

  function discardOptimizedTemplate() {
    setOptimizationComparison(null);
  }

  async function saveTemplate() {
    if (selected) {
      await props.onSave(selected.id, form);
    } else {
      await props.onCreate(form);
    }
    setHasImportedDraft(false);
    setHasOptimizedDraft(false);
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
    <div className="report-grid templates-layout">
      <section className="list-panel template-list-container">
        <div className="list-panel-header">
          <div>
            <p className="section-eyebrow">可复用格式</p>
            <h2>报告模板</h2>
          </div>
          <span className="count-badge">{props.templates.length} 个</span>
        </div>
        <div className="template-items-scroll">
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
        </div>
      </section>

      <section className="template-editor template-editor-container">
        <div className="panel-heading full">
          <div>
            <p className="section-eyebrow">模板编辑器</p>
            <h2>{selected ? "编辑模板" : "创建模板"}</h2>
          </div>
          {selected?.is_default && <span className="status-badge">默认模板</span>}
        </div>
        <div className="template-editor-scroll form-grid">
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
          <input
            type="checkbox"
            checked={form.is_default}
            disabled={Boolean(selected?.is_default)}
            onChange={(event) => setField("is_default", event.target.checked)}
          />
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
                <button className="primary" type="button" disabled={isImporting || isOptimizing} onClick={() => void importExample()}>
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
        {hasOptimizedDraft && (
          <div className="draft-callout full" role="status">
            <CheckCircle2 size={16} />
            <span>LLM 已优化模板内容，请检查变量和格式，确认后再保存。</span>
          </div>
        )}
        <div className="template-content-field full">
          <div className="template-content-header">
            <label htmlFor="template-content">模板内容</label>
          </div>
          <textarea
            id="template-content"
            ref={contentEditorRef}
            className="template-markdown-editor"
            value={form.content}
            disabled={isOptimizing}
            onChange={(event) => setField("content", event.target.value)}
          />
          <div className="template-optimize-box">
            <label htmlFor="optimization-request">优化需求</label>
            <div className="template-optimize-row">
              <textarea
                id="optimization-request"
                value={optimizationRequest}
                disabled={isOptimizing}
                maxLength={1000}
                placeholder="例如：改成 OKR 风格，增加风险与下周计划章节，并让表达更精炼。"
                onChange={(event) => setOptimizationRequest(event.target.value)}
              />
              <button
                className="secondary"
                type="button"
                disabled={
                  isOptimizing
                  || isImporting
                  || !form.content.trim()
                  || optimizationRequest.trim().length < 2
                }
                onClick={() => void optimizeTemplate()}
              >
                {isOptimizing ? <LoaderCircle className="spin" size={16} /> : <Sparkles size={16} />}
                {isOptimizing ? "优化中" : "按需求优化"}
              </button>
            </div>
            <small>AI 会生成候选版本；对比新旧差异并选择应用后，才会回填编辑器。</small>
          </div>
        </div>
        </div>
        <div className="button-row template-fixed-actions">
          <button
            className="primary"
            type="button"
            disabled={isOptimizing || isImporting}
            onClick={() => void saveTemplate()}
          >
            <Save size={18} />
            保存模板
          </button>
          {selected && !selected.is_default && (
            <button className="secondary danger" type="button" onClick={() => void deleteTemplate()}>
              <Trash2 size={18} />
              删除模板
            </button>
          )}
        </div>
      </section>
      {optimizationComparison && (
        <section className="template-diff-backdrop" role="presentation">
          <div
            className="template-diff-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="template-diff-title"
          >
            <div className="dialog-heading">
              <div>
                <p className="section-eyebrow">LLM 优化结果</p>
                <h2 id="template-diff-title">对比模板修改</h2>
                <p>红色为原内容中删除的行，绿色为优化版本新增的行。</p>
              </div>
            </div>
            <div className="template-diff-summary" aria-label="差异统计">
              <span className="removed">
                删除 {comparisonRows.filter((row) => row.kind === "removed").length} 行
              </span>
              <span className="added">
                新增 {comparisonRows.filter((row) => row.kind === "added").length} 行
              </span>
            </div>
            <div className="template-diff-view">
              <div className="template-diff-column-heading">原模板</div>
              <div className="template-diff-column-heading optimized">优化后</div>
              <div className="template-diff-rows">
                {comparisonRows.map((row, index) => (
                  <div className={`template-diff-row ${row.kind}`} key={`${row.kind}-${index}`}>
                    <div className="template-diff-cell old">
                      <span className="template-diff-line-number">{row.oldLineNumber ?? ""}</span>
                      <pre>{row.oldText ?? " "}</pre>
                    </div>
                    <div className="template-diff-cell new">
                      <span className="template-diff-line-number">{row.newLineNumber ?? ""}</span>
                      <pre>{row.newText ?? " "}</pre>
                    </div>
                  </div>
                ))}
              </div>
            </div>
            <div className="template-diff-actions">
              <p>应用后仅替换编辑器内容，仍需点击“保存模板”才会写入。</p>
              <div>
                <button className="secondary" type="button" onClick={discardOptimizedTemplate}>
                  不应用
                </button>
                <button className="primary" type="button" onClick={applyOptimizedTemplate}>
                  <CheckCircle2 size={17} />
                  应用优化
                </button>
              </div>
            </div>
          </div>
        </section>
      )}
    </div>
  );
}

function SettingsPage(props: {
  setting: LlmSetting | null;
  settings: LlmSetting[];
  templates: Template[];
  emailSetting: EmailSetting | null;
  recipients: Recipient[];
  reportSchedules: ReportSchedule[];
  desktopPreferences: DesktopPreferences | null;
  migrationResult: MigrationResult | null;
  activitySources: ActivitySource[];
  activityCandidates: ActivitySourceCandidate[];
  dailyCaptureSettings: DailyCaptureSettings | null;
  dailyCaptureRuns: DailyCaptureRun[];
  onSave: (payload: LlmSetting) => Promise<void>;
  onApplyLlmSetting: (id: number) => Promise<void>;
  onDeleteLlmSetting: (id: number) => Promise<void>;
  onSaveReportSchedule: (
    reportType: ReportType,
    payload: Pick<ReportSchedule, "enabled" | "weekday" | "day_of_month" | "template_id" | "run_time" | "auto_send" | "recipient_ids">
  ) => Promise<void>;
  onSaveEmail: (payload: EmailSetting) => Promise<void>;
  onTestEmail: (address: string) => Promise<void>;
  onCreateRecipient: (payload: Pick<Recipient, "name" | "email" | "is_default">) => Promise<void>;
  onUpdateRecipient: (id: number, payload: Partial<Pick<Recipient, "name" | "email" | "is_default">>) => Promise<void>;
  onDeleteRecipient: (id: number) => Promise<void>;
  onCreateActivitySource: (payload: {
    source_type: ActivitySourceType;
    path: string;
    display_name?: string;
    enabled: boolean;
    discovered: boolean;
  }) => Promise<void>;
  onUpdateActivitySource: (id: number, payload: {
    source_type: ActivitySourceType;
    path: string;
    display_name: string;
    enabled: boolean;
    discovered: boolean;
  }) => Promise<void>;
  onDeleteActivitySource: (id: number) => Promise<void>;
  onSaveDailyCaptureSettings: (payload: { enabled: boolean; run_time: string }) => Promise<void>;
  onRunDailyCapture: (date: string, forceOverwrite: boolean) => Promise<DailyCaptureRun>;
  onSetLaunchAtLogin: (enabled: boolean) => Promise<void>;
  onImportLegacy: () => Promise<void>;
}) {
  const [activeSubTab, setActiveSubTab] = React.useState<"llm" | "capture" | "schedules" | "recipients" | "desktop">("llm");
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
  const [gatewayStatus, setGatewayStatus] = React.useState("");
  const [gatewayChecking, setGatewayChecking] = React.useState(false);
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
    setGatewayStatus("");
    setForm({ ...setting, api_key: "" });
    setHeadersText(JSON.stringify(setting.extra_headers ?? {}, null, 2));
    setHeadersError("");
  }

  function setProvider(provider: Provider) {
    setGatewayStatus("");
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

  async function checkGateway() {
    if (!form.id || gatewayChecking) return;
    setGatewayChecking(true);
    setGatewayStatus("");
    try {
      const result = await api.testXcliConnection(form.id);
      const location = { local: "本机推理", remote: "转发云端", unknown: "推理位置未知，按云端处理" }[result.inference_location];
      setGatewayStatus(`已保存配置可连接：${result.model}；${location}。未调用模型。`);
    } catch (error) {
      setGatewayStatus(typeof error === "string" ? error : error instanceof Error ? error.message : "网关检查失败");
    } finally {
      setGatewayChecking(false);
    }
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
    <div className="settings-page-wrapper">
      <div className="settings-subtabs-bar">
        <button
          type="button"
          className={`settings-subtab-btn ${activeSubTab === "llm" ? "active" : ""}`}
          onClick={() => setActiveSubTab("llm")}
        >
          <Cpu size={15} />
          <span>AI 模型配置</span>
        </button>
        <button
          type="button"
          className={`settings-subtab-btn ${activeSubTab === "capture" ? "active" : ""}`}
          onClick={() => setActiveSubTab("capture")}
        >
          <Radio size={15} />
          <span>每日活动采集</span>
        </button>
        <button
          type="button"
          className={`settings-subtab-btn ${activeSubTab === "schedules" ? "active" : ""}`}
          onClick={() => setActiveSubTab("schedules")}
        >
          <Clock size={15} />
          <span>定时与邮件</span>
        </button>
        <button
          type="button"
          className={`settings-subtab-btn ${activeSubTab === "recipients" ? "active" : ""}`}
          onClick={() => setActiveSubTab("recipients")}
        >
          <Users size={15} />
          <span>常用收件人</span>
        </button>
        <button
          type="button"
          className={`settings-subtab-btn ${activeSubTab === "desktop" ? "active" : ""}`}
          onClick={() => setActiveSubTab("desktop")}
        >
          <Settings size={15} />
          <span>偏好与系统</span>
        </button>
      </div>

      <div className="settings-subtab-viewport">
        {activeSubTab === "desktop" && (
          <section className="panel settings-panel desktop-settings-panel">
            <div className="settings-intro">
              <p className="section-eyebrow">桌面应用</p>
              <h2>本地运行</h2>
              <p>关闭主窗口后 Worklog 会留在系统托盘，定时报告仍会继续执行。</p>
              <div className="settings-side-note">
                <span>数据库位置</span>
                <strong className="path-value">{props.desktopPreferences?.database_path ?? "正在读取"}</strong>
                <p>{props.migrationResult?.imported ? `已从 ${props.migrationResult.source_path ?? "旧版本"} 自动迁移` : "数据保存在操作系统应用数据目录。"}</p>
              </div>
            </div>
            <div className="settings-section">
              <label className="toggle">
                <input
                  type="checkbox"
                  checked={Boolean(props.desktopPreferences?.launch_at_login)}
                  onChange={(event) => void props.onSetLaunchAtLogin(event.target.checked)}
                />
                登录电脑后自动启动 Worklog
              </label>
              <p className="form-hint">未自动发现旧版数据时，可手动选择原来的 worklog.db；已有新数据后将禁止覆盖导入。</p>
              <button className="secondary" type="button" onClick={() => void props.onImportLegacy()}>
                <Download size={16} />
                导入旧版数据库
              </button>
            </div>
          </section>
        )}

        {activeSubTab === "llm" && (
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
                    <option value="xcli">xcli 本机网关</option>
                    <option value="nvidia">NVIDIA</option>
                    <option value="openrouter">OpenRouter</option>
                    <option value="minimax">MiniMax（国内）</option>
                    <option value="bailian">阿里云百炼（国内）</option>
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
              {form.provider === "xcli" && <button className="secondary" type="button" disabled={!form.id || gatewayChecking} onClick={() => void checkGateway()}>{gatewayChecking ? "正在检查…" : "检查已保存网关（不调用模型）"}</button>}
              <button className="primary" type="submit">
                <Save size={18} />
                {form.id ? "保存并应用修改" : "保存并应用新配置"}
              </button>
            </div>
            {form.provider === "xcli" && <p className="form-hint" role="status">{gatewayStatus || "填写 xcli 独立应用 Token。网关在本机，上游推理可能在云端；先保存配置再检查。"}</p>}
          </form>
        )}

        {activeSubTab === "capture" && (
          <DailyCapturePanel
            sources={props.activitySources}
            candidates={props.activityCandidates}
            settings={props.dailyCaptureSettings}
            runs={props.dailyCaptureRuns}
            onCreateSource={props.onCreateActivitySource}
            onUpdateSource={props.onUpdateActivitySource}
            onDeleteSource={props.onDeleteActivitySource}
            onSaveSettings={props.onSaveDailyCaptureSettings}
            onRun={props.onRunDailyCapture}
          />
        )}

        {activeSubTab === "schedules" && (
          <div className="settings-stack">
            <ReportSchedulesPanel
              schedules={props.reportSchedules}
              templates={props.templates}
              emailSetting={props.emailSetting}
              recipients={props.recipients}
              onSave={props.onSaveReportSchedule}
            />
            <EmailSettingsPanel setting={props.emailSetting} onSave={props.onSaveEmail} onTest={props.onTestEmail} />
          </div>
        )}

        {activeSubTab === "recipients" && (
          <RecipientDirectory
            recipients={props.recipients}
            onCreate={props.onCreateRecipient}
            onUpdate={props.onUpdateRecipient}
            onDelete={props.onDeleteRecipient}
          />
        )}
      </div>
    </div>
  );
}

function DailyCapturePanel(props: {
  sources: ActivitySource[];
  candidates: ActivitySourceCandidate[];
  settings: DailyCaptureSettings | null;
  runs: DailyCaptureRun[];
  onCreateSource: (payload: {
    source_type: ActivitySourceType;
    path: string;
    display_name?: string;
    enabled: boolean;
    discovered: boolean;
  }) => Promise<void>;
  onUpdateSource: (id: number, payload: {
    source_type: ActivitySourceType;
    path: string;
    display_name: string;
    enabled: boolean;
    discovered: boolean;
  }) => Promise<void>;
  onDeleteSource: (id: number) => Promise<void>;
  onSaveSettings: (payload: { enabled: boolean; run_time: string }) => Promise<void>;
  onRun: (date: string, forceOverwrite: boolean) => Promise<DailyCaptureRun>;
}) {
  const [form, setForm] = React.useState({ enabled: false, run_time: "18:00" });
  const [scanDate, setScanDate] = React.useState(today());
  const [evidence, setEvidence] = React.useState<ActivityEvidence[]>([]);
  const [isAdding, setIsAdding] = React.useState<ActivitySourceType | null>(null);
  const [isScanning, setIsScanning] = React.useState(false);

  React.useEffect(() => {
    if (props.settings) {
      setForm({
        enabled: props.settings.enabled,
        run_time: props.settings.run_time.slice(0, 5)
      });
    }
  }, [props.settings?.enabled, props.settings?.run_time]);

  React.useEffect(() => {
    let active = true;
    void api.listActivityEvidence(scanDate).then((items) => {
      if (active) {
        setEvidence(items);
      }
    }).catch(() => {
      if (active) {
        setEvidence([]);
      }
    });
    return () => {
      active = false;
    };
  }, [scanDate, props.runs]);

  const unconfiguredCandidates = props.candidates.filter((candidate) =>
    !props.sources.some((source) => source.source_type === candidate.source_type && source.path === candidate.path)
  );
  const selectedRun = props.runs.find((run) => run.capture_date === scanDate);

  async function chooseDirectories(sourceType: ActivitySourceType) {
    setIsAdding(sourceType);
    try {
      const paths = await api.chooseActivityDirectories();
      for (const path of paths) {
        await props.onCreateSource({
          source_type: sourceType,
          path,
          enabled: true,
          discovered: false
        });
      }
    } finally {
      setIsAdding(null);
    }
  }

  async function runCapture(forceOverwrite: boolean) {
    if (forceOverwrite && !window.confirm("这会覆盖该日期已手工编辑的自动记录，是否继续？")) {
      return;
    }
    setIsScanning(true);
    try {
      await props.onRun(scanDate, forceOverwrite);
      setEvidence(await api.listActivityEvidence(scanDate));
    } finally {
      setIsScanning(false);
    }
  }

  async function removeSource(source: ActivitySource) {
    if (window.confirm(`删除采集目录「${source.display_name}」？已生成的工作记录不会被删除。`)) {
      await props.onDeleteSource(source.id);
    }
  }

  return (
    <section className="panel settings-panel daily-capture-panel">
      <div className="settings-intro">
        <p className="section-eyebrow">自动采集</p>
        <h2>每日自动记录</h2>
        <p>从本人 Git 提交和已完成的 Codex/Cursor 会话生成每天一条“多项目”工作记录。</p>
        <div className="settings-side-note">
          <span>隐私范围</span>
          <strong>只读取完成摘要与变更统计</strong>
          <p>不会读取提示词、推理、工具输出或完整对话；发送给 LLM 的证据最多 64 KiB。</p>
        </div>
      </div>

      <div className="settings-section activity-source-section">
        <div className="saved-llm-header">
          <div>
            <h3>采集目录</h3>
            <p>Git 仓库可以添加多个；Codex 和 Cursor 默认目录需要确认后才会启用。</p>
          </div>
          <span className="count-badge">{props.sources.length} 个</span>
        </div>

        {unconfiguredCandidates.length > 0 && (
          <div className="activity-candidate-list">
            {unconfiguredCandidates.map((candidate) => (
              <div className="activity-candidate" key={`${candidate.source_type}-${candidate.path}`}>
                <div>
                  <strong>发现 {activitySourceLabel(candidate.source_type)}</strong>
                  <small>{candidate.path}</small>
                </div>
                <button
                  className="secondary"
                  type="button"
                  onClick={() => void props.onCreateSource({
                    ...candidate,
                    enabled: true,
                    discovered: true
                  })}
                >
                  确认启用
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="activity-source-list">
          {props.sources.length === 0 ? (
            <p className="empty-hint">尚未配置采集目录。先添加至少一个 Git 仓库或 Agent 数据目录。</p>
          ) : props.sources.map((source) => (
            <div className={source.enabled ? "activity-source active" : "activity-source"} key={source.id}>
              <label className="toggle">
                <input
                  type="checkbox"
                  checked={source.enabled}
                  onChange={(event) => void props.onUpdateSource(source.id, {
                    source_type: source.source_type,
                    path: source.path,
                    display_name: source.display_name,
                    enabled: event.target.checked,
                    discovered: source.discovered
                  })}
                />
                <span>{activitySourceLabel(source.source_type)}</span>
              </label>
              <div className="activity-source-main">
                <strong>{source.display_name}</strong>
                <small>{source.path}</small>
                {source.last_error && <em>{source.last_error}</em>}
              </div>
              <button
                className="icon-button danger"
                type="button"
                title="删除采集目录"
                aria-label={`删除 ${source.display_name}`}
                onClick={() => void removeSource(source)}
              >
                <Trash2 size={16} />
              </button>
            </div>
          ))}
        </div>

        <div className="activity-add-actions">
          {(["git", "codex", "cursor"] as ActivitySourceType[]).map((sourceType) => (
            <button
              className="secondary"
              type="button"
              disabled={isAdding !== null}
              key={sourceType}
              onClick={() => void chooseDirectories(sourceType)}
            >
              <FolderPlus size={16} />
              {isAdding === sourceType ? "正在添加" : `添加 ${activitySourceLabel(sourceType)}`}
            </button>
          ))}
        </div>
      </div>

      <form
        className="settings-section capture-schedule-form"
        onSubmit={(event) => {
          event.preventDefault();
          void props.onSaveSettings(form);
        }}
      >
        <h3>执行计划</h3>
        <p>默认北京时间 18:00 汇总当天；下次执行还会重扫前一天，将晚间活动补回原自然日。</p>
        <div className="capture-schedule-row">
          <label className="toggle">
            <input
              type="checkbox"
              checked={form.enabled}
              onChange={(event) => setForm({ ...form, enabled: event.target.checked })}
            />
            启用每日自动记录
          </label>
          <label>
            每日执行时间
            <input
              type="time"
              required
              value={form.run_time}
              onChange={(event) => setForm({ ...form, run_time: event.target.value })}
            />
          </label>
          <button className="primary" type="submit" disabled={form.enabled && props.sources.every((source) => !source.enabled)}>
            <Save size={16} />保存计划
          </button>
        </div>
        <p className="form-hint">
          下次执行：{props.settings?.next_run_at ? formatScheduleDate(props.settings.next_run_at) : "未安排"}
          {props.settings?.last_success_at ? ` · 最近成功：${new Date(props.settings.last_success_at).toLocaleString("zh-CN")}` : ""}
        </p>
      </form>

      <div className="settings-section capture-run-section">
        <div className="saved-llm-header">
          <div>
            <h3>立即扫描与运行记录</h3>
            <p>可扫描任意日期；重复扫描会按提交哈希和会话 ID 去重。</p>
          </div>
        </div>
        <div className="capture-run-controls">
          <input type="date" value={scanDate} onChange={(event) => setScanDate(event.target.value)} />
          <button className="primary" type="button" disabled={isScanning || props.sources.every((source) => !source.enabled)} onClick={() => void runCapture(false)}>
            <Play size={16} />{isScanning ? "扫描中" : "立即扫描"}
          </button>
          {selectedRun && selectedRun.pending_evidence_count > 0 && (
            <button className="secondary danger" type="button" disabled={isScanning} onClick={() => void runCapture(true)}>
              覆盖并合并待处理来源
            </button>
          )}
        </div>

        {props.runs.length > 0 && (
          <div className="capture-run-list">
            {props.runs.slice(0, 7).map((run) => (
              <button className={run.capture_date === scanDate ? "capture-run active" : "capture-run"} type="button" key={run.id} onClick={() => setScanDate(run.capture_date)}>
                <strong>{run.capture_date}</strong>
                <span className={`capture-status ${run.status}`}>{captureStatusLabel(run.status)}</span>
                <small>
                  来源 {run.source_count}
                  {run.failed_source_count > 0 ? `（失败 ${run.failed_source_count}）` : ""}
                  {` · Git ${run.git_commit_count} · Agent ${run.agent_session_count}`}
                </small>
                {run.pending_evidence_count > 0 && <em>{run.pending_evidence_count} 条待合并</em>}
              </button>
            ))}
          </div>
        )}

        {selectedRun?.message && <p className={selectedRun.status === "failed" ? "field-error" : "form-hint"}>{selectedRun.message}</p>}
        {evidence.length > 0 && (
          <details className="capture-evidence">
            <summary>查看 {scanDate} 的 {evidence.length} 条来源摘要</summary>
            <div>
              {evidence.map((item) => (
                <article key={item.id}>
                  <span>{activitySourceLabel(item.source_type)} · {item.project}</span>
                  <p>{item.summary}</p>
                </article>
              ))}
            </div>
          </details>
        )}
      </div>
    </section>
  );
}

function activitySourceLabel(sourceType: ActivitySourceType) {
  return { git: "Git 仓库", codex: "Codex", cursor: "Cursor" }[sourceType];
}

function captureStatusLabel(status: DailyCaptureRun["status"]) {
  return {
    pending: "执行中",
    success: "成功",
    partial: "部分成功",
    failed: "失败",
    skipped: "无事项"
  }[status];
}

const weekdayLabels: Record<ScheduleWeekday, string> = {
  mon: "周一",
  tue: "周二",
  wed: "周三",
  thu: "周四",
  fri: "周五",
  sat: "周六",
  sun: "周日"
};

function ReportSchedulesPanel(props: {
  schedules: ReportSchedule[];
  templates: Template[];
  emailSetting: EmailSetting | null;
  recipients: Recipient[];
  onSave: (
    reportType: ReportType,
    payload: Pick<ReportSchedule, "enabled" | "weekday" | "day_of_month" | "template_id" | "run_time" | "auto_send" | "recipient_ids">
  ) => Promise<void>;
}) {
  return (
    <section className="panel settings-panel report-schedules-panel">
      <div className="settings-intro">
        <p className="section-eyebrow">自动生成</p>
        <h2>定时报告</h2>
        <p>每种报告独立设置执行时间；到达计划时间时会更新同期间旧草稿，并避免重复生成或发送。</p>
        <div className="settings-side-note">
          <span>错过执行</span>
          <strong>启动后只补生成最近一期草稿</strong>
          <p>补生成不会自动发送邮件，避免应用启动时意外投递。</p>
        </div>
      </div>
      <div className="schedule-card-grid">
        {props.schedules.map((schedule) => (
          <ReportScheduleCard
            key={schedule.report_type}
            schedule={schedule}
            templates={props.templates.filter((template) => template.template_type === schedule.report_type)}
            emailSetting={props.emailSetting}
            recipients={props.recipients}
            onSave={props.onSave}
          />
        ))}
      </div>
    </section>
  );
}

function ReportScheduleCard(props: {
  schedule: ReportSchedule;
  templates: Template[];
  emailSetting: EmailSetting | null;
  recipients: Recipient[];
  onSave: (
    reportType: ReportType,
    payload: Pick<ReportSchedule, "enabled" | "weekday" | "day_of_month" | "template_id" | "run_time" | "auto_send" | "recipient_ids">
  ) => Promise<void>;
}) {
  const toForm = React.useCallback((schedule: ReportSchedule) => ({
    weekday: (schedule.weekday ?? "fri") as ScheduleWeekday,
    day_of_month: schedule.day_of_month ?? null,
    template_id: schedule.template_id ?? null,
    run_time: schedule.run_time.slice(0, 5),
    auto_send: schedule.auto_send,
    recipient_ids: [...schedule.recipient_ids]
  }), []);
  const [form, setForm] = React.useState(() => toForm(props.schedule));
  const [isToggling, setIsToggling] = React.useState(false);

  React.useEffect(() => {
    setForm(toForm(props.schedule));
  }, [props.schedule.updated_at, toForm]);

  const missingSmtp = form.auto_send && !props.emailSetting;
  const missingRecipients = form.auto_send && form.recipient_ids.length === 0;
  const invalidAutoSend = missingSmtp || missingRecipients;
  const monthly = props.schedule.report_type !== "weekly_report";
  const defaultTemplate = props.templates.find((template) => template.is_default);
  const selectedTemplate = props.templates.find((template) => template.id === form.template_id);

  function payload(enabled: boolean) {
    return {
      enabled,
      weekday: monthly ? null : form.weekday,
      day_of_month: monthly ? form.day_of_month : null,
      template_id: form.template_id,
      run_time: form.run_time,
      auto_send: form.auto_send,
      recipient_ids: form.recipient_ids
    };
  }

  function toggleRecipient(recipientId: number) {
    setForm((current) => ({
      ...current,
      recipient_ids: current.recipient_ids.includes(recipientId)
        ? current.recipient_ids.filter((id) => id !== recipientId)
        : [...current.recipient_ids, recipientId]
    }));
  }

  async function save(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (invalidAutoSend) {
      return;
    }
    await props.onSave(props.schedule.report_type, payload(props.schedule.enabled));
  }

  async function toggleEnabled() {
    if (invalidAutoSend || !form.run_time || isToggling) {
      return;
    }
    setIsToggling(true);
    try {
      await props.onSave(props.schedule.report_type, payload(!props.schedule.enabled));
    } finally {
      setIsToggling(false);
    }
  }

  return (
    <form className={props.schedule.enabled ? "schedule-card active" : "schedule-card"} onSubmit={(event) => void save(event)}>
      <div className="schedule-card-heading">
        <div>
          <p className="section-eyebrow">自动生成状态</p>
          <h3>{reportLabels[props.schedule.report_type]}</h3>
        </div>
        <button
          className={props.schedule.enabled ? "schedule-enable-button enabled" : "schedule-enable-button"}
          type="button"
          aria-pressed={props.schedule.enabled}
          disabled={invalidAutoSend || !form.run_time || isToggling}
          onClick={() => void toggleEnabled()}
        >
          <span className="schedule-status-dot" aria-hidden="true" />
          <span>
            <strong>{isToggling ? "正在保存…" : props.schedule.enabled ? "已启用" : "已停用"}</strong>
            <small>{props.schedule.enabled ? "点击停用" : "点击启用"}</small>
          </span>
        </button>
      </div>

      <div className="schedule-fields">
        {monthly ? (
          <label>
            每月执行日期
            <select
              value={form.day_of_month ?? "last"}
              onChange={(event) => setForm({
                ...form,
                day_of_month: event.target.value === "last" ? null : Number(event.target.value)
              })}
            >
              <option value="last">每月最后一天</option>
              {Array.from({ length: 28 }, (_, index) => index + 1).map((day) => (
                <option value={day} key={day}>每月 {day} 日</option>
              ))}
            </select>
          </label>
        ) : (
          <label>
            每周执行日期
            <select
              value={form.weekday}
              onChange={(event) => setForm({ ...form, weekday: event.target.value as ScheduleWeekday })}
            >
              {(Object.keys(weekdayLabels) as ScheduleWeekday[]).map((weekday) => (
                <option value={weekday} key={weekday}>{weekdayLabels[weekday]}</option>
              ))}
            </select>
          </label>
        )}
        <label>
          执行时间
          <input
            type="time"
            required
            value={form.run_time}
            onChange={(event) => setForm({ ...form, run_time: event.target.value })}
          />
        </label>
      </div>

      <label className="schedule-template-field">
        报告模板
        <select
          value={form.template_id ?? ""}
          onChange={(event) => setForm({
            ...form,
            template_id: event.target.value ? Number(event.target.value) : null
          })}
        >
          <option value="">
            跟随默认模板{defaultTemplate ? `（${defaultTemplate.name}）` : ""}
          </option>
          {props.templates.map((template) => (
            <option value={template.id} key={template.id}>
              {template.name}{template.is_default ? "（当前默认）" : ""}
            </option>
          ))}
        </select>
        <small>
          {selectedTemplate
            ? `已固定使用「${selectedTemplate.name}」，默认模板变化不会影响此任务。`
            : "当前跟随该报告类型的默认模板。"}
        </small>
      </label>

      <label className="toggle schedule-send-toggle">
        <input
          type="checkbox"
          checked={form.auto_send}
          onChange={(event) => setForm({ ...form, auto_send: event.target.checked })}
        />
        <span className="toggle-copy">
          <strong>生成后直接发送邮件</strong>
          <small>{form.auto_send ? "报告生成成功后将立即投递" : "当前仅生成草稿，不会发送邮件"}</small>
        </span>
        <span className={form.auto_send ? "toggle-state enabled" : "toggle-state"}>
          {form.auto_send ? "已开启" : "已关闭"}
        </span>
      </label>

      {form.auto_send && (
        <fieldset className="schedule-recipients">
          <legend>自动发送收件人 · 已选择 {form.recipient_ids.length} 位</legend>
          {props.recipients.length ? props.recipients.map((recipient) => (
            <label className="recipient-check" key={recipient.id}>
              <input
                type="checkbox"
                checked={form.recipient_ids.includes(recipient.id)}
                onChange={() => toggleRecipient(recipient.id)}
              />
              <span><strong>{recipient.name}</strong><small>{recipient.email}</small></span>
            </label>
          )) : <p className="form-hint">通讯录暂无收件人，请先在下方添加。</p>}
        </fieldset>
      )}

      {missingSmtp && <p className="field-error">请先保存 SMTP 邮箱配置，再启用自动发送。</p>}
      {missingRecipients && <p className="field-error">自动发送至少需要选择一位收件人。</p>}

      <div className="schedule-footer">
        <div>
          <span>下次执行</span>
          <strong>{props.schedule.enabled && props.schedule.next_run_at ? formatScheduleDate(props.schedule.next_run_at) : "未安排"}</strong>
        </div>
        <button className="primary" type="submit" disabled={invalidAutoSend || !form.run_time}>
          <Save size={16} />保存设置
        </button>
      </div>
    </form>
  );
}

function formatScheduleDate(value: string) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false
  }).format(new Date(value));
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
