use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Preferences {
    pub theme: String,
    pub compact: bool,
    pub library_folder: String,
    pub skip_long_rests: bool,
    pub start_from_highlight: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "paper".into(),
            compact: false,
            library_folder: String::new(),
            skip_long_rests: true,
            start_from_highlight: false,
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            ["paper", "forest", "blue", "plum"].contains(&self.theme.as_str()),
            "未知主题"
        );
        anyhow::ensure!(!self.library_folder.contains('\0'), "曲库路径无效");
        Ok(())
    }
}
pub fn load_preferences(path: &Path) -> Preferences {
    let mut p = Preferences::default();
    if let Ok(bytes) = std::fs::read(path) {
        if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(s) = v["theme"].as_str() {
                if ["paper", "forest", "blue", "plum"].contains(&s) {
                    p.theme = s.into();
                }
            }
            if let Some(b) = v["compact"].as_bool() {
                p.compact = b;
            }
            if let Some(b) = v["skip_long_rests"].as_bool() {
                p.skip_long_rests = b;
            }
            if let Some(b) = v["start_from_highlight"].as_bool() {
                p.start_from_highlight = b;
            }
            if let Some(s) = v["library_folder"].as_str() {
                if !s.contains('\0') {
                    p.library_folder = s.into();
                }
            }
        }
    }
    p
}
pub fn save_preferences(path: &Path, p: &Preferences) -> Result<()> {
    p.validate()?;
    crate::paths::atomic_write(path, &serde_json::to_vec_pretty(p)?)
}

pub fn load_options(path: &Path) -> crate::models::Options {
    let read = || -> Result<crate::models::Options> {
        let mut value: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("转换设置必须是对象"))?;
        object.remove("track");
        object.remove("channel");
        let options: crate::models::Options = serde_json::from_value(value)?;
        options.validate()?;
        Ok(options)
    };
    read().unwrap_or_default()
}
pub fn save_options(path: &Path, options: &crate::models::Options) -> Result<()> {
    options.validate()?;
    let mut persistent = options.clone();
    persistent.track = None;
    persistent.channel = None;
    crate::paths::atomic_write(path, &serde_json::to_vec_pretty(&persistent)?)
}
