use super::support::*;

#[test]
fn project_normalization_is_sorted_idempotent_and_independent() {
    let original = vec![
        note(62, 1.7, 2.012345678901),
        note(60, 1.4, 1.7000000000000002),
    ];
    let project = make_project(original.clone(), "  我的曲谱  ").unwrap();
    assert_eq!(project.title, "我的曲谱");
    assert_eq!(project.notes[0].end, 1.7);
    assert_eq!(project.notes[1], original[0]);
    assert_eq!(validate_project(&project).unwrap(), project);
    assert_eq!(original[1].end, 1.7000000000000002);
}
#[test]
fn note_contract_rejects_invalid_fields_and_overlap() {
    let cases = vec![
        vec![note(47, 0.0, 1.0)],
        vec![note(86, 0.0, 1.0)],
        vec![note(60, -0.1, 1.0)],
        vec![note(60, 0.0, 1200.1)],
        vec![note(60, f64::NAN, 1.0)],
        vec![note(60, 0.0, f64::INFINITY)],
        vec![note(60, 0.0, 0.0)],
        vec![note(60, 0.0, 0.2001), note(62, 0.2, 0.4)],
        vec![note(60, 0.0, 1e-9), note(62, 0.0, 1e-9)],
    ];
    for notes in cases {
        assert!(normalize_score_notes(&notes).is_err());
    }
    let mut bad = note(60, 0.0, 1.0);
    bad.velocity = 0;
    assert!(normalize_score_notes(&[bad]).is_err());
}
#[test]
fn old_project_json_and_bom_are_compatible() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("旧版.hstudio");
    fs::write(&path,"\u{feff}{\"schema_version\":1,\"title\":\"旧工程\",\"notes\":[{\"pitch\":60,\"start\":0,\"end\":0.2}],\"source\":{\"path\":\"missing.mid\"},\"options\":{\"skip_long_rests\":false}}").unwrap();
    let project = load_project(&path).unwrap();
    assert_eq!(project.notes[0].velocity, 80);
    save_project(&path, &project).unwrap();
    assert_eq!(load_project(&path).unwrap(), project);
    for data in [
        "[]",
        "{\"schema_version\":true,\"notes\":[]}",
        "{\"schema_version\":2,\"notes\":[]}",
        "{\"schema_version\":1,\"notes\":[{\"pitch\":60,\"start\":false,\"end\":1}]}",
        "{\"schema_version\":1,\"notes\":[{\"pitch\":60,\"start\":NaN,\"end\":1}]}",
    ] {
        fs::write(&path, data).unwrap();
        assert!(load_project(&path).is_err());
    }
}
#[test]
fn invalid_project_cannot_overwrite_previous_save() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("saved.hstudio");
    let mut project = make_project(vec![note(60, 0.0, 0.2)], "曲谱").unwrap();
    save_project(&path, &project).unwrap();
    let before = fs::read(&path).unwrap();
    project.notes[0].end = f64::NAN;
    assert!(save_project(&path, &project).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}
#[cfg(windows)]
#[test]
fn locked_destination_save_preserves_previous_project_and_cleans_temporary() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("locked.hstudio");
    let mut project = make_project(vec![note(60, 0.0, 0.2)], "原工程").unwrap();
    save_project(&path, &project).unwrap();
    let original = fs::read(&path).unwrap();
    let guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    project.title = "不能写入的修改".into();
    assert!(save_project(&path, &project).is_err());
    drop(guard);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}
