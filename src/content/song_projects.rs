use crate::project::{Project, load_project};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
pub fn song_project_path(root: &Path, source: &Path, digest: &str) -> PathBuf {
    let path = crate::paths::absolute(source).unwrap_or_else(|_| source.to_path_buf());
    let identity = format!(
        "{}\n{}",
        path.to_string_lossy().replace('/', "\\").to_lowercase(),
        digest.to_lowercase()
    );
    root.join(format!("{:x}.hstudio", Sha256::digest(identity.as_bytes())))
}
pub fn find_song_project(
    root: &Path,
    source: &Path,
    cancel: &AtomicBool,
) -> Result<(PathBuf, Option<Project>)> {
    let digest = crate::library::file_digest(source, cancel)?;
    let path = song_project_path(root, source, &digest);
    let project =
        if path.exists() {
            Some(load_project(&path).with_context(|| {
                format!("无法打开自动工程，请检查或另存后重试：{}", path.display())
            })?)
        } else {
            None
        };
    Ok((path, project))
}
