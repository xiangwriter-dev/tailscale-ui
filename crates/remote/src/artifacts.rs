use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tailtask_core::remote::{validate_relative, AgentStore, Artifact, RemoteTask};

const MAX_FILE: u64 = 500 * 1024 * 1024;
const MAX_TOTAL: u64 = 1024 * 1024 * 1024;

pub async fn collect(
    store: &AgentStore,
    task: &RemoteTask,
    data_dir: &Path,
    roots: &[PathBuf],
) -> Result<(), String> {
    let Some(relative) = task.request.result_directory.clone() else {
        return Ok(());
    };
    let cwd = crate::identity::cwd_in_roots(&task.request.cwd, roots).await;
    let root = data_dir.join("artifacts");
    let task_id = task.id.clone();
    let result = tokio::task::spawn_blocking(move || {
        cwd.and_then(|cwd| snapshot(&cwd, &relative, &root, &task_id))
    })
    .await
    .map_err(|e| e.to_string())?;
    let mut tx = store
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|e| e.to_string())?;
    let (status, message) = match result {
        Ok(artifacts) => {
            for a in artifacts {
                sqlx::query("INSERT INTO agent_artifacts(id,task_id,name,size,sha256,storage_name) VALUES(?,?,?,?,?,?)").bind(&a.id).bind(&a.task_id).bind(&a.name).bind(a.size as i64).bind(&a.sha256).bind(&a.id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
            }
            ("complete", "结果文件已复制并生成校验清单".to_string())
        }
        Err(error) => ("incomplete", format!("结果收集不完整：{error}")),
    };
    sqlx::query("UPDATE agent_tasks SET result_status=? WHERE id=?")
        .bind(status)
        .bind(&task.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO agent_events(task_id,seq,kind,text,occurred_at) SELECT ?,COALESCE(MAX(seq),0)+1,'results',?,? FROM agent_events WHERE task_id=?").bind(&task.id).bind(message).bind(tailtask_core::now()).bind(&task.id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())
}

fn snapshot(
    cwd: &Path,
    relative: &str,
    destination: &Path,
    task_id: &str,
) -> Result<Vec<Artifact>, String> {
    validate_relative(relative)?;
    let cwd = cwd.canonicalize().map_err(|e| e.to_string())?;
    let mut source = cwd.clone();
    for component in relative.split('/') {
        source.push(component);
        let metadata = std::fs::symlink_metadata(&source).map_err(|e| e.to_string())?;
        if linked(&metadata) || !metadata.is_dir() {
            return Err("RESULT_DIRECTORY_NOT_REGULAR".into());
        }
    }
    let source = source.canonicalize().map_err(|e| e.to_string())?;
    if !source.starts_with(&cwd) {
        return Err("RESULT_PATH_ESCAPE".into());
    }
    std::fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    let mut pending = vec![source.clone()];
    let mut artifacts = Vec::new();
    let mut total = 0u64;
    let mut visited = 0;
    let result = (|| {
        while let Some(dir) = pending.pop() {
            if !dir
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(&source)
            {
                return Err("RESULT_PATH_ESCAPE".into());
            }
            for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
                visited += 1;
                if visited > 1000 {
                    return Err("RESULT_ENTRY_LIMIT".into());
                }
                let path = entry.map_err(|e| e.to_string())?.path();
                let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if linked(&metadata) {
                    return Err("RESULT_SYMLINK_REJECTED".into());
                }
                if metadata.is_dir() {
                    pending.push(path);
                    continue;
                }
                if !metadata.is_file() {
                    return Err("RESULT_SPECIAL_FILE_REJECTED".into());
                }
                if artifacts.len() >= 100 {
                    return Err("RESULT_FILE_COUNT_LIMIT".into());
                }
                let mut input = open_checked(&path, &source)?;
                if input.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE {
                    return Err("RESULT_FILE_SIZE_LIMIT".into());
                }
                let id = uuid::Uuid::new_v4().to_string();
                let output_path = destination.join(&id);
                let name = path
                    .strip_prefix(&source)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                validate_relative(&name)?;
                let mut options = OpenOptions::new();
                options.create_new(true).write(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut output = options.open(&output_path).map_err(|e| e.to_string())?;
                let mut size = 0u64;
                let mut hash = Sha256::new();
                let mut buffer = [0u8; 65536];
                // Register before copying so partial snapshots are removed on any error.
                artifacts.push(Artifact {
                    id,
                    task_id: task_id.into(),
                    name,
                    size: 0,
                    sha256: String::new(),
                });
                loop {
                    let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
                    if count == 0 {
                        break;
                    }
                    size += count as u64;
                    total += count as u64;
                    if size > MAX_FILE || total > MAX_TOTAL {
                        return Err("RESULT_SIZE_LIMIT".into());
                    }
                    hash.update(&buffer[..count]);
                    output
                        .write_all(&buffer[..count])
                        .map_err(|e| e.to_string())?;
                }
                output.sync_all().map_err(|e| e.to_string())?;
                let artifact = artifacts.last_mut().unwrap();
                artifact.size = size;
                artifact.sha256 = format!("{:x}", hash.finalize());
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        for a in artifacts {
            let _ = std::fs::remove_file(destination.join(a.id));
        }
        return Err(error);
    }
    Ok(artifacts)
}

fn linked(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(unix)]
    {
        metadata.file_type().is_symlink()
    }
}

fn open_checked(path: &Path, root: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || linked(&metadata) {
        return Err("RESULT_NOT_REGULAR_FILE".into());
    }
    let actual = handle_path(&file)?;
    if !actual.starts_with(root) {
        return Err("RESULT_PATH_ESCAPE".into());
    }
    Ok(file)
}
#[cfg(windows)]
fn handle_path(file: &File) -> Result<PathBuf, String> {
    use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;
    let mut buffer = vec![0u16; 32768];
    let len = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle() as _,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if len == 0 || len as usize >= buffer.len() {
        return Err("RESULT_PATH_UNVERIFIABLE".into());
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(
        &buffer[..len as usize],
    )))
}
#[cfg(target_os = "linux")]
fn handle_path(file: &File) -> Result<PathBuf, String> {
    use std::os::fd::AsRawFd;
    std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).map_err(|e| e.to_string())
}
#[cfg(target_os = "macos")]
fn handle_path(file: &File) -> Result<PathBuf, String> {
    use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};
    let mut bytes = [0u8; libc::PATH_MAX as usize];
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, bytes.as_mut_ptr()) } != 0 {
        return Err("RESULT_PATH_UNVERIFIABLE".into());
    }
    let len = bytes
        .iter()
        .position(|b| *b == 0)
        .ok_or("RESULT_PATH_UNVERIFIABLE")?;
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(&bytes[..len])))
}

pub async fn list(
    store: &AgentStore,
    task_id: &str,
    controller: &str,
) -> Result<Vec<Artifact>, String> {
    use sqlx::Row;
    store.get(task_id, Some(controller)).await?;
    Ok(
        sqlx::query("SELECT * FROM agent_artifacts WHERE task_id=? ORDER BY name")
            .bind(task_id)
            .fetch_all(store.pool())
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| Artifact {
                id: r.get("id"),
                task_id: r.get("task_id"),
                name: r.get("name"),
                size: r.get::<i64, _>("size") as u64,
                sha256: r.get("sha256"),
            })
            .collect(),
    )
}

pub fn byte_range(header: Option<&str>, size: u64) -> Result<(u64, u64, bool), String> {
    let Some(header) = header else {
        return Ok((0, size, false));
    };
    let value = header.strip_prefix("bytes=").ok_or("INVALID_RANGE")?;
    let (start, end) = value.split_once('-').ok_or("INVALID_RANGE")?;
    let start: u64 = start.parse().map_err(|_| "INVALID_RANGE")?;
    let end = if end.is_empty() {
        size.saturating_sub(1)
    } else {
        end.parse::<u64>()
            .map_err(|_| "INVALID_RANGE")?
            .min(size.saturating_sub(1))
    };
    if start >= size || end < start {
        return Err("RANGE_NOT_SATISFIABLE".into());
    }
    Ok((start, end - start + 1, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_are_independent_and_reject_escape_paths() {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("cwd");
        let out = cwd.join("results");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("中文.txt"), "原始结果").unwrap();
        let dest = root.path().join("private");
        let records = snapshot(&cwd, "results", &dest, "task").unwrap();
        assert_eq!(records.len(), 1);
        std::fs::write(out.join("中文.txt"), "修改").unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join(&records[0].id)).unwrap(),
            "原始结果"
        );
        assert_eq!(
            records[0].sha256,
            tailtask_core::remote::hex_digest("原始结果".as_bytes())
        );
        assert!(snapshot(&cwd, "../private", &dest, "task").is_err());
        assert!(open_checked(&dest.join(&records[0].id), &cwd.canonicalize().unwrap()).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_and_special_files_are_not_opened() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("results");
        std::fs::create_dir(&out).unwrap();
        symlink("/etc/passwd", out.join("escape")).unwrap();
        assert!(
            snapshot(root.path(), "results", &root.path().join("private"), "task")
                .unwrap_err()
                .contains("SYMLINK")
        );
    }
    #[test]
    fn ranges_reject_overflow_multipart_and_invalid_bounds() {
        assert_eq!(byte_range(Some("bytes=5-"), 10).unwrap(), (5, 5, true));
        assert_eq!(byte_range(Some("bytes=2-6"), 10).unwrap(), (2, 5, true));
        for s in [
            "bytes=10-",
            "bytes=7-2",
            "bytes=1-2,3-4",
            "bytes=18446744073709551616-",
        ] {
            assert!(byte_range(Some(s), 10).is_err());
        }
    }
}
