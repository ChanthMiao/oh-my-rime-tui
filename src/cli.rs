use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::core::downloader::{self, ProgressInfo};
use crate::core::system::{self, OsType};
use crate::core::updater::{self, ExtractCallback, ExtractProgress};
use crate::error::AppResult;

// ── interactive menu ──────────────────────────────────────────

pub async fn run_menu() -> AppResult<()> {
    let os = system::detect_os();
    if os == OsType::Unknown {
        eprintln!("无法识别当前操作系统，请确保在支持的操作系统上运行");
        std::process::exit(1);
    }
    println!("当前操作系统: {:?}", os);

    let mut target_dir = None;
    let stdin = io::stdin();
    let mut reader = stdin.lock();

    loop {
        show_menu(&os, target_dir.as_ref());

        print!("请输入选项: ");
        io::stdout().flush()?;

        let mut input = String::new();
        reader.read_line(&mut input)?;
        let choice = input.trim().to_lowercase();

        if choice == "q" {
            println!("感谢使用！记得更新后，重新部署方案以使更改生效");
            break;
        }

        if target_dir.is_none() && choice != "b" && choice != "d" {
            target_dir = Some(resolve_target_dir(&os, &mut reader)?);
        }

        match choice.as_str() {
            "1" => run_update_scheme(target_dir.as_ref().unwrap()).await?,
            "2" => run_update_model(target_dir.as_ref().unwrap()).await?,
            "3" => run_update_dict(target_dir.as_ref().unwrap()).await?,
            "4" => run_custom_update(target_dir.as_ref().unwrap(), &mut reader).await?,
            "b" => system::open_url(crate::core::constants::APP_AUTHOR_BILIBILI),
            "d" => system::open_url(crate::core::constants::APP_URL),
            _ => println!("无效选项，请重新输入"),
        }
    }

    Ok(())
}

// ── direct (non-interactive) entry points ─────────────────────

pub async fn run_direct_scheme(target_dir: &std::path::Path) -> AppResult<()> {
    run_update_scheme(target_dir).await
}

pub async fn run_direct_model(target_dir: &std::path::Path) -> AppResult<()> {
    run_update_model(target_dir).await
}

pub async fn run_direct_dict(target_dir: &std::path::Path) -> AppResult<()> {
    run_update_dict(target_dir).await
}

pub async fn run_direct_custom(target_dir: &std::path::Path, url: &str) -> AppResult<()> {
    let lower = url.to_lowercase();
    let is_zip = lower.ends_with(".zip");
    let is_gram = lower.ends_with(".gram");
    if !is_zip && !is_gram {
        eprintln!("不支持的文件类型，请提供 zip 或 gram 文件的 URL");
        return Ok(());
    }

    let cancel = AtomicBool::new(false);
    eprintln!("正在下载...");
    let data = downloader::download_with_progress(
        url,
        Some(Arc::new(print_download_progress)),
        &cancel,
    )
    .await?;

    if is_zip {
        eprintln!("正在解压...");
        updater::update_main_scheme_with_progress(&data, target_dir, Some(make_extract_callback()))?;
        eprintln!();
        eprintln!("自定义方案更新完成");
    } else {
        eprintln!("正在写入模型文件...");
        updater::update_model(&data, target_dir)?;
        eprintln!("自定义模型更新完成");
    }
    Ok(())
}

// ── target dir resolution ─────────────────────────────────────

/// Resolve Rime target directory once. On Windows uses registry;
/// on Linux/macOS asks for flavor. Returns error on unknown OS.
pub fn resolve_target_dir_interactive() -> AppResult<PathBuf> {
    let os = system::detect_os();
    if os == OsType::Unknown {
        return Err(crate::error::AppError::UnsupportedOs(
            "无法识别当前操作系统".into(),
        ));
    }
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    resolve_target_dir(&os, &mut reader)
}

fn resolve_target_dir(os: &OsType, reader: &mut impl BufRead) -> AppResult<PathBuf> {
    if *os == OsType::Windows {
        match system::get_windows_rime_dir() {
            Ok(dir) => {
                eprintln!("Rime 目录: {}", dir.display());
                return Ok(dir);
            }
            Err(e) => {
                eprintln!("注册表读取失败: {e}");
                let appdata = std::env::var("APPDATA").unwrap_or_default();
                let dir = PathBuf::from(appdata).join("Rime");
                eprintln!("使用默认目录: {}", dir.display());
                return Ok(dir);
            }
        }
    }

    let flavors = if *os == OsType::Linux {
        system::get_linux_flavors()
    } else {
        system::get_darwin_flavors()
    };

    eprintln!("请选择 Rime 输入法类型:");
    for (i, flavor) in flavors.iter().enumerate() {
        eprintln!("  [{}] {}", i + 1, flavor.name());
    }

    eprint!("请输入选项 (1-{}): ", flavors.len());
    io::stdout().flush()?;
    let mut input = String::new();
    reader.read_line(&mut input)?;
    let n: usize = input.trim().parse().unwrap_or(1);
    let idx = n.max(1).min(flavors.len()) - 1;

    let dir = flavors[idx].target_dir()?;
    eprintln!("目标地址: {}", dir.display());
    Ok(dir)
}

// ── menu UI ───────────────────────────────────────────────────

fn show_menu(os: &OsType, target_dir: Option<&PathBuf>) {
    println!("\n{}", "=".repeat(60));
    println!(
        " {} v{} - by {}",
        crate::core::constants::APP_NAME,
        crate::core::constants::APP_VERSION,
        crate::core::constants::APP_AUTHOR
    );
    println!("{}", "=".repeat(60));
    println!("OS: {:?}", os);
    if let Some(dir) = target_dir {
        println!("目标目录: {}", dir.display());
    }
    println!("\n功能选项:");
    println!("  [1] 更新薄荷方案              [2] 更新万象模型");
    println!("  [3] 更新万象词库(Lite版)      [4] 自定义更新");
    println!("\n其他选项:");
    println!("  [b] 打开作者 Bilibili");
    println!("  [d] 打开薄荷输入法文档");
    println!("  [q] 退出程序");
    println!("{}", "-".repeat(60));
}

// ── progress bars ─────────────────────────────────────────────

fn print_download_progress(info: ProgressInfo) {
    if info.completed {
        eprintln!();
        return;
    }
    if info.total > 0 {
        let bar_width = 30;
        let filled = (info.percentage * bar_width as f64 / 100.0) as usize;
        let bar: String = (0..bar_width)
            .map(|i| if i < filled { '█' } else { '░' })
            .collect();
        let eta = info.format_eta();
        eprint!(
            "\r  [{bar}] {:.1}% {} / {} {}/s ETA: {}",
            info.percentage,
            ProgressInfo::format_bytes(info.downloaded),
            ProgressInfo::format_bytes(info.total),
            ProgressInfo::format_speed(info.speed),
            if eta.is_empty() { "-" } else { &eta },
        );
    } else {
        eprint!(
            "\r  下载中... {}",
            ProgressInfo::format_bytes(info.downloaded),
        );
    }
    io::stdout().flush().ok();
}

fn print_extract_progress(p: ExtractProgress) {
    if p.total > 0 {
        let bar_width = 25;
        let pct = p.current as f64 / p.total as f64;
        let filled = (pct * bar_width as f64) as usize;
        let bar: String = (0..bar_width)
            .map(|i| if i < filled { '█' } else { '░' })
            .collect();
        let mut name = p.current_file;
        if name.len() > 35 {
            name = format!("...{}", &name[name.len().saturating_sub(32)..]);
        }
        eprint!("\r  [{bar}] {}/{}  {name}", p.current, p.total);
    } else {
        eprint!("\r  解压中...");
    }
    io::stdout().flush().ok();
}

fn make_extract_callback() -> ExtractCallback {
    Box::new(print_extract_progress)
}

// ── action handlers ───────────────────────────────────────────

async fn run_update_scheme(target_dir: &std::path::Path) -> AppResult<()> {
    let cancel = AtomicBool::new(false);
    let url = crate::core::constants::OH_MY_RIME_REPO;

    eprintln!("正在下载方案包...");
    let data = downloader::download_with_progress(
        url,
        Some(Arc::new(print_download_progress)),
        &cancel,
    )
    .await?;

    eprintln!("正在解压方案包...");
    updater::update_main_scheme_with_progress(&data, target_dir, Some(make_extract_callback()))?;
    eprintln!();
    eprintln!("主方案更新完成");
    Ok(())
}

async fn run_update_model(target_dir: &std::path::Path) -> AppResult<()> {
    let cancel = AtomicBool::new(false);
    let url = crate::core::constants::WAN_XIANG_GRA;

    eprintln!("正在下载模型...");
    let data = downloader::download_with_progress(
        url,
        Some(Arc::new(print_download_progress)),
        &cancel,
    )
    .await?;

    eprintln!("正在写入模型文件...");
    updater::update_model(&data, target_dir)?;
    eprintln!("模型更新完成");
    Ok(())
}

async fn run_update_dict(target_dir: &std::path::Path) -> AppResult<()> {
    let cancel = AtomicBool::new(false);
    let url = crate::core::constants::OH_MY_RIME_REPO;

    eprintln!("正在下载方案包以提取词库...");
    let data = downloader::download_with_progress(
        url,
        Some(Arc::new(print_download_progress)),
        &cancel,
    )
    .await?;

    eprintln!("正在提取词库...");
    updater::update_dict_with_progress(&data, target_dir, Some(make_extract_callback()))?;
    eprintln!();
    eprintln!("词库更新完成");
    Ok(())
}

async fn run_custom_update(
    target_dir: &std::path::Path,
    reader: &mut impl BufRead,
) -> AppResult<()> {
    println!("\n自定义更新功能:");
    println!("  粘贴 zip/gram 文件的 URL，将下载并替换同名文件");
    print!("请粘贴 URL: ");
    io::stdout().flush()?;

    let mut url = String::new();
    reader.read_line(&mut url)?;
    let url = url.trim().to_string();

    let lower = url.to_lowercase();
    let is_zip = lower.ends_with(".zip");
    let is_gram = lower.ends_with(".gram");
    if !is_zip && !is_gram {
        println!("不支持的文件类型");
        return Ok(());
    }

    let cancel = AtomicBool::new(false);
    println!("正在下载...");
    let data = downloader::download_with_progress(
        &url,
        Some(Arc::new(print_download_progress)),
        &cancel,
    )
    .await?;

    if is_zip {
        println!("正在解压...");
        updater::update_main_scheme_with_progress(&data, target_dir, Some(make_extract_callback()))?;
        println!();
        println!("自定义方案更新完成");
    } else {
        println!("正在写入模型文件...");
        updater::update_model(&data, target_dir)?;
        println!("自定义模型更新完成");
    }
    Ok(())
}
