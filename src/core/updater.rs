use std::io::Read;
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::core::backup;
use crate::error::{AppError, AppResult};

pub fn update_main_scheme(zip_data: &[u8], target_dir: &Path) -> AppResult<()> {
    backup::run_with_backup(target_dir, "主方案更新", || {
        std::fs::create_dir_all(target_dir)?;
        let mut archive = ZipArchive::new(std::io::Cursor::new(zip_data))?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let target_path = safe_join(target_dir, file.name())?;

            if file.is_dir() {
                std::fs::create_dir_all(&target_path)?;
            } else {
                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                extract_file(&mut file, &target_path)?;
                tracing::info!("解压: {}", target_path.display());
            }
        }
        Ok(())
    })
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

pub fn update_dict(zip_data: &[u8], target_dir: &Path) -> AppResult<()> {
    backup::run_with_backup(target_dir, "词库更新", || {
        let dicts_dir = target_dir.join("dicts");
        std::fs::create_dir_all(&dicts_dir)?;

        let mut archive = ZipArchive::new(std::io::Cursor::new(zip_data))?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name();

            if !name.starts_with("dicts/") {
                continue;
            }

            let relative = name.strip_prefix("dicts/").unwrap_or(name);
            if relative.is_empty() {
                continue;
            }

            let target_path = safe_join(&dicts_dir, relative)?;

            if file.is_dir() {
                std::fs::create_dir_all(&target_path)?;
            } else {
                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                extract_file(&mut file, &target_path)?;
                tracing::info!("词库: {}", target_path.display());
            }
        }
        Ok(())
    })
}

fn safe_join(base: &Path, name: &str) -> AppResult<PathBuf> {
    let clean = PathBuf::from(name);
    if clean.has_root() || clean.components().any(|c| c == std::path::Component::ParentDir) {
        return Err(AppError::UnsafePath {
            path: clean,
        });
    }

    let joined = base.join(&clean);
    let canonical_base = base
        .canonicalize()
        .unwrap_or_else(|_| base.to_path_buf());
    let canonical_joined = joined
        .canonicalize()
        .unwrap_or_else(|_| joined.clone());

    if !canonical_joined.starts_with(&canonical_base) {
        return Err(AppError::UnsafePath {
            path: clean,
        });
    }

    Ok(joined)
}

fn extract_file(file: &mut zip::read::ZipFile<'_>, target_path: &Path) -> AppResult<()> {
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut out = std::fs::File::create(target_path)?;
    let mut reader = file;
    std::io::copy(&mut reader, &mut out)?;
    Ok(())
}

#[allow(dead_code)]
fn extract_file_direct<R: Read + ?Sized>(reader: &mut R, target_path: &Path) -> AppResult<()> {
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::fs::File::create(target_path)?;
    std::io::copy(reader, &mut out)?;
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
}
