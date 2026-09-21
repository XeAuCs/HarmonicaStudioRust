//! Interpretable 0–100 melody preference, not a confidence probability.
use super::{PartKey, Parts, TrackNames, median, part_features, shifts};
use crate::notes::{MAX_PITCH, MIN_PITCH};

pub fn rank_parts(parts: &Parts, names: &TrackNames) -> Vec<(PartKey, f64)> {
    let song_start = parts
        .values()
        .flatten()
        .map(|n| n.start)
        .fold(f64::INFINITY, f64::min);
    let song_end = parts.values().flatten().map(|n| n.end).fold(0.0, f64::max);
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
        let reachable = shifts()
            .map(|shift| {
                notes
                    .iter()
                    .filter(|n| (MIN_PITCH..=MAX_PITCH).contains(&(n.pitch + shift)))
                    .count()
            })
            .max()
            .unwrap_or(0) as f64
            / notes.len() as f64;
        let mut pitches = std::collections::BTreeMap::new();
        for n in notes {
            *pitches.entry(n.pitch).or_insert(0usize) += 1;
        }
        let dominant = *pitches.values().max().unwrap() as f64 / notes.len() as f64;
        let variety = ((1.0 - dominant) / 0.7).clamp(0.0, 1.0);
        // Prefer a singable middle register over either bass or very high ornaments.
        let center = median(notes.iter().map(|n| n.pitch as f64));
        let register = (1.0 - (center - 72.0).abs() / 36.0).clamp(0.0, 1.0);
        let count_evidence = (notes.len() as f64 / 32.0).sqrt().min(1.0);
        let coverage = f.coverage.clamp(0.0, 1.0).sqrt();
        let density = notes.len() as f64 / (f.coverage * (song_end - song_start)).max(0.1);
        let busy_penalty = ((density - 8.0) / 16.0).clamp(0.0, 1.0) * 8.0;
        let preference = 30.0 * f.monophony
            + 20.0 * coverage
            + 12.0 * f.continuity
            + 12.0 * reachable
            + 10.0 * variety
            + 10.0 * register
            + 6.0 * f.duration
            + if named { 5.0 } else { 0.0 }
            - if accompaniment { 5.0 } else { 0.0 }
            - busy_penalty;
        // Sparse fragments and a single held note should not win on monophony alone.
        let score = (preference
            * (0.35 + 0.65 * count_evidence)
            * (0.65 + 0.35 * coverage)
            * (0.75 + 0.25 * variety))
            .clamp(0.0, 100.0);
        ranked.push((key, (score * 10.0).round() / 10.0));
    }
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked
}
