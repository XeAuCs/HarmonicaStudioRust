//! Interpretable melody preference, independent of conversion/range settings.
use super::{PartKey, Parts, TrackNames, median, part_features};

pub fn rank_parts(parts: &Parts, names: &TrackNames) -> Vec<(PartKey, f64)> {
    let song_start = parts
        .values()
        .flatten()
        .map(|n| n.start)
        .fold(f64::INFINITY, f64::min);
    let song_end = parts.values().flatten().map(|n| n.end).fold(0.0, f64::max);
    let centers: std::collections::BTreeMap<_, _> = parts
        .iter()
        .filter(|(_, n)| !n.is_empty())
        .map(|(&key, notes)| (key, median(notes.iter().map(|n| n.pitch as f64))))
        .collect();
    let upper_center = centers.values().copied().fold(0.0, f64::max);
    let mut ranked = Vec::new();
    for (&key, notes) in parts.iter().filter(|(_, notes)| !notes.is_empty()) {
        let f = part_features(notes, song_start, song_end);
        let label = names
            .get(&key.0)
            .map_or(String::new(), |s| s.to_lowercase());
        let named = ["melody", "vocal", "lead", "主旋律", "人声"]
            .iter()
            .any(|s| label.contains(s));
        let accompaniment = ["bass", "drum", "伴奏", "贝斯", "低音", "和弦"]
            .iter()
            .any(|s| label.contains(s));
        let mut pitches = std::collections::BTreeMap::new();
        for n in notes {
            *pitches.entry(n.pitch).or_insert(0usize) += 1;
        }
        let dominant = *pitches.values().max().unwrap() as f64 / notes.len() as f64;
        let variety = ((1.0 - dominant) / 0.7).clamp(0.0, 1.0);
        let center = centers[&key];
        let register = (1.0 - (center - 72.0).abs() / 36.0).clamp(0.0, 1.0);
        let relative_low = ((upper_center - center) / 12.0).clamp(0.0, 1.0);
        // Repeated stepwise tunes remain valid melodies. A penalty needs a joint
        // pitch/rhythm cycle, broken-chord movement, and supporting context.
        let accompaniment_penalty = 16.0
            * f.cyclic_repetition
            * f.arpeggio_motion
            * (0.35 + 0.65 * relative_low)
            * (f.onset_rate / 2.0).min(1.0);
        let busy_penalty = ((f.onset_rate - 8.0) / 16.0).clamp(0.0, 1.0) * 8.0;
        // Global span contributes once, weakly. Segments are measured relative
        // to this part's entrance, so late melodies are not penalised twice.
        let preference = 32.0 * f.monophony
            + 18.0 * f.segment_activity
            + 14.0 * f.continuity
            + 14.0 * register
            + 12.0 * variety
            + 10.0 * f.active_span_coverage.sqrt()
            + if named { 4.0 } else { 0.0 }
            - if accompaniment { 4.0 } else { 0.0 }
            - accompaniment_penalty
            - busy_penalty;
        let evidence = (notes.len() as f64 / 32.0).sqrt().min(1.0);
        let score = (preference * (0.35 + 0.65 * evidence)).clamp(0.0, 100.0);
        ranked.push((key, (score * 10.0).round() / 10.0));
    }
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked
}
