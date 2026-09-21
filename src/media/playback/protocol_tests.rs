use super::*;
use std::{thread, time::Duration};

// Run production command parsing AND status publishing in AHK. Only input-producing
// handlers are replaced. No hotkeys, tray, window targeting or SendEvent is loaded.
struct Harness {
    player: ScriptPlayer,
    dir: tempfile::TempDir,
}
impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let source = include_str!("../../../assets/player.ahk");
        let control = source
            .split("\nCheckControl() {")
            .nth(1)
            .unwrap()
            .split("\nHandleExit(*) {")
            .next()
            .unwrap();
        let script = format!(
            r#"#Requires AutoHotkey v2.0
#SingleInstance Off
#Warn All, StdOut
stopFile := A_Args[1]
commandFile := A_Args[2]
statusFile := A_Args[3]
lastCommandId := 0
defaultStartMs := 0
state := "ready"
stateMessage := "fixture"
position := 0.0
duration := 100.0
running := false
startAt := 0
playOffsetMs := 0
minimumPosition := 0
; Match production's periodic status publication: a transient replacement
; failure must not permanently lose an otherwise accepted command's receipt.
SetTimer(PublishStatus, 100)
PublishStatus()
loop {{
    CheckControl()
    Sleep(10)
}}
CheckControl() {{{control}
BeginPlay() {{
    global state, position, defaultStartMs, stateMessage, commandFile
    state := "countdown"
    position := defaultStartMs / 1000
    stateMessage := "play dispatched"
    FileAppend("dispatched", commandFile ".dispatched")
}}
StopPlay(message) {{
    global state, position, stateMessage
    state := "ready"
    position := 0.0
    stateMessage := "stop dispatched"
}}
NowMs() {{
    return 0
}}
"#
        );
        let path = dir.path().join("protocol.ahk");
        fs::write(&path, script).unwrap();
        let mut command = Command::new(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("third_party/AutoHotkey/AutoHotkey64.exe"),
        );
        command
            .arg("/ErrorStdOut")
            .arg(path)
            .arg(dir.path().join("exit.stop"))
            .arg(dir.path().join("command.json"))
            .arg(dir.path().join("status.json"));
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let mut player = ScriptPlayer::new(dir.path().to_owned());
        player.process = Some(command.spawn().expect("bundled AHK must be available"));
        player.instance = Some(dir.path().to_owned());
        let h = Self { player, dir };
        h.wait_id(0);
        h
    }
    fn snapshot(&self) -> Value {
        fs::read_to_string(self.dir.path().join("status.json"))
            .ok()
            .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
            .unwrap_or(Value::Null)
    }
    fn wait_id(&self, id: u64) -> Value {
        let start = Instant::now();
        loop {
            let v = self.snapshot();
            if v["request_id"] == id {
                return v;
            }
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "AHK did not acknowledge {id}: {v}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn raw(&self, text: &str) {
        write_command_file(&self.dir.path().join("command.json"), text.as_bytes()).unwrap();
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.player.stop();
        let start = Instant::now();
        while self.player.alive() && start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(10));
        }
        if let Some(mut child) = self.player.process.take() {
            // Only this test-owned, input-free helper may be terminated on failure.
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn command_retries_short_lock_but_preserves_id_on_persistent_lock() {
    use std::os::windows::fs::OpenOptionsExt;
    let mut h = Harness::new();
    h.player.command("stop", None).unwrap();
    h.wait_id(1);
    let path = h.dir.path().join("command.json");
    let reader = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let release = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        drop(reader);
    });
    h.player.command("play", None).unwrap();
    release.join().unwrap();
    h.wait_id(2);
    let reader = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let previous = fs::read(&path).unwrap();
    assert!(h.player.command("stop", None).is_err());
    assert_eq!(h.player.command_id, 2);
    assert_eq!(fs::read(&path).unwrap(), previous);
    drop(reader);
    h.player.command("stop", None).unwrap();
    assert_eq!(h.wait_id(3)["state"], "ready");
}

#[test]
fn status_recovers_after_reader_temporarily_blocks_replacement() {
    use std::os::windows::fs::OpenOptionsExt;
    let mut h = Harness::new();
    // Reproduce a failed status publication without altering command parsing.
    let reader = fs::OpenOptions::new()
        .read(true)
        .share_mode(1) // FILE_SHARE_READ: deliberately deny replacement.
        .open(h.dir.path().join("status.json"))
        .unwrap();
    h.player.command("play", None).unwrap();
    let start = Instant::now();
    while !h.dir.path().join("command.json.dispatched").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "play was not dispatched"
        );
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(150));
    assert_eq!(h.snapshot()["request_id"], 0);
    drop(reader);
    assert_eq!(h.wait_id(1)["state"], "countdown");
    assert_eq!(h.player.status().state, "countdown");
}

#[test]
fn rust_commands_are_dispatched_and_status_is_accepted_by_rust() {
    let mut h = Harness::new();
    for (action, offset, id, state) in [
        ("play", None, 1, "countdown"),
        ("stop", None, 2, "ready"),
        ("play", Some(1001), 3, "countdown"),
        ("stop", None, 4, "ready"),
        ("play", Some(0), 5, "countdown"),
    ] {
        h.player.command(action, offset).unwrap();
        let ack = h.wait_id(id);
        assert_eq!(ack["state"], state);
        let accepted = h.player.status();
        assert_eq!(accepted.state, state);
        assert_eq!(
            accepted.position,
            if action == "play" {
                offset.unwrap_or(0) as f64 / 1000.0
            } else {
                0.0
            }
        );
        // Compatibility with already exported protocol-1 scripts.
        let bytes = fs::read_to_string(h.dir.path().join("command.json")).unwrap();
        assert!(bytes.starts_with(&format!("{{\"id\":{id},\"action\":")));
    }
}

#[test]
fn ahk_accepts_field_permutations_and_rejects_malformed_or_replayed_commands() {
    let h = Harness::new();
    let permutations = [
        r#"{"id":1,"action":"play","start_ms":1001}"#,
        r#"{"action":"stop","id":2}"#,
        r#"{"start_ms":1001,"action":"play","id":3}"#,
        r#"{"id":4,"start_ms":1001,"action":"play"}"#,
        r#"{"action":"play","id":5,"start_ms":1001}"#,
        r#"{"action":"play","start_ms":1001,"id":6}"#,
        r#"{"start_ms":1001,"id":7,"action":"play"}"#,
    ];
    for (i, text) in permutations.iter().enumerate() {
        h.raw(text);
        h.wait_id(i as u64 + 1);
    }
    let previous = h.snapshot();
    for text in [
        r#"{"id":7,"action":"stop"}"#,
        r#"{"id":6,"action":"stop"}"#,
        r#"{"id":8,"id":9,"action":"stop"}"#,
        r#"{"id":8,"action":"play","action":"stop"}"#,
        r#"{"id":8,"action":"play","start_ms":1,"start_ms":2}"#,
        r#"{"id":8,"action":"other"}"#,
        r#"{"id":8}"#,
        r#"{"action":"play"}"#,
        r#"{"id":8,"action":"play","start_ms":-1}"#,
        r#"{"id":8,"action":"play","start_ms":86400001}"#,
        r#"{"id":8.5,"action":"play"}"#,
        r#"{"id":08,"action":"play"}"#,
        r#"{"id":18446744073709551615,"action":"play"}"#,
        r#"{"id":9007199254740992,"action":"play"}"#,
        r#"{"id":8,"action":"play","start_ms":1.5}"#,
        r#"{"id":8,"action":"play","start_ms":01}"#,
        r#"{"id":8,"action":"play","start_ms":"1"}"#,
        r#"{"id":8,"action":{"action":"play"}}"#,
        r#"{"id":8,"action":"play","extra":1}"#,
        r#"{"id":8,"action":"play",}"#,
        r#"{"id":8,"action":"play"} trailing"#,
    ] {
        h.raw(text);
        thread::sleep(Duration::from_millis(60));
        assert_eq!(
            h.snapshot(),
            previous,
            "accepted invalid/replayed command: {text}"
        );
    }
    h.raw("{\n \"action\" : \"stop\", \"id\" : 8\n}");
    assert_eq!(h.wait_id(8)["state"], "ready");
}
