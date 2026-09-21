use super::support::*;

// Original synthetic phrases; no user songs or catalog needed.
fn fixture(pitches: &[u8], gap: u64) -> Vec<u8> {
    let track = |name: &str, channel: u8, pitches: &[u8]| {
        let mut bytes = vec![0, 0xff, 3, name.len() as u8];
        bytes.extend(name.as_bytes());
        for (i, &pitch) in pitches.iter().enumerate() {
            bytes.extend(vlq_out(if i == 0 { 0 } else { gap }));
            bytes.extend([0x90 | channel, pitch, 80]);
            bytes.extend(vlq_out(360));
            bytes.extend([0x80 | channel, pitch, 0]);
        }
        bytes.extend(eot());
        bytes
    };
    smf(
        vec![track("Bass", 0, &[36; 32]), track("Flute", 1, pitches)],
        1,
    )
}

#[test]
fn generated_midi_recommends_varied_melody_over_repeated_bass() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture.mid");
    let pitches: Vec<_> = (0..32).map(|i| [67, 72, 69, 76, 71][i % 5]).collect();
    fs::write(&path, fixture(&pitches, 120)).unwrap();
    let (parts, names) = read_midi(&path).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(rank_parts(&parts, &names)[0].0, (1, 1));
    let (notes, report) = prepare(&parts, &names, &Options::default()).unwrap();
    assert_eq!(report["track_name"], "Flute");
    assert_eq!(
        notes.iter().map(|n| n.pitch).collect::<Vec<_>>(),
        pitches.iter().map(|&p| p as i32).collect::<Vec<_>>()
    );
}

#[test]
fn generated_midis_preserve_score_and_event_contracts_in_every_mode() {
    let output = tempfile::tempdir().unwrap();
    let exported = output.path().join("roundtrip.mid");
    let cases = [
        fixture(&[67, 72, 69, 76, 71, 65, 74, 67], 120),
        fixture(&[43, 48, 45, 52, 47, 41, 50, 43], 4800),
        fixture(&[91, 96, 93, 100, 95, 89, 98, 91], 120),
    ];
    let mut variants = 0;
    for (index, bytes) in cases.iter().enumerate() {
        let file = format!("generated-{index}");
        let (parts, names) = parse_midi(bytes).unwrap();
        let original = parts.clone();
        for mode in ["highest", "sustain", "continuous"] {
            for phrase_octave in [false, true] {
                let options = Options {
                    track: Some(1),
                    channel: Some(1),
                    melody_mode: mode.into(),
                    phrase_octave,
                    ..Options::default()
                };
                let (notes, report) = prepare(&parts, &names, &options).unwrap_or_else(|error| {
                    panic!("{file}/{mode}/{phrase_octave}: 提取失败: {error}")
                });
                assert_eq!(
                    normalize_score_notes(&notes).unwrap(),
                    notes,
                    "{file}/{mode}: 原谱未规范化"
                );
                assert_eq!(
                    report["melody_notes"].as_u64().unwrap() as usize,
                    notes.len()
                );
                let canonical = notes.clone();
                assert!(
                    !notes.is_empty(),
                    "{file}/{mode}: fixture must exercise notes"
                );
                for skip in [false, true] {
                    let (performance, _, _) = compress_long_rests(&notes, skip);
                    let (events, _) = build_events(&performance).unwrap_or_else(|error| {
                        panic!("{file}/{mode}/{phrase_octave}/{skip}: 编排失败: {error}")
                    });
                    let actual = decode_events(&events).unwrap();
                    assert_eq!(
                        actual.iter().map(|n| n.pitch).collect::<Vec<_>>(),
                        notes.iter().map(|n| n.pitch).collect::<Vec<_>>(),
                        "{file}/{mode}: 演奏音高发生变化"
                    );
                    assert!(actual.windows(2).all(|notes| notes[0].end < notes[1].start));
                    write_midi(&actual, &exported).unwrap();
                    let (roundtrip, _) = read_midi(&exported).unwrap();
                    assert_eq!(roundtrip[&(0, 0)].len(), actual.len());
                    for (left, right) in roundtrip[&(0, 0)].iter().zip(&actual) {
                        assert_eq!((left.pitch, left.velocity), (right.pitch, right.velocity));
                        approx(left.start, right.start);
                        approx(left.end, right.end);
                    }
                    let anchors = playback_anchors(&notes, &actual);
                    let map = TimeMap::new(anchors.clone());
                    let inverse = TimeMap::new(anchors.into_iter().map(|(a, b)| (b, a)));
                    for (logical, physical) in notes.iter().zip(&actual) {
                        approx(map.map(logical.start), physical.start);
                        approx(inverse.map(physical.start), logical.start);
                    }
                    assert_eq!(notes, canonical, "{file}/{mode}: 编排修改了原始时间");
                    variants += 1;
                }
            }
        }
        assert_eq!(parts, original, "{file}: 提取修改了原始 MIDI");
    }
    assert_eq!(variants, 36);
}
