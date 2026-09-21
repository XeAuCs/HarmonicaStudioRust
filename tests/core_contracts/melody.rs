use super::support::*;

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
