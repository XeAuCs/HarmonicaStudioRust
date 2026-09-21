use super::*;
fn note(pitch: i32, start: f64, end: f64) -> Note {
    Note {
        pitch,
        start,
        end,
        velocity: 80,
    }
}
fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
fn assert_centered(editor: &EditorModel) {
    approx(
        editor.x_at(editor.position),
        editor.left() + (editor.width - editor.left()) / 2.0,
    );
    approx(
        editor.time_at(editor.x_at(editor.position)),
        editor.position,
    );
}
#[test]
fn original_default_dimensions_and_full_instrument_range_are_preserved() {
    let mut editor = EditorModel::default();
    assert_eq!(
        (editor.left(), editor.row_height(), editor.zoom),
        (136.0, 20.0, 70.0)
    );
    assert_eq!((editor.low_pitch, editor.high_pitch), (48, 85));
    editor.set_document(&[note(60, 0.0, 1.0)], None);
    assert_eq!(editor.row_height(), 20.0);
    assert_eq!((editor.low_pitch, editor.high_pitch), (48, 85));
    assert!(!editor.has_position());
    approx(
        editor.pitch_center(60),
        (editor.height + RULER_HEIGHT) / 2.0,
    );
}
#[test]
fn pitch_zoom_preserves_unsnapped_note_times_and_hit_coordinates() {
    let mut editor = EditorModel::default();
    let source = vec![note(60, 0.123, 0.723), note(64, 1.2, 1.7)];
    editor.set_document(&source, None);
    editor.pitch_zoom_in();
    assert_eq!(editor.row_height(), 25.0);
    let x = editor.x_at(0.323);
    let y = editor.pitch_center(60);
    assert_eq!(editor.hit_test(x, y), Some(0));
    assert_eq!(editor.pitch_at(y), 60);
    editor.begin_pointer(x, y, false);
    editor.move_pointer(x, y - editor.row_height());
    assert!(editor.end_pointer(x).unwrap().changed);
    assert_eq!(editor.notes[0], note(61, 0.123, 0.723));
    editor.pitch_zoom_out();
    assert_eq!(editor.row_height(), 20.0);
    assert_eq!(
        editor.hit_test(editor.x_at(0.323), editor.pitch_center(61)),
        Some(0)
    );
}
#[test]
fn fit_shows_both_pitch_extremes_and_recomputes_after_resize_without_editing() {
    let mut editor = EditorModel::default();
    let source = vec![note(48, 0.123, 0.8), note(85, 1.2, 1.7)];
    editor.set_document(&source, None);
    editor.resize(850.0, 280.0);
    editor.fit_pitches();
    for height in [280.0, 420.0, 245.0] {
        editor.resize(850.0, height);
        for (index, note) in source.iter().enumerate() {
            let y = editor.pitch_center(note.pitch);
            assert!((RULER_HEIGHT..editor.height).contains(&y));
            assert_eq!(editor.pitch_at(y), note.pitch);
            assert_eq!(
                editor.hit_test(editor.x_at((note.start + note.end) / 2.0), y),
                Some(index)
            );
        }
    }
    assert_eq!(editor.notes, source);
    assert!(!editor.can_undo());
}
#[test]
fn manual_pitch_scroll_reaches_high_and_low_keys_and_clamps_safely() {
    let mut editor = EditorModel::default();
    editor.resize(850.0, 280.0);
    editor.set_document(&[note(48, 0.0, 0.5), note(85, 1.0, 1.5)], None);
    editor.set_pitch_scroll_fraction(0.0);
    approx(editor.pitch_center(85), RULER_HEIGHT + 10.0);
    assert_eq!(
        editor.hit_test(editor.x_at(1.2), editor.pitch_center(85)),
        Some(1)
    );
    editor.set_pitch_scroll_fraction(1.0);
    approx(editor.pitch_center(48), editor.height - 10.0);
    assert_eq!(
        editor.hit_test(editor.x_at(0.2), editor.pitch_center(48)),
        Some(0)
    );
    assert_eq!(
        editor.hit_test(editor.x_at(1.2), editor.pitch_center(85)),
        None
    );
    editor.pan_pitches(1e6);
    approx(editor.pitch_scroll_fraction(), 1.0);
    editor.pan_pitches(-1e6);
    approx(editor.pitch_scroll_fraction(), 0.0);
    let view = editor.pitch_offset;
    editor.pan_pitches(f64::NAN);
    assert_eq!(editor.pitch_offset, view);
}
#[test]
fn compact_restores_full_pitch_view_and_undo_history() {
    let mut editor = EditorModel::default();
    editor.add_note(0.0, 60).unwrap();
    editor.nudge(0.0, 2, 0.0).unwrap();
    editor.set_pitch_zoom(32.0);
    editor.set_pitch_scroll_fraction(0.65);
    let view = (editor.row_height(), editor.pitch_offset);
    let notes = editor.notes.clone();
    editor.set_compact(true);
    assert_eq!(editor.left(), 12.0);
    assert!(!editor.can_undo());
    assert_eq!(editor.note_height(), 6.0);
    assert!(!editor.undo());
    assert!(editor.nudge(0.0, 1, 0.0).is_ok_and(|changed| !changed));
    editor.set_compact(false);
    assert_eq!(editor.left(), 136.0);
    assert_eq!((editor.row_height(), editor.pitch_offset), view);
    assert_eq!(editor.notes, notes);
    assert!(editor.can_undo());
    assert!(editor.undo());
    assert_eq!(editor.notes[0].pitch, 60);
}
#[test]
fn piano_geometry_has_contiguous_white_keys_and_two_three_black_key_pattern() {
    let mut editor = EditorModel::default();
    editor.set_document(&[note(60, 0.0, 0.5)], None);
    for row in [9.0, 12.0, 20.0, 32.0, 44.0] {
        editor.set_pitch_zoom(row);
        let (white, black) = editor.piano_geometry();
        let key = |pitch: i32| white.iter().find(|key| key.pitch == pitch).unwrap();
        for pair in [60, 62, 64, 65, 67, 69, 71, 72].windows(2) {
            approx(key(pair[0]).y, key(pair[1]).y + key(pair[1]).height);
        }
        assert_eq!(
            black
                .iter()
                .filter(|key| (60..72).contains(&key.pitch))
                .map(|key| key.pitch)
                .collect::<Vec<_>>(),
            [61, 63, 66, 68, 70]
        );
        for black in black.iter().filter(|key| (60..72).contains(&key.pitch)) {
            approx(
                black.y + black.height / 2.0,
                editor.pitch_center(black.pitch),
            );
            assert!(black.width < key(black.pitch - 1).width);
        }
        for (lower, higher) in [(64, 65), (71, 72)] {
            let join = key(lower).y;
            approx(
                join,
                (editor.pitch_center(lower) + editor.pitch_center(higher)) / 2.0,
            );
            assert!(
                !black
                    .iter()
                    .any(|key| key.y < join && key.y + key.height > join)
            );
        }
    }
}
#[test]
fn centered_playhead_survives_time_zoom_resize_and_compact_at_both_ends() {
    let mut editor = EditorModel::default();
    let notes = vec![note(60, 0.0, 1.0), note(64, 40.0, 41.0)];
    editor.set_document(&notes, None);
    for position in [0.0, 20.1234, 41.0] {
        editor.follow_position(position, true);
        assert_centered(&editor);
        editor.zoom_by(1.25);
        assert_centered(&editor);
        editor.resize(973.0, 320.0);
        assert_centered(&editor);
        editor.set_compact(true);
        assert_centered(&editor);
        editor.zoom_by(0.8);
        assert_centered(&editor);
        editor.set_compact(false);
        assert_centered(&editor);
    }
    assert_eq!(editor.notes, notes);
    assert!(!editor.can_undo());
    let pitch_view = (editor.row_height(), editor.pitch_offset);
    editor.reset_timeline();
    assert_eq!(editor.offset, 0.0);
    assert!(!editor.has_position());
    assert_eq!((editor.row_height(), editor.pitch_offset), pitch_view);
}
#[test]
fn device_updates_cannot_move_local_scrub_and_piano_column_is_not_a_drag_target() {
    let mut editor = EditorModel::default();
    editor.set_document(&[note(60, 0.0, 10.0)], None);
    editor.begin_pointer(40.0, 90.0, true);
    assert!(!editor.is_dragging());
    editor.follow_position(4.0, true);
    editor.begin_pointer(400.0, 10.0, false);
    assert!(editor.move_pointer(370.0, 10.0).pause);
    let position = editor.position;
    let offset = editor.offset;
    editor.follow_position(3.0, true);
    assert_eq!((editor.position, editor.offset), (position, offset));
    let action = editor.end_pointer(370.0).unwrap();
    assert_eq!(action.seek, Some(position));
    assert_centered(&editor);
}
#[test]
fn playback_follow_recovers_from_stale_pointer_capture() {
    let mut editor = EditorModel::default();
    editor.set_document(&[note(60, 0.0, 40.0)], None);
    editor.begin_pointer(400.0, 10.0, true);
    assert!(editor.is_dragging());

    editor.follow_position(20.0, true);
    assert_eq!(editor.position, 0.0);

    editor.follow_playback_position(20.0);
    assert_eq!(editor.position, 20.0);
    assert_centered(&editor);
}
#[test]
fn draw_and_hit_rectangles_share_note_insets_in_both_modes() {
    let mut editor = EditorModel::default();
    editor.set_document(&[note(60, 0.0, 0.3)], None);
    for compact in [false, true] {
        editor.set_compact(compact);
        for row in [5.0, 12.0, 20.0, 44.0] {
            editor.set_pitch_zoom(row);
            editor.center_pitches();
            let (x, y, width, height) = editor.note_rect(&editor.notes[0]);
            approx(y + height / 2.0, editor.pitch_center(60));
            assert_eq!(editor.hit_test(x + width / 2.0, y + height / 2.0), Some(0));
            assert_eq!(editor.pitch_at(y + height / 2.0), 60);
        }
    }
}
