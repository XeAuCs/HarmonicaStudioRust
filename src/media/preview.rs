use crate::{models::Note, schedule::Event};
use anyhow::{Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufWriter, Seek, SeekFrom, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
fn key_pitch(key: &str) -> Option<i32> {
    match key {
        "SC02C" => Some(0),
        "SC02D" => Some(2),
        "SC02E" => Some(4),
        "SC02F" => Some(5),
        "SC030" => Some(7),
        "SC031" => Some(9),
        "SC032" => Some(11),
        "SC033" => Some(12),
        _ => None,
    }
}
pub fn decode_events(events: &[Event]) -> Result<Vec<Note>> {
    let mut held: BTreeSet<&str> = BTreeSet::new();
    let mut playing: BTreeMap<&str, (u64, i32)> = BTreeMap::new();
    let mut notes = Vec::new();
    let mut last = 0;
    for (ms, key, down) in events {
        let key = key.as_str();
        ensure!(
            *ms >= last
                && (key_pitch(key).is_some() || ["LButton", "RButton", "MButton"].contains(&key))
                && *down <= 1,
            "按键时间表无效。"
        );
        last = *ms;
        if *down == 1 {
            ensure!(!held.contains(key), "发现重复按下事件。");
            if let Some(base) = key_pitch(key) {
                ensure!(playing.is_empty(), "发现重叠发声音符。");
                let pitch = 60 + base + 12 * i32::from(held.contains("RButton"))
                    - 12 * i32::from(held.contains("LButton"))
                    + i32::from(held.contains("MButton"));
                playing.insert(key, (*ms, pitch));
            }
            held.insert(key);
            ensure!(
                !(held.contains("LButton") && held.contains("RButton")),
                "高低八度修饰键冲突。"
            );
        } else {
            ensure!(held.contains(key), "发现不配对的松开事件。");
            if let Some((start, pitch)) = playing.remove(key) {
                ensure!(*ms > start, "发声时长无效。");
                notes.push(Note {
                    pitch,
                    start: start as f64 / 1000.0,
                    end: *ms as f64 / 1000.0,
                    velocity: 80,
                });
            }
            held.remove(key);
        }
    }
    ensure!(
        held.is_empty() && playing.is_empty(),
        "播放结束后仍有按键按住。"
    );
    Ok(notes)
}
pub fn render_wav(
    events: &[Event],
    path: &Path,
    cancel: &AtomicBool,
    sample_rate: u32,
) -> Result<()> {
    ensure!(
        (8000..=192000).contains(&sample_rate),
        "试听采样率须为 8000 至 192000 Hz。"
    );
    let notes = decode_events(events)?;
    ensure!(!notes.is_empty(), "没有可试听的音符。");
    let check = || -> Result<()> {
        ensure!(!cancel.load(Ordering::Acquire), "转换已取消。");
        Ok(())
    };
    check()?;
    let mut out = BufWriter::new(File::create(path)?);
    out.write_all(b"RIFF")?;
    out.write_all(&0u32.to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?;
    out.write_all(&sample_rate.to_le_bytes())?;
    out.write_all(&(sample_rate * 2).to_le_bytes())?;
    out.write_all(&2u16.to_le_bytes())?;
    out.write_all(&16u16.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&0u32.to_le_bytes())?;
    let mut position = 0u64;
    let mut frames = 0u64;
    let zeros = vec![0u8; sample_rate as usize * 2];
    for note in notes {
        check()?;
        let start = (note.start * f64::from(sample_rate)).round_ties_even() as u64;
        let end = (note.end * f64::from(sample_rate)).round_ties_even() as u64;
        let mut gap = start.saturating_sub(position);
        while gap > 0 {
            check()?;
            let size = gap.min(u64::from(sample_rate));
            out.write_all(&zeros[..size as usize * 2])?;
            frames += size;
            gap -= size;
        }
        let count = end.saturating_sub(start).max(1);
        let frequency = 440.0 * 2f64.powf((note.pitch - 69) as f64 / 12.0);
        for offset in (0..count).step_by(sample_rate as usize) {
            check()?;
            let mut chunk = Vec::with_capacity(sample_rate as usize * 2);
            for i in offset..count.min(offset + u64::from(sample_rate)) {
                let time = i as f64 / f64::from(sample_rate);
                let phase = 2.0 * std::f64::consts::PI * frequency * time;
                let envelope = (time / 0.012).min(1.0)
                    * ((count - i) as f64 / f64::from(sample_rate) / 0.025).min(1.0);
                let tone = phase.sin() + 0.28 * (2.0 * phase).sin() + 0.13 * (3.0 * phase).sin();
                chunk.extend(
                    ((32767.0 * 0.17 * tone * envelope).round_ties_even() as i16).to_le_bytes(),
                );
            }
            frames += chunk.len() as u64 / 2;
            out.write_all(&chunk)?;
        }
        position = end;
    }
    out.write_all(&zeros[..(sample_rate / 3) as usize * 2])?;
    frames += u64::from(sample_rate / 3);
    ensure!(
        frames * 2 <= u64::from(u32::MAX) - 36,
        "试听 WAV 文件过大。"
    );
    out.seek(SeekFrom::Start(4))?;
    out.write_all(&((frames * 2 + 36) as u32).to_le_bytes())?;
    out.seek(SeekFrom::Start(40))?;
    out.write_all(&((frames * 2) as u32).to_le_bytes())?;
    out.flush()?;
    check()?;
    Ok(())
}
