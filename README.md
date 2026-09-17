# Worklog Desktop

Worklog 2.0 是使用 Tauri 2、Rust、React 和 SQLite 构建的本地桌面工作记录与报告应用。安装后不需要 Python、Node.js 或后台 HTTP 服务。

## 功能

- 每日工作记录、分页浏览与项目/工时/优先级管理。
- 周报、月报和绩效考核表草稿生成、编辑、AI 优化及 DOCX 导出。
- Markdown + Jinja 风格模板管理，并可使用 LLM 从示例生成或优化模板。
- OpenAI、NVIDIA、OpenRouter、国内 MiniMax M3 等 OpenAI-compatible LLM Provider。
- 多 Git 仓库与 Codex/Cursor 完成会话的每日自动采集，可配置北京时间执行时间。
- SMTP 测试、收件人通讯录、HTML 正文与 DOCX 附件投递。
- 周报、月报、绩效考核表定时生成及可选自动发送。
- 关闭主窗口后在系统托盘继续运行，可选择登录电脑后自动启动。

## 数据与安全

数据库保存在操作系统的应用数据目录。第一次启动时会检测旧版项目的 `data/worklog.db` 和 `.env` 中的 `WORKLOG_DATABASE_URL`，验证 SQLite 完整性后复制并迁移；旧数据库不会被修改。未自动发现时，可在“系统设置 → 本地运行”中手动选择数据库。

LLM API Key 和 SMTP 密码会从旧数据库迁移到 macOS Keychain 或 Windows Credential Manager，读取接口只返回脱敏值。

每日自动采集只保存 Git 提交信息以及 Agent 会话的完成摘要、项目路径映射和变更统计，不保存提示词、推理、工具输出或完整对话。启用 AI 汇总后，这些最小化证据会发送给当前启用的 LLM，单日最多 64 KiB。

## 下载安装

前往 [GitHub Releases](https://github.com/Garden12138/worklog/releases/latest) 下载最新版本：

- macOS：下载 `.dmg`，打开后将 Worklog 拖入“应用程序”。
- Windows：下载 `-setup.exe`，运行安装程序并按提示完成安装。

当前公开安装包可能使用 ad-hoc 签名或未签名。如果系统显示开发者或 SmartScreen 提醒，请先确认文件来自本仓库的 Release 页面，再通过系统的安全设置允许打开。

## 使用步骤

### 1. 首次启动

启动 Worklog 后，数据会保存在本机应用数据目录。应用会自动尝试导入旧版 `data/worklog.db`；如未发现，可进入“系统设置 → 本地运行”，点击“导入旧版数据库”手动选择。

建议开启“登录电脑后自动启动 Worklog”。关闭主窗口后应用仍会驻留系统托盘，定时报告可以继续执行。

### 2. 配置报告模型

进入“系统设置 → LLM 设置”：

1. 选择 OpenAI、NVIDIA、OpenRouter 或国内 MiniMax；MiniMax 默认使用 `https://api.minimax.cn/v1` 和 `MiniMax-M3`。
2. 填写模型名称、API Key 和请求超时时间。
3. 点击“保存并应用新配置”。保存多组配置后，可在列表中切换当前使用的模型。

### 3. 记录每日工作

进入“每日记录”，填写日期、项目、事项和进展；结果、阻塞、工时、优先级与备注可按需填写。保存后的记录会作为报告生成的数据来源，并可随时编辑或删除。

### 4. 生成和编辑报告

进入“报告草稿”：

1. 选择周报、月报或绩效考核表，并设置参考日期与模板。
2. 点击“生成草稿”，确认生成周期和内容。
3. 在 Markdown 编辑器中修改内容，或使用“AI 优化”生成候选版本。
4. 保存后可预览、导出 DOCX，或通过邮件发送。

如需使用公司固定格式，可在“模板管理”中新建模板，也可以粘贴现有报告示例，让 LLM 生成可复用的模板草稿。

### 5. 配置邮件发送

进入“系统设置”：

1. 在“SMTP 邮箱设置”中填写服务器、端口、用户名和应用专用密码，保存后先发送测试邮件。
2. 在“收件人通讯录”中添加常用收件人，可将其设为默认收件人。
3. 返回“报告草稿”，点击邮件按钮，选择收件人后发送。

### 6. 设置定时报告

在“系统设置 → 定时报告”中分别配置周报、月报和绩效考核表：

1. 选择执行日期、时间和报告模板。
2. 如需自动投递，开启“生成后直接发送邮件”并选择收件人。
3. 点击“保存设置”并确认状态为“已启用”。

到达执行时间时，应用会自动生成报告；如果同周期已有旧草稿，会先更新草稿再继续发送。同一次计划任务不会重复生成或重复发送。应用需要保持运行或驻留系统托盘；若执行时未运行，下次启动只会补生成最近一期草稿，不会自动补发邮件。

### 7. 设置每日自动记录

进入“系统设置 → 每日自动记录”：

1. 添加一个或多个 Git 仓库。采集依赖本机 `git` 命令，并只读取与仓库 `user.email`（无邮箱时为 `user.name`）匹配的提交。
2. 确认自动发现的 Codex/Cursor 默认目录，或手动添加多个数据目录。默认位置只会展示候选项，未经确认不会读取。
3. 启用执行计划。默认北京时间 18:00 汇总当天，应用启动时会补齐最近 7 个漏跑日。
4. 可选择日期立即扫描。重复扫描按提交哈希和会话 ID 去重；18:00 后的内容会在下次扫描补回原自然日。

每个自然日最多生成一条“多项目”自动工作记录。若自动记录已被手工编辑，后续扫描不会覆盖，而会显示待合并来源；只有用户确认“覆盖并合并”后才会重新生成。

## 开发环境

- Node.js 22
- Rust stable（最低 1.85）
- macOS：Xcode Command Line Tools
- Windows：Microsoft C++ Build Tools 与 WebView2

安装前端依赖：

```bash
npm --prefix frontend install
```

启动桌面开发模式：

```bash
npm run desktop:dev
```

## 测试

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm --prefix frontend run build
```

## 打包

当前平台安装包：

```bash
npm run desktop:build
```

macOS 通用架构 DMG：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
APPLE_SIGNING_IDENTITY=- npm run tauri -- build --target universal-apple-darwin --bundles dmg
```

Windows x64 NSIS：

```powershell
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis
```

`.github/workflows/desktop-build.yml` 会在推送 `v2.0.0` 分支或手动触发时分别在 macOS 与 Windows runner 上生成安装包。推送 `v*` 版本标签时，两个平台打包成功后还会自动创建公开 GitHub Release，并上传 DMG 和 EXE。

发布新版本：

```bash
git tag v2.0.2
git push origin refs/tags/v2.0.2
```

请将示例版本号替换为本次发布版本。如果版本分支与标签同名，请保留完整的 `refs/tags/...` 写法，避免 Git 无法判断要推送分支还是标签。重新运行同一标签的工作流时，会更新该 Release 中的同名安装包，而不会重复创建 Release。

没有正式证书时，macOS 使用 ad-hoc 签名，Windows 生成未签名安装包，系统可能显示安全提醒。正式发布时可参考 `src-tauri/tauri.macos-signing.conf.example.json` 和 `src-tauri/tauri.windows-signing.conf.example.json` 配置证书及 CI secrets。

## 项目结构

- `frontend/`：React/Vite 用户界面与 Tauri command 调用层。
- `src-tauri/`：Rust 业务核心、SQLite 迁移、定时任务、系统托盘与安装包配置。
