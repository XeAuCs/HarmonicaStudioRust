use super::*;
#[test]
fn playing_score_allows_scrubbing_but_rejects_note_edits() {
    let mut editor = EditorModel::default();
    editor.set_document(
        &[Note {
            pitch: 60,
            start: 1.0,
            end: 2.0,
            velocity: 80,
        }],
        None,
    );
    editor.allow_note_edits = false;
    editor.selected = Some(0);
    assert!(editor.nudge(0.0, 1, 0.0).is_err());
    let x = editor.x_at(1.5);
    let y = editor.y_at(60) + editor.row_height() / 2.0;
    editor.begin_pointer(x, y, false);
    assert!(editor.move_pointer(x - 20.0, y).pause);
    assert!(editor.end_pointer(x - 20.0).unwrap().seek.is_some());
    assert_eq!(editor.notes[0].pitch, 60);
    assert_eq!(editor.notes[0].start, 1.0);
    assert!(!editor.can_undo());
}
