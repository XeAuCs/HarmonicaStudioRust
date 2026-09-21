//! Analysis-only features: never rewrite source note timing.
use super::{median, onset_groups, simplify};
use crate::models::Note;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default)]
pub struct PartFeatures {
    pub monophony: f64,
    /// Raw sounding-time coverage, retained for diagnostics, not a density denominator.
    pub coverage: f64,
    pub continuity: f64,
    pub register: f64,
    pub duration: f64,
    pub onset_rate: f64,
    pub active_span_coverage: f64,
    pub segment_activity: f64,
    pub cyclic_repetition: f64,
    pub arpeggio_motion: f64,
}

fn occupancy(mut events: Vec<(f64, i32)>) -> (f64, f64) {
    events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let (mut active, mut last, mut sounding, mut overlap) = (0, 0.0, 0.0, 0.0);
    for (time, change) in events {
        if active > 0 {
            sounding += time - last;
        }
        if active > 1 {
            overlap += time - last;
        }
        active += change;
        last = time;
    }
    (sounding, overlap)
}

pub fn part_features(notes: &[Note], song_start: f64, song_end: f64) -> PartFeatures {
    if notes.is_empty() {
        return PartFeatures::default();
    }
    let mut ordered = notes.to_vec();
    ordered.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    let groups = onset_groups(&ordered);
    let (sounding, _) = occupancy(
        ordered
            .iter()
            .flat_map(|n| [(n.start, 1), (n.end, -1)])
            .collect(),
    );
    let mut events = Vec::with_capacity(notes.len() * 2);
    for (i, group) in groups.iter().enumerate() {
        for n in *group {
            let mut end = n.end;
            if group.len() == 1 {
                if let Some(next) = groups.get(i + 1).filter(|g| g.len() == 1) {
                    let gap = next[0].start - n.start;
                    let tail = n.end - next[0].start;
                    // Only short tails between distinct singleton onsets, never chords.
                    if tail > 0.0 && tail <= (gap * 0.2).min(0.08) && n.end < next[0].end {
                        end = next[0].start;
                    }
                }
            }
            events.extend([(n.start, 1), (end, -1)]);
        }
    }
    let (effective, overlap) = occupancy(events);
    let onsets: Vec<f64> = groups.iter().map(|g| g[0].start).collect();
    let iois: Vec<f64> = onsets.windows(2).map(|w| w[1] - w[0]).collect();
    // Typical local onset rate; release length does not affect this feature.
    let onset_rate = if iois.is_empty() {
        0.0
    } else {
        1.0 / median(iois.iter().copied()).max(0.001)
    };
    let start = ordered[0].start;
    let end = ordered.iter().map(|n| n.end).fold(start, f64::max);
    let span = (end - start).max(0.001);
    // Occupied onset bins within this part's own active span, not the whole song.
    let bins: BTreeSet<_> = onsets
        .iter()
        .map(|t| ((t - start) / 4.0).floor() as usize)
        .collect();
    let segment_activity = bins.len() as f64 / (span / 4.0).ceil().max(1.0);
    let line = simplify(&ordered, "highest", false);
    let intervals: Vec<_> = line.windows(2).map(|w| w[1].pitch - w[0].pitch).collect();
    let rhythm: Vec<_> = line.windows(2).map(|w| w[1].start - w[0].start).collect();
    let mut cyclic_repetition: f64 = 0.0;
    // Require at least three cycles and matching pitch AND onset rhythm.
    for lag in 1..=8 {
        if intervals.len() < lag * 3 {
            continue;
        }
        let matches = (lag..intervals.len())
            .filter(|&i| {
                intervals[i] == intervals[i - lag]
                    && (rhythm[i] - rhythm[i - lag]).abs()
                        <= (rhythm[i].min(rhythm[i - lag]) * 0.1).max(0.015)
            })
            .count();
        cyclic_repetition = cyclic_repetition.max(matches as f64 / (intervals.len() - lag) as f64);
    }
    PartFeatures {
        monophony: (1.0 - overlap / effective.max(0.001)).clamp(0.0, 1.0),
        coverage: (sounding / (song_end - song_start).max(0.001)).clamp(0.0, 1.0),
        continuity: intervals
            .iter()
            .map(|i| (1.0 - i.abs() as f64 / 24.0).max(0.0))
            .sum::<f64>()
            / intervals.len().max(1) as f64,
        register: ((median(ordered.iter().map(|n| n.pitch as f64)) - 36.0) / 48.0).clamp(0.0, 1.0),
        duration: (median(ordered.iter().map(|n| n.end - n.start)) / 0.25).min(1.0),
        onset_rate,
        active_span_coverage: (span / (song_end - song_start).max(0.001)).clamp(0.0, 1.0),
        segment_activity: segment_activity.clamp(0.0, 1.0),
        cyclic_repetition,
        arpeggio_motion: intervals
            .iter()
            .filter(|i| (3..=12).contains(&i.abs()))
            .count() as f64
            / intervals.len().max(1) as f64,
    }
}
