//! Deterministic bounded melody heuristics. Scores are preferences, not probabilities.
use crate::{
    midi::{PartKey, Parts, TrackNames},
    models::{Note, Options},
    notes::{MAX_PITCH, MAX_SECONDS, MIN_PITCH},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
fn shifts() -> impl Iterator<Item = i32> {
    (-144..=144).step_by(12)
}
fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut values: Vec<_> = values.collect();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}
pub fn onset_groups(ordered: &[Note]) -> Vec<&[Note]> {
    let mut result = Vec::new();
    let mut beginning = 0;
    let mut earliest_end = 0.0;
    for (i, note) in ordered.iter().enumerate() {
        if i > beginning
            && (note.start - ordered[beginning].start > 0.025 || note.start >= earliest_end)
        {
            result.push(&ordered[beginning..i]);
            beginning = i;
        }
        earliest_end = if i == beginning {
            note.end
        } else {
            earliest_end.min(note.end)
        };
    }
    if beginning < ordered.len() {
        result.push(&ordered[beginning..]);
    }
    result
}
pub fn continuous_line(ordered: &[Note]) -> Vec<Note> {
    if ordered.is_empty() {
        return Vec::new();
    }
    let mut nodes: Vec<&Note> = Vec::new();
    let mut scores: Vec<f64> = Vec::new();
    let mut parents: Vec<Option<usize>> = Vec::new();
    let mut beam: Vec<usize> = Vec::new();
    let low = ordered.iter().map(|n| n.pitch).min().unwrap();
    let span = (ordered.iter().map(|n| n.pitch).max().unwrap() - low).max(12) as f64;
    for group in onset_groups(ordered) {
        let mut additions = Vec::new();
        for note in group {
            let reward = 2.0
                + 2.0 * (note.end - note.start).min(2.0)
                + 0.8 * (note.pitch - low) as f64 / span
                + 0.2 * note.velocity as f64 / 127.0;
            let mut best = reward;
            let mut parent = None;
            for &previous in &beam {
                let p = nodes[previous];
                let interval = (note.pitch - p.pitch).abs();
                let mut cost = 0.06 * interval.min(24) as f64;
                let overlap = p.end - note.start;
                if overlap > 0.0 {
                    cost += 8.0 * (overlap / (p.end - p.start).max(0.001)).min(1.0);
                    if note.end <= p.end && note.pitch < p.pitch {
                        cost += 4.0;
                    }
                } else {
                    cost += 0.1 * (-overlap).min(3.0);
                }
                let candidate = scores[previous] + reward - cost;
                if candidate > best {
                    best = candidate;
                    parent = Some(previous);
                }
            }
            additions.push(nodes.len());
            nodes.push(note);
            scores.push(best);
            parents.push(parent);
        }
        beam.extend(additions);
        beam.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]).then(a.cmp(&b)));
        beam.truncate(32);
    }
    let mut selected = Vec::new();
    let mut current = beam.first().copied();
    while let Some(i) = current {
        selected.push(nodes[i].clone());
        current = parents[i];
    }
    selected.reverse();
    selected
}
pub fn simplify(notes: &[Note], mode: &str, trim: bool) -> Vec<Note> {
    let mut ordered = notes.to_vec();
    ordered.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    if ordered.is_empty() {
        return Vec::new();
    }
    let selected = if mode == "continuous" {
        continuous_line(&ordered)
    } else {
        let mut selected: Vec<Note> = Vec::new();
        for group in onset_groups(&ordered) {
            let chosen = group
                .iter()
                .reduce(|best, n| {
                    if (n.pitch, n.velocity) > (best.pitch, best.velocity) {
                        n
                    } else {
                        best
                    }
                })
                .unwrap();
            if mode == "sustain" {
                if let Some(previous) = selected.last() {
                    if previous.pitch - chosen.pitch >= 7
                        && chosen.start < previous.end - 0.025
                        && chosen.end <= previous.end + 0.025
                    {
                        continue;
                    }
                }
            }
            selected.push(chosen.clone());
        }
        selected
    };
    let origin = if trim {
        selected.first().map_or(0.0, |n| n.start)
    } else {
        0.0
    };
    selected
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let end = selected
                .get(i + 1)
                .map_or(n.end, |next| n.end.min(next.start));
            (end > n.start).then(|| Note {
                start: n.start - origin,
                end: end - origin,
                ..n.clone()
            })
        })
        .collect()
}
#[path = "part_features.rs"]
mod features;
pub use features::{PartFeatures, part_features};
#[path = "recommendation.rs"]
mod recommendation;
pub use recommendation::rank_parts;
pub fn note_weights(notes: &[Note]) -> Vec<f64> {
    let typical = median(notes.iter().map(|n| n.end - n.start)).max(0.001);
    notes
        .iter()
        .map(|n| 0.5 + 0.5 * ((n.end - n.start) / typical).min(4.0))
        .collect()
}
pub fn fit_phrases(notes: &[Note], base_shift: i32) -> (Vec<Note>, Vec<Value>) {
    if notes.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let threshold = (0.75 * median(notes.iter().map(|n| n.end - n.start))).clamp(0.35, 1.0);
    let mut phrases: Vec<&[Note]> = Vec::new();
    let mut start = 0;
    for i in 1..notes.len() {
        if notes[i].start - notes[i - 1].end >= threshold {
            phrases.push(&notes[start..i]);
            start = i;
        }
    }
    phrases.push(&notes[start..]);
    let weights = note_weights(notes);
    let mut offset = 0;
    let shifts: Vec<i32> = shifts().collect();
    let mut previous: Vec<(f64, f64)> = Vec::new();
    let mut history: Vec<Vec<usize>> = Vec::new();
    for phrase in &phrases {
        let local = &weights[offset..offset + phrase.len()];
        offset += phrase.len();
        let mut costs = Vec::new();
        let mut links = Vec::new();
        for &shift in &shifts {
            let lost = phrase
                .iter()
                .zip(local)
                .filter(|(n, _)| !(MIN_PITCH..=MAX_PITCH).contains(&(n.pitch + base_shift + shift)))
                .map(|(_, w)| *w)
                .sum::<f64>();
            let displacement = shift.abs() as f64 / 12.0 * 0.2;
            if previous.is_empty() {
                costs.push((lost, displacement));
                links.push(12);
            } else {
                let parent = (0..shifts.len())
                    .min_by(|&a, &b| {
                        previous[a]
                            .0
                            .total_cmp(&previous[b].0)
                            .then(
                                (previous[a].1 + (shift - shifts[a]).abs() as f64 / 12.0)
                                    .total_cmp(
                                        &(previous[b].1 + (shift - shifts[b]).abs() as f64 / 12.0),
                                    ),
                            )
                            .then(shifts[a].abs().cmp(&shifts[b].abs()))
                            .then(shifts[a].cmp(&shifts[b]))
                    })
                    .unwrap();
                costs.push((
                    previous[parent].0 + lost,
                    previous[parent].1
                        + (shift - shifts[parent]).abs() as f64 / 12.0
                        + displacement,
                ));
                links.push(parent);
            }
        }
        previous = costs;
        history.push(links);
    }
    let mut index = (0..shifts.len())
        .min_by(|&a, &b| {
            previous[a]
                .0
                .total_cmp(&previous[b].0)
                .then(previous[a].1.total_cmp(&previous[b].1))
                .then(shifts[a].abs().cmp(&shifts[b].abs()))
                .then(shifts[a].cmp(&shifts[b]))
        })
        .unwrap();
    let mut selected = Vec::new();
    for links in history.iter().rev() {
        selected.push(shifts[index]);
        index = links[index];
    }
    selected.reverse();
    let mut result = Vec::new();
    let mut adjustments = Vec::new();
    for (phrase, shift) in phrases.into_iter().zip(selected) {
        let mut kept: Vec<_> = phrase
            .iter()
            .filter_map(|n| {
                let pitch = n.pitch + base_shift + shift;
                (MIN_PITCH..=MAX_PITCH)
                    .contains(&pitch)
                    .then(|| Note { pitch, ..n.clone() })
            })
            .collect();
        if shift != 0 && !kept.is_empty() {
            adjustments.push(json!({"start":phrase[0].start,"end":phrase.last().unwrap().end,"semitones":shift,"notes":kept.len()}));
        }
        result.append(&mut kept);
    }
    (result, adjustments)
}
pub fn prepare(parts: &Parts, names: &TrackNames, options: &Options) -> Result<(Vec<Note>, Value)> {
    options.validate()?;
    let key = rank_parts(parts, names)
        .into_iter()
        .map(|(key, _)| key)
        .find(|key| {
            options.track.is_none_or(|v| key.0 == v) && options.channel.is_none_or(|v| key.1 == v)
        });
    let key = key.ok_or_else(|| anyhow::anyhow!("所选音轨或通道没有可用音符。"))?;
    let raw = &parts[&key];
    let (playable, mut report) = fit_part(raw, options)?;
    ensure!(
        !playable.is_empty(),
        "所有音符都超出口琴音域，请开启自动八度或调整移调。"
    );
    ensure!(
        playable.last().unwrap().end <= MAX_SECONDS,
        "演奏超过 20 分钟，请先裁剪曲谱或提高速度。"
    );
    report["track"] = json!(key.0);
    report["channel"] = json!(key.1 + 1);
    report["track_name"] = json!(names.get(&key.0).cloned().unwrap_or_default());
    Ok((playable, report))
}

/// Apply the real extraction and pitch fitting pipeline to one part, independently
/// of melody ranking. Zero retained notes is a valid fit result (prepare rejects it).
pub fn fit_part(raw: &[Note], options: &Options) -> Result<(Vec<Note>, Value)> {
    options.validate()?;
    let notes = simplify(raw, &options.melody_mode, options.trim_silence);
    ensure!(!notes.is_empty(), "所选声部提取后没有可用音符。");
    let weights = note_weights(&notes);
    let available: Vec<_> = if options.auto_octave {
        shifts().collect()
    } else {
        vec![0]
    };
    let weighted = |shift: i32| {
        notes
            .iter()
            .zip(&weights)
            .filter(|(n, _)| {
                (MIN_PITCH..=MAX_PITCH).contains(&(n.pitch + options.transpose + shift))
            })
            .map(|(_, w)| *w)
            .sum::<f64>()
    };
    let octave = *available
        .iter()
        .max_by(|&&a, &&b| {
            weighted(a)
                .total_cmp(&weighted(b))
                .then(b.abs().cmp(&a.abs()))
                .then(b.cmp(&a))
        })
        .unwrap();
    let shift = options.transpose + octave;
    let mut playable: Vec<_> = notes
        .iter()
        .filter_map(|n| {
            let pitch = n.pitch + shift;
            (MIN_PITCH..=MAX_PITCH)
                .contains(&pitch)
                .then(|| Note { pitch, ..n.clone() })
        })
        .collect();
    let mut adjustments = Vec::new();
    if options.phrase_octave && playable.len() < notes.len() {
        (playable, adjustments) = fit_phrases(&notes, shift);
    }
    for n in &mut playable {
        n.start /= options.speed;
        n.end /= options.speed;
    }
    for a in &mut adjustments {
        a["start"] = json!(a["start"].as_f64().unwrap() / options.speed);
        a["end"] = json!(a["end"].as_f64().unwrap() / options.speed);
    }
    let adjusted: usize = adjustments
        .iter()
        .map(|a| a["notes"].as_u64().unwrap_or(0) as usize)
        .sum();
    let report = json!({"source_notes":raw.len(),"extracted_notes":notes.len(),"range_retention":playable.len() as f64 / notes.len() as f64,"source_retention":playable.len() as f64 / raw.len() as f64,"melody_notes":playable.len(),"removed_polyphony":raw.len()-notes.len(),"dropped_out_of_range":notes.len()-playable.len(),"transpose_semitones":shift,"speed":options.speed,"octave_adjustments":adjustments,"phrase_adjusted_notes":adjusted,"melody_mode":options.melody_mode});
    Ok((playable, report))
}
