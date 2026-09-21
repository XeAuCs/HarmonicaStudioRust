use crate::{
    models::Note,
    notes::{MAX_PITCH, MAX_SECONDS, MIN_PITCH, normalize_score_notes},
};
use anyhow::{Result, ensure};
pub type Event = (u64, String, u8);
const KEYS: [&str; 12] = [
    "SC02C", "SC02C", "SC02D", "SC02D", "SC02E", "SC02F", "SC02F", "SC030", "SC030", "SC031",
    "SC031", "SC032",
];
pub fn mapping(pitch: i32) -> Result<(&'static str, Vec<&'static str>)> {
    ensure!(
        (MIN_PITCH..=MAX_PITCH).contains(&pitch),
        "音高超出 C3 至 C6# 的映射范围。"
    );
    let pc = pitch % 12;
    let mut modifiers = Vec::new();
    if pitch < 60 {
        modifiers.push("LButton");
    } else if pitch >= 72 {
        modifiers.push("RButton");
    }
    if [1, 3, 6, 8, 10].contains(&pc) {
        modifiers.push("MButton");
    }
    Ok((
        if pitch >= 84 {
            "SC033"
        } else {
            KEYS[pc as usize]
        },
        modifiers,
    ))
}
pub fn build_events(notes: &[Note]) -> Result<(Vec<Event>, usize)> {
    let notes = normalize_score_notes(notes)?;
    let mut events = Vec::new();
    let mut released = 0u64;
    let mut delayed = 0;
    for (index, note) in notes.iter().enumerate() {
        let (key, mods) = mapping(note.pitch)?;
        let desired = 100 + (note.start * 1000.0).round_ties_even() as u64;
        let onset = desired.max(released + 20 + if mods.is_empty() { 0 } else { 25 });
        if onset > desired {
            delayed += 1;
        }
        let mut intended_end = 100 + (note.end * 1000.0).round_ties_even() as u64;
        if let Some(next) = notes.get(index + 1) {
            let (_, next_mods) = mapping(next.pitch)?;
            let next_onset = 100 + (next.start * 1000.0).round_ties_even() as u64;
            intended_end = intended_end
                .min(next_onset.saturating_sub(20 + if next_mods.is_empty() { 0 } else { 25 }));
        }
        let end = (onset + 25).max(intended_end);
        ensure!(
            end <= (MAX_SECONDS * 1000.0) as u64,
            "按键编排后超过 20 分钟，请减少音符或裁剪曲谱。"
        );
        for modifier in &mods {
            events.push((onset - 25, (*modifier).to_owned(), 1));
        }
        events.push((onset, key.to_owned(), 1));
        events.push((end, key.to_owned(), 0));
        for modifier in mods {
            events.push((end, modifier.to_owned(), 0));
        }
        released = end;
    }
    events.sort_by_key(|e| e.0);
    Ok((events, delayed))
}
