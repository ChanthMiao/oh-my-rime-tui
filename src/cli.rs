use std::io::{self, BufRead, Write};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::core::downloader::{self, ProgressInfo};
use crate::core::system::{self, OsType};
use crate::core::updater;
use crate::error::AppResult;

pub async fn run_interactive() -> AppResult<()> {
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
            target_dir = Some(select_target_dir(&os, &mut reader)?);
        }

        match choice.as_str() {
            "1" => handle_update_scheme(target_dir.as_ref().unwrap()).await?,
            "2" => handle_update_model(target_dir.as_ref().unwrap()).await?,
            "3" => handle_update_dict(target_dir.as_ref().unwrap()).await?,
            "4" => handle_custom_update(target_dir.as_ref().unwrap(), &mut reader).await?,
            "b" => {
                system::open_url(crate::core::constants::APP_AUTHOR_BILIBILI);
            }
            "d" => {
                system::open_url(crate::core::constants::APP_URL);
            }
            _ => println!("无效选项，请重新输入"),
        }
    }

    Ok(())
}

fn show_menu(os: &OsType, target_dir: Option<&std::path::PathBuf>) {
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

fn select_target_dir(os: &OsType, reader: &mut impl BufRead) -> AppResult<std::path::PathBuf> {
    if *os == OsType::Windows {
        match system::get_windows_rime_dir() {
            Ok(dir) => {
                println!("从注册表获取 Rime 目录: {}", dir.display());
                return Ok(dir);
            }
            Err(e) => {
                println!("注册表读取失败: {e}，使用默认目录");
                let appdata = std::env::var("APPDATA").unwrap_or_default();
                return Ok(std::path::PathBuf::from(appdata).join("Rime"));
            }
        }
    }

    let flavors = if *os == OsType::Linux {
        system::get_linux_flavors()
    } else {
        system::get_darwin_flavors()
    };

    println!("\n请选择 Rime 输入法类型:");
    for (i, flavor) in flavors.iter().enumerate() {
        println!("  [{}] {}", i + 1, flavor.name());
    }

    print!("请输入选项 (1-{}): ", flavors.len());
    io::stdout().flush()?;
    let mut input = String::new();
    reader.read_line(&mut input)?;
    let n: usize = input.trim().parse().unwrap_or(1);
    let idx = n.max(1).min(flavors.len()) - 1;

    let dir = flavors[idx].target_dir()?;
    println!("目标地址: {}", dir.display());
    Ok(dir)
}

fn print_progress(info: ProgressInfo) {
    if info.completed {
        println!();
        return;
    }
    if info.total > 0 {
        let bar_width = 40;
        let filled = (info.percentage * bar_width as f64 / 100.0) as usize;
        let bar: String = (0..bar_width)
            .map(|i| if i < filled { '█' } else { '░' })
            .collect();
        let eta = info.format_eta();
        print!(
            "\r[{}] {:.1}% {}/{} {}/s ETA: {}",
            bar,
            info.percentage,
            ProgressInfo::format_bytes(info.downloaded),
            ProgressInfo::format_bytes(info.total),
            ProgressInfo::format_speed(info.speed),
            if eta.is_empty() { "-" } else { &eta }
        );
    } else {
        print!(
            "\r下载中... {}",
            ProgressInfo::format_bytes(info.downloaded)
        );
    }
    io::stdout().flush().ok();
}

async fn handle_update_scheme(target_dir: &std::path::Path) -> AppResult<()> {
    let cancel = AtomicBool::new(false);
    let url = crate::core::constants::OH_MY_RIME_REPO;

    println!("正在下载方案包...");
    let data = downloader::download_with_progress(
        url,
        Some(Arc::new(print_progress)),
        &cancel,
    ).await?;

    println!("正在更新主方案...");
    updater::update_main_scheme(&data, target_dir)?;
    println!("✅ 主方案更新完成！");
    Ok(())
}

async fn handle_update_model(target_dir: &std::path::Path) -> AppResult<()> {
    let cancel = AtomicBool::new(false);
    let url = crate::core::constants::WAN_XIANG_GRA;

    println!("正在下载模型...");
    let data = downloader::download_with_progress(
        url,
        Some(Arc::new(print_progress)),
        &cancel,
    ).await?;

    println!("正在更新模型...");
    updater::update_model(&data, target_dir)?;
    println!("✅ 模型更新完成！");
    Ok(())
}

async fn handle_update_dict(target_dir: &std::path::Path) -> AppResult<()> {
    let cancel = AtomicBool::new(false);
    let url = crate::core::constants::OH_MY_RIME_REPO;

    println!("正在下载方案包以提取词库...");
    let data = downloader::download_with_progress(
        url,
        Some(Arc::new(print_progress)),
        &cancel,
    ).await?;

    println!("正在更新词库...");
    updater::update_dict(&data, target_dir)?;
    println!("✅ 词库更新完成！");
    Ok(())
}

async fn handle_custom_update(target_dir: &std::path::Path, reader: &mut impl BufRead) -> AppResult<()> {
    println!("\n自定义更新功能:");
    println!("  粘贴方案打包的 zip 文件 URL => 将下载并替换当前 Rime 配置目录下的文件");
    println!("  粘贴模型的 gram 文件 URL => 将下载并替换当前 Rime 配置目录下的同名文件");
    println!(
        "  例如: https://github.com/Mintimate/oh-my-rime/archive/refs/heads/main.zip"
    );
    print!("请粘贴 URL: ");
    io::stdout().flush()?;

    let mut url = String::new();
    reader.read_line(&mut url)?;
    let url = url.trim().to_string();

    let lower = url.to_lowercase();
    let is_zip = lower.ends_with(".zip");
    let is_gram = lower.ends_with(".gram");

    if !is_zip && !is_gram {
        println!("不支持的文件类型，请提供 zip 或 gram 文件的 URL");
        return Ok(());
    }

    let cancel = AtomicBool::new(false);
    println!("正在下载...");
    let data = downloader::download_with_progress(
        &url,
        Some(Arc::new(print_progress)),
        &cancel,
    ).await?;

    if is_zip {
        updater::update_main_scheme(&data, target_dir)?;
        println!("✅ 自定义方案更新完成！");
    } else {
        updater::update_model(&data, target_dir)?;
        println!("✅ 自定义模型更新完成！");
    }

    Ok(())
}
