use harmonica_studio::{controller::AppController, performance};
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};

#[test]
fn phone_selection_has_private_stage_timings() {
    const CHILD: &str = "HARMONICA_PERF_TEST_ROOT";
    let Ok(root) = std::env::var(CHILD) else {
        let dir = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "phone_selection_has_private_stage_timings",
                "--nocapture",
            ])
            .env(CHILD, dir.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let text = std::fs::read_to_string(dir.path().join("timing.jsonl")).unwrap();
        assert!(!text.contains("private-song"));
        assert!(!text.contains(&dir.path().to_string_lossy().to_string()));
        for stage in [
            "remote.command_queue",
            "job.queue",
            "job.result_wait",
            "midi.parse",
            "midi.rank_parts",
            "convert.extract_melody",
            "preview.render_wav",
            "preview.open_audio",
            "selection.install_result",
        ] {
            let rows: Vec<serde_json::Value> = text
                .lines()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect();
            let row = rows
                .iter()
                .find(|r| r["stage"] == stage && r["event"] != "begin")
                .expect(stage);
            println!("{stage}: {} ms", row["duration_ms"]);
        }
        return;
    };
    let root = std::path::PathBuf::from(root);
    let log = root.join("timing.jsonl");
    performance::initialize(&log).unwrap();
    let library = root.join("library");
    std::fs::create_dir_all(&library).unwrap();
    let source = library.join("private-song.mid");
    let notes = (0..24)
        .map(|i| harmonica_studio::models::Note {
            pitch: 60 + i % 12,
            start: i as f64 * 0.08,
            end: i as f64 * 0.08 + 0.06,
            velocity: 80,
        })
        .collect::<Vec<_>>();
    harmonica_studio::midi::write_midi(&notes, &source).unwrap();
    let home = root.join("data");
    std::fs::create_dir_all(&home).unwrap();
    harmonica_studio::preferences::save_preferences(
        &home.join("preferences.json"),
        &harmonica_studio::preferences::Preferences {
            library_folder: library.to_string_lossy().into(),
            ..Default::default()
        },
    )
    .unwrap();
    let mut c = AppController::silent(home).unwrap();
    c.refresh_library().unwrap();
    c.wait_idle(Duration::from_secs(10)).unwrap();
    assert_eq!(c.library().len(), 1);
    let url = c.start_remote_local(0).unwrap();
    let (base, token) = url.split_once("/#token=").unwrap();
    let port: u16 = base.rsplit(':').next().unwrap().parse().unwrap();
    let token = token.to_owned();
    let secret = token.clone();
    let song = harmonica_studio::library::song_id(&source);
    let client = std::thread::spawn(move || {
        let body =
            serde_json::json!({"action":"select", "song_id":song, "autoplay":false}).to_string();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(stream,"POST /api/command HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("\"ok\":true"));
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        c.poll();
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(!text.contains(&secret), "日志不应包含遥控凭据");
        if text.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .is_ok_and(|r| r["stage"] == "selection.install_result" && r["event"] == "end")
        }) {
            break;
        }
        assert!(Instant::now() < deadline, "选歌分阶段日志没有完成");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(c.state().result.is_some());
    client.join().unwrap();
    c.stop_remote();
    c.close().unwrap();
    c.wait_idle(Duration::from_secs(10)).unwrap();
}
