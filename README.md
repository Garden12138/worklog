# Worklog Desktop

Worklog 2.0 是使用 Tauri 2、Rust、React 和 SQLite 构建的本地桌面工作记录与报告应用。安装后不需要 Python、Node.js 或后台 HTTP 服务。

## 功能

- 每日工作记录、分页浏览与项目/工时/优先级管理。
- 周报、月报和绩效考核表草稿生成、编辑、AI 优化及 DOCX 导出。
- Markdown + Jinja 风格模板管理，并可使用 LLM 从示例生成或优化模板。
- OpenAI、NVIDIA、OpenRouter 等 OpenAI-compatible LLM Provider。
- SMTP 测试、收件人通讯录、HTML 正文与 DOCX 附件投递。
- 周报、月报、绩效考核表定时生成及可选自动发送。
- 关闭主窗口后在系统托盘继续运行，可选择登录电脑后自动启动。

## 数据与安全

数据库保存在操作系统的应用数据目录。第一次启动时会检测旧版项目的 `data/worklog.db` 和 `.env` 中的 `WORKLOG_DATABASE_URL`，验证 SQLite 完整性后复制并迁移；旧数据库不会被修改。未自动发现时，可在“系统设置 → 本地运行”中手动选择数据库。

LLM API Key 和 SMTP 密码会从旧数据库迁移到 macOS Keychain 或 Windows Credential Manager，读取接口只返回脱敏值。

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

`.github/workflows/desktop-build.yml` 会在推送 `v2.0.0` 或手动触发时分别在 macOS 与 Windows runner 上生成安装包。

没有正式证书时，macOS 使用 ad-hoc 签名，Windows 生成未签名安装包，系统可能显示安全提醒。正式发布时可参考 `src-tauri/tauri.macos-signing.conf.example.json` 和 `src-tauri/tauri.windows-signing.conf.example.json` 配置证书及 CI secrets。

## 项目结构

- `frontend/`：React/Vite 用户界面与 Tauri command 调用层。
- `src-tauri/`：Rust 业务核心、SQLite 迁移、定时任务、系统托盘与安装包配置。
