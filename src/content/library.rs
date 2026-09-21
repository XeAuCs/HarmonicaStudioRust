use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub file: String,
    pub title: String,
    pub path: PathBuf,
    pub options: Option<Value>,
    pub duration_seconds: Option<f64>,
}
pub fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    anyhow::ensure!(!cancel.load(Ordering::Relaxed), "操作已取消");
    Ok(())
}
pub fn file_digest(path: &Path, cancel: &AtomicBool) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut chunk = [0u8; 65536];
    loop {
        check_cancel(cancel)?;
        let n = f.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        hash.update(&chunk[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn sample_entries(root: &Path, cancel: &AtomicBool) -> Result<Vec<LibraryEntry>> {
    check_cancel(cancel)?;
    let rows = fs::read(root.join("catalog.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok())
        .unwrap_or_default();
    let metadata: BTreeMap<String, Value> = rows
        .into_iter()
        .filter_map(|r| r["file"].as_str().map(|s| s.to_lowercase()).map(|s| (s, r)))
        .collect();
    let iter = match fs::read_dir(root) {
        Ok(i) => i,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let mut paths = Vec::new();
    for entry in iter {
        check_cancel(cancel)?;
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let p = entry.path();
            if p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|s| ["mid", "midi", "kar", "rmi"].contains(&s.to_lowercase().as_str()))
            {
                paths.push(p);
            }
        }
    }
    paths.sort_by_key(|p| {
        (
            p.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase(),
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase(),
        )
    });
    let mut entries = Vec::new();
    for path in paths {
        check_cancel(cancel)?;
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut row = metadata.get(&file.to_lowercase());
        if let Some(r) = row {
            if r["sha256"].as_str().map(|s| s.to_lowercase()) != Some(file_digest(&path, cancel)?) {
                row = None;
            }
        }
        let title = row
            .and_then(|r| r["title"].as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(200).collect())
            .unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        entries.push(LibraryEntry {
            file,
            title,
            path: crate::paths::absolute(&path)?,
            options: row.and_then(|r| r.get("options").cloned()),
            duration_seconds: row
                .and_then(|r| r["duration_seconds"].as_f64())
                .filter(|f| f.is_finite() && *f >= 0.),
        });
    }
    Ok(entries)
}
pub fn song_id(path: &Path) -> String {
    let p = crate::paths::absolute(path).unwrap_or_else(|_| path.to_owned());
    format!(
        "{:x}",
        Sha256::digest(p.to_string_lossy().to_lowercase().as_bytes())
    )[..24]
        .into()
}
