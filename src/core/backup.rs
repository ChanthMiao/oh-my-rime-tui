use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::{AppError, AppResult};
use crate::core::constants::BACKUP_KEEP_COUNT;

pub fn create_backup(target_dir: &Path) -> AppResult<(PathBuf, bool)> {
    if !target_dir.exists() {
        return Ok((PathBuf::new(), false));
    }
    if !target_dir.is_dir() {
        return Err(AppError::Backup(format!(
            "目标路径不是目录: {}",
            target_dir.display()
        )));
    }

    let backup_root = backup_root_dir(target_dir);
    std::fs::create_dir_all(&backup_root)?;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let backup_dir = backup_root.join(format!("backup-{timestamp}"));
    copy_dir(target_dir, &backup_dir)?;

    tracing::info!("已创建备份: {}", backup_dir.display());
    Ok((backup_dir, true))
}

pub fn restore_backup(target_dir: &Path, backup_dir: &Path) -> AppResult<()> {
    if target_dir.exists() {
        std::fs::remove_dir_all(target_dir)?;
    }
    copy_dir(backup_dir, target_dir)?;
    tracing::info!("已恢复备份: {} -> {}", backup_dir.display(), target_dir.display());
    Ok(())
}

pub fn prune_backups(target_dir: &Path) -> AppResult<()> {
    let backup_root = backup_root_dir(target_dir);
    if !backup_root.exists() {
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(&backup_root)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();

    entries.sort_by_key(|e| e.file_name());

    if entries.len() <= BACKUP_KEEP_COUNT {
        return Ok(());
    }

    for entry in entries.iter().take(entries.len() - BACKUP_KEEP_COUNT) {
        let path = entry.path();
        std::fs::remove_dir_all(&path)?;
        tracing::info!("已清理旧备份: {}", path.display());
    }
    Ok(())
}

pub fn run_with_backup<F>(target_dir: &Path, operation: &str, f: F) -> AppResult<()>
where
    F: FnOnce() -> AppResult<()>,
{
    let (backup_dir, has_backup) = create_backup(target_dir)?;

    if let Err(err) = f() {
        if has_backup {
            tracing::error!("{operation} 失败，正在恢复备份...");
            restore_backup(target_dir, &backup_dir)?;
        }
        return Err(err);
    }

    prune_backups(target_dir)?;
    Ok(())
}

fn backup_root_dir(target_dir: &Path) -> PathBuf {
    let parent = target_dir.parent().unwrap_or(Path::new("."));
    let dir_name = target_dir
        .file_name()
        .map(|n| {
            let mut s = n.to_string_lossy().to_string();
            s.push_str(".backups");
            s
        })
        .unwrap_or_else(|| "backups".to_string());
    parent.join(dir_name)
}

fn copy_dir(src: &Path, dst: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if file_type.is_dir() {
            copy_dir(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_prune_restore() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("rime");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("test.txt"), b"hello").unwrap();

        let (backup_dir, has_backup) = create_backup(&target).unwrap();
        assert!(has_backup);
        assert!(backup_dir.exists());
        assert!(backup_dir.join("test.txt").exists());

        std::fs::write(target.join("new.txt"), b"world").unwrap();

        restore_backup(&target, &backup_dir).unwrap();
        assert!(target.join("test.txt").exists());
        assert!(!target.join("new.txt").exists());
    }

    #[test]
    fn test_prune_old_backups() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("rime");
        std::fs::create_dir_all(&target).unwrap();

        let root = backup_root_dir(&target);
        for i in 0..5 {
            std::fs::create_dir_all(root.join(format!("backup-{i}"))).unwrap();
        }

        prune_backups(&target).unwrap();

        let remaining: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .collect();

        assert!(remaining.len() <= BACKUP_KEEP_COUNT);
    }

    #[test]
    fn test_nonexistent_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nonexistent");
        let (_, has_backup) = create_backup(&target).unwrap();
        assert!(!has_backup);
    }
}
