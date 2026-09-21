use super::*;
fn note(pitch: i32, start: f64, end: f64) -> Note {
    Note {
        pitch,
        start,
        end,
        velocity: 80,
    }
}
#[test]
fn invalid_drag_rolls_back_without_polluting_history() {
    let mut e = EditorModel::default();
    e.set_document(&[note(60, 0.0, 0.5), note(62, 1.0, 1.5)], None);
    let (x, y) = (e.x_at(0.2), e.y_at(60) + e.row_height() / 2.0);
    e.begin_pointer(x, y, false);
    e.move_pointer(x + e.zoom, y);
    assert!(e.end_pointer(x + e.zoom).is_err());
    assert_eq!(e.notes[0].start, 0.0);
    assert!(!e.can_undo());
}
#[test]
fn edit_history_is_independent_and_document_switch_resets_it() {
    let mut e = EditorModel::default();
    e.add_note(0.0, 60).unwrap();
    e.nudge(0.0, 2, 0.0).unwrap();
    assert!(e.undo());
    assert_eq!(e.notes[0].pitch, 60);
    assert!(e.redo());
    assert_eq!(e.notes[0].pitch, 62);
    e.set_document(&[note(70, 2.0, 2.5)], Some(2.1));
    assert!(!e.can_undo());
    assert!(!e.can_redo());
}
#[test]
fn panning_pauses_once_and_commits_one_seek_without_editing() {
    let mut e = EditorModel::default();
    e.set_document(&[note(60, 1.0, 2.0)], None);
    let original = e.notes.clone();
    e.begin_pointer(400.0, 10.0, false);
    assert!(!e.move_pointer(402.0, 10.0).pause);
    assert!(e.move_pointer(380.0, 10.0).pause);
    assert!(!e.move_pointer(350.0, 10.0).pause);
    assert!(e.end_pointer(350.0).unwrap().seek.is_some());
    assert_eq!(e.notes, original);
    assert!(!e.can_undo());
}
#[test]
fn adding_in_occupied_time_rejects_and_gap_caps_length() {
    let mut e = EditorModel::default();
    e.set_document(&[note(60, 0.2, 0.5)], None);
    assert!(e.add_note(0.3, 62).is_err());
    e.add_note(0.0, 62).unwrap();
    assert_eq!(e.notes[0].end, 0.2);
}
#[test]
fn following_and_zoom_do_not_change_score_time() {
    let mut e = EditorModel::default();
    e.set_document(&[note(60, 20.0, 20.5)], None);
    let original = e.notes.clone();
    e.follow_position(20.0, true);
    assert!(e.offset > 0.0);
    e.zoom_by(2.0);
    assert_eq!(e.notes, original);
}
#[test]
fn readonly_prevents_edit_and_history_mutations() {
    let mut e = EditorModel::default();
    e.add_note(0.0, 60).unwrap();
    e.read_only = true;
    assert!(!e.undo());
    assert!(e.delete_selected().is_err());
    e.begin_pointer(80.0, 50.0, false);
    assert!(!e.is_dragging());
}
