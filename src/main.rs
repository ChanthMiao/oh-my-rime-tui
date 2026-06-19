mod cli;
mod core;
mod error;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "oh-my-rime-tui", version, author)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 更新薄荷主方案
    Scheme,
    /// 更新万象模型
    Model,
    /// 更新万象词库 (Lite版)
    Dict,
    /// 使用自定义 URL 更新 (zip 或 gram 文件)
    Custom {
        /// 文件的下载 URL
        url: String,
    },
    /// 启动交互式 TUI 界面
    Tui,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Tui) => {
            #[cfg(feature = "tui")]
            {
                run_tui().await?;
            }
            #[cfg(not(feature = "tui"))]
            {
                eprintln!("TUI 功能未启用。请使用 `--features tui` 重新编译，或使用 CLI 子命令。");
                std::process::exit(1);
            }
        }
        Some(cmd) => {
            run_cli(cmd).await?;
        }
    }

    Ok(())
}

#[cfg(feature = "tui")]
async fn run_tui() -> anyhow::Result<()> {
    crate::tui::run().await?;
    Ok(())
}

#[cfg(not(feature = "tui"))]
async fn run_tui() -> anyhow::Result<()> {
    unreachable!()
}

async fn run_cli(cmd: Commands) -> anyhow::Result<()> {
    match cmd {
        Commands::Scheme | Commands::Model | Commands::Dict | Commands::Custom { .. } => {
            cli::run_interactive().await?;
        }
        Commands::Tui => unreachable!(),
    }
    Ok(())
}

#[cfg(feature = "tui")]
mod tui {
    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;

    use crossterm::{
        event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{
        backend::CrosstermBackend,
        layout::{Alignment, Constraint, Direction, Layout, Rect},
        style::{Color, Modifier, Style, Stylize},
        text::{Line, Span},
        widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
        Frame, Terminal,
    };

    use crate::core::constants;
    use crate::core::downloader::{self};
    use crate::core::system::{self, OsType, RimeFlavor};
    use crate::core::updater::{self, ExtractProgress};
    use crate::error::AppResult;

    #[derive(Clone, Copy, PartialEq)]
    enum ProgressPhase {
        Download,
        Extract,
        Idle,
    }

    #[derive(Clone)]
    struct SharedProgress {
        phase: ProgressPhase,
        downloaded: i64,
        total: i64,
        percentage: f64,
        speed: f64,
        extract_current: usize,
        extract_total: usize,
        extract_file: String,
        label: String,
        done: bool,
        #[allow(dead_code)]
        error: Option<String>,
    }

    impl Default for SharedProgress {
        fn default() -> Self {
            Self {
                phase: ProgressPhase::Idle,
                downloaded: 0,
                total: 0,
                percentage: 0.0,
                speed: 0.0,
                extract_current: 0,
                extract_total: 0,
                extract_file: String::new(),
                label: "准备中...".into(),
                done: false,
                error: None,
            }
        }
    }

    #[derive(PartialEq)]
    enum AppState {
        Menu,
        OsSelect,
        Progress,
        Result,
        Exit,
    }

    enum Action {
        UpdateScheme,
        UpdateModel,
        UpdateDict,
        Custom,
        #[allow(dead_code)]
        OpenBilibili,
        #[allow(dead_code)]
        OpenDocs,
    }

    struct App {
        state: AppState,
        os: OsType,
        selected: usize,
        target_dir: Option<std::path::PathBuf>,
        #[allow(dead_code)]
        rime_flavors: Vec<RimeFlavor>,
        flavor_selected: Option<usize>,
        active_flavors: Vec<RimeFlavor>,
        result_message: String,
        #[allow(dead_code)]
        custom_url: String,
        #[allow(dead_code)]
        custom_input_active: bool,
        action: Option<Action>,
        progress: Arc<Mutex<SharedProgress>>,
    }

    impl App {
        fn new() -> Self {
            let os = system::detect_os();
            let active_flavors = match os {
                OsType::Windows => vec![],
                OsType::Linux => system::get_linux_flavors(),
                OsType::Darwin => system::get_darwin_flavors(),
                OsType::Unknown => vec![],
            };

            Self {
                state: AppState::Menu,
                os,
                selected: 0,
                target_dir: None,
                rime_flavors: Vec::new(),
                flavor_selected: None,
                active_flavors,
                result_message: String::new(),
                custom_url: String::new(),
                custom_input_active: false,
                action: None,
                progress: Arc::new(Mutex::new(SharedProgress::default())),
            }
        }
    }

    fn menu_items() -> Vec<&'static str> {
        vec![
            "更新薄荷方案",
            "更新万象模型",
            "更新词库 (Lite版)",
            "自定义更新",
        ]
    }

    pub async fn run() -> anyhow::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut app = App::new();
        let res = run_app(&mut terminal, &mut app).await;

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        if let Err(e) = res {
            eprintln!("错误: {e}");
        }

        Ok(())
    }

    async fn run_app(
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
        app: &mut App,
    ) -> anyhow::Result<()> {
        let tick_rate = std::time::Duration::from_millis(33);

        loop {
            terminal.draw(|f| ui(f, app))?;

            // Poll for user input
            if event::poll(tick_rate)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match app.state {
                        AppState::Menu => handle_menu_key(key.code, app),
                        AppState::OsSelect => handle_osselect_key(key.code, app),
                        AppState::Progress => {
                            // Allow Esc to cancel while in progress
                            if key.code == KeyCode::Esc {
                                // Cancel is handled via AtomicBool
                            }
                        }
                        AppState::Result => match key.code {
                            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ') => {
                                app.state = AppState::Menu;
                                app.result_message.clear();
                            }
                            _ => {}
                        },
                        AppState::Exit => return Ok(()),
                    }
                }
            }

            // Check if we need to start an action
            if app.state == AppState::Progress && app.action.is_some() {
                let action = app.action.take().unwrap();
                let target_dir = app.target_dir.clone().unwrap_or_default();
                let progress = Arc::clone(&app.progress);

                // Reset progress
                {
                    let mut p = progress.lock().unwrap();
                    *p = SharedProgress::default();
                    p.label = "下载中...".into();
                    p.phase = ProgressPhase::Download;
                }

                let handle = tokio::spawn(async move {
                    run_action(action, &target_dir, progress).await
                });

                app.action = None;

                // Wait for completion while keeping the UI alive
                loop {
                    terminal.draw(|f| ui(f, app))?;

                    if handle.is_finished() {
                        match handle.await {
                            Ok(Ok(())) => {
                                app.result_message = "✅ 操作成功完成！\n请重新部署 Rime 方案以使更改生效。".into();
                            }
                            Ok(Err(e)) => {
                                app.result_message = format!("❌ 操作失败: {e}");
                            }
                            Err(e) => {
                                app.result_message = format!("❌ 任务异常: {e}");
                            }
                        }
                        app.state = AppState::Result;
                        break;
                    }

                    // Handle cancel
                    if event::poll(std::time::Duration::from_millis(50))? {
                        if let Event::Key(key) = event::read()? {
                            if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                                // Signal cancel and break out
                                break;
                            }
                        }
                    }
                }
            }

            if app.state == AppState::Exit {
                return Ok(());
            }
        }
    }

    fn handle_menu_key(code: KeyCode, app: &mut App) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                app.state = AppState::Exit;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = if app.os == OsType::Unknown {
                    0
                } else {
                    menu_items().len() - 1
                };
                app.selected = (app.selected + 1).min(max);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.selected = app.selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                let action = match app.selected {
                    0 => Action::UpdateScheme,
                    1 => Action::UpdateModel,
                    2 => Action::UpdateDict,
                    3 => Action::Custom,
                    _ => return,
                };

                if app.os == OsType::Unknown {
                    app.result_message =
                        "不支持的操作系统，请确保在支持的操作系统上运行".into();
                    app.state = AppState::Result;
                } else if app.os == OsType::Windows {
                    match system::get_windows_rime_dir() {
                        Ok(dir) => {
                            app.target_dir = Some(dir);
                            app.action = Some(action);
                            app.state = AppState::Progress;
                        }
                        Err(_) => {
                            let appdata =
                                std::env::var("APPDATA").unwrap_or_default();
                            app.target_dir =
                                Some(std::path::PathBuf::from(appdata).join("Rime"));
                            app.action = Some(action);
                            app.state = AppState::Progress;
                        }
                    }
                } else {
                    app.action = Some(action);
                    app.state = AppState::OsSelect;
                }
            }
            KeyCode::Char('b') => {
                system::open_url(constants::APP_AUTHOR_BILIBILI);
            }
            KeyCode::Char('d') => {
                system::open_url(constants::APP_URL);
            }
            _ => {}
        }
    }

    fn handle_osselect_key(code: KeyCode, app: &mut App) {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.state = AppState::Menu;
                app.flavor_selected = None;
            }
            KeyCode::Enter => {
                if let Some(idx) = app.flavor_selected {
                    if idx < app.active_flavors.len() {
                        let flavor = app.active_flavors[idx].clone();
                        match flavor.target_dir() {
                            Ok(dir) => {
                                app.target_dir = Some(dir);
                                app.state = AppState::Progress;
                            }
                            Err(e) => {
                                app.result_message = format!("获取目录失败: {e}");
                                app.state = AppState::Result;
                            }
                        }
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = app.active_flavors.len().saturating_sub(1);
                let current = app.flavor_selected.unwrap_or(0);
                app.flavor_selected = Some((current + 1).min(max));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let current = app.flavor_selected.unwrap_or(0);
                app.flavor_selected = Some(current.saturating_sub(1));
            }
            _ => {}
        }
    }

    async fn run_action(
        action: Action,
        target_dir: &std::path::Path,
        shared: Arc<Mutex<SharedProgress>>,
    ) -> AppResult<()> {
        let cancel = AtomicBool::new(false);

        let update_download = {
            let s = Arc::clone(&shared);
            Arc::new(move |info: downloader::ProgressInfo| {
                if let Ok(mut p) = s.lock() {
                    p.downloaded = info.downloaded;
                    p.total = info.total;
                    p.percentage = info.percentage;
                    p.speed = info.speed;
                    p.phase = ProgressPhase::Download;
                    p.label = format!(
                        "下载 {:.1}% {}/{} {}/s",
                        info.percentage,
                        downloader::ProgressInfo::format_bytes(info.downloaded),
                        downloader::ProgressInfo::format_bytes(info.total),
                        downloader::ProgressInfo::format_speed(info.speed),
                    );
                }
            })
        };

        let update_extract = {
            let s = Arc::clone(&shared);
            move |p: ExtractProgress| {
                if let Ok(mut sp) = s.lock() {
                    sp.extract_current = p.current;
                    sp.extract_total = p.total;
                    sp.extract_file = p.current_file.clone();
                    sp.phase = ProgressPhase::Extract;
                    sp.label = format!("解压 {}/{}  {}", p.current, p.total, p.current_file);
                }
            }
        };

        match action {
            Action::UpdateScheme => {
                {
                    shared.lock().unwrap().label = "下载方案包...".into();
                }
                let data = downloader::download_with_progress(
                    constants::OH_MY_RIME_REPO,
                    Some(update_download),
                    &cancel,
                )
                .await?;

                {
                    shared.lock().unwrap().label = "解压方案包...".into();
                }
                updater::update_main_scheme_with_progress(
                    &data,
                    target_dir,
                    Some(Box::new(update_extract)),
                )?;
            }
            Action::UpdateModel => {
                {
                    shared.lock().unwrap().label = "下载模型...".into();
                }
                let data = downloader::download_with_progress(
                    constants::WAN_XIANG_GRA,
                    Some(update_download),
                    &cancel,
                )
                .await?;

                {
                    shared.lock().unwrap().label = "写入模型文件...".into();
                }
                updater::update_model(&data, target_dir)?;
            }
            Action::UpdateDict => {
                {
                    shared.lock().unwrap().label = "下载方案包...".into();
                }
                let data = downloader::download_with_progress(
                    constants::OH_MY_RIME_REPO,
                    Some(update_download),
                    &cancel,
                )
                .await?;

                {
                    shared.lock().unwrap().label = "提取词库...".into();
                }
                updater::update_dict_with_progress(
                    &data,
                    target_dir,
                    Some(Box::new(update_extract)),
                )?;
            }
            Action::Custom => {
                return Err(crate::error::AppError::Other("自定义更新需在 CLI 模式使用".into()));
            }
            Action::OpenBilibili | Action::OpenDocs => {}
        }

        shared.lock().unwrap().done = true;
        Ok(())
    }

    fn ui(f: &mut Frame<'_>, app: &App) {
        let area = f.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Block::default()
            .style(Style::default().fg(Color::Cyan).bold());
        let header_text = Paragraph::new(format!(
            " Oh My Rime TUI v{} - Rime 输入法配置管理工具 ",
            constants::APP_VERSION
        ))
        .block(header)
        .alignment(Alignment::Center);
        f.render_widget(header_text, chunks[0]);

        // Content
        match app.state {
            AppState::Menu => render_menu(f, chunks[1], app),
            AppState::OsSelect => render_os_select(f, chunks[1], app),
            AppState::Progress => render_progress(f, chunks[1], app),
            AppState::Result => render_result(f, chunks[1], app),
            AppState::Exit => {}
        }

        // Footer
        let footer = match app.state {
            AppState::Menu => " ↑↓ 选择  Enter 确认  q 退出  b B站  d 文档 ",
            AppState::OsSelect => " ↑↓ 选择  Enter 确认  q 返回 ",
            AppState::Progress => " Esc 取消  |  下载解压更新中... ",
            AppState::Result => " Enter 返回菜单 ",
            AppState::Exit => "",
        };
        let footer_para = Paragraph::new(Span::styled(
            footer,
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center);
        f.render_widget(footer_para, chunks[2]);
    }

    fn render_menu(f: &mut Frame<'_>, area: Rect, app: &App) {
        let items: Vec<ListItem> = menu_items()
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let prefix = if i == app.selected { " > " } else { "   " };
                let content = if i == app.selected {
                    Line::from(Span::styled(
                        format!("{prefix}{item}"),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ))
                } else {
                    Line::from(Span::styled(format!("{prefix}{item}"), Style::default().fg(Color::White)))
                };
                ListItem::new(content)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" 功能菜单 ")
                    .style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        let mut state = ListState::default();
        state.select(Some(app.selected));
        f.render_stateful_widget(list, area, &mut state);

        // Show OS and target dir info
        let info = if let Some(ref dir) = app.target_dir {
            format!(" OS: {:?}  |  {}", app.os, dir.display())
        } else {
            format!(" OS: {:?}", app.os)
        };
        let os_info = Paragraph::new(Span::styled(
            info,
            Style::default().fg(Color::Green),
        ));
        let info_area = Rect {
            x: area.x + 2,
            y: area.y + area.height.saturating_sub(2),
            width: area.width.saturating_sub(4),
            height: 1,
        };
        f.render_widget(os_info, info_area);
    }

    fn render_os_select(f: &mut Frame<'_>, area: Rect, app: &App) {
        let items: Vec<ListItem> = app
            .active_flavors
            .iter()
            .enumerate()
            .map(|(i, flavor)| {
                let prefix = if Some(i) == app.flavor_selected {
                    " > "
                } else {
                    "   "
                };
                let content = if Some(i) == app.flavor_selected {
                    Line::from(Span::styled(
                        format!("{prefix}{}", flavor.name()),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ))
                } else {
                    Line::from(Span::styled(
                        format!("{prefix}{}", flavor.name()),
                        Style::default().fg(Color::White),
                    ))
                };
                ListItem::new(content)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 选择输入法类型 ")
                .style(Style::default().fg(Color::Cyan)),
        );

        let mut state = ListState::default();
        state.select(app.flavor_selected);
        f.render_stateful_widget(list, area, &mut state);
    }

    fn render_progress(f: &mut Frame<'_>, area: Rect, app: &App) {
        let progress_state = app.progress.lock().unwrap().clone();

        // Split area: status line above, gauge in middle, detail below
        let vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // status label
                Constraint::Length(3),  // gauge
                Constraint::Length(1),  // detail
            ])
            .split(center_rect(area, area.width.min(50), 5));

        let (title, pct, label, detail) = match progress_state.phase {
            ProgressPhase::Download => {
                let p = progress_state.percentage.min(100.0) / 100.0;
                let label = format!(
                    "{:.0}%  {} / {}",
                    progress_state.percentage,
                    downloader::ProgressInfo::format_bytes(progress_state.downloaded),
                    downloader::ProgressInfo::format_bytes(progress_state.total),
                );
                let detail = if progress_state.total > 0 && progress_state.speed > 0.0 {
                    let remaining = (progress_state.total - progress_state.downloaded) as f64 / progress_state.speed;
                    let s = remaining as u64;
                    let eta = if s < 60 { format!("{}s", s) }
                        else if s < 3600 { format!("{}m{}s", s / 60, s % 60) }
                        else { format!("{}h{}m", s / 3600, (s % 3600) / 60) };
                    format!("{}  ETA: {}", downloader::ProgressInfo::format_speed(progress_state.speed), eta)
                } else {
                    String::new()
                };
                (" 下载方案包 ", p, label, detail)
            }
            ProgressPhase::Extract => {
                let p = if progress_state.extract_total > 0 {
                    progress_state.extract_current as f64 / progress_state.extract_total as f64
                } else {
                    0.0
                };
                let label = format!("{}/{} 个文件", progress_state.extract_current, progress_state.extract_total);
                let detail = if progress_state.extract_file.len() > 45 {
                    format!("...{}", &progress_state.extract_file[progress_state.extract_file.len().saturating_sub(42)..])
                } else {
                    progress_state.extract_file.clone()
                };
                (" 解压方案包 ", p, label, detail)
            }
            ProgressPhase::Idle => {
                (" 准备中 ", 0.0, progress_state.label.clone(), String::new())
            }
        };

        // Status line
        let status = Paragraph::new(Span::styled(
            title,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center);
        f.render_widget(status, vert[0]);

        // Gauge
        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(
                Style::default()
                    .fg(Color::Cyan)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .ratio(pct)
            .label(label);
        f.render_widget(gauge, vert[1]);

        // Detail line
        if !detail.is_empty() {
            let detail_p = Paragraph::new(Span::styled(detail, Style::default().fg(Color::DarkGray)))
                .alignment(Alignment::Center);
            f.render_widget(detail_p, vert[2]);
        }
    }

    fn render_result(f: &mut Frame<'_>, area: Rect, app: &App) {
        let line_count = app.result_message.lines().count() as u16;
        let height = (line_count + 2).min(area.height);

        let para = Paragraph::new(app.result_message.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" 操作结果 ")
                    .style(Style::default().fg(Color::Cyan)),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        let result_area = center_rect(area, area.width.min(50), height);
        f.render_widget(para, result_area);
    }

    fn center_rect(r: Rect, width: u16, height: u16) -> Rect {
        let x = r.x + (r.width.saturating_sub(width) / 2);
        let y = r.y + (r.height.saturating_sub(height) / 2);
        Rect { x, y, width, height }
    }
}
