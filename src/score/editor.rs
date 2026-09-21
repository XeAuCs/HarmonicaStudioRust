//! Pure piano-roll interaction model. All committed notes use the shared score validator.
use crate::models::Note;
use crate::notes::{
    MAX_EDITOR_PITCH, MAX_PITCH, MAX_SECONDS, MIN_EDITOR_PITCH, MIN_PITCH, normalize_editor_notes,
};
use anyhow::{Result, bail};

pub const LABEL_WIDTH: f64 = 136.0;
pub const COMPACT_LABEL_WIDTH: f64 = 12.0;
pub const RULER_HEIGHT: f64 = 28.0;
pub const DEFAULT_ROW_HEIGHT: f64 = 20.0;
pub const DEFAULT_TIME_ZOOM: f64 = 70.0;
pub const SNAP: f64 = 0.05;
pub const MIN_LENGTH: f64 = 0.025;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PianoKeyRect {
    pub pitch: i32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Copy, Debug)]
struct PitchView {
    row_height: f64,
    offset: f64,
    fitted: bool,
}
#[derive(Clone, Debug)]
struct History {
    notes: Vec<Note>,
    out_of_range_notes: Vec<Note>,
    selected: Option<usize>,
}
#[derive(Clone, Debug)]
enum Drag {
    Note {
        before: History,
        index: usize,
        x: f64,
        y: f64,
        resize: bool,
        time_offset: f64,
        pitch_offset: f64,
    },
    Pan {
        x: f64,
        original_offset: f64,
        moved: bool,
        ruler: bool,
    },
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Interaction {
    pub pause: bool,
    pub seek: Option<f64>,
    pub changed: bool,
}

#[derive(Clone, Debug)]
pub struct EditorModel {
    pub notes: Vec<Note>,
    /// Editable hollow notes, excluded from playback until moved into range.
    pub out_of_range_notes: Vec<Note>,
    pub selected: Option<usize>,
    pub zoom: f64,
    /// Timeline offset in seconds. Half a viewport of negative padding is valid.
    pub offset: f64,
    pub width: f64,
    pub height: f64,
    pub low_pitch: i32,
    pub high_pitch: i32,
    /// Vertical scrollbar position in pixels, independent of time zoom.
    pub pitch_offset: f64,
    pub position: f64,
    pub highlight: Option<f64>,
    pub compact: bool,
    pub follow: bool,
    pub read_only: bool,
    pub allow_note_edits: bool,
    pitch_row: f64,
    fit_pitch_mode: bool,
    pitch_view_before_compact: Option<PitchView>,
    centered_position: bool,
    undo: Vec<History>,
    redo: Vec<History>,
    drag: Option<Drag>,
}
impl Default for EditorModel {
    fn default() -> Self {
        Self {
            notes: Vec::new(),
            out_of_range_notes: Vec::new(),
            selected: None,
            zoom: DEFAULT_TIME_ZOOM,
            offset: 0.0,
            width: 800.0,
            height: 420.0,
            low_pitch: MIN_PITCH,
            high_pitch: MAX_PITCH,
            pitch_offset: 0.0,
            position: 0.0,
            highlight: None,
            compact: false,
            follow: true,
            read_only: false,
            allow_note_edits: true,
            pitch_row: DEFAULT_ROW_HEIGHT,
            fit_pitch_mode: false,
            pitch_view_before_compact: None,
            centered_position: false,
            undo: Vec::new(),
            redo: Vec::new(),
            drag: None,
        }
    }
}
impl EditorModel {
    /// Selection indices address solid notes first, then hollow notes.
    pub fn editable_notes(&self) -> Vec<Note> {
        self.notes
            .iter()
            .chain(&self.out_of_range_notes)
            .cloned()
            .collect()
    }
    fn note_at(&self, index: usize) -> &Note {
        if index < self.notes.len() {
            &self.notes[index]
        } else {
            &self.out_of_range_notes[index - self.notes.len()]
        }
    }
    fn assign_notes(&mut self, notes: Vec<Note>, selected: Option<Note>) {
        (self.notes, self.out_of_range_notes) = notes
            .into_iter()
            .partition(|n| (MIN_PITCH..=MAX_PITCH).contains(&n.pitch));
        self.selected = selected.and_then(|target| {
            self.notes
                .iter()
                .chain(&self.out_of_range_notes)
                .position(|n| *n == target)
        });
        // Preserve the viewport during edits while allowing newly moved notes
        // to be reached by scrolling. Document changes reset these bounds.
        let old_high = self.high_pitch;
        for n in &self.out_of_range_notes {
            self.low_pitch = self.low_pitch.min(n.pitch);
            self.high_pitch = self.high_pitch.max(n.pitch);
        }
        self.pitch_offset += f64::from(self.high_pitch - old_high) * self.row_height();
    }
    pub fn set_document(&mut self, notes: &[Note], highlight: Option<f64>) {
        self.out_of_range_notes.clear();
        self.low_pitch = MIN_PITCH;
        self.high_pitch = MAX_PITCH;
        self.notes = notes.to_vec();
        self.highlight = highlight;
        self.selected = None;
        self.undo.clear();
        self.redo.clear();
        self.drag = None;
        self.offset = 0.0;
        self.position = 0.0;
        self.centered_position = false;
        if self.fit_pitch_mode {
            self.fit_pitches();
        } else {
            self.center_pitches();
        }
    }
    pub fn sync_notes(&mut self, notes: &[Note], highlight: Option<f64>) {
        if self.notes != notes {
            self.notes = notes.to_vec();
            self.selected = None;
        }
        self.highlight = highlight;
        self.clamp_pitch_offset();
    }
    pub fn duration(&self) -> f64 {
        self.notes
            .iter()
            .chain(&self.out_of_range_notes)
            .map(|n| n.end)
            .fold(0.0, f64::max)
    }
    pub fn set_range_hints(&mut self, report: Option<&serde_json::Value>) {
        self.out_of_range_notes = Self::read_range_hints(report);
        self.low_pitch = self
            .out_of_range_notes
            .iter()
            .map(|n| n.pitch)
            .min()
            .unwrap_or(MIN_PITCH)
            .min(MIN_PITCH);
        self.high_pitch = self
            .out_of_range_notes
            .iter()
            .map(|n| n.pitch)
            .max()
            .unwrap_or(MAX_PITCH)
            .max(MAX_PITCH);
        if self.fit_pitch_mode {
            self.fit_pitches();
        } else {
            self.center_pitches();
        }
    }
    pub fn sync_range_hints(&mut self, report: Option<&serde_json::Value>) {
        let hints = Self::read_range_hints(report);
        if hints != self.out_of_range_notes {
            self.assign_notes(self.notes.iter().chain(&hints).cloned().collect(), None);
        }
    }
    fn read_range_hints(report: Option<&serde_json::Value>) -> Vec<Note> {
        let mut hints = report
            .and_then(|r| r.get("out_of_range_notes"))
            .and_then(|v| serde_json::from_value::<Vec<Note>>(v.clone()).ok())
            .unwrap_or_default();
        hints.truncate(crate::notes::MAX_NOTES);
        hints.retain(|n| {
            (MIN_EDITOR_PITCH..=MAX_EDITOR_PITCH).contains(&n.pitch)
                && !(MIN_PITCH..=MAX_PITCH).contains(&n.pitch)
                && n.start.is_finite()
                && n.end.is_finite()
                && n.start >= 0.0
                && n.end > n.start
                && n.end <= MAX_SECONDS
                && (1..=127).contains(&n.velocity)
        });
        hints.sort_by(|a, b| a.start.total_cmp(&b.start));
        hints
    }
    pub fn row_height(&self) -> f64 {
        self.pitch_row
    }
    pub fn left(&self) -> f64 {
        if self.compact {
            COMPACT_LABEL_WIDTH
        } else {
            LABEL_WIDTH
        }
    }
    pub fn visible_seconds(&self) -> f64 {
        ((self.width - self.left()).max(1.0) / self.zoom).max(0.001)
    }
    pub fn x_at(&self, time: f64) -> f64 {
        self.left() + (time - self.offset) * self.zoom
    }
    pub fn time_at(&self, x: f64) -> f64 {
        (self.offset + (x - self.left()) / self.zoom).clamp(0.0, MAX_SECONDS)
    }
    pub fn y_at(&self, pitch: i32) -> f64 {
        RULER_HEIGHT + f64::from(self.high_pitch - pitch) * self.row_height() - self.pitch_offset
    }
    pub fn pitch_center(&self, pitch: i32) -> f64 {
        self.y_at(pitch) + self.row_height() / 2.0
    }
    pub fn pitch_at(&self, y: f64) -> i32 {
        (self.high_pitch
            - ((y - RULER_HEIGHT + self.pitch_offset) / self.row_height()).floor() as i32)
            .clamp(self.low_pitch, self.high_pitch)
    }
    pub fn note_height(&self) -> f64 {
        if self.compact {
            6.0f64.min(self.row_height() - 2.0)
        } else {
            (self.row_height() - 6.0f64.min(self.row_height() * 0.3)).max(2.0)
        }
    }
    pub fn note_inset(&self) -> f64 {
        (self.row_height() - self.note_height()) / 2.0
    }
    /// Generous interaction rectangle; drawing should retain the true time width.
    pub fn note_rect(&self, note: &Note) -> (f64, f64, f64, f64) {
        (
            self.x_at(note.start),
            self.y_at(note.pitch) + self.note_inset(),
            ((note.end - note.start) * self.zoom).max(4.0),
            self.note_height(),
        )
    }
    pub fn piano_geometry(&self) -> (Vec<PianoKeyRect>, Vec<PianoKeyRect>) {
        let sharp = |pitch: i32| [1, 3, 6, 8, 10].contains(&pitch.rem_euclid(12));
        let naturals: Vec<_> = (self.low_pitch - 4..=self.high_pitch + 4)
            .filter(|&p| !sharp(p))
            .collect();
        let white = naturals
            .windows(3)
            .map(|p| {
                let center = self.pitch_center(p[1]);
                let top = (center + self.pitch_center(p[2])) / 2.0;
                let bottom = (center + self.pitch_center(p[0])) / 2.0;
                PianoKeyRect {
                    pitch: p[1],
                    x: 0.0,
                    y: top,
                    width: 96.0,
                    height: bottom - top,
                }
            })
            .collect();
        let black = (self.low_pitch..=self.high_pitch)
            .filter(|&p| sharp(p))
            .map(|pitch| PianoKeyRect {
                pitch,
                x: 0.0,
                y: self.pitch_center(pitch) - self.row_height() * 0.42,
                width: 62.0,
                height: self.row_height() * 0.84,
            })
            .collect();
        (white, black)
    }
    pub fn pitch_scroll_max(&self) -> f64 {
        (f64::from(self.high_pitch - self.low_pitch + 1) * self.row_height()
            - (self.height - RULER_HEIGHT).max(1.0))
        .max(0.0)
    }
    pub fn pitch_scroll_fraction(&self) -> f64 {
        let maximum = self.pitch_scroll_max();
        if maximum > 0.0 {
            (self.pitch_offset / maximum).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
    pub fn set_pitch_scroll_fraction(&mut self, fraction: f64) {
        if fraction.is_finite() {
            self.pitch_offset = fraction.clamp(0.0, 1.0) * self.pitch_scroll_max();
        }
    }
    pub fn pan_pitches(&mut self, delta_pixels: f64) {
        if delta_pixels.is_finite() {
            self.pitch_offset =
                (self.pitch_offset + delta_pixels).clamp(0.0, self.pitch_scroll_max());
        }
    }
    fn clamp_pitch_offset(&mut self) {
        self.pitch_offset = self.pitch_offset.clamp(0.0, self.pitch_scroll_max());
    }
    fn center_pitches(&mut self) {
        let pitch = match (
            self.notes.iter().map(|n| n.pitch).min(),
            self.notes.iter().map(|n| n.pitch).max(),
        ) {
            (Some(low), Some(high)) => f64::from(low + high) / 2.0,
            _ => 64.0,
        };
        self.pitch_offset = ((f64::from(self.high_pitch) - pitch + 0.5) * self.row_height()
            - (self.height - RULER_HEIGHT) / 2.0)
            .round_ties_even();
        self.clamp_pitch_offset();
    }
    pub fn resize(&mut self, width: f64, height: f64) {
        if !width.is_finite() || !height.is_finite() {
            return;
        }
        let width = width.max(1.0);
        let height = height.max(1.0);
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        if self.fit_pitch_mode {
            self.fit_pitches();
        } else {
            self.clamp_pitch_offset();
        }
        self.update_timeline_view();
    }
    pub fn fit_pitches(&mut self) {
        self.cancel_drag();
        self.fit_pitch_mode = true;
        // Only playable (solid) notes determine fitting. Hints merely extend
        // the scrollable range, so an extreme lost note cannot shrink the score.
        let low = self.notes.iter().map(|n| n.pitch).min().unwrap_or(60);
        let high = self.notes.iter().map(|n| n.pitch).max().unwrap_or(71);
        let count = (high + 1).min(MAX_PITCH) - (low - 1).max(MIN_PITCH) + 1;
        let available = (self.height - RULER_HEIGHT - 2.0).max(1.0);
        self.pitch_row = (available / f64::from(count.max(1)))
            .floor()
            .clamp(5.0, 44.0);
        self.center_pitches();
    }
    pub fn set_pitch_zoom(&mut self, row_height: f64) {
        if !row_height.is_finite() {
            return;
        }
        self.cancel_drag();
        let height = (self.height - RULER_HEIGHT).max(1.0);
        let center = (self.pitch_offset + height / 2.0) / self.row_height();
        self.fit_pitch_mode = false;
        self.pitch_row = row_height.round_ties_even().clamp(5.0, 44.0);
        self.pitch_offset = (center * self.row_height() - height / 2.0).round_ties_even();
        self.clamp_pitch_offset();
    }
    pub fn pitch_zoom_in(&mut self) {
        self.set_pitch_zoom(self.row_height() * 1.25);
    }
    pub fn pitch_zoom_out(&mut self) {
        self.set_pitch_zoom(self.row_height() / 1.25);
    }
    pub fn set_compact(&mut self, compact: bool) {
        if self.compact == compact {
            return;
        }
        self.cancel_drag();
        self.selected = None;
        if compact {
            self.pitch_view_before_compact = Some(PitchView {
                row_height: self.row_height(),
                offset: self.pitch_offset,
                fitted: self.fit_pitch_mode,
            });
            self.compact = true;
            self.fit_pitches();
        } else {
            self.compact = false;
            if let Some(view) = self.pitch_view_before_compact.take() {
                self.pitch_row = view.row_height;
                self.pitch_offset = view.offset;
                self.fit_pitch_mode = view.fitted;
                if self.fit_pitch_mode {
                    self.fit_pitches();
                } else {
                    self.clamp_pitch_offset();
                }
            }
        }
        self.update_timeline_view();
    }
    fn update_timeline_view(&mut self) {
        if self.centered_position && self.follow {
            self.offset = self.position - self.visible_seconds() / 2.0;
        } else {
            self.clamp_time_offset();
        }
    }
    fn clamp_time_offset(&mut self) {
        let half = self.visible_seconds() / 2.0;
        let total = (self.duration() + 3.0).max(8.0).min(MAX_SECONDS);
        self.offset = self.offset.clamp(-half, total - half);
    }
    pub fn fit_time(&mut self) {
        self.centered_position = false;
        self.offset = 0.0;
        self.zoom =
            ((self.width - self.left()).max(1.0) / self.duration().max(4.0)).clamp(20.0, 350.0);
    }
    pub fn zoom_by(&mut self, factor: f64) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let anchor_x = (self.width / 2.0).max(self.left());
        let anchor = self.time_at(anchor_x);
        self.zoom = (self.zoom * factor).clamp(20.0, 350.0);
        self.offset = anchor - (anchor_x - self.left()) / self.zoom;
        self.update_timeline_view();
    }
    pub fn pan_seconds(&mut self, delta: f64) {
        if !delta.is_finite() {
            return;
        }
        self.offset += delta;
        self.clamp_time_offset();
        if self.centered_position {
            self.position =
                (self.offset + self.visible_seconds() / 2.0).clamp(0.0, self.duration());
            self.update_timeline_view();
        }
    }
    pub fn has_position(&self) -> bool {
        self.centered_position
    }
    pub fn sync_transport(&mut self, position: f64, show_cursor: bool, playing: bool) {
        if playing {
            self.follow_playback_position(position);
        } else if !self.is_dragging() {
            if show_cursor {
                self.follow_position(position, true);
            } else {
                // Invalidating a preview hides the cursor, not the user's viewport.
                self.centered_position = false;
                if position.is_finite() {
                    self.position = position.clamp(0.0, MAX_SECONDS);
                }
            }
        }
    }
    pub fn reset_timeline(&mut self) {
        self.cancel_drag();
        self.centered_position = false;
        self.position = 0.0;
        self.offset = 0.0;
    }
    pub fn follow_position(&mut self, position: f64, playing: bool) {
        self.set_follow_position(position, playing, false);
    }
    /// Playback is authoritative while the transport is running.  A stale
    /// pointer capture must not freeze the visible score at its old position.
    pub fn follow_playback_position(&mut self, position: f64) {
        self.set_follow_position(position, true, true);
    }
    fn set_follow_position(&mut self, position: f64, playing: bool, ignore_drag: bool) {
        if !position.is_finite() || (!ignore_drag && self.drag.is_some()) {
            return;
        }
        let position = position.clamp(0.0, MAX_SECONDS);
        if playing || position != self.position {
            self.centered_position = true;
        }
        self.position = position;
        self.update_timeline_view();
    }
    pub fn active_pitch(&self) -> Option<i32> {
        if !self.has_position() {
            return None;
        }
        let index = self
            .notes
            .partition_point(|n| n.start <= self.position)
            .checked_sub(1)?;
        let note = &self.notes[index];
        (self.position < note.end).then_some(note.pitch)
    }
    pub fn hit_test(&self, x: f64, y: f64) -> Option<usize> {
        if !x.is_finite()
            || !y.is_finite()
            || x < self.left()
            || x > self.width
            || y < RULER_HEIGHT
            || y > self.height
        {
            return None;
        }
        let time = self.time_at(x);
        let first = self
            .notes
            .partition_point(|n| n.end < time - 5.0 / self.zoom);
        for (index, note) in self.notes.iter().enumerate().skip(first) {
            if note.start > time + 1.0 / self.zoom {
                break;
            }
            let (left, top, width, height) = self.note_rect(note);
            if x >= left - 1.0
                && x <= left + width + 1.0
                && y >= top - 2.0
                && y <= top + height + 2.0
            {
                return Some(index);
            }
        }
        for (index, note) in self.out_of_range_notes.iter().enumerate() {
            let (left, top, width, height) = self.note_rect(note);
            if x >= left - 1.0
                && x <= left + width + 1.0
                && y >= top - 2.0
                && y <= top + height + 2.0
            {
                return Some(self.notes.len() + index);
            }
        }
        None
    }
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn begin_pointer(&mut self, x: f64, y: f64, force_pan: bool) {
        if self.read_only
            || (self.notes.is_empty() && self.out_of_range_notes.is_empty())
            || x < self.left()
            || x > self.width
            || y < 0.0
            || y > self.height
        {
            return;
        }
        if !force_pan && !self.compact && self.allow_note_edits {
            if let Some(index) = self.hit_test(x, y) {
                self.selected = Some(index);
                let (left, _, width, _) = self.note_rect(self.note_at(index));
                let resize = left + width - x <= 7.0f64.min(width * 0.3);
                self.centered_position = false;
                self.drag = Some(Drag::Note {
                    before: self.snapshot(),
                    index,
                    x,
                    y,
                    resize,
                    time_offset: self.offset,
                    pitch_offset: self.pitch_offset,
                });
                return;
            }
        }
        self.selected = None;
        self.drag = Some(Drag::Pan {
            x,
            original_offset: self.offset,
            moved: false,
            ruler: y < RULER_HEIGHT,
        });
    }
    pub fn move_pointer(&mut self, x: f64, y: f64) -> Interaction {
        let Some(drag) = self.drag.clone() else {
            return Interaction::default();
        };
        match drag {
            Drag::Pan {
                x: start,
                original_offset,
                moved,
                ruler,
            } => {
                let did_move = moved || (x - start).abs() > 4.0;
                if did_move {
                    self.position =
                        (original_offset + (start - x) / self.zoom + self.visible_seconds() / 2.0)
                            .clamp(0.0, self.duration());
                    self.centered_position = true;
                    self.offset = self.position - self.visible_seconds() / 2.0;
                }
                self.drag = Some(Drag::Pan {
                    x: start,
                    original_offset,
                    moved: did_move,
                    ruler,
                });
                Interaction {
                    pause: did_move && !moved,
                    seek: None,
                    changed: false,
                }
            }
            Drag::Note {
                before,
                index,
                x: start_x,
                y: start_y,
                resize,
                time_offset,
                pitch_offset,
            } => {
                let original = if index < before.notes.len() {
                    &before.notes[index]
                } else {
                    &before.out_of_range_notes[index - before.notes.len()]
                };
                let dx = x - start_x + (self.offset - time_offset) * self.zoom;
                let dy = y - start_y + self.pitch_offset - pitch_offset;
                let delta = (dx / self.zoom / SNAP).round_ties_even() * SNAP;
                let mut changed = original.clone();
                if resize {
                    changed.end =
                        (original.end + delta).clamp(original.start + MIN_LENGTH, MAX_SECONDS);
                } else {
                    let length = original.end - original.start;
                    if dx.abs() >= 3.0 {
                        changed.start = (original.start + delta).clamp(0.0, MAX_SECONDS - length);
                        changed.end = changed.start + length;
                    }
                    changed.pitch = (original.pitch
                        - (dy / self.row_height()).round_ties_even() as i32)
                        .clamp(MIN_EDITOR_PITCH, MAX_EDITOR_PITCH);
                }
                self.notes = before.notes.clone();
                self.out_of_range_notes = before.out_of_range_notes.clone();
                if index < self.notes.len() {
                    self.notes[index] = changed;
                } else {
                    self.out_of_range_notes[index - self.notes.len()] = changed;
                }
                Interaction::default()
            }
        }
    }
    pub fn end_pointer(&mut self, x: f64) -> Result<Interaction> {
        let Some(drag) = self.drag.take() else {
            return Ok(Interaction::default());
        };
        match drag {
            Drag::Pan { moved, ruler, .. } => {
                if !moved && !ruler {
                    return Ok(Interaction::default());
                }
                let value = if moved {
                    self.position
                } else {
                    self.time_at(x).min(self.duration())
                };
                self.position = value;
                self.centered_position = true;
                self.update_timeline_view();
                Ok(Interaction {
                    seek: Some(value),
                    ..Interaction::default()
                })
            }
            Drag::Note { before, index, .. } => {
                if self.notes == before.notes
                    && self.out_of_range_notes == before.out_of_range_notes
                {
                    return Ok(Interaction::default());
                }
                let chosen = self.note_at(index).clone();
                match normalize_editor_notes(&self.editable_notes()) {
                    Ok(notes) => {
                        self.assign_notes(notes, Some(chosen));
                        self.push_history(before);
                        Ok(Interaction {
                            changed: true,
                            ..Interaction::default()
                        })
                    }
                    Err(error) => {
                        self.restore(before);
                        Err(error)
                    }
                }
            }
        }
    }
    pub fn cancel_drag(&mut self) {
        if let Some(Drag::Note { before, .. }) = self.drag.take() {
            self.restore(before);
        }
    }
    fn snapshot(&self) -> History {
        History {
            notes: self.notes.clone(),
            out_of_range_notes: self.out_of_range_notes.clone(),
            selected: self.selected,
        }
    }
    fn restore(&mut self, history: History) {
        let selected = history.selected.and_then(|i| {
            history
                .notes
                .iter()
                .chain(&history.out_of_range_notes)
                .nth(i)
                .cloned()
        });
        self.assign_notes(
            history
                .notes
                .into_iter()
                .chain(history.out_of_range_notes)
                .collect(),
            selected,
        );
    }
    fn push_history(&mut self, previous: History) {
        self.undo.push(previous);
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    fn commit(&mut self, notes: Vec<Note>, selected: Option<Note>) -> Result<bool> {
        if self.read_only || !self.allow_note_edits || self.compact {
            bail!("当前曲谱处于只读状态。");
        }
        let normalized = normalize_editor_notes(&notes)?;
        let mut current = self.editable_notes();
        current.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
        if normalized == current {
            return Ok(false);
        }
        let previous = self.snapshot();
        self.assign_notes(normalized, selected);
        self.push_history(previous);
        Ok(true)
    }
    // Keyboard edits supersede an unfinished pointer preview. Restore the
    // committed score first, then derive the new edit and its undo snapshot.
    fn begin_keyboard_edit(&mut self) -> Result<()> {
        if self.read_only || !self.allow_note_edits || self.compact {
            bail!("当前曲谱处于只读状态。");
        }
        self.cancel_drag();
        Ok(())
    }
    pub fn add_note(&mut self, time: f64, pitch: i32) -> Result<bool> {
        self.begin_keyboard_edit()?;
        let start = (time / SNAP).round() * SNAP;
        let next = self
            .notes
            .iter()
            .find(|n| n.start >= start)
            .map_or(MAX_SECONDS, |n| n.start);
        if self.notes.iter().any(|n| n.start <= start && n.end > start) || next - start < MIN_LENGTH
        {
            bail!("此处没有足够的空白，请选择其他位置。");
        }
        let note = Note {
            pitch,
            start,
            end: (start + 0.4).min(next).min(MAX_SECONDS),
            velocity: 80,
        };
        let mut notes = self.editable_notes();
        notes.push(note.clone());
        self.commit(notes, Some(note))
    }
    pub fn delete_selected(&mut self) -> Result<bool> {
        if self.selected.is_none() {
            return Ok(false);
        }
        self.begin_keyboard_edit()?;
        let Some(index) = self.selected else {
            return Ok(false);
        };
        let mut notes = self.editable_notes();
        notes.remove(index);
        self.commit(notes, None)
    }
    pub fn nudge(&mut self, time: f64, pitch: i32, length: f64) -> Result<bool> {
        if self.selected.is_none() {
            return Ok(false);
        }
        self.begin_keyboard_edit()?;
        let Some(index) = self.selected else {
            return Ok(false);
        };
        let mut notes = self.editable_notes();
        let n = &mut notes[index];
        n.start += time;
        n.end += time + length;
        n.pitch += pitch;
        let selected = n.clone();
        self.commit(notes, Some(selected))
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() && !self.read_only && self.allow_note_edits && !self.compact
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty() && !self.read_only && self.allow_note_edits && !self.compact
    }
    pub fn undo(&mut self) -> bool {
        if self.begin_keyboard_edit().is_err() {
            return false;
        }
        if !self.can_undo() {
            return false;
        }
        let previous = self.undo.pop().unwrap();
        let current = self.snapshot();
        self.redo.push(current);
        self.restore(previous);
        true
    }
    pub fn redo(&mut self) -> bool {
        if self.begin_keyboard_edit().is_err() {
            return false;
        }
        if !self.can_redo() {
            return false;
        }
        let next = self.redo.pop().unwrap();
        let current = self.snapshot();
        self.undo.push(current);
        self.restore(next);
        true
    }
}

#[cfg(test)]
#[path = "editor/original_view_contract_tests.rs"]
mod original_view_contract_tests;
#[cfg(test)]
#[path = "editor/playback_guard_tests.rs"]
mod playback_guard_tests;
#[cfg(test)]
#[path = "editor/tests.rs"]
mod tests;
