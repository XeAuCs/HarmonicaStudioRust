use crate::models::Note;
pub const LONG_REST_THRESHOLD: f64 = 3.0;
pub const RETAINED_REST: f64 = 0.6;
pub const REST_EPSILON: f64 = 1e-8;
pub fn compress_long_rests(notes: &[Note], enabled: bool) -> (Vec<Note>, usize, f64) {
    let mut shifted = Vec::with_capacity(notes.len());
    let mut removed = 0.0;
    let mut count = 0;
    let mut previous_end = None;
    for note in notes {
        if let Some(previous) = previous_end.filter(|_| enabled) {
            let gap = note.start - previous;
            if gap > LONG_REST_THRESHOLD + REST_EPSILON {
                removed += gap - RETAINED_REST;
                count += 1;
            }
        }
        shifted.push(Note {
            start: note.start - removed,
            end: note.end - removed,
            ..note.clone()
        });
        previous_end = Some(note.end);
    }
    (shifted, count, removed)
}
