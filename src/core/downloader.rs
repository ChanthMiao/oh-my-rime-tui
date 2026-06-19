use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Instant;

use crate::core::constants::USER_AGENT;
use crate::error::{AppError, AppResult};

pub type ProgressCallback = Arc<dyn Fn(ProgressInfo) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct ProgressInfo {
    pub downloaded: i64,
    pub total: i64,
    pub percentage: f64,
    pub speed: f64,
    pub completed: bool,
    #[allow(dead_code)]
    pub error: Option<String>,
}

impl ProgressInfo {
    pub fn format_bytes(bytes: i64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
        let mut size = bytes as f64;
        let mut unit_idx = 0;
        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }
        format!("{:.1} {}", size, UNITS[unit_idx])
    }

    pub fn format_speed(speed: f64) -> String {
        Self::format_bytes(speed as i64) + "/s"
    }

    pub fn format_eta(&self) -> String {
        if self.speed <= 0.0 || self.downloaded >= self.total {
            return String::new();
        }
        let remaining = (self.total - self.downloaded) as f64 / self.speed;
        let secs = remaining as u64;
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m{}s", secs / 60, secs % 60)
        } else {
            format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
        }
    }
}

#[allow(dead_code)]
pub async fn download(url: &str) -> AppResult<Vec<u8>> {
    download_with_progress(url, None, &AtomicBool::new(false)).await
}

pub async fn download_with_progress(
    url: &str,
    progress: Option<ProgressCallback>,
    cancel: &AtomicBool,
) -> AppResult<Vec<u8>> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| AppError::Download(format!("创建 HTTP 客户端失败: {e}")))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Download(format!("请求失败: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::Http {
            status: response.status(),
            url: url.to_string(),
        });
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("text/html") {
        return Err(AppError::Download(
            "下载内容是 HTML 页面，链接可能无效或需要权限认证".into(),
        ));
    }

    let total = response.content_length().unwrap_or(0) as i64;
    let downloaded = Arc::new(AtomicI64::new(0));
    let start = Instant::now();

    let stream = response.bytes_stream();
    let mut data = Vec::with_capacity(total.max(0) as usize);

    use futures_util::StreamExt;
    futures_util::pin_mut!(stream);

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Download("下载已取消".into()));
        }

        let chunk = chunk.map_err(|e| AppError::Download(format!("流读取失败: {e}")))?;
        data.extend_from_slice(&chunk);

        let current = downloaded.fetch_add(chunk.len() as i64, Ordering::Relaxed) + chunk.len() as i64;
        let elapsed = start.elapsed().as_secs_f64();
        let speed = if elapsed > 0.0 {
            current as f64 / elapsed
        } else {
            0.0
        };
        let pct = if total > 0 {
            current as f64 / total as f64 * 100.0
        } else {
            0.0
        };

        if let Some(ref cb) = progress {
            cb(ProgressInfo {
                downloaded: current,
                total,
                percentage: pct,
                speed,
                completed: false,
                error: None,
            });
        }
    }

    if let Some(ref cb) = progress {
        cb(ProgressInfo {
            downloaded: data.len() as i64,
            total: total.max(data.len() as i64),
            percentage: 100.0,
            speed: 0.0,
            completed: true,
            error: None,
        });
    }

    Ok(data)
}
