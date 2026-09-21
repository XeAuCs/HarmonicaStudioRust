use super::*;
use crate::project::{make_project, save_project};
use std::sync::Arc;

fn project(pitch: i32) -> Project {
    make_project(
        vec![Note {
            pitch,
            start: 0.,
            end: 0.08,
            velocity: 90,
        }],
        "测试曲谱",
    )
    .unwrap()
}
fn controller(root: &Path) -> AppController {
    let file = root.join("input.hstudio");
    save_project(&file, &project(60)).unwrap();
    let mut c = AppController::silent(root.join("data")).unwrap();
    c.open_project(&file).unwrap();
    c.wait_idle(Duration::from_secs(5)).unwrap();
    c
}
fn drain(c: &mut AppController) {
    c.wait_idle(Duration::from_secs(15)).unwrap();
}
#[test]
fn old_document_save_receipt_cannot_mark_new_document_saved() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    c.save_project(&root.path().join("old.hstudio")).unwrap();
    c.state.set_project(Some(project(67)), true, true);
    let revision = c.state.saved_revision;
    drain(&mut c);
    assert_eq!(c.state.saved_revision, revision);
    assert_eq!(c.state.autosave_revision, None);
}
#[test]
fn preserving_failure_aborts_switch_and_keeps_current_document() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    c.set_notes(project(62).notes).unwrap();
    drain(&mut c);
    c.state.saved_revision = 0;
    let other = root.path().join("other.hstudio");
    save_project(&other, &project(65)).unwrap();
    c.set_save_writer(Arc::new(|_, _| anyhow::bail!("simulated disk failure")))
        .unwrap();
    c.open_project(&other).unwrap();
    assert!(c.state.transition.is_some());
    assert!(c.wait_idle(Duration::from_secs(5)).is_err());
    assert_eq!(c.state.project.as_ref().unwrap().notes[0].pitch, 62);
    assert!(c.state.transition.is_none());
    assert!(!c.state.closed);
    c.set_save_writer(Arc::new(save_project)).unwrap();
}
#[test]
fn stale_export_cannot_overwrite_current_revision() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    c.export().unwrap();
    c.state.set_project(Some(project(67)), false, false);
    drain(&mut c);
    assert_eq!(c.state.project.as_ref().unwrap().notes[0].pitch, 67);
    assert!(c.state.result.is_none());
}
