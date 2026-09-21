//! Portable paths do not depend on the process working directory.
use anyhow::{Context, Result};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
pub fn unique_id() -> String {
    format!(
        "{:x}-{:x}-{:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
pub fn application_root() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
    }
}
pub fn resource_root() -> PathBuf {
    application_root()
}
pub fn data_root() -> PathBuf {
    std::env::var_os("HARMONICA_STUDIO_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| application_root().join("data"))
}
pub fn default_library_root() -> PathBuf {
    application_root().join("samples")
}
pub fn library_path(setting: &str) -> PathBuf {
    if setting.is_empty() {
        default_library_root()
    } else {
        let p = PathBuf::from(setting);
        if p.is_absolute() {
            p
        } else {
            application_root().join(p)
        }
    }
}
pub fn library_setting(setting: &str) -> String {
    if setting.is_empty() {
        return String::new();
    }
    let p = library_path(setting);
    if p == default_library_root() {
        String::new()
    } else {
        p.strip_prefix(application_root())
            .unwrap_or(&p)
            .to_string_lossy()
            .into_owned()
    }
}
pub fn template_path() -> PathBuf {
    resource_root().join("assets/player.ahk")
}
pub fn icon_path() -> PathBuf {
    resource_root().join("assets/studio.ico")
}
pub fn absolute(path: &Path) -> Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let resolved = if path.exists() {
        fs::canonicalize(path)?
    } else {
        path
    };
    // Rust's Windows canonicalization emits verbatim prefixes. Persist the same
    // ordinary absolute spelling as existing .hstudio source identities.
    #[cfg(windows)]
    {
        let text = resolved.to_string_lossy();
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{unc}")));
        }
        if let Some(drive) = text.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(drive));
        }
    }
    Ok(resolved)
}
/// Write and flush a fresh sibling before atomically replacing the destination.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("文件没有父目录")?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".hs-{}.tmp", unique_id()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        atomic_replace(&temp, path)
    })();
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}
#[cfg(windows)]
pub fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    let a: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let b: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 1 | 8) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to)?;
    Ok(())
}
