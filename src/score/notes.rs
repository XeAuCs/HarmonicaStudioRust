pub use crate::models::Note;
use anyhow::{Result, bail};
pub const MIN_PITCH: i32 = 48;
pub const MAX_PITCH: i32 = 85;
pub const MAX_NOTES: usize = 100_000;
pub const MAX_SECONDS: f64 = 1200.0;
pub const TIME_EPSILON: f64 = 1e-8;
pub const DEFAULT_VELOCITY: i32 = 80;
pub const MIN_EDITOR_PITCH: i32 = -256;
pub const MAX_EDITOR_PITCH: i32 = 383;

pub fn normalize_score_notes(notes: &[Note]) -> Result<Vec<Note>> {
    normalize_notes(notes, MIN_PITCH, MAX_PITCH)
}

/// Same timing, velocity and monophony rules, including editable range hints.
pub fn normalize_editor_notes(notes: &[Note]) -> Result<Vec<Note>> {
    normalize_notes(notes, MIN_EDITOR_PITCH, MAX_EDITOR_PITCH)
}

fn normalize_notes(notes: &[Note], min_pitch: i32, max_pitch: i32) -> Result<Vec<Note>> {
    if notes.len() > MAX_NOTES {
        bail!("工程音符列表无效，最多支持 {MAX_NOTES} 个音符。");
    }
    let mut result = notes.to_vec();
    for (index, note) in result.iter().enumerate() {
        let index = index + 1;
        if !(min_pitch..=max_pitch).contains(&note.pitch) {
            bail!("第 {index} 个音符超出允许音域（{min_pitch} 至 {max_pitch}）。");
        }
        if !(1..=127).contains(&note.velocity) {
            bail!("第 {index} 个音符力度须为 1 至 127。");
        }
        if !note.start.is_finite()
            || !note.end.is_finite()
            || note.start < 0.0
            || note.start >= note.end
            || note.end > MAX_SECONDS
        {
            bail!("第 {index} 个音符时间无效，结束时间须晚于开始且在 20 分钟以内。");
        }
    }
    result.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    for i in 1..result.len() {
        if result[i - 1].end > result[i].start {
            if result[i - 1].end - result[i].start <= TIME_EPSILON
                && result[i].start > result[i - 1].start
            {
                result[i - 1].end = result[i].start;
            } else {
                bail!("音符存在重叠；口琴一次只能演奏一个音，请移开或缩短音符。");
            }
        }
    }
    Ok(result)
}
