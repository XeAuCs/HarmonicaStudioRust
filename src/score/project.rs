use crate::{models::Note, notes::normalize_score_notes};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
fn default_title() -> String {
    "曲谱".into()
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    #[serde(default = "default_title")]
    pub title: String,
    pub notes: Vec<Note>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<Value>,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            schema_version: 1,
            title: default_title(),
            notes: Vec::new(),
            highlight: None,
            source: None,
            options: None,
            report: None,
        }
    }
}
pub fn make_project(notes: Vec<Note>, title: impl Into<String>) -> Result<Project> {
    validate_project(&Project {
        notes,
        title: title.into(),
        ..Project::default()
    })
}
pub fn validate_project(project: &Project) -> Result<Project> {
    if project.schema_version != SCHEMA_VERSION {
        bail!("不支持此工程版本，请使用版本 1 的 .hstudio 工程。");
    }
    if project.title.trim().is_empty() || project.title.chars().count() > 200 {
        bail!("工程名称须为 1 至 200 个字符。");
    }
    let mut result = project.clone();
    result.title = result.title.trim().to_owned();
    result.notes = normalize_score_notes(&project.notes)?;
    if let Some(highlight) = result.highlight {
        let duration = result.notes.iter().map(|n| n.end).fold(0.0, f64::max);
        if !highlight.is_finite() || highlight < 0.0 || highlight >= duration {
            bail!("心动片段标记须位于曲谱开始至结束之前。");
        }
    }
    if result
        .source
        .as_ref()
        .is_some_and(|v| !v.is_string() && !v.is_object())
    {
        bail!("工程来源信息无效。");
    }
    if [&result.options, &result.report]
        .iter()
        .any(|v| v.as_ref().is_some_and(|v| !v.is_object()))
    {
        bail!("工程设置或报告格式无效。");
    }
    json_bytes(&result)?;
    Ok(result)
}
fn json_bytes(project: &Project) -> Result<Vec<u8>> {
    let raw = serde_json::to_vec_pretty(project)?;
    if raw.len() as u64 > MAX_FILE_BYTES {
        bail!("工程文件不能超过 20 MB。");
    }
    Ok(raw)
}
pub(crate) fn unique_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{:x}-{:x}-{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
pub fn save_project(path: &Path, project: &Project) -> Result<()> {
    let raw = json_bytes(&validate_project(project)?)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        unique_id()
    ));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&raw)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path).context("替换工程文件失败，原工程保持不变。")?;
        Ok(())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub fn load_project(path: &Path) -> Result<Project> {
    if fs::metadata(path)?.len() > MAX_FILE_BYTES {
        bail!("工程文件不能超过 20 MB。");
    }
    let raw = fs::read(path)?;
    if raw.len() as u64 > MAX_FILE_BYTES {
        bail!("工程文件不能超过 20 MB。");
    }
    let raw = raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&raw);
    let project: Project =
        serde_json::from_slice(raw).context("工程文件损坏或不是有效的 .hstudio 文件。")?;
    validate_project(&project)
}
