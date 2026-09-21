pub(super) use harmonica_studio::{
    melody::{prepare, rank_parts, simplify},
    midi::{Parts, TrackNames, parse_midi, read_midi, vlq_out, write_midi},
    models::{Note, Options},
    notes::normalize_score_notes,
    preview::{decode_events, render_wav},
    project::{load_project, make_project, save_project, validate_project},
    rests::compress_long_rests,
    schedule::{Event, build_events, mapping},
    service::{convert, export_project},
    transport::{PlaybackClock, TimeMap, playback_anchors},
};
pub(super) use serde_json::json;
pub(super) use std::{fs, sync::atomic::AtomicBool};
pub(super) fn note(pitch: i32, start: f64, end: f64) -> Note {
    Note {
        pitch,
        start,
        end,
        velocity: 80,
    }
}
pub(super) fn smf(tracks: Vec<Vec<u8>>, format: u16) -> Vec<u8> {
    let mut out = b"MThd".to_vec();
    out.extend(6u32.to_be_bytes());
    out.extend(format.to_be_bytes());
    out.extend((tracks.len() as u16).to_be_bytes());
    out.extend(480u16.to_be_bytes());
    for t in tracks {
        out.extend(b"MTrk");
        out.extend((t.len() as u32).to_be_bytes());
        out.extend(t);
    }
    out
}
pub(super) fn eot() -> Vec<u8> {
    vec![0, 255, 47, 0]
}
pub(super) fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
