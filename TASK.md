## 任务目标

基于 [oh-my-rime-cli](https://github.com/Mintimate/oh-my-rime-cli) 的核心逻辑，使用 Rust 重构一个 TUI + CLI 版本的 Rime 配置管理工具。

## 参考项目分析

原项目 (Go) 的核心模块：

| 模块 | 文件 | 职责 |
|------|------|------|
| constants | `internal/constants/` | 应用元信息、下载地址常量 |
| system | `internal/system/` | OS 检测、目标目录解析、注册表读取(Win)、浏览器/文件管理器打开 |
| downloader | `internal/downloader/` | HTTP 下载 + 进度条显示 |
| updater | `internal/updater/` | ZIP 解压、备份/恢复、主方案/模型/词库更新 |
| cli | `cmd/cli/main.go` | 交互式命令行菜单界面 |
| app | `app.go`, `main.go` | Wails GUI 入口（本次不需要） |

核心业务流程（来自原项目 README 流程图）：

```
启动 → 检测OS → 主菜单
  ├─ ① 更新薄荷方案 → 下载 zip → 解压替换方案包
  ├─ ② 更新万象模型 → 下载 gram → 替换模型文件
  ├─ ③ 更新词库(Lite版) → 下载 zip → 提取 dicts/ 词库
  └─ ④ 自定义更新 → 粘贴 URL (zip/gram) → 同上处理
```

---

## 技术选型

| 层 | 方案 | 说明 |
|---|------|------|
| 语言 | Rust (stable) | edition 2024 |
| 异步运行时 | `tokio` | HTTP 下载需要异步 |
| TUI 框架 | `ratatui` + `crossterm` | 终端 UI 首选 |
| CLI 参数 | `clap` (derive) | 子命令模式 |
| HTTP 客户端 | `reqwest` | 支持进度回调 |
| ZIP 解压 | `zip` crate | 纯 Rust |
| 压缩文件.tar.gz | `flate2` + `tar` | 备用格式支持 |
| 日志 | `tracing` + `tracing-subscriber` | 结构化日志 |
| 错误处理 | `thiserror` + `anyhow` | 分层错误 |
| 序列化 | `serde` + `serde_json` / `toml` | 配置文件读写 |
| 平台条件编译 | `#[cfg(windows)]` / `#[cfg(unix)]` | 注册表、路径差异 |

---

## 项目结构

```
oh-my-rime-tui/
├── Cargo.toml
├── src/
│   ├── main.rs              # 入口：解析 clap → 分发 CLI / TUI 模式
│   ├── cli.rs               # CLI 非交互模式（子命令直接执行）
│   ├── tui/
│   │   ├── mod.rs
│   │   ├── app.rs           # TUI 应用状态机
│   │   ├── pages/
│   │   │   ├── mod.rs
│   │   │   ├── menu.rs      # 主菜单页
│   │   │   ├── os_select.rs # OS/输入法目录选择页
│   │   │   └── progress.rs  # 下载进度页
│   │   └── widgets/         # 可复用 UI 组件
│   ├── core/
│   │   ├── mod.rs
│   │   ├── constants.rs     # 应用元信息常量、下载 URL
│   │   ├── system.rs         # OS 检测、Rime 目录解析
│   │   ├── downloader.rs     # HTTP 下载（带进度回调）
│   │   ├── updater.rs        # 核心更新逻辑：方案/模型/词库
│   │   └── backup.rs         # 备份/恢复/清理旧备份
│   └── error.rs              # 统一错误类型
├── config.toml               # 可选配置文件（自定义下载源等）
└── README.md
```

---

## 分阶段实施计划

### Phase 1: 项目骨架 & 核心库 (core)

**目标**：完成无界面依赖的核心业务逻辑，可直接被 CLI 和 TUI 调用。

#### 1.1 初始化项目
- 创建 `Cargo.toml`，引入全部依赖
- 配置 `[features]` 用于条件编译 Windows 注册表功能

#### 1.2 `core/constants.rs`
- `APP_NAME`, `APP_VERSION`, `APP_AUTHOR`, `APP_URL` 等
- `OH_MY_RIME_REPO` — 薄荷方案 zip 下载地址
- `WAN_XIANG_GRA` — 万象模型 gram 下载地址

#### 1.3 `core/system.rs`
- `detect_os() -> OsType` — 返回 `Windows_NT | Linux | Darwin | Unknown`
- `get_target_dir(os_type, rime_flavor) -> PathBuf` — 根据 OS + 输入法类型返回 Rime 配置目录
  - Windows: 注册表 → `%APPDATA%\Rime`
  - Linux: `~/.config/ibus/rime` / `~/.local/share/fcitx5/rime` / Flatpak 路径
  - macOS: `~/Library/Rime` (鼠须管) / `~/.local/share/fcitx5/rime` (小企鹅)
- `open_url(url)` — 跨平台打开浏览器
- `open_folder(path)` — 跨平台打开文件管理器
- `expand_homedir(path) -> PathBuf` — 展开 `~`

#### 1.4 `core/downloader.rs`
- `download(url) -> Result<Vec<u8>>` — 基础下载
- `download_with_progress(url, progress_callback) -> Result<Vec<u8>>` — 带进度回调
- 进度条渲染（内联 `\r` 覆盖，CLI 模式用；TUI 模式只用回调数值）
- User-Agent 伪装
- Content-Type 校验（拒绝 text/html）
- 超时/重试机制

#### 1.5 `core/updater.rs`
- `update_main_scheme(zip_data, target_dir)` — 解压 zip → 覆盖写入目标目录
- `update_model(gram_data, target_dir)` — 写入 wanxiang-lts-zh-hans.gram
- `update_dict(zip_data, target_dir)` — 解压并提取 dicts/ 子目录
- ZIP 路径安全校验（防止 Zip Slip 攻击）
- 调用 `backup.rs` 的备份/恢复/清理

#### 1.6 `core/backup.rs`
- `create_backup(target_dir) -> (backup_path, has_backup)`
- `restore_backup(target_dir, backup_path)`
- `prune_backups(target_dir, keep_count)` — 保留最近 N 个，清理旧备份
- 备份目录命名：`{target_dir}.backups/{timestamp}`

#### 1.7 `error.rs`
- `AppError` enum：`Download`, `Extract`, `Io`, `Backup`, `Platform` 等

#### 1.8 单元测试
- `system.rs` 的路径解析
- `updater.rs` 的 ZIP 安全路径
- `backup.rs` 的创建/恢复/清理

---

### Phase 2: CLI 命令行界面

**目标**：提供与原始 oh-my-rime-cli 兼容的 CLI 交互模式。

#### 2.1 `main.rs` — 入口分发
```rust
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

enum Commands {
    /// 更新薄荷主方案
    Scheme,
    /// 更新万象模型
    Model,
    /// 更新词库(Lite版)
    Dict,
    /// 自定义 URL 更新
    Custom { url: String },
}
```
- 无子命令时 → 启动 TUI 模式
- 有子命令时 → CLI 直接执行

#### 2.2 `cli.rs` — CLI 执行流程
- 检测 OS（不支持则退出）
- 询问/选择 Rime 配置目录（Linux/macOS 列出输入法选项）
- 执行对应子命令（下载 → 备份 → 更新）
- 显示进度条（内联 `\r` 方式）
- 结果输出 + 退出码

---

### Phase 3: TUI 终端界面

**目标**：基于 ratatui 构建美观的终端 UI。

#### 3.1 `tui/app.rs` — 应用状态机
```rust
enum AppState {
    Menu,           // 主菜单
    OsSelect,       // OS/输入法类型选择
    Progress,       // 下载/更新进度
    Result,         // 操作结果页
    ConfirmQuit,    // 退出确认
}
```
- 全局状态：`current_os`, `target_dir`, `selected_action`, `progress`

#### 3.2 `tui/pages/menu.rs` — 主菜单
- 彩色标题 banner
- 功能列表（上下键选择 + 回车确认）
  1. 更新薄荷方案
  2. 更新万象模型
  3. 更新词库(Lite版)
  4. 自定义更新（弹出输入框输入 URL）
- 底部快捷键提示：`q` 退出, `↑↓` 选择, `Enter` 确认

#### 3.3 `tui/pages/os_select.rs` — 平台选择
- Windows: 自动检测注册表（一键），失败则显示默认路径
- Linux: 列表选择 iBus / Fcitx5 / Fcitx5-Flatpak
- macOS: 列表选择 鼠须管 / 小企鹅
- 支持手动输入路径

#### 3.4 `tui/pages/progress.rs` — 进度页面
- 当前操作描述（下载中/解压中/更新中）
- 进度条（ratatui `Gauge` widget）
- 百分比、已下载/总大小、速度、ETA
- 文件变更日志滚动区域

#### 3.5 `tui/widgets/` — 可复用组件
- `banner` — 顶部标题栏
- `menu_list` — 可导航选择列表
- `status_bar` — 底部状态栏
- `input_popup` — 文本输入弹窗（自定义 URL）

---

### Phase 4: Windows 平台支持（条件编译）

#### 4.1 注册表读取
- `#[cfg(windows)]` 使用 `winreg` crate 读取 `HKCU\Software\Rime\Weasel\RimeUserDir`
- `#[cfg(not(windows))]` 该函数为空实现

#### 4.2 路径差异
- Windows 路径分隔符、`%APPDATA%` 环境变量
- 浏览器打开：`cmd /c start` vs `xdg-open`/`open`

---

### Phase 5: 构建、测试、发布

#### 5.1 构建脚本
- Makefile / `justfile`：`build`, `test`, `lint`, `release`
- GitHub Actions CI：跨平台编译 Linux/macOS/Windows

#### 5.2 测试
- 核心库单元测试
- CLI 集成测试（mock 下载）
- TUI snapshot 测试（ratatui 有测试支持）

#### 5.3 发布
- `cargo build --release` 产出单二进制
- GitHub Releases 发布三种平台二进制
- 可选：AUR (Arch)、Homebrew (macOS)、Scoop (Windows)

---

## 关键依赖清单 (Cargo.toml)

```toml
[package]
name = "oh-my-rime-tui"
version = "3.0.0"
edition = "2024"

[dependencies]
# TUI
ratatui = "0.29"
crossterm = "0.28"

# CLI
clap = { version = "4", features = ["derive"] }

# Async + HTTP
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
reqwest = { version = "0.12", features = ["stream"] }

# File formats
zip = "2"
flate2 = "1"
tar = "0.4"

# Error handling
thiserror = "2"
anyhow = "1"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# Platform specific
[target.'cfg(windows)'.dependencies]
winreg = "0.52"

[dev-dependencies]
tempfile = "3"
```

---

## TUI 界面概念稿

```
┌─────────────────────────────────────────────────────────┐
│                                                         │
│         Oh My Rime - 薄荷输入法配置管理工具 v3.0           │
│                                                         │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  当前系统: Linux (Fcitx5)                               │
│  目标目录: ~/.local/share/fcitx5/rime                    │
│                                                         │
│  ┌─────────────────────────────────────────────────┐    │
│  │                                                  │    │
│  │   > 1. 更新薄荷方案                               │    │
│  │     2. 更新万象模型                               │    │
│  │     3. 更新词库 (Lite版)                          │    │
│  │     4. 自定义更新                                 │    │
│  │                                                  │    │
│  └─────────────────────────────────────────────────┘    │
│                                                         │
├─────────────────────────────────────────────────────────┤
│  ↑↓ 选择  Enter 确认  q 退出  d 文档  b B站             │
└─────────────────────────────────────────────────────────┘
```
