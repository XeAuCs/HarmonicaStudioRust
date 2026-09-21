use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

fn default_velocity() -> i32 {
    80
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub pitch: i32,
    pub start: f64,
    pub end: f64,
    #[serde(default = "default_velocity")]
    pub velocity: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub speed: f64,
    pub transpose: i32,
    pub auto_octave: bool,
    pub trim_silence: bool,
    pub melody_mode: String,
    pub track: Option<usize>,
    pub channel: Option<u8>,
    pub skip_long_rests: bool,
    pub phrase_octave: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            speed: 1.0,
            transpose: 0,
            auto_octave: true,
            trim_silence: true,
            melody_mode: "sustain".into(),
            track: None,
            channel: None,
            skip_long_rests: false,
            phrase_octave: false,
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        if !self.speed.is_finite() || !(0.25..=2.0).contains(&self.speed) {
            bail!("速度必须在 0.25 至 2 倍之间。");
        }
        if !(-24..=24).contains(&self.transpose) {
            bail!("移调必须是 -24 至 24 之间的整数。");
        }
        if !["highest", "sustain", "continuous"].contains(&self.melody_mode.as_str()) {
            bail!("未知旋律提取模式。");
        }
        if self.track.is_some_and(|v| v >= 1024) {
            bail!("音轨编号无效。");
        }
        if self.channel.is_some_and(|v| v >= 16) {
            bail!("通道编号无效。");
        }
        Ok(())
    }
}
