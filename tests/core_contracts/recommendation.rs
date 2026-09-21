use super::support::*;

#[test]
fn equivalent_mode_choices_keep_selected_and_compare_all_note_fields() {
    use harmonica_studio::melody::distinct_melody_modes;
    let mono = vec![note(72, 0.0, 0.5), note(74, 0.5, 1.0), note(76, 1.0, 1.5)];
    for mode in ["sustain", "highest", "continuous"] {
        let options = Options {
            melody_mode: mode.into(),
            ..Options::default()
        };
        assert_eq!(distinct_melody_modes(&mono, &options), vec![mode]);
    }
    let mixed = vec![note(72, 0.0, 1.0), note(48, 0.25, 0.5), note(74, 1.0, 1.5)];
    let original = mixed.clone();
    let choices = distinct_melody_modes(&mixed, &Options::default());
    assert_eq!(choices, vec!["sustain", "highest"]);
    let options = Options {
        melody_mode: "continuous".into(),
        ..Options::default()
    };
    assert_eq!(
        distinct_melody_modes(&mixed, &options),
        vec!["continuous", "highest"]
    );
    // Current pitch fitting can remove a difference; changing it restores choices.
    let very_low: Vec<_> = mixed.iter().map(|n| Note { pitch: n.pitch - 48, ..n.clone() }).collect();
    let options = Options { auto_octave: false, ..Options::default() };
    assert_eq!(distinct_melody_modes(&very_low, &options), vec!["sustain"]);
    assert_eq!(distinct_melody_modes(&very_low, &Options::default()).len(), 2);
    assert_eq!(mixed, original);
    assert_eq!(distinct_melody_modes(&[], &Options::default()).len(), 3);
}

fn line(count: usize, start: f64, step: f64, duration: f64, pitches: &[i32]) -> Vec<Note> {
    (0..count)
        .map(|i| {
            note(
                pitches[i % pitches.len()],
                start + i as f64 * step,
                start + i as f64 * step + duration,
            )
        })
        .collect()
}

#[test]
fn fixed_ranking_scenarios() {
    let tune = [
        72, 74, 76, 75, 72, 69, 71, 74, 72, 76, 77, 74, 71, 69, 72, 71,
    ];
    let cases = [
        (
            "arpeggio",
            line(64, 0.0, 0.5, 0.42, &tune),
            line(128, 0.0, 0.25, 0.24, &[60, 64, 67, 72]),
        ),
        (
            "late_entry",
            line(64, 40.0, 0.5, 0.42, &tune),
            line(144, 0.0, 0.5, 0.48, &[60, 64, 67, 72]),
        ),
        (
            "legato",
            line(64, 0.0, 0.5, 0.56, &tune),
            line(128, 0.0, 0.25, 0.24, &[60, 64, 67, 72]),
        ),
        (
            "sparse_staccato",
            line(32, 0.0, 1.0, 0.04, &tune),
            line(128, 0.0, 0.25, 0.24, &[60, 64, 67, 72]),
        ),
    ];
    let mut misses = Vec::new();
    for (name, melody, accompaniment) in cases {
        let parts = Parts::from([((0, 0), accompaniment), ((1, 0), melody)]);
        let original = parts.clone();
        let ranked = rank_parts(&parts, &TrackNames::new());
        println!("{name}: {ranked:?}");
        if ranked[0].0 != (1, 0) {
            misses.push(name);
        }
        assert_eq!(parts, original);
    }
    assert!(misses.is_empty(), "melody not ranked first: {misses:?}");
}

#[test]
fn onset_density_ignores_release_length_and_distinguishes_fast_onsets() {
    use harmonica_studio::melody::part_features;
    let short = line(32, 0.0, 1.0, 0.04, &[72, 74, 76, 74]);
    let long = line(32, 0.0, 1.0, 0.8, &[72, 74, 76, 74]);
    let fast = line(320, 0.0, 0.1, 0.04, &[72, 74, 76, 74]);
    let s = part_features(&short, 0.0, 32.0);
    let l = part_features(&long, 0.0, 32.0);
    approx(s.onset_rate, 1.0);
    approx(s.onset_rate, l.onset_rate);
    assert!(part_features(&fast, 0.0, 32.0).onset_rate > 9.0);
    let ranks = rank_parts(
        &Parts::from([((0, 0), short), ((1, 0), long)]),
        &TrackNames::new(),
    );
    assert!((ranks[0].1 - ranks[1].1).abs() < 1.0);
}

#[test]
fn legato_tolerance_does_not_hide_chords_or_sustained_polyphony() {
    use harmonica_studio::melody::part_features;
    let legato = line(32, 0.0, 0.5, 0.56, &[72, 74, 76, 74]);
    let original = legato.clone();
    approx(part_features(&legato, 0.0, 16.1).monophony, 1.0);
    assert_eq!(legato, original);
    let chords: Vec<_> = (0..32)
        .flat_map(|i| [60, 64, 67].map(|p| note(p, i as f64 * 0.5, i as f64 * 0.5 + 0.04)))
        .collect();
    approx(part_features(&chords, 0.0, 16.0).monophony, 0.0);
    let poly = line(32, 0.0, 0.5, 0.9, &[72, 74, 76, 74]);
    assert!(part_features(&poly, 0.0, 16.4).monophony < 0.3);
    let mut reversed = legato.clone();
    reversed.reverse();
    approx(part_features(&reversed, 0.0, 16.1).monophony, 1.0);
    assert_eq!(reversed.first(), original.last());
}

#[test]
fn repetition_needs_accompaniment_context_and_internal_gaps_reduce_evidence() {
    use harmonica_studio::melody::part_features;
    let repeated_tune = line(64, 0.0, 0.5, 0.4, &[72, 74, 76, 74]);
    let arpeggio = line(128, 0.0, 0.25, 0.24, &[60, 64, 67, 72]);
    assert_eq!(
        rank_parts(
            &Parts::from([((0, 0), arpeggio), ((1, 0), repeated_tune.clone())]),
            &TrackNames::new()
        )[0]
        .0,
        (1, 0)
    );
    let a = part_features(&repeated_tune, 0.0, 72.0);
    let mut late = repeated_tune.clone();
    for n in &mut late {
        n.start += 40.0;
        n.end += 40.0;
    }
    let b = part_features(&late, 0.0, 72.0);
    approx(a.segment_activity, b.segment_activity);
    approx(a.active_span_coverage, b.active_span_coverage);
    let mut gapped = repeated_tune;
    for n in &mut gapped[32..] {
        n.start += 40.0;
        n.end += 40.0;
    }
    assert!(part_features(&gapped, 0.0, 72.0).segment_activity < b.segment_activity * 0.6);
}

#[test]
fn fit_retention_matches_actual_conversion_for_all_parameter_combinations() {
    use harmonica_studio::melody::fit_part;
    let raw = vec![
        note(36, 2.0, 2.5),
        note(40, 2.0, 2.5),
        note(38, 2.5, 3.0),
        note(96, 4.0, 4.5),
        note(100, 4.5, 5.0),
    ];
    let parts = Parts::from([((0, 0), raw.clone())]);
    let original_rank = rank_parts(&parts, &TrackNames::new());
    for mode in ["highest", "sustain", "continuous"] {
        for auto in [false, true] {
            for phrase in [false, true] {
                for transpose in [-12, 0, 12] {
                    for speed in [0.5, 1.5] {
                        let options = Options {
                            melody_mode: mode.into(),
                            auto_octave: auto,
                            phrase_octave: phrase,
                            transpose,
                            speed,
                            ..Options::default()
                        };
                        let (fitted, report) = fit_part(&raw, &options).unwrap();
                        let extracted = simplify(&raw, mode, options.trim_silence);
                        approx(
                            report["range_retention"].as_f64().unwrap(),
                            fitted.len() as f64 / extracted.len() as f64,
                        );
                        approx(
                            report["source_retention"].as_f64().unwrap(),
                            fitted.len() as f64 / raw.len() as f64,
                        );
                        let actual = prepare(&parts, &TrackNames::new(), &options);
                        if fitted.is_empty() {
                            assert!(actual.is_err());
                        } else {
                            let (notes, converted) = actual.unwrap();
                            assert_eq!(notes, fitted);
                            for field in [
                                "range_retention",
                                "source_retention",
                                "transpose_semitones",
                                "octave_adjustments",
                            ] {
                                assert_eq!(converted[field], report[field]);
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(parts[&(0, 0)], raw);
    assert_eq!(rank_parts(&parts, &TrackNames::new()), original_rank);
    assert!(
        fit_part(
            &raw,
            &Options {
                speed: f64::NAN,
                ..Options::default()
            }
        )
        .is_err()
    );
}

#[cfg(all(windows, feature = "desktop"))]
#[test]
fn inspect_keeps_ranking_fields_and_reports_option_specific_fit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("synthetic.mid");
    write_midi(
        &[
            note(36, 0.0, 0.5),
            note(40, 0.5, 1.0),
            note(96, 2.0, 2.5),
            note(100, 2.5, 3.0),
        ],
        &source,
    )
    .unwrap();
    let inspect = |args: &[&str]| {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_HarmonicaStudio"))
            .arg("inspect")
            .arg(&source)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap()
    };
    let default = inspect(&[]);
    let no_shift = inspect(&["--no-auto-octave"]);
    let phrases = inspect(&["--phrase-octave"]);
    assert_eq!(no_shift[0]["fit"]["range_retention"], 0.0);
    assert_eq!(phrases[0]["fit"]["range_retention"], 1.0);
    for field in ["track", "channel", "name", "notes", "recommendation_score"] {
        assert_eq!(default[0][field], phrases[0][field]);
        assert_eq!(default[0][field], no_shift[0][field]);
    }
}
