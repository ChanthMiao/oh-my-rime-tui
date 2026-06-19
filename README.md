# Oh My Rime TUI

[原项目 oh-my-rime-cli](https://github.com/Mintimate/oh-my-rime-cli) 的 **Rust 重写版本**，提供 TUI 和 CLI 双界面。

用于 [Oh-my-rime](https://github.com/Mintimate/oh-my-rime) 薄荷输入法方案的自动下载和更新管理，支持 Windows / Linux / macOS。

## 与原项目的区别

| | oh-my-rime-cli (Go) | oh-my-rime-tui (Rust) |
|---|---|---|
| 语言 | Go + Wails | Rust |
| GUI | Wails WebView | 不需要（仅 TUI + CLI） |
| 体积 | ~10MB | ~5MB（单二进制） |
| 运行时依赖 | WebView2 / WebKit | 无 |
| UI | Wails GUI / 简单 CLI | ratatui TUI / clap CLI |

## 为什么重写

原工具很实用，但个人不满意当前交互体验：

1. 旧版 GUI 过于单调，新版 GUI 基于 WebView，都不喜欢
2. CLI 滚动进度条太浪费终端空间

改用 Rust 是因为它对 AI 编码相对友好，容易迭代维护。

## 功能

- 一键下载和更新 Rime 主方案、万象模型、词库
- 自动检测操作系统，智能选择 Rime 配置目录
- Windows 下支持注册表自动查找 Rime 用户目录
- 终端 TUI 界面（ratatui）+ 命令行子命令模式
- 下载进度条（大小/速度/ETA）+ 解压文件级进度
- 更新前自动备份，失败自动回滚，保留最近 3 份备份

## 安装

```bash
cargo install --path .
```

或从源码构建：

```bash
git clone https://github.com/Mintimate/oh-my-rime-tui.git
cd oh-my-rime-tui
cargo build --release
# 二进制文件在 target/release/oh-my-rime-tui
```

## 使用

### TUI 模式（默认）

```bash
oh-my-rime-tui
```

上下箭头选择功能，回车确认。

### CLI 模式

```bash
oh-my-rime-tui scheme   # 更新薄荷方案
oh-my-rime-tui model    # 更新万象模型
oh-my-rime-tui dict     # 更新词库 (Lite版)
oh-my-rime-tui tui      # 显式启动 TUI
```

CLI 模式会进入交互菜单，也可以直接传子命令一键执行。

## Rime 配置目录

| 平台 | 输入法 | 路径 |
|------|--------|------|
| Windows | 小狼毫 | 注册表 `RimeUserDir` → `%APPDATA%\Rime` |
| Linux | iBus | `~/.config/ibus/rime` |
| Linux | Fcitx5 | `~/.local/share/fcitx5/rime` |
| Linux | Fcitx5 Flatpak | `~/.var/app/org.fcitx.Fcitx5/data/fcitx5/rime` |
| macOS | 鼠须管 | `~/Library/Rime` |
| macOS | 小企鹅 | `~/.local/share/fcitx5/rime` |

## 技术栈

- [ratatui](https://ratatui.rs/) — 终端 TUI 框架
- [clap](https://docs.rs/clap) — CLI 参数解析
- [tokio](https://tokio.rs/) — 异步运行时
- [reqwest](https://docs.rs/reqwest) — HTTP 下载
- [zip](https://docs.rs/zip) — ZIP 解压

## 许可

MIT License
