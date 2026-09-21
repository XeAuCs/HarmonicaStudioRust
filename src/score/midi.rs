//! Bounded Standard MIDI 0/1 and RIFF/RMID parser; preserves the raw MIDI range.
use crate::models::Note;
use anyhow::{Result, bail, ensure};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::Path,
};
pub type PartKey = (usize, u8);
pub type Parts = BTreeMap<PartKey, Vec<Note>>;
pub type TrackNames = BTreeMap<usize, String>;
const MAX_BYTES: usize = 20 * 1024 * 1024;
#[path = "midi_text.rs"]
mod text;
struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8> {
        let value = *self
            .bytes
            .get(self.position)
            .ok_or_else(|| anyhow::anyhow!("MIDI 事件被截断。"))?;
        self.position += 1;
        Ok(value)
    }
    fn vlq(&mut self) -> Result<u64> {
        let mut value = 0;
        for _ in 0..4 {
            let b = self.byte()?;
            value = (value << 7) | u64::from(b & 127);
            if b & 128 == 0 {
                return Ok(value);
            }
        }
        bail!("MIDI 可变长度数值无效。")
    }
    fn block(&mut self, size: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(size)
            .ok_or_else(|| anyhow::anyhow!("MIDI 长度溢出。"))?;
        let result = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| anyhow::anyhow!("MIDI 事件被截断。"))?;
        self.position = end;
        Ok(result)
    }
}
fn be16(data: &[u8]) -> u16 {
    u16::from_be_bytes([data[0], data[1]])
}
fn be32(data: &[u8]) -> usize {
    u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize
}
pub fn read_midi(path: &Path) -> Result<(Parts, TrackNames)> {
    ensure!(
        fs::metadata(path)?.len() <= MAX_BYTES as u64,
        "文件超过 20 MB，请先裁剪或分轨。"
    );
    let bytes = fs::read(path)?;
    ensure!(bytes.len() <= MAX_BYTES, "文件超过 20 MB，请先裁剪或分轨。");
    parse_midi(&bytes)
}
pub fn parse_midi(bytes: &[u8]) -> Result<(Parts, TrackNames)> {
    ensure!(bytes.len() <= MAX_BYTES, "文件超过 20 MB，请先裁剪或分轨。");
    let mut data = bytes;
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"RMID") {
        let mut position = 12;
        while position + 8 <= data.len() {
            let size = u32::from_le_bytes(data[position + 4..position + 8].try_into()?) as usize;
            let end = position
                .checked_add(8 + size)
                .ok_or_else(|| anyhow::anyhow!("RMID 长度无效。"))?;
            ensure!(end <= data.len(), "RMID 文件被截断。");
            if &data[position..position + 4] == b"data" {
                data = &data[position + 8..end];
                break;
            }
            position = end + size % 2;
        }
    }
    ensure!(
        data.len() >= 14 && data.starts_with(b"MThd"),
        "不是有效 MIDI 文件，不能把网页或音频改后缀当作 MIDI。"
    );
    let header_size = be32(&data[4..8]);
    ensure!(
        header_size >= 6 && header_size <= data.len() - 8,
        "MIDI 文件头长度无效。"
    );
    let format = be16(&data[8..10]);
    let count = be16(&data[10..12]) as usize;
    let ppq = be16(&data[12..14]);
    ensure!(
        format <= 1 && ppq != 0 && ppq & 0x8000 == 0,
        "目前支持标准 MIDI 格式 0/1、每拍刻度计时。"
    );
    ensure!(
        (1..=1024).contains(&count) && (format != 0 || count == 1),
        "MIDI 音轨数量无效。"
    );
    let mut position = 8 + header_size;
    let mut raw = Vec::new();
    let mut tempos = BTreeMap::from([(0u64, 500_000u32)]);
    let mut names = TrackNames::new();
    for track in 0..count {
        ensure!(
            position + 8 <= data.len() && &data[position..position + 4] == b"MTrk",
            "MIDI 音轨头缺失或文件不完整。"
        );
        let size = be32(&data[position + 4..position + 8]);
        ensure!(size <= data.len() - position - 8, "MIDI 音轨被截断。");
        let mut reader = Reader {
            bytes: &data[position + 8..position + 8 + size],
            position: 0,
        };
        position += 8 + size;
        let mut tick = 0u64;
        let mut running = None;
        let mut active: BTreeMap<(u8, u8), VecDeque<(u64, u8)>> = BTreeMap::new();
        while reader.position < reader.bytes.len() {
            tick = tick
                .checked_add(reader.vlq()?)
                .ok_or_else(|| anyhow::anyhow!("MIDI 时间溢出。"))?;
            let mut status = reader.byte()?;
            if status < 128 {
                status = running.ok_or_else(|| anyhow::anyhow!("MIDI running status 无效。"))?;
                reader.position -= 1;
            }
            match status {
                255 => {
                    let kind = reader.byte()?;
                    let length = reader.vlq()? as usize;
                    let value = reader.block(length)?;
                    running = None;
                    match kind {
                        81 if value.len() == 3 => {
                            let tempo = (u32::from(value[0]) << 16)
                                | (u32::from(value[1]) << 8)
                                | u32::from(value[2]);
                            ensure!(tempo > 0, "MIDI 速度无效。");
                            tempos.insert(tick, tempo);
                        }
                        3 => {
                            names.insert(track, text::decode_name(value));
                        }
                        47 => break,
                        _ => {}
                    }
                }
                240 | 247 => {
                    let length = reader.vlq()? as usize;
                    reader.block(length)?;
                    running = None;
                }
                128..=239 => {
                    running = Some(status);
                    let kind = status >> 4;
                    let channel = status & 15;
                    let pitch = reader.byte()?;
                    let velocity = if kind == 12 || kind == 13 {
                        0
                    } else {
                        reader.byte()?
                    };
                    ensure!(pitch < 128 && velocity < 128, "MIDI 通道数据无效。");
                    if kind == 9 && velocity != 0 {
                        active
                            .entry((channel, pitch))
                            .or_default()
                            .push_back((tick, velocity));
                    } else if kind == 8 || (kind == 9 && velocity == 0) {
                        if let Some((start, velocity)) = active
                            .get_mut(&(channel, pitch))
                            .and_then(VecDeque::pop_front)
                        {
                            if tick > start {
                                ensure!(raw.len() < 100_000, "音符过多，请先拆分曲谱。");
                                raw.push((track, channel, pitch, start, tick, velocity));
                            }
                        }
                    }
                }
                _ => bail!("不支持的 MIDI 系统事件。"),
            }
        }
        ensure!(
            active.values().all(VecDeque::is_empty),
            "MIDI 存在未结束的音符，请先用打谱软件修复。"
        );
    }
    let mut map: Vec<(u64, u32, f64)> = Vec::new();
    let mut elapsed = 0.0;
    for (tick, tempo) in tempos {
        if let Some(&(previous, previous_tempo, _)) = map.last() {
            elapsed += (tick - previous) as f64 * f64::from(previous_tempo) / f64::from(ppq) / 1e6;
        }
        map.push((tick, tempo, elapsed));
    }
    let to_seconds = |tick: u64| {
        let i = map
            .partition_point(|&(t, _, _)| t <= tick)
            .saturating_sub(1);
        let (origin, tempo, seconds) = map[i];
        seconds + (tick - origin) as f64 * f64::from(tempo) / f64::from(ppq) / 1e6
    };
    let mut parts = Parts::new();
    for (track, channel, pitch, start, end, velocity) in raw {
        if channel != 9 {
            parts.entry((track, channel)).or_default().push(Note {
                pitch: i32::from(pitch),
                start: to_seconds(start),
                end: to_seconds(end),
                velocity: i32::from(velocity),
            });
        }
    }
    ensure!(!parts.is_empty(), "文件没有可用的非打击乐音符。");
    for notes in parts.values_mut() {
        notes.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    }
    Ok((parts, names))
}
pub fn vlq_out(mut n: u64) -> Vec<u8> {
    let mut result = vec![(n & 127) as u8];
    while n >> 7 != 0 {
        n >>= 7;
        result.push(((n & 127) | 128) as u8);
    }
    result.reverse();
    result
}
pub fn write_midi(notes: &[Note], path: &Path) -> Result<()> {
    let mut events = Vec::with_capacity(notes.len() * 2);
    for note in notes {
        ensure!(
            (0..=127).contains(&note.pitch) && (1..=127).contains(&note.velocity),
            "MIDI 音高或力度无效。"
        );
        ensure!(
            note.start.is_finite()
                && note.end.is_finite()
                && note.start >= 0.0
                && note.end > note.start
                && note.end <= u32::MAX as f64 / 1000.0,
            "MIDI 音符时间无效。"
        );
        let start = (note.start * 1000.0).round_ties_even() as u64;
        let end = ((note.end * 1000.0).round_ties_even() as u64).max(start + 1);
        events.push((start, 1u8, [144, note.pitch as u8, note.velocity as u8]));
        events.push((end, 0u8, [128, note.pitch as u8, 0]));
    }
    events.sort();
    let mut body = vec![0, 255, 81, 3, 7, 161, 32];
    let mut last = 0;
    for (tick, _, payload) in events {
        ensure!(tick - last <= 0x0fff_ffff, "MIDI 事件间隔过长。");
        body.extend(vlq_out(tick - last));
        body.extend(payload);
        last = tick;
    }
    body.extend([0, 255, 47, 0]);
    let mut output = b"MThd".to_vec();
    output.extend(6u32.to_be_bytes());
    output.extend(0u16.to_be_bytes());
    output.extend(1u16.to_be_bytes());
    output.extend(500u16.to_be_bytes());
    output.extend(b"MTrk");
    output.extend((body.len() as u32).to_be_bytes());
    output.extend(body);
    fs::write(path, output)?;
    Ok(())
}
