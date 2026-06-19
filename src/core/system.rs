use std::path::PathBuf;
use std::process::Command;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OsType {
    Windows,
    Linux,
    Darwin,
    Unknown,
}

#[derive(Debug, Clone)]
pub enum RimeFlavor {
    Ibus,
    Fcitx5,
    Fcitx5Flatpak,
    Squirrel,
    #[allow(dead_code)]
    Weasel,
    #[allow(dead_code)]
    Custom(PathBuf),
}

impl RimeFlavor {
    pub fn name(&self) -> &str {
        match self {
            RimeFlavor::Ibus => "iBus",
            RimeFlavor::Fcitx5 => "Fcitx5",
            RimeFlavor::Fcitx5Flatpak => "Fcitx5-Flatpak",
            RimeFlavor::Squirrel => "鼠须管",
            RimeFlavor::Weasel => "小狼毫 (Weasel)",
            RimeFlavor::Custom(_) => "自定义",
        }
    }

    pub fn target_dir(&self) -> AppResult<PathBuf> {
        match self {
            RimeFlavor::Custom(p) => Ok(expand_homedir(p)),
            _ => default_target_dir(self.clone()),
        }
    }
}

pub fn detect_os() -> OsType {
    match std::env::consts::OS {
        "windows" => OsType::Windows,
        "linux" => OsType::Linux,
        "macos" => OsType::Darwin,
        _ => OsType::Unknown,
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    pub fn get_windows_rime_dir() -> AppResult<PathBuf> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey(r"Software\Rime\Weasel")
            .map_err(|e| AppError::Other(format!("无法打开注册表键: {e}")))?;
        let dir: String = key
            .get_value("RimeUserDir")
            .map_err(|e| AppError::Other(format!("无法读取 RimeUserDir: {e}")))?;
        if dir.is_empty() {
            Ok(PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("Rime"))
        } else {
            Ok(PathBuf::from(dir))
        }
    }

    pub fn get_linux_flavors() -> Vec<RimeFlavor> {
        vec![
            RimeFlavor::Custom(PathBuf::from("C:\\ProgramData\\Rime")),
        ]
    }

    pub fn get_darwin_flavors() -> Vec<RimeFlavor> {
        vec![]
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub fn get_windows_rime_dir() -> AppResult<PathBuf> {
        Err(AppError::UnsupportedOs(
            "Windows 注册表仅在 Windows 上可用".into(),
        ))
    }

    pub fn get_linux_flavors() -> Vec<RimeFlavor> {
        vec![
            RimeFlavor::Ibus,
            RimeFlavor::Fcitx5,
            RimeFlavor::Fcitx5Flatpak,
        ]
    }

    pub fn get_darwin_flavors() -> Vec<RimeFlavor> {
        vec![RimeFlavor::Squirrel, RimeFlavor::Fcitx5]
    }
}

pub use platform::*;

fn default_target_dir(flavor: RimeFlavor) -> AppResult<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| AppError::Other("无法获取用户主目录".into()))?;

    let path = match flavor {
        RimeFlavor::Ibus => PathBuf::from(home).join(".config/ibus/rime"),
        RimeFlavor::Fcitx5 => PathBuf::from(home).join(".local/share/fcitx5/rime"),
        RimeFlavor::Fcitx5Flatpak => {
            PathBuf::from(home).join(".var/app/org.fcitx.Fcitx5/data/fcitx5/rime")
        }
        RimeFlavor::Squirrel => PathBuf::from(home).join("Library/Rime"),
        RimeFlavor::Weasel => PathBuf::from(home).join("AppData/Roaming/Rime"),
        RimeFlavor::Custom(p) => p,
    };
    Ok(path)
}

pub fn expand_homedir(path: &std::path::Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str == "~" {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| path.to_path_buf())
    } else if path_str.starts_with("~/") || path_str.starts_with("~\\") {
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            PathBuf::from(home).join(&path_str[2..])
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    }
}

pub fn open_url(url: &str) {
    let cmd = match detect_os() {
        OsType::Windows => Command::new("cmd").args(["/c", "start", url]).spawn(),
        OsType::Linux => Command::new("xdg-open").arg(url).spawn(),
        OsType::Darwin => Command::new("open").arg(url).spawn(),
        OsType::Unknown => return,
    };
    if let Err(e) = cmd {
        tracing::warn!("打开浏览器失败: {e}");
    }
}

#[allow(dead_code)]
pub fn open_folder(path: &std::path::Path) {
    let cmd = match detect_os() {
        OsType::Windows => Command::new("explorer").arg(path).spawn(),
        OsType::Linux => Command::new("xdg-open").arg(path).spawn(),
        OsType::Darwin => Command::new("open").arg(path).spawn(),
        OsType::Unknown => return,
    };
    if let Err(e) = cmd {
        tracing::warn!("打开目录失败: {e}");
    }
}
