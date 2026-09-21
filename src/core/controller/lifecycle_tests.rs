use super::*;
use std::sync::{Arc, atomic::AtomicBool, mpsc};
#[test]
fn close_does_not_complete_until_cancelled_worker_has_released_its_resources() {
    let root = tempfile::tempdir().unwrap();
    let mut c = AppController::silent(root.path().join("data")).unwrap();
    let (start_tx, start_rx) = mpsc::channel();
    let (go_tx, go_rx) = mpsc::channel();
    let released = Arc::new(AtomicBool::new(false));
    let observed = released.clone();
    c.jobs
        .start(JobKind::Load, 0, 0, FollowUp::Play, move |cancel| {
            start_tx.send(()).unwrap();
            go_rx.recv().unwrap();
            assert!(cancel.load(Ordering::Relaxed));
            observed.store(true, Ordering::Release);
            anyhow::bail!("cancelled test worker")
        })
        .unwrap();
    start_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(!c.close().unwrap());
    assert!(c.busy());
    assert!(!c.state.closed);
    c.poll();
    assert!(!c.state.closed);
    assert!(!released.load(Ordering::Acquire));
    go_tx.send(()).unwrap();
    c.wait_idle(Duration::from_secs(3)).unwrap();
    assert!(released.load(Ordering::Acquire));
    assert!(c.state.closed);
    assert!(!c.busy());
}
#[test]
fn unchanged_phone_score_reuses_cached_note_buffer() {
    let root = tempfile::tempdir().unwrap();
    let mut c = AppController::silent(root.path().join("data")).unwrap();
    c.set_project(
        crate::project::make_project(
            vec![Note {
                pitch: 60,
                start: 0.,
                end: 1.,
                velocity: 90,
            }],
            "cache",
        )
        .unwrap(),
    )
    .unwrap();
    c.remote_state();
    let original = c.score_cache["notes"].as_array().unwrap().as_ptr();
    for _ in 0..20 {
        c.remote_state();
        assert_eq!(
            original,
            c.score_cache["notes"].as_array().unwrap().as_ptr()
        );
    }
    let old_id = c.score_cache["id"].clone();
    c.set_notes(vec![Note {
        pitch: 62,
        start: 0.,
        end: 1.,
        velocity: 90,
    }])
    .unwrap();
    c.remote_state();
    assert_ne!(old_id, c.score_cache["id"]);
}
