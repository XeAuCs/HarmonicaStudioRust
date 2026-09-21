use super::support::*;

#[test]
fn midi_tempo_from_other_track() {
    let mut tempos = vec![0, 255, 81, 3, 7, 161, 32];
    tempos.extend(vlq_out(480));
    tempos.extend([255, 81, 3, 15, 66, 64]);
    tempos.extend(eot());
    let mut notes = vec![0, 144, 60, 80];
    notes.extend(vlq_out(960));
    notes.extend([128, 60, 0]);
    notes.extend(eot());
    let (parts, _) = parse_midi(&smf(vec![tempos, notes], 1)).unwrap();
    approx(parts[&(1, 0)][0].end, 1.5);
}
#[test]
fn midi_running_status_and_velocity_zero() {
    let mut track = vec![0, 144, 60, 80];
    track.extend(vlq_out(240));
    track.extend([60, 0, 0, 62, 80]);
    track.extend(vlq_out(240));
    track.extend([62, 0]);
    track.extend(eot());
    let (parts, _) = parse_midi(&smf(vec![track], 0)).unwrap();
    assert_eq!(
        parts[&(0, 0)].iter().map(|n| n.pitch).collect::<Vec<_>>(),
        [60, 62]
    );
}
#[test]
fn midi_invalid_truncated_unclosed_and_percussion_are_rejected() {
    let cases = [
        b"<html>".to_vec(),
        smf(vec![vec![0, 144, 60]], 0),
        smf(vec![eot()], 2),
        smf(vec![vec![0, 144, 60, 80, 0, 255, 47, 0]], 0),
        smf(
            vec![vec![0, 153, 60, 80, 120, 137, 60, 0, 0, 255, 47, 0]],
            0,
        ),
    ];
    for raw in cases {
        assert!(parse_midi(&raw).is_err());
    }
}
#[test]
fn midi_wide_range_and_rmid_survive_until_adaptation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("wide.mid");
    write_midi(&[note(36, 0.0, 0.5), note(96, 1.0, 1.5)], &path).unwrap();
    let raw = fs::read(&path).unwrap();
    let mut rmid = b"RIFF".to_vec();
    rmid.extend(((raw.len() + 12) as u32).to_le_bytes());
    rmid.extend(b"RMIDdata");
    rmid.extend((raw.len() as u32).to_le_bytes());
    rmid.extend(raw);
    let (parts, names) = parse_midi(&rmid).unwrap();
    assert_eq!(parts[&(0, 0)][0].pitch, 36);
    let prepared = prepare(
        &parts,
        &names,
        &Options {
            phrase_octave: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(prepared.0.len(), 2);
    assert!(prepared.0.iter().all(|n| (48..=85).contains(&n.pitch)));
}
#[test]
fn midi_same_tick_tempos_use_last_track_event_and_duplicate_notes_use_fifo() {
    let mut first = vec![0, 255, 81, 3, 7, 161, 32];
    first.extend(eot());
    let mut second = vec![0, 255, 81, 3, 15, 66, 64];
    second.extend(eot());
    let mut notes = vec![0, 144, 60, 80];
    notes.extend(vlq_out(120));
    notes.extend([144, 60, 90]);
    notes.extend(vlq_out(120));
    notes.extend([128, 60, 0]);
    notes.extend(vlq_out(240));
    notes.extend([128, 60, 0]);
    notes.extend(eot());
    let (parts, _) = parse_midi(&smf(vec![first, second, notes], 1)).unwrap();
    let notes = &parts[&(2, 0)];
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].velocity, 80);
    assert_eq!(notes[1].velocity, 90);
    approx(notes[0].start, 0.0);
    approx(notes[0].end, 0.5);
    approx(notes[1].start, 0.25);
    approx(notes[1].end, 1.0);
}
#[test]
fn midi_meta_events_and_sysex_clear_running_status() {
    for intervening in [vec![0, 255, 1, 0], vec![0, 240, 1, 247]] {
        let mut track = vec![0, 144, 60, 80];
        track.extend(intervening);
        track.extend([10, 60, 0]);
        track.extend(eot());
        assert!(
            parse_midi(&smf(vec![track], 0))
                .unwrap_err()
                .to_string()
                .contains("running status")
        );
    }
}
