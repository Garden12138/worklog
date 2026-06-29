# Worklog

本地自托管的工作记录与自动报告系统。它支持每日工作记录、周报/月报/绩效考核表草稿生成、Markdown 模板管理、DOCX 导出，以及 OpenAI / NVIDIA / OpenRouter 等 OpenAI-compatible LLM Provider。

## 功能

- 每日工作记录：开始日期、结束日期、项目、事项、进展、结果、阻塞、工时、优先级、备注，并支持分页浏览。
- 报告草稿：按自然周/月生成周报、月报、绩效考核表，支持在线编辑、DOCX 导出和直接邮件发送。
- 邮件投递：配置 SMTP 后，可维护上级等收件人通讯录、选择多个默认收件人或临时邮箱，邮件正文附带 HTML 报告与 DOCX 文件，并保留发送成功/失败记录。
- 模板管理：使用 Markdown + Jinja 风格变量，例如 `{{ title }}`、`{{ ai_content }}`、`{{ work_items }}`。
- LLM 设置：本地保存 provider、base URL、model、API key、extra headers；API key 展示时脱敏。
- 定时报告：可分别设置周报、月报和绩效考核表的执行日期、时间与模板，并可选择生成后直接发送给指定通讯录收件人。三类报告的默认执行时间均为 15:00，其中周报默认每周五执行，月报和绩效考核表默认每月最后一天执行。

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

## 配置 SMTP 并发送报告

在“系统设置 → SMTP 邮箱设置”中填写企业邮箱或个人邮箱的 SMTP 信息：主机、端口、STARTTLS/SSL、用户名、应用专用密码、发件人邮箱与显示名。保存后可先向任意邮箱发送测试邮件。

通常建议在“收件人通讯录”中维护直属上级，并勾选“默认收件人”。编辑完成报告后，点击编辑器工具栏的邮件图标，确认主题、收件人和临时邮箱，即可保存草稿并将 HTML 正文与 DOCX 附件一并发送。SMTP 密码只保存在本地后端数据库，接口读取时会脱敏显示。

需要自动生成或投递时，可在“系统设置 → 定时报告”中分别启用报告类型、设置执行时间并选择收件人。应用错过执行时间后只会补生成最近一期草稿，不会补发邮件；自动发送失败会保留失败记录，供你在报告页手工重发。
