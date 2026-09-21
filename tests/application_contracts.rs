use harmonica_studio::{
    controller::{AppController, Transport},
    jobs::{FollowUp, JobKind, JobRunner, SaveKind, SaveQueue},
    models::Note,
    project::{Project, load_project, make_project, save_project},
    remote::validate_command,
};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
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
fn manual_save_has_immutable_snapshot_and_new_edits_remain_dirty() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    let (start_tx, start_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let release = Mutex::new(release_rx);
    let first = AtomicBool::new(true);
    c.set_save_writer(Arc::new(move |path, p| {
        if first.swap(false, Ordering::Relaxed) {
            start_tx.send(()).unwrap();
            release.lock().unwrap().recv().unwrap();
        }
        save_project(path, p)
    }))
    .unwrap();
    let target = root.path().join("copy.hstudio");
    c.save_project(&target).unwrap();
    start_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    c.set_notes(project(64).notes).unwrap();
    release_tx.send(()).unwrap();
    drain(&mut c);
    assert_eq!(load_project(&target).unwrap().notes[0].pitch, 60);
    assert_eq!(
        load_project(&c.home.join("autosave.hstudio"))
            .unwrap()
            .notes[0]
            .pitch,
        64
    );
    assert!(c.state().project_dirty());
}
#[test]
fn failed_manual_save_aborts_requested_close_even_if_later_save_succeeds() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    c.set_save_writer(Arc::new(|path, p| {
        if path.file_name().unwrap() == "failure.hstudio" {
            anyhow::bail!("manual save failed")
        }
        save_project(path, p)
    }))
    .unwrap();
    c.save_project(&root.path().join("failure.hstudio"))
        .unwrap();
    assert!(!c.close().unwrap());
    assert!(c.wait_idle(Duration::from_secs(5)).is_err());
    assert!(!c.state().closed);
    assert!(c.state().transition.is_none());
    assert!(c.capabilities().can_edit);
}
#[test]
fn close_waits_for_latest_snapshot_and_stops_transport() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    c.set_notes(project(66).notes).unwrap();
    assert!(!c.close().unwrap());
    drain(&mut c);
    assert!(c.state().closed);
    assert_eq!(
        load_project(&c.home.join("autosave.hstudio"))
            .unwrap()
            .notes[0]
            .pitch,
        66
    );
    assert_ne!(c.state().transport, Transport::Playing);
}
#[test]
fn cancelled_export_never_installs_result_or_autoplays() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    c.listen().unwrap();
    c.cancel().unwrap();
    drain(&mut c);
    assert!(c.state().result.is_none());
    assert_ne!(c.state().transport, Transport::Playing);
}
#[test]
fn edited_notes_invalidate_old_export_and_follow_up_stop_prevents_autoplay() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    c.export().unwrap();
    drain(&mut c);
    assert!(!c.state().export_dirty());
    c.set_notes(project(64).notes).unwrap();
    assert!(c.state().export_dirty());
    assert!(c.state().result.is_none());
    c.listen().unwrap();
    c.stop().unwrap();
    drain(&mut c);
    assert!(!c.state().export_dirty());
    assert_ne!(c.state().transport, Transport::Playing);
    assert_eq!(c.state().result.as_ref().unwrap().actual[0].pitch, 64);
}
#[test]
fn highlight_removed_after_score_shortening() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    c.set_highlight(Some(0.06)).unwrap();
    c.set_notes(vec![Note {
        end: 0.04,
        ..project(62).notes[0].clone()
    }])
    .unwrap();
    assert_eq!(c.state().project.as_ref().unwrap().highlight, None);
    drain(&mut c);
}
#[test]
fn job_results_are_installed_only_by_owner_poll() {
    let mut jobs = JobRunner::default();
    let (tx, rx) = mpsc::channel();
    jobs.start(JobKind::Load, 7, 9, FollowUp::Play, move |_| {
        tx.send(()).unwrap();
        Ok(42)
    })
    .unwrap();
    rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(jobs.busy());
    let deadline = Instant::now() + Duration::from_secs(3);
    let done = loop {
        if let Some(done) = jobs.take_completed() {
            break done;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    };
    assert_eq!(done.result.unwrap(), 42);
    assert_eq!(done.job.revision, 7);
    assert_eq!(done.job.document, 9);
    assert!(!jobs.busy());
}
#[test]
fn save_queue_coalesces_adjacent_autosaves_but_preserves_manual_barrier() {
    let dir = tempfile::tempdir().unwrap();
    let (start_tx, start_rx) = mpsc::sync_channel(1);
    let (go_tx, go_rx) = mpsc::sync_channel(1);
    let gate = Mutex::new(go_rx);
    let first = AtomicBool::new(true);
    let seen = Arc::new(Mutex::new(vec![]));
    let observed = seen.clone();
    let mut queue = SaveQueue::with_writer(Arc::new(move |path, p| {
        if first.swap(false, Ordering::Relaxed) {
            start_tx.send(()).unwrap();
            gate.lock().unwrap().recv().unwrap();
        }
        observed.lock().unwrap().push((
            path.file_name().unwrap().to_string_lossy().into_owned(),
            p.notes[0].pitch,
        ));
        Ok(())
    }));
    let auto = vec![dir.path().join("auto.hstudio")];
    queue
        .submit(1, 1, Some(project(60)), auto.clone(), SaveKind::Auto)
        .unwrap();
    start_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    queue
        .submit(1, 2, Some(project(61)), auto.clone(), SaveKind::Auto)
        .unwrap();
    queue
        .submit(1, 3, Some(project(62)), auto.clone(), SaveKind::Auto)
        .unwrap();
    queue
        .submit(
            1,
            3,
            Some(project(62)),
            vec![dir.path().join("manual.hstudio")],
            SaveKind::Manual,
        )
        .unwrap();
    queue
        .submit(1, 4, Some(project(63)), auto, SaveKind::Auto)
        .unwrap();
    go_tx.send(()).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while queue.busy() {
        queue.take_completed();
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            ("auto.hstudio".into(), 60),
            ("auto.hstudio".into(), 62),
            ("manual.hstudio".into(), 62),
            ("auto.hstudio".into(), 63)
        ]
    );
}
#[test]
fn command_validation_rejects_path_injection_and_wrong_types() {
    for invalid in [
        json!({"action":"select","song_id":"x","path":"C:\\secret"}),
        json!({"action":"seek","position":true}),
        json!({"action":"select","song_id":"x","autoplay":1}),
        json!({"action":"seek","position":-1}),
        json!({"action":"run","command":"anything"}),
    ] {
        assert!(validate_command(invalid).is_err());
    }
    assert!(validate_command(json!({"action":"seek","position":2.5})).is_ok());
}
fn http(port: u16, request: String) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    answer
}
#[test]
fn remote_requires_bearer_and_correct_host_without_revealing_paths() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = controller(dir.path());
    let url = c.start_remote_local(0).unwrap();
    assert!(url.starts_with("http://127.0.0.1:"));
    let before = url.split("/#token=").next().unwrap();
    let port: u16 = before.rsplit(':').next().unwrap().parse().unwrap();
    // A wildcard listener would also occupy this address. Prove isolation using
    // real sockets, without contacting any LAN interface or changing firewall rules.
    let _other_loopback = std::net::TcpListener::bind(("127.0.0.2", port)).unwrap();
    assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_err());
    let token = url.split("#token=").nth(1).unwrap();
    let answer = http(
        port,
        format!("GET /api/state HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
    );
    assert!(answer.starts_with("HTTP/1.0 401"));
    let answer = http(
        port,
        format!(
            "GET /api/state HTTP/1.1\r\nHost: attacker.test:{port}\r\nAuthorization: Bearer {token}\r\n\r\n"
        ),
    );
    assert!(answer.starts_with("HTTP/1.0 403"));
    let answer = http(
        port,
        format!(
            "GET /api/state HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\n\r\n"
        ),
    );
    assert!(answer.starts_with("HTTP/1.0 200"));
    assert!(answer.contains("score_id"));
    assert!(!answer.contains(&dir.path().display().to_string()));
    c.stop_remote();
}
#[test]
fn phone_command_is_delivered_only_when_controller_polls() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = controller(dir.path());
    let url = c.start_remote_local(0).unwrap();
    let port: u16 = url
        .split("/#token=")
        .next()
        .unwrap()
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let token = url.split("#token=").nth(1).unwrap().to_string();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let body = r#"{"action":"seek","position":0.04}"#;
        let reply = http(
            port,
            format!(
                "POST /api/command HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        );
        done_tx.send(reply).unwrap();
    });
    let end = Instant::now() + Duration::from_secs(5);
    let response = loop {
        c.poll();
        if let Ok(reply) = done_rx.try_recv() {
            break reply;
        }
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(5));
    };
    assert!(response.contains("\"ok\":true"));
    assert!((c.state().logical_seek - 0.04).abs() < 0.001);
    c.stop_remote();
}

#[test]
fn source_catalog_options_are_retained_when_preparing_midi() {
    let root = tempfile::tempdir().unwrap();
    let midi = root.path().join("input.mid");
    harmonica_studio::midi::write_midi(
        &[Note {
            pitch: 60,
            start: 0.,
            end: 0.4,
            velocity: 90,
        }],
        &midi,
    )
    .unwrap();
    let mut c = AppController::silent(root.path().join("data")).unwrap();
    let options = harmonica_studio::models::Options {
        speed: 2.,
        transpose: 2,
        auto_octave: false,
        melody_mode: "highest".into(),
        phrase_octave: true,
        ..Default::default()
    };
    c.load_file(&midi, Some(options), true, false).unwrap();
    drain(&mut c);
    let p = c.state().project.as_ref().unwrap();
    assert_eq!(p.notes[0].pitch, 62);
    assert!((p.notes[0].end - 0.2).abs() < 0.01);
    assert_eq!(p.options.as_ref().unwrap()["melody_mode"], "highest");
    assert_eq!(p.options.as_ref().unwrap()["phrase_octave"], true);
}
#[test]
fn highlight_is_used_after_background_export_without_manual_seek() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    c.set_notes(vec![Note {
        pitch: 60,
        start: 0.,
        end: 1.,
        velocity: 90,
    }])
    .unwrap();
    c.set_highlight(Some(0.5)).unwrap();
    let mut preferences = c.preferences().clone();
    preferences.start_from_highlight = true;
    c.update_preferences(preferences).unwrap();
    c.listen().unwrap();
    drain(&mut c);
    assert_eq!(c.state().transport, Transport::Playing);
    assert!(c.state().logical_seek >= 0.49);
    c.pause().unwrap();
}
#[test]
fn closing_during_export_does_not_install_or_autoplay_late_result() {
    let root = tempfile::tempdir().unwrap();
    let mut c = controller(root.path());
    c.listen().unwrap();
    c.close().unwrap();
    drain(&mut c);
    assert!(c.state().closed);
    assert!(!c.busy());
    assert!(c.state().result.is_none());
    assert_ne!(c.state().transport, Transport::Playing);
}

#[test]
fn managed_project_identity_is_stable_for_equivalent_source_paths() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("曲谱.mid");
    std::fs::write(&source, b"MIDI").unwrap();
    let same = dir.path().join(".").join("曲谱.mid");
    let a = harmonica_studio::song_projects::song_project_path(dir.path(), &source, "ABC");
    let b = harmonica_studio::song_projects::song_project_path(dir.path(), &same, "abc");
    assert_eq!(a, b);
}
#[test]
fn malformed_preference_fields_fall_back_independently() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    std::fs::write(&path,serde_json::to_vec(&json!({"theme":"forest","compact":"yes","library_folder":42,"skip_long_rests":false,"start_from_highlight":true})).unwrap()).unwrap();
    let p = harmonica_studio::preferences::load_preferences(&path);
    assert_eq!(p.theme, "forest");
    assert!(!p.compact);
    assert!(p.library_folder.is_empty());
    assert!(!p.skip_long_rests);
    assert!(p.start_from_highlight);
}
#[test]
fn replaced_library_file_does_not_inherit_catalog_track_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("arrangement.mid");
    std::fs::write(&path, b"old arrangement").unwrap();
    let cancel = AtomicBool::new(false);
    let digest = harmonica_studio::library::file_digest(&path, &cancel).unwrap();
    std::fs::write(dir.path().join("catalog.json"),serde_json::to_vec(&json!([{"file":"arrangement.mid","title":"catalog title","sha256":digest,"options":{"track":4}}])).unwrap()).unwrap();
    let first = harmonica_studio::library::sample_entries(dir.path(), &cancel).unwrap();
    assert_eq!(first[0].title, "catalog title");
    assert!(first[0].options.is_some());
    std::fs::write(&path, b"replacement arrangement").unwrap();
    let next = harmonica_studio::library::sample_entries(dir.path(), &cancel).unwrap();
    assert_eq!(next[0].title, "arrangement");
    assert!(next[0].options.is_none());
}

#[test]
fn conversion_settings_roundtrip_omits_source_track_and_channel() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let options = harmonica_studio::models::Options {
        speed: 1.5,
        transpose: 3,
        track: Some(9),
        channel: Some(3),
        ..Default::default()
    };
    harmonica_studio::preferences::save_options(&path, &options).unwrap();
    let loaded = harmonica_studio::preferences::load_options(&path);
    assert_eq!(loaded.speed, 1.5);
    assert_eq!(loaded.transpose, 3);
    assert!(loaded.track.is_none());
    assert!(loaded.channel.is_none());
}
#[test]
fn invalid_conversion_settings_fall_back_to_valid_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, b"{\"speed\":0,\"transpose\":999}").unwrap();
    assert_eq!(
        harmonica_studio::preferences::load_options(&path),
        Default::default()
    );
}

#[derive(Default)]
struct RecordedGameState {
    armed: bool,
    starts: usize,
    plays: usize,
    playback_stops: usize,
    exits: usize,
}
struct RecordedGame(Arc<Mutex<RecordedGameState>>);
impl harmonica_studio::playback::GameBackend for RecordedGame {
    fn active(&self) -> bool {
        self.0.lock().unwrap().armed
    }
    fn start(&mut self, _: &Path, _: f64) -> anyhow::Result<()> {
        let mut state = self.0.lock().unwrap();
        anyhow::ensure!(!state.armed, "演奏器已在运行");
        state.armed = true;
        state.starts += 1;
        Ok(())
    }
    fn play(&mut self, _: &Path, _: f64) -> anyhow::Result<()> {
        let mut state = self.0.lock().unwrap();
        state.armed = true;
        state.plays += 1;
        Ok(())
    }
    fn stop_playback(&mut self) -> anyhow::Result<()> {
        self.0.lock().unwrap().playback_stops += 1;
        Ok(())
    }
    fn stop(&mut self) -> anyhow::Result<()> {
        let mut state = self.0.lock().unwrap();
        state.armed = false;
        state.exits += 1;
        Ok(())
    }
    fn reap(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn status(&mut self) -> harmonica_studio::playback::GameStatus {
        harmonica_studio::playback::GameStatus {
            state: if self.0.lock().unwrap().armed {
                "ready"
            } else {
                "idle"
            }
            .into(),
            ..Default::default()
        }
    }
}
fn recorded_game_controller(root: &Path) -> (AppController, Arc<Mutex<RecordedGameState>>) {
    let recorded = Arc::new(Mutex::new(RecordedGameState::default()));
    let mut c = AppController::with_backends(
        root.join("data"),
        Box::<harmonica_studio::playback::FakeAudio>::default(),
        Box::new(RecordedGame(recorded.clone())),
    )
    .unwrap();
    c.set_project(project(60)).unwrap();
    drain(&mut c);
    (c, recorded)
}
#[test]
fn desktop_end_game_exits_helper_and_allows_arming_again() {
    let dir = tempfile::tempdir().unwrap();
    let (mut c, recorded) = recorded_game_controller(dir.path());
    c.game_play(true).unwrap();
    drain(&mut c);
    assert!(recorded.lock().unwrap().armed);
    assert!(c.game_active());
    let exits = recorded.lock().unwrap().exits;
    c.stop_game().unwrap();
    assert!(!recorded.lock().unwrap().armed);
    assert!(!c.game_active());
    assert_eq!(recorded.lock().unwrap().exits, exits + 1);
    assert_eq!(recorded.lock().unwrap().playback_stops, 0);
    c.game_play(true).unwrap();
    assert_eq!(recorded.lock().unwrap().starts, 2);
}
#[test]
fn phone_game_stop_keeps_helper_armed_for_next_play_command() {
    let dir = tempfile::tempdir().unwrap();
    let (mut c, recorded) = recorded_game_controller(dir.path());
    c.game_play(true).unwrap();
    drain(&mut c);
    let exits = recorded.lock().unwrap().exits;
    let response = harmonica_studio::remote::handle_command(&mut c, json!({"action":"game_stop"}));
    assert_eq!(response["ok"], true);
    assert!(recorded.lock().unwrap().armed);
    assert!(c.game_active());
    assert_eq!(recorded.lock().unwrap().exits, exits);
    assert_eq!(recorded.lock().unwrap().playback_stops, 1);
    let response = harmonica_studio::remote::handle_command(&mut c, json!({"action":"game_play"}));
    assert_eq!(response["ok"], true);
    assert_eq!(recorded.lock().unwrap().plays, 1);
    assert_eq!(recorded.lock().unwrap().starts, 1);
}
#[test]
fn ending_game_during_export_cancels_pending_arm() {
    let dir = tempfile::tempdir().unwrap();
    let (mut c, recorded) = recorded_game_controller(dir.path());
    c.game_play(true).unwrap();
    assert!(c.game_active());
    c.stop_game().unwrap();
    drain(&mut c);
    assert_eq!(recorded.lock().unwrap().starts, 0);
    assert!(!recorded.lock().unwrap().armed);
    assert!(!c.game_active());
}
#[test]
fn preview_position_label_tracks_transport_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = controller(dir.path());
    c.listen().unwrap();
    drain(&mut c);
    assert_eq!(c.state().position_label, "试听中");
    c.pause().unwrap();
    assert_eq!(c.state().position_label, "已暂停");
    c.stop().unwrap();
    assert_eq!(c.state().position_label, "未播放");
    c.listen().unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    while c.state().transport == Transport::Playing && Instant::now() < until {
        thread::sleep(Duration::from_millis(10));
        c.poll();
    }
    assert_eq!(c.state().transport, Transport::Ended);
    assert_eq!(c.state().position_label, "试听结束");
}
