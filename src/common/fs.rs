// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 文件写入工具

/// 原子写文件：写临时文件 → fsync → rename 替换 → fsync 父目录。
/// 同目录 rename 在 POSIX 上是原子操作，避免写半截导致目标文件损坏。
pub(crate) fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));

    // 写临时文件并落盘
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    // 原子替换目标文件
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp); // 清理残留临时文件
        return Err(e);
    }

    // fsync 父目录，确保 rename 元数据落盘（容器持久化卷必须）
    if let Ok(dir_file) = std::fs::File::open(dir) {
        let _ = dir_file.sync_all();
    }
    Ok(())
}
