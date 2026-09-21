use super::support::*;

#[test]
fn all_playable_pitches_event_roundtrip() {
    let notes: Vec<_> = (48..=85)
        .enumerate()
        .map(|(i, p)| note(p, i as f64 * 0.5, i as f64 * 0.5 + 0.3))
        .collect();
    let (events, delayed) = build_events(&notes).unwrap();
    assert_eq!(delayed, 0);
    assert_eq!(
        decode_events(&events)
            .unwrap()
            .iter()
            .map(|n| n.pitch)
            .collect::<Vec<_>>(),
        (48..=85).collect::<Vec<_>>()
    );
    assert!(mapping(47).is_err());
    assert!(mapping(86).is_err());
}
#[test]
fn dense_notes_are_balanced_and_duration_limited() {
    let notes: Vec<_> = [48, 85, 61, 73, 60]
        .into_iter()
        .enumerate()
        .map(|(i, p)| note(p, i as f64 * 0.01, i as f64 * 0.01 + 0.01))
        .collect();
    let (events, delayed) = build_events(&notes).unwrap();
    assert!(delayed > 0);
    let actual = decode_events(&events).unwrap();
    assert!(actual.windows(2).all(|n| n[0].end < n[1].start));
    assert!(build_events(&[note(60, 1199.95, 1200.0)]).is_err());
}
#[test]
fn malformed_events_are_rejected() {
    let cases: Vec<Vec<Event>> = vec![
        vec![(0, "SC02C".into(), 1)],
        vec![(1, "SC02C".into(), 0)],
        vec![(0, "SC02C".into(), 1), (1, "SC02D".into(), 1)],
        vec![(0, "LButton".into(), 1), (0, "RButton".into(), 1)],
    ];
    for events in cases {
        assert!(decode_events(&events).is_err());
    }
}
#[test]
fn rest_threshold_and_canonical_times_are_preserved() {
    for (gap, count) in [(2.999999, 0), (3.0, 0), (3.0 + 5e-9, 0), (3.0 + 1e-7, 1)] {
        let notes = vec![note(60, 0.2, 0.5), note(62, 0.5 + gap, 1.0 + gap)];
        let (shifted, got, saved) = compress_long_rests(&notes, true);
        assert_eq!(got, count);
        approx(saved, if count == 1 { gap - 0.6 } else { 0.0 });
        approx(
            shifted[1].end - shifted[1].start,
            notes[1].end - notes[1].start,
        );
    }
}
#[test]
fn transport_maps_legato_and_compressed_rest_smoothly() {
    let notes = vec![
        note(60, 2.0, 12.0),
        note(62, 20.0, 21.0),
        note(73, 21.0, 22.0),
    ];
    let (performance, _, _) = compress_long_rests(&notes, true);
    let actual = decode_events(&build_events(&performance).unwrap().0).unwrap();
    let anchors = playback_anchors(&notes, &actual);
    let forward = TimeMap::new(anchors.clone());
    let inverse = TimeMap::new(anchors.into_iter().map(|(a, b)| (b, a)));
    for s in [2.0, 3.0, 7.0, 11.0, 12.0] {
        approx(forward.map(s), s + 0.1);
    }
    for i in 0..=100 {
        let score = 2.0 + 20.0 * i as f64 / 100.0;
        approx(inverse.map(forward.map(score)), score);
    }
    assert_eq!(TimeMap::default().map(-3.0), 0.0);
}
#[test]
fn playback_clock_corrects_smoothly_and_seeks_immediately() {
    let mut clock = PlaybackClock::default();
    clock.reset_at(0.0, true, 0.0);
    approx(clock.position_at(0.1, None), 0.1);
    clock.synchronize_at(0.08, true, false, 0.1);
    approx(clock.position_at(0.1, None), 0.1);
    assert!(clock.position_at(0.16, None) > 0.1);
    approx(clock.position_at(0.22, None), 0.2);
    clock.synchronize_at(5.0, true, true, 0.3);
    approx(clock.position_at(0.3, None), 5.0);
    clock.synchronize_at(5.05, false, false, 0.35);
    approx(clock.position_at(10.0, None), 5.05);
}
