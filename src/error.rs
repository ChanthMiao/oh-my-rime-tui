use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("下载失败: {0}")]
    Download(String),

    #[error("HTTP 错误: {status} - {url}")]
    Http { status: reqwest::StatusCode, url: String },

    #[error("I/O 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("ZIP 错误: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[allow(dead_code)]
    #[error("解压失败: {0}")]
    Extract(String),

    #[error("备份失败: {0}")]
    Backup(String),

    #[error("不支持的操作系统: {0}")]
    UnsupportedOs(String),

    #[allow(dead_code)]
    #[error("目录未设置")]
    DirNotSet,

    #[error("路径不安全: {path}")]
    UnsafePath { path: PathBuf },

    #[error("{0}")]
    Other(String),
}

pub type AppResult<T> = Result<T, AppError>;
