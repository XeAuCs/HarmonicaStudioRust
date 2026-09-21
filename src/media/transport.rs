use crate::{
    models::Note,
    rests::{LONG_REST_THRESHOLD, REST_EPSILON, RETAINED_REST},
};
use std::time::Instant;
pub fn playback_anchors(notes: &[Note], actual: &[Note]) -> Vec<(f64, f64)> {
    let pairs: Vec<_> = notes.iter().zip(actual).collect();
    let mut anchors: Vec<_> = pairs.iter().map(|(n, p)| (n.start, p.start)).collect();
    for pair in pairs.windows(2) {
        let (n, p) = pair[0];
        let (next, next_played) = pair[1];
        if next.start - n.end > LONG_REST_THRESHOLD + REST_EPSILON
            && next_played.start - p.end <= RETAINED_REST + 0.002
        {
            anchors.push((n.end, p.end));
        }
    }
    if let Some((n, p)) = pairs.last() {
        anchors.push((n.end, p.end));
    }
    anchors
}
#[derive(Clone, Debug, Default)]
pub struct TimeMap {
    pub points: Vec<(f64, f64)>,
}
impl TimeMap {
    pub fn new(anchors: impl IntoIterator<Item = (f64, f64)>) -> Self {
        let mut points: Vec<_> = anchors.into_iter().collect();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut unique: Vec<(f64, f64)> = Vec::new();
        for point in points {
            if unique.last().is_some_and(|p| p.0 == point.0) {
                *unique.last_mut().unwrap() = point;
            } else {
                unique.push(point);
            }
        }
        Self { points: unique }
    }
    pub fn map(&self, value: f64) -> f64 {
        if self.points.is_empty() {
            return value.max(0.0);
        }
        let first = self.points[0];
        if value < first.0 {
            return (first.1 + value - first.0).max(0.0);
        }
        let i = self
            .points
            .partition_point(|p| p.0 <= value)
            .saturating_sub(1);
        let (x, y) = self.points[i];
        if let Some(&(x2, y2)) = self.points.get(i + 1) {
            y + (value - x) / (x2 - x) * (y2 - y)
        } else {
            y + value - x
        }
    }
}
#[derive(Clone, Debug)]
pub struct PlaybackClock {
    origin: Instant,
    position: f64,
    sampled_at: f64,
    playing: bool,
    last: f64,
    correction: f64,
}
impl Default for PlaybackClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            position: 0.0,
            sampled_at: 0.0,
            playing: false,
            last: 0.0,
            correction: 0.0,
        }
    }
}
impl PlaybackClock {
    pub fn reset(&mut self, position: f64, playing: bool) {
        self.reset_at(position, playing, self.origin.elapsed().as_secs_f64());
    }
    pub fn reset_at(&mut self, position: f64, playing: bool, now: f64) {
        self.position = position.max(0.0);
        self.sampled_at = now;
        self.playing = playing;
        self.last = self.position;
        self.correction = 0.0;
    }
    pub fn synchronize(&mut self, position: f64, playing: bool, discontinuity: bool) {
        self.synchronize_at(
            position,
            playing,
            discontinuity,
            self.origin.elapsed().as_secs_f64(),
        );
    }
    pub fn synchronize_at(&mut self, position: f64, playing: bool, discontinuity: bool, now: f64) {
        let actual = position.max(0.0);
        let estimated = self.position_at(now, None);
        if discontinuity || !playing || !self.playing || (actual - estimated).abs() > 0.25 {
            self.reset_at(actual, playing, now);
        } else {
            self.position = estimated;
            self.sampled_at = now;
            self.correction = actual - estimated;
            self.playing = true;
        }
    }
    pub fn position(&mut self, duration: Option<f64>) -> f64 {
        self.position_at(self.origin.elapsed().as_secs_f64(), duration)
    }
    pub fn position_at(&mut self, now: f64, duration: Option<f64>) -> f64 {
        let elapsed = if self.playing {
            (now - self.sampled_at).max(0.0)
        } else {
            0.0
        };
        let mut value = self.position + elapsed + self.correction * (elapsed / 0.12).min(1.0);
        if self.playing {
            value = value.max(self.last);
        }
        if let Some(duration) = duration {
            value = value.min(duration.max(0.0));
        }
        self.last = value;
        value
    }
}
