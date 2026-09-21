use super::support::*;

#[test]
fn out_of_range_hints_preserve_fitted_time_and_stay_out_of_score() {
    let raw = vec![
        note(47, 0.0, 0.8),
        note(48, 1.0, 1.8),
        note(85, 2.0, 2.8),
        note(86, 3.0, 3.8),
    ];
    let options = Options {
        auto_octave: false,
        phrase_octave: false,
        trim_silence: false,
        speed: 2.0,
        ..Options::default()
    };
    let (notes, report) = harmonica_studio::melody::fit_part(&raw, &options).unwrap();
    assert_eq!(notes.iter().map(|n| n.pitch).collect::<Vec<_>>(), [48, 85]);
    let hints: Vec<Note> = serde_json::from_value(report["out_of_range_notes"].clone()).unwrap();
    assert_eq!(hints, [note(47, 0.0, 0.4), note(86, 1.5, 1.9)]);
    let mut project = harmonica_studio::project::make_project(notes.clone(), "音域提示").unwrap();
    project.report = Some(report);
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("range.hstudio");
    harmonica_studio::project::save_project(&file, &project).unwrap();
    let loaded = harmonica_studio::project::load_project(&file).unwrap();
    assert_eq!(loaded, project);
    let mut editor = harmonica_studio::editor::EditorModel::default();
    editor.set_document(&loaded.notes, None);
    editor.set_range_hints(loaded.report.as_ref());
    editor.fit_pitches();
    assert_eq!(editor.notes, notes);
    assert_eq!(editor.out_of_range_notes, hints);
    assert_eq!((editor.low_pitch, editor.high_pitch), (47, 86));
    assert_eq!(
        editor.hit_test(editor.x_at(0.2), editor.pitch_center(47)),
        Some(notes.len())
    );
    editor.set_document(&notes, None);
    assert!(editor.out_of_range_notes.is_empty());
    assert_eq!((editor.low_pitch, editor.high_pitch), (48, 85));
}

#[test]
fn fitting_ignores_hollow_hints_in_full_and_compact_views() {
    use harmonica_studio::editor::EditorModel;
    let notes = vec![note(60, 0.0, 0.4), note(72, 1.0, 1.4)];
    let mut baseline = EditorModel::default();
    baseline.set_document(&notes, None);
    baseline.fit_pitches();
    let mut hinted = EditorModel::default();
    hinted.set_document(&notes, None);
    hinted.set_range_hints(Some(&serde_json::json!({
        "out_of_range_notes": [note(12, 2.0, 2.4), note(120, 3.0, 3.4)]
    })));
    hinted.fit_pitches();
    for compact in [false, true, false] {
        baseline.set_compact(compact);
        hinted.set_compact(compact);
        assert_eq!(hinted.row_height(), baseline.row_height());
        for pitch in [60, 72] {
            assert!((hinted.pitch_center(pitch) - baseline.pitch_center(pitch)).abs() < 1e-8);
        }
    }
    assert_eq!((hinted.low_pitch, hinted.high_pitch), (12, 120));
}

#[test]
fn phrase_fitting_hints_use_the_final_octave_once() {
    let raw = vec![note(20, 0.0, 0.4), note(60, 0.5, 0.9), note(110, 1.0, 1.4)];
    let options = Options {
        auto_octave: false,
        phrase_octave: true,
        trim_silence: false,
        ..Options::default()
    };
    let (notes, report) = harmonica_studio::melody::fit_part(&raw, &options).unwrap();
    let (expected, _) = harmonica_studio::melody::fit_phrases(&raw, 0);
    assert_eq!(notes, expected);
    let hints: Vec<Note> = serde_json::from_value(report["out_of_range_notes"].clone()).unwrap();
    assert_eq!(notes.len() + hints.len(), raw.len());
    assert!(hints.iter().all(|n| !(48..=85).contains(&n.pitch)));
    let mut all = notes;
    all.extend(hints);
    all.sort_by(|a, b| a.start.total_cmp(&b.start));
    let shift = all[0].pitch - raw[0].pitch;
    assert!(
        all.iter()
            .zip(&raw)
            .all(|(fitted, original)| fitted.pitch - original.pitch == shift
                && fitted.start == original.start
                && fitted.end == original.end)
    );
}

#[test]
fn recommendation_prefers_sustained_melody_over_fragments_bass_and_repetition() {
    let melody: Vec<_> = (0..64)
        .map(|i| {
            note(
                [67, 69, 72, 71, 69, 67, 64, 67][i % 8],
                i as f64 * 0.5,
                i as f64 * 0.5 + 0.42,
            )
        })
        .collect();
    let sparse = vec![note(84, 0.0, 0.2), note(86, 31.0, 31.2)];
    let repeated = (0..64)
        .map(|i| note(72, i as f64 * 0.5, i as f64 * 0.5 + 0.45))
        .collect();
    let bass = melody
        .iter()
        .map(|n| Note {
            pitch: n.pitch - 36,
            ..n.clone()
        })
        .collect();
    let held = vec![note(72, 0.0, 32.0)];
    let parts = Parts::from([
        ((0, 0), sparse),
        ((1, 0), repeated),
        ((2, 0), bass),
        ((3, 0), held),
        ((4, 0), melody),
    ]);
    let original = parts.clone();
    let ranked = rank_parts(&parts, &TrackNames::from([(0, "Lead Melody".into())]));
    assert_eq!(ranked[0].0, (4, 0));
    assert!(
        ranked
            .iter()
            .all(|(_, score)| score.is_finite() && (0.0..=100.0).contains(score))
    );
    assert!(ranked.windows(2).all(|w| w[0].1 >= w[1].1));
    assert_eq!(parts, original);
}

#[test]
fn recommendation_has_stable_ties_and_handles_empty_or_short_songs() {
    assert!(rank_parts(&Parts::new(), &TrackNames::new()).is_empty());
    let short = vec![note(72, 2.0, 2.1)];
    let ranked = rank_parts(
        &Parts::from([((2, 0), short.clone()), ((1, 0), short), ((0, 0), vec![])]),
        &TrackNames::new(),
    );
    assert_eq!(
        ranked.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![(1, 0), (2, 0)]
    );
    assert_eq!(ranked[0].1, ranked[1].1);
    assert!(ranked[0].1.is_finite());
}

#[test]
fn all_modes_keep_consecutive_short_notes() {
    let notes = vec![
        note(60, 0.0, 0.02),
        note(84, 0.02, 0.04),
        note(48, 0.04, 0.06),
    ];
    for mode in ["highest", "sustain", "continuous"] {
        assert_eq!(simplify(&notes, mode, false), notes);
    }
}
#[test]
fn sustain_and_continuous_skip_accompaniment() {
    let notes = vec![
        note(72, 0.0, 2.0),
        note(67, 0.5, 0.8),
        note(65, 1.0, 1.3),
        note(74, 2.0, 2.5),
    ];
    assert_eq!(
        simplify(&notes, "continuous", true),
        vec![notes[0].clone(), notes[3].clone()]
    );
    assert_eq!(simplify(&notes, "sustain", true).len(), 4);
    let source = vec![note(72, 0.0, 1.0), note(48, 0.25, 0.5), note(74, 1.0, 1.5)];
    assert_eq!(simplify(&source, "sustain", true).len(), 2);
}
#[test]
fn continuous_prefers_middle_voice_over_short_high_ornaments() {
    let melody: Vec<_> = [72, 74, 76, 74]
        .into_iter()
        .enumerate()
        .map(|(i, p)| note(p, i as f64 * 0.5, i as f64 * 0.5 + 0.5))
        .collect();
    let mut source = melody.clone();
    source.extend((0..4).map(|i| note(91, i as f64 * 0.5 + 0.01, i as f64 * 0.5 + 0.06)));
    assert_eq!(simplify(&source, "continuous", true), melody);
}
#[test]
fn misleading_track_name_does_not_override_monophonic_melody() {
    let melody: Vec<_> = (0..16)
        .map(|i| {
            note(
                [72, 74, 76, 74][i % 4],
                i as f64 * 0.5,
                i as f64 * 0.5 + 0.45,
            )
        })
        .collect();
    let accompaniment: Vec<_> = (0..16)
        .flat_map(|i| [48, 52, 55].map(|p| note(p, i as f64 * 0.5, i as f64 * 0.5 + 0.45)))
        .collect();
    assert_eq!(
        rank_parts(
            &Parts::from([((0, 0), accompaniment), ((1, 0), melody)]),
            &TrackNames::from([(0, "Melody".into())])
        )[0]
        .0,
        (1, 0)
    );
}
#[test]
fn phrase_octaves_preserve_intervals_and_timing() {
    let notes = vec![
        note(36, 2.0, 2.5),
        note(40, 2.5, 3.0),
        note(96, 4.0, 4.5),
        note(100, 4.5, 5.0),
    ];
    let parts = Parts::from([((0, 0), notes.clone())]);
    let options = Options {
        melody_mode: "continuous".into(),
        phrase_octave: true,
        trim_silence: false,
        speed: 0.5,
        ..Options::default()
    };
    let (got, report) = prepare(&parts, &TrackNames::new(), &options).unwrap();
    assert_eq!(got.len(), 4);
    assert_eq!(got[1].pitch - got[0].pitch, 4);
    assert_eq!(got[3].pitch - got[2].pitch, 4);
    assert_eq!(
        got.iter().map(|n| (n.start, n.end)).collect::<Vec<_>>(),
        [(4.0, 5.0), (5.0, 6.0), (8.0, 9.0), (9.0, 10.0)]
    );
    assert_eq!(report["dropped_out_of_range"], 0);
    assert_eq!(parts[&(0, 0)], notes);
}
#[test]
fn phrase_mode_does_not_fold_individual_notes() {
    let parts = Parts::from([((0, 0), vec![note(36, 0.0, 0.5), note(96, 0.5, 1.0)])]);
    let (got, report) = prepare(
        &parts,
        &TrackNames::new(),
        &Options {
            phrase_octave: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(report["dropped_out_of_range"], 1);
}
#[test]
fn global_fit_protects_long_notes_and_extreme_transpose() {
    let parts = Parts::from([(
        (0, 0),
        vec![
            note(36, 0.0, 0.04),
            note(36, 0.04, 0.08),
            note(96, 1.0, 4.0),
        ],
    )]);
    let (got, _) = prepare(&parts, &TrackNames::new(), &Options::default()).unwrap();
    assert_eq!(got.len(), 1);
    approx(got[0].end - got[0].start, 3.0);
    let (got, _) = prepare(
        &Parts::from([((0, 0), vec![note(0, 0.0, 1.0)])]),
        &TrackNames::new(),
        &Options {
            transpose: -24,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(got[0].pitch, 48);
}
#[test]
fn equal_pitch_velocity_chord_choice_keeps_first_note_duration() {
    let source = vec![note(60, 0.0, 0.5), note(60, 0.0, 1.0)];
    assert_eq!(simplify(&source, "highest", false), vec![source[0].clone()]);
    assert_eq!(source[1].end, 1.0);
}
#[test]
fn phrase_octave_threshold_and_register_tiebreak_match_original_rule() {
    use harmonica_studio::melody::fit_phrases;
    // Median duration .5 yields a .375 second phrase boundary.
    let joined = vec![note(36, 0.0, 0.5), note(96, 0.874, 1.374)];
    assert_eq!(fit_phrases(&joined, 0).0.len(), 1);
    let separate = vec![note(36, 0.0, 0.5), note(96, 0.875, 1.375)];
    let (kept, adjustments) = fit_phrases(&separate, 0);
    assert_eq!(kept.iter().map(|n| n.pitch).collect::<Vec<_>>(), [48, 84]);
    assert_eq!(adjustments[0]["semitones"], 12);
    assert_eq!(adjustments[1]["semitones"], -12);
    assert_eq!((kept[0].start, kept[0].end), (0.0, 0.5));
    assert_eq!((kept[1].start, kept[1].end), (0.875, 1.375));
}
