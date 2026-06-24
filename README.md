# Worklog

本地自托管的工作记录与自动报告系统。它支持每日工作记录、周报/月报/绩效考核表草稿生成、Markdown 模板管理、DOCX 导出，以及 OpenAI / NVIDIA / OpenRouter 等 OpenAI-compatible LLM Provider。

## 功能

- 每日工作记录：开始日期、结束日期、项目、事项、进展、结果、阻塞、工时、优先级、备注，并支持分页浏览。
- 报告草稿：按自然周/月生成周报、月报、绩效考核表，并支持在线编辑。
- 模板管理：使用 Markdown + Jinja 风格变量，例如 `{{ title }}`、`{{ ai_content }}`、`{{ work_items }}`。
- LLM 设置：本地保存 provider、base URL、model、API key、extra headers；API key 展示时脱敏。
- 自动任务：Asia/Shanghai 时区，周日 20:00 生成周报，每月最后一天 20:30 生成月报和绩效考核表。

## 本地运行

后端：

```bash
cd backend
python3 -m venv .venv
source .venv/bin/activate
python3 -m pip install -r requirements.txt
python3 -m uvicorn app.main:app --host 127.0.0.1 --port 8000
```

前端：

```bash
cd frontend
npm install
npm run dev
```

打开 `http://127.0.0.1:5173`。

## 测试

```bash
cd backend
python3 -m pytest

cd ../frontend
npm run build
```
