use super::support::*;

#[test]
fn export_outputs_share_events_and_preserve_score_on_reexport() {
    let temp = tempfile::tempdir().unwrap();
    let mut project = make_project(
        vec![
            note(61, 0.0, 0.01),
            note(73, 0.01, 0.02),
            note(60, 5.0, 5.2),
        ],
        "兼容测试",
    )
    .unwrap();
    project.options = Some(json!({"skip_long_rests":true}));
    let original = project.clone();
    let cancel = AtomicBool::new(false);
    let first = export_project(&project, temp.path(), &cancel).unwrap();
    assert_eq!(project, original);
    assert_eq!(fs::read_dir(&first.folder).unwrap().count(), 7);
    let restored = load_project(&first.folder.join("工程.hstudio")).unwrap();
    assert_eq!(restored.notes, original.notes);
    let second = export_project(&restored, temp.path(), &cancel).unwrap();
    assert_ne!(first.folder, second.folder);
    for filename in ["按键时间表.json", "音符.json", "口琴单旋律.mid", "试听.wav"] {
        assert_eq!(
            fs::read(first.folder.join(filename)).unwrap(),
            fs::read(second.folder.join(filename)).unwrap(),
            "{filename}"
        );
    }
    let events: Vec<Event> =
        serde_json::from_slice(&fs::read(first.folder.join("按键时间表.json")).unwrap()).unwrap();
    assert_eq!(decode_events(&events).unwrap(), first.actual);
    assert_eq!(
        read_midi(&first.folder.join("口琴单旋律.mid")).unwrap().0[&(0, 0)],
        first.actual
    );
    let script = fs::read_to_string(first.folder.join("演奏脚本.ahk")).unwrap();
    assert!(!script.contains("__EVENTS__"));
    for (ms, key, down) in events {
        assert!(script.contains(&format!("{ms},{key},{down}")));
    }
    let wav = fs::read(first.folder.join("试听.wav")).unwrap();
    assert_eq!(&wav[..4], b"RIFF");
    let data_size = u32::from_le_bytes(wav[40..44].try_into().unwrap());
    let duration = data_size as f64 / 2.0 / 22050.0;
    assert!(
        (duration - first.report["duration_seconds"].as_f64().unwrap() - 1.0 / 3.0).abs() < 0.001
    );
}
#[test]
fn conversion_is_self_contained_after_source_removed() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.mid");
    write_midi(&[note(36, 0.0, 0.5), note(96, 2.0, 2.5)], &path).unwrap();
    let cancel = AtomicBool::new(false);
    let first = convert(
        &path,
        &temp.path().join("exports"),
        &Options {
            phrase_octave: true,
            ..Options::default()
        },
        &cancel,
    )
    .unwrap();
    fs::remove_file(path).unwrap();
    let second = export_project(&first.project, &temp.path().join("exports"), &cancel).unwrap();
    assert_eq!(second.project.notes, first.project.notes);
    assert_eq!(
        second.report["octave_adjustments"],
        first.report["octave_adjustments"]
    );
    assert_eq!(second.report["edited"], true);
}
#[test]
fn invalid_options_and_cancel_preserve_existing_exports() {
    let temp = tempfile::tempdir().unwrap();
    let mut project = make_project(vec![note(60, 0.0, 0.2)], "保留").unwrap();
    let first = export_project(&project, temp.path(), &AtomicBool::new(false)).unwrap();
    let before = fs::read(first.folder.join("工程.hstudio")).unwrap();
    assert!(export_project(&project, temp.path(), &AtomicBool::new(true)).is_err());
    for value in [
        json!(null),
        json!(0),
        json!(1),
        json!("true"),
        json!([]),
        json!({}),
    ] {
        project.options = Some(json!({"skip_long_rests":value}));
        assert!(export_project(&project, temp.path(), &AtomicBool::new(false)).is_err());
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    assert_eq!(fs::read(first.folder.join("工程.hstudio")).unwrap(), before);
}
#[test]
fn wav_cancellation_does_not_create_output() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("cancel.wav");
    let (events, _) = build_events(&[note(60, 0.0, 0.2)]).unwrap();
    assert!(render_wav(&events, &path, &AtomicBool::new(true), 22050).is_err());
    assert!(!path.exists());
}
#[test]
fn cancellation_during_render_removes_only_current_staging_directory() {
    use std::{
        sync::{Arc, atomic::Ordering},
        thread,
        time::{Duration, Instant},
    };
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("existing.txt");
    fs::write(&existing, b"preserve").unwrap();
    let project = make_project(vec![note(60, 0.0, 1000.0)], "取消中的导出").unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let token = cancel.clone();
    let output = temp.path().to_path_buf();
    let worker = thread::spawn(move || export_project(&project, &output, &token));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut rendering = false;
    while Instant::now() < deadline && !worker.is_finished() {
        rendering = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry.file_name().to_string_lossy().starts_with(".partial-")
                    && entry.path().join("试听.wav").is_file()
            });
        if rendering {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    cancel.store(true, Ordering::Release);
    let outcome = worker.join().unwrap();
    assert!(
        rendering,
        "没有观察到 WAV 渲染，不能将预取消当作中途取消验证"
    );
    assert!(outcome.unwrap_err().to_string().contains("已取消"));
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    assert_eq!(fs::read(&existing).unwrap(), b"preserve");
}
#[test]
fn invalid_conversion_options_do_not_create_output() {
    let options = Options {
        speed: f64::NAN,
        ..Options::default()
    };
    assert!(options.validate().is_err());
    assert!(
        Options {
            transpose: 25,
            ..Options::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        Options {
            channel: Some(16),
            ..Options::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        Options {
            track: Some(1024),
            ..Options::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        Options {
            melody_mode: "guess".into(),
            ..Options::default()
        }
        .validate()
        .is_err()
    );
}
