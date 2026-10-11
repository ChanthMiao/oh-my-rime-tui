use std::path::{Component, Path, PathBuf};

use zip::ZipArchive;

use crate::core::backup;
use crate::error::{AppError, AppResult};

pub type ExtractCallback = Box<dyn Fn(ExtractProgress) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct ExtractProgress {
    pub current: usize,
    pub total: usize,
    pub current_file: String,
}

/// 档案内部文件迭代 —— 避免调用方持有 ZipArchive 的借用
struct ArchiveFiles {
    names: Vec<String>,
    body: Vec<u8>,
}

impl ArchiveFiles {
    fn new(data: &[u8]) -> AppResult<Self> {
        let mut archive = ZipArchive::new(std::io::Cursor::new(data))?;
        let mut names = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            names.push(archive.by_index(i)?.name()?.into_owned());
        }
        Ok(Self {
            names,
            body: data.to_vec(),
        })
    }
}

fn extract_zip_with_progress(
    data: &[u8],
    target_dir: &Path,
    filter: Option<&str>,
    cb: Option<&ExtractCallback>,
) -> AppResult<()> {
    let af = ArchiveFiles::new(data)?;
    let total = af.names.len();

    let mut archive = ZipArchive::new(std::io::Cursor::new(&af.body))?;
    for (entry_index, name) in af.names.iter().enumerate() {
        let rel = match filter {
            Some(prefix) => match name.strip_prefix(prefix) {
                Some(rest) if !rest.is_empty() => rest,
                _ => continue,
            },
            None => name.as_str(),
        };

        let target_path = safe_join(target_dir, rel)?;
        let mut file = archive.by_index(entry_index)?;

        if file.is_dir() {
            std::fs::create_dir_all(&target_path)?;
        } else {
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            extract_file(&mut file, &target_path)?;
        }

        if let Some(ref cb) = cb {
            cb(ExtractProgress {
                current: entry_index + 1,
                total,
                current_file: rel.to_string(),
            });
        }
    }
    Ok(())
}

pub fn update_main_scheme_with_progress(
    zip_data: &[u8],
    target_dir: &Path,
    cb: Option<ExtractCallback>,
) -> AppResult<()> {
    backup::run_with_backup(target_dir, "主方案更新", || {
        std::fs::create_dir_all(target_dir)?;
        extract_zip_with_progress(zip_data, target_dir, None, cb.as_ref())
    })
}

pub fn update_dict_with_progress(
    zip_data: &[u8],
    target_dir: &Path,
    cb: Option<ExtractCallback>,
) -> AppResult<()> {
    backup::run_with_backup(target_dir, "词库更新", || {
        let dicts_dir = target_dir.join("dicts");
        std::fs::create_dir_all(&dicts_dir)?;
        extract_zip_with_progress(zip_data, &dicts_dir, Some("dicts/"), cb.as_ref())
    })
}

#[allow(dead_code)]
pub fn update_main_scheme(zip_data: &[u8], target_dir: &Path) -> AppResult<()> {
    update_main_scheme_with_progress(zip_data, target_dir, None)
}

pub fn update_model(model_data: &[u8], target_dir: &Path) -> AppResult<()> {
    if model_data.is_empty() {
        return Err(AppError::Other("模型数据无效".into()));
    }

    backup::run_with_backup(target_dir, "模型更新", || {
        std::fs::create_dir_all(target_dir)?;
        let model_path = target_dir.join("wanxiang-lts-zh-hans.gram");
        std::fs::write(&model_path, model_data)?;
        tracing::info!("模型已更新: {}", model_path.display());
        Ok(())
    })
}

#[allow(dead_code)]
pub fn update_dict(zip_data: &[u8], target_dir: &Path) -> AppResult<()> {
    update_dict_with_progress(zip_data, target_dir, None)
}

/// 路径中最深的存在祖先的规范化形式；整条路径都不存在时返回原路径。
///
/// 两侧都必须经本函数再比较：Windows 上 `canonicalize` 返回 `\\?\C:\...`
/// 形式，而尚未创建的路径无法规范化、只能退回普通形式，直接与规范化结果
/// 比较会把合法的目标目录误判为越界。
fn canonical_existing(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    loop {
        if let Ok(canonical) = current.canonicalize() {
            return canonical;
        }
        match current.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => current = parent.to_path_buf(),
            _ => return path.to_path_buf(),
        }
    }
}

fn safe_join(base: &Path, name: &str) -> AppResult<PathBuf> {
    let rel = Path::new(name);
    if rel.components().any(|c| {
        matches!(
            c,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        )
    }) {
        return Err(AppError::UnsafePath {
            path: rel.to_path_buf(),
        });
    }

    // 词法上已排除 `..`、根路径与盘符前缀；此处再解析一次链接，
    // 防止目标目录内已有的符号链接把写入带出 base 之外。
    let joined = base.join(rel);
    if !canonical_existing(&joined).starts_with(canonical_existing(base)) {
        return Err(AppError::UnsafePath {
            path: rel.to_path_buf(),
        });
    }

    Ok(joined)
}

fn extract_file<R: std::io::Read>(
    file: &mut zip::read::ZipFile<'_, R>,
    target_path: &Path,
) -> AppResult<()> {
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut out = std::fs::File::create(target_path)?;
    let mut reader = file;
    std::io::copy(&mut reader, &mut out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_test_zip() -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let options = zip::write::SimpleFileOptions::default();

        zip.add_directory("schemes/", options).unwrap();
        zip.start_file("schemes/default.yaml", options).unwrap();
        zip.write_all(b"schema: default").unwrap();
        zip.start_file("default.yaml", options).unwrap();
        zip.write_all(b"config: default").unwrap();

        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn test_update_main_scheme() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("rime");

        let zip_data = create_test_zip();
        update_main_scheme(&zip_data, &target).unwrap();

        assert!(target.join("default.yaml").exists());
        assert!(target.join("schemes/default.yaml").exists());
    }

    #[test]
    fn test_update_model() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("rime");

        update_model(b"model data", &target).unwrap();
        assert!(target.join("wanxiang-lts-zh-hans.gram").exists());
    }

    #[test]
    fn test_safe_join_rejects_absolute() {
        let base = Path::new("/tmp/rime");
        let result = safe_join(base, "/etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_safe_join_rejects_parent_escape() {
        let base = Path::new("/tmp/rime");
        let result = safe_join(base, "../../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_safe_join_accepts_normal() {
        let base = Path::new("/tmp/rime");
        let result = safe_join(base, "dicts/cn.dict.yaml");
        assert!(result.is_ok());
    }

    /// 目标目录已存在、而待写入路径尚未创建时也必须通过：
    /// Windows 上 `canonicalize(base)` 会带上 `\\?\` 前缀，未创建的路径
    /// 则保持原样，二者直接比较会误判为越界。
    #[test]
    fn test_safe_join_accepts_path_that_does_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("rime");
        std::fs::create_dir_all(&base).unwrap();

        assert_eq!(
            safe_join(&base, "schemes/default.yaml").unwrap(),
            base.join("schemes/default.yaml")
        );
        assert!(safe_join(&base, "schemes/").is_ok());
    }

    /// Windows 上 `C:evil` 是盘符相对路径，`join` 会丢弃 base。
    #[cfg(windows)]
    #[test]
    fn test_safe_join_rejects_drive_relative() {
        let base = Path::new(r"C:\rime");
        assert!(safe_join(base, r"C:evil").is_err());
    }

    /// 目标目录内已存在的符号链接不能把写入带出 base 之外。
    #[cfg(unix)]
    #[test]
    fn test_safe_join_rejects_symlink_escape() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("rime");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, base.join("link")).unwrap();

        assert!(safe_join(&base, "link/evil.yaml").is_err());
        assert!(safe_join(&base, "link").is_err());
    }

    #[test]
    fn test_extract_with_progress() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("rime");
        let zip_data = create_test_zip();

        let files = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        {
            let files = std::sync::Arc::clone(&files);
            let cb: ExtractCallback = Box::new(move |p| {
                files.lock().unwrap().push(p.current_file);
            });

            update_main_scheme_with_progress(&zip_data, &target, Some(cb)).unwrap();
        }

        let recorded = files.lock().unwrap();
        assert!(!recorded.is_empty(), "progress callback should be called");
    }
}
