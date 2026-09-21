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
fn inactive_transport_does_not_reset_edited_view_or_cancel_panning() {
    let mut e = EditorModel::default();
    e.set_document(&[note(60, 0.0, 0.5), note(62, 20.0, 20.5)], None);
    e.follow_position(20.0, true);
    let offset = e.offset;
    // Editing invalidates the audio preview, but is not a viewport reset.
    e.sync_transport(0.0, false, false);
    assert_eq!(e.offset, offset);
    e.begin_pointer(400.0, 10.0, true);
    e.move_pointer(440.0, 10.0);
    let panned = e.offset;
    e.sync_transport(0.0, false, false);
    assert!(e.is_dragging());
    assert_eq!(e.offset, panned);
    e.move_pointer(480.0, 10.0);
    assert!(e.offset < panned);
    assert!(e.end_pointer(480.0).unwrap().seek.is_some());
}
#[test]
fn inactive_transport_preserves_note_drag_after_edit() {
    let mut e = EditorModel::default();
    e.set_document(&[note(60, 0.0, 0.5), note(62, 20.0, 20.5)], None);
    e.follow_position(20.0, true);
    let (x, y) = drag_second(&mut e);
    e.move_pointer(x + e.zoom * 0.2, y);
    e.end_pointer(x).unwrap();
    let offset = e.offset;
    e.sync_transport(0.0, false, false);
    assert_eq!(e.offset, offset);
    let (x, y) = drag_second(&mut e);
    e.sync_transport(0.0, false, false);
    assert!(e.is_dragging());
    e.move_pointer(x + e.zoom * 0.2, y);
    assert!(e.end_pointer(x).unwrap().changed);
}
fn drag_second(e: &mut EditorModel) -> (f64, f64) {
    let n = &e.notes[1];
    let (x, y) = (
        e.x_at(n.start + 0.2),
        e.y_at(n.pitch) + e.row_height() / 2.0,
    );
    e.begin_pointer(x, y, false);
    assert!(matches!(e.drag, Some(Drag::Note { index: 1, .. })));
    (x, y)
}
#[test]
fn drag_delete_release_does_not_panic_and_undo_restores_once() {
    let mut e = EditorModel::default();
    let original = vec![note(60, 0.0, 0.5), note(62, 1.0, 1.5)];
    e.set_document(&original, None);
    let (x, _) = drag_second(&mut e);
    assert!(e.delete_selected().unwrap());
    assert!(!e.end_pointer(x).unwrap().changed);
    assert_eq!(e.notes, original[..1]);
    assert!(e.undo());
    assert_eq!(e.notes, original);
    assert!(!e.can_undo());
    assert!(e.redo());
    assert_eq!(e.notes, original[..1]);
}
#[test]
fn drag_delete_move_does_not_resurrect_notes() {
    let mut e = EditorModel::default();
    e.set_document(&[note(60, 0.0, 0.5), note(62, 1.0, 1.5)], None);
    let (x, y) = drag_second(&mut e);
    e.move_pointer(x + e.zoom * 0.2, y);
    e.delete_selected().unwrap();
    e.move_pointer(x + e.zoom * 0.4, y);
    assert_eq!(e.notes, vec![note(60, 0.0, 0.5)]);
    e.cancel_drag();
    assert_eq!(e.notes.len(), 1);
    assert!(e.undo());
    assert_eq!(e.notes[1], note(62, 1.0, 1.5));
}
#[test]
fn drag_keyboard_edit_cancels_preview_before_nudge_and_add() {
    let mut e = EditorModel::default();
    e.set_document(&[note(60, 0.0, 0.5), note(62, 1.0, 1.5)], None);
    let (x, y) = drag_second(&mut e);
    e.move_pointer(x + e.zoom * 0.2, y);
    e.nudge(0.0, 1, 0.0).unwrap();
    e.move_pointer(x + e.zoom * 0.4, y);
    assert!(!e.end_pointer(x).unwrap().changed);
    assert_eq!(e.notes[1], note(63, 1.0, 1.5));
    let (x, y) = drag_second(&mut e);
    e.move_pointer(x + e.zoom * 0.2, y);
    e.add_note(2.0, 65).unwrap();
    e.cancel_drag();
    assert_eq!(e.notes.len(), 3);
    assert_eq!(e.notes[1], note(63, 1.0, 1.5));
}
#[test]
fn drag_undo_redo_do_not_restore_stale_preview_on_release() {
    let mut e = EditorModel::default();
    e.set_document(&[note(60, 0.0, 0.5)], None);
    e.add_note(1.0, 62).unwrap();
    let (x, y) = drag_second(&mut e);
    e.move_pointer(x + e.zoom * 0.2, y);
    assert!(e.undo());
    assert!(!e.end_pointer(x).unwrap().changed);
    assert_eq!(e.notes.len(), 1);
    assert!(e.redo());
    assert_eq!(e.notes[1], note(62, 1.0, 1.4));
    e.nudge(0.0, 1, 0.0).unwrap();
    e.undo();
    let (x, y) = drag_second(&mut e);
    e.move_pointer(x + e.zoom * 0.2, y);
    assert!(e.redo());
    e.move_pointer(x + e.zoom * 0.4, y);
    assert!(!e.end_pointer(x).unwrap().changed);
    assert_eq!(e.notes[1], note(63, 1.0, 1.4));
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

fn hollow_editor() -> EditorModel {
    let mut e = EditorModel::default();
    e.set_document(&[note(60, 1.0, 1.5)], None);
    e.set_range_hints(Some(
        &serde_json::json!({"out_of_range_notes": [note(47, 0.0, 0.5)]}),
    ));
    e.set_pitch_zoom(10.0);
    e.set_pitch_scroll_fraction(1.0);
    e
}
fn select_hollow(e: &mut EditorModel) -> (f64, f64) {
    let x = e.x_at(0.2);
    let y = e.pitch_center(47);
    assert_eq!(e.hit_test(x, y), Some(e.notes.len()));
    e.begin_pointer(x, y, false);
    (x, y)
}
#[test]
fn hollow_drag_moves_time_and_pitch_without_snapping_into_range() {
    let mut e = hollow_editor();
    let (x, y) = select_hollow(&mut e);
    e.move_pointer(x + e.zoom * 0.2, y);
    assert!(e.end_pointer(x).unwrap().changed);
    assert_eq!(e.out_of_range_notes, [note(47, 0.2, 0.7)]);
    assert_eq!(e.notes, [note(60, 1.0, 1.5)]);
    assert!(e.undo());
    assert_eq!(e.out_of_range_notes, [note(47, 0.0, 0.5)]);
    assert!(e.redo());
    assert_eq!(e.out_of_range_notes, [note(47, 0.2, 0.7)]);
}
#[test]
fn hollow_crossing_boundary_becomes_playable_and_undo_restores_hollow() {
    let mut e = hollow_editor();
    let (x, y) = select_hollow(&mut e);
    e.move_pointer(x, y - e.row_height());
    assert!(e.end_pointer(x).unwrap().changed);
    assert_eq!(e.notes, [note(48, 0.0, 0.5), note(60, 1.0, 1.5)]);
    assert!(e.out_of_range_notes.is_empty());
    assert_eq!(e.selected, Some(0));
    assert!(e.undo());
    assert_eq!(e.selected, Some(1));
    assert_eq!(e.out_of_range_notes, [note(47, 0.0, 0.5)]);
    assert!(e.redo());
    e.nudge(0.0, -1, 0.0).unwrap();
    assert_eq!(e.out_of_range_notes, [note(47, 0.0, 0.5)]);
}
#[test]
fn hollow_delete_during_drag_is_undoable_and_release_cannot_resurrect_it() {
    let mut e = hollow_editor();
    let (x, y) = select_hollow(&mut e);
    e.move_pointer(x + 7.0, y);
    assert!(e.delete_selected().unwrap());
    assert!(!e.end_pointer(x).unwrap().changed);
    assert!(e.out_of_range_notes.is_empty());
    assert_eq!(e.notes, [note(60, 1.0, 1.5)]);
    assert!(e.undo());
    assert_eq!(e.out_of_range_notes, [note(47, 0.0, 0.5)]);
    assert!(!e.can_undo());
    assert!(e.redo());
    assert!(e.out_of_range_notes.is_empty());
}
#[test]
fn hollow_invalid_drag_and_cancel_preserve_both_lists() {
    let mut e = hollow_editor();
    let (x, y) = select_hollow(&mut e);
    e.move_pointer(x + e.zoom, y - e.row_height());
    assert!(e.end_pointer(x).is_err());
    assert_eq!(e.out_of_range_notes, [note(47, 0.0, 0.5)]);
    assert_eq!(e.notes, [note(60, 1.0, 1.5)]);
    assert!(!e.can_undo());
    let (x, y) = select_hollow(&mut e);
    e.move_pointer(x, y - e.row_height());
    e.cancel_drag();
    assert_eq!(e.out_of_range_notes, [note(47, 0.0, 0.5)]);
}
#[test]
fn hollow_only_document_supports_editing_and_readonly_guard() {
    let mut e = hollow_editor();
    e.notes.clear();
    let (x, _) = select_hollow(&mut e);
    e.end_pointer(x).unwrap();
    e.allow_note_edits = false;
    assert!(e.delete_selected().is_err());
    assert!(e.nudge(0.0, 1, 0.0).is_err());
    e.allow_note_edits = true;
    assert!(e.delete_selected().unwrap());
    assert!(e.undo());
    e.nudge(0.0, 1, 0.0).unwrap();
    assert_eq!(e.notes, [note(48, 0.0, 0.5)]);
}
