//! Seekable WAV audio and cooperative AutoHotkey lifecycle. No input from tests.
#[cfg(all(test, windows))]
#[path = "playback/protocol_tests.rs"]
mod protocol_tests;
use crate::paths::{atomic_write, resource_root, unique_id};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Instant,
};
pub trait AudioBackend {
    fn load(&mut self, path: &Path) -> Result<()>;
    fn duration(&self) -> f64;
    fn position(&mut self) -> Result<f64>;
    fn playing(&mut self) -> Result<bool>;
    fn play(&mut self, start: f64) -> Result<()>;
    fn pause(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn seek(&mut self, seconds: f64, resume: bool) -> Result<()>;
    fn close(&mut self) -> Result<()>;
}
#[derive(Debug)]
pub struct AudioDeviceError {
    pub code: u32,
    pub detail: String,
}
impl std::fmt::Display for AudioDeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "试听播放失败：{}（错误 {}）。", self.detail, self.code)
    }
}
impl std::error::Error for AudioDeviceError {}
pub type MciSender = Box<dyn FnMut(&str) -> Result<String>>;
pub struct AudioPlayer {
    alias: String,
    loaded: bool,
    duration_ms: u64,
    pub path: Option<PathBuf>,
    staged: Option<PathBuf>,
    sender: Option<MciSender>,
}
impl Default for AudioPlayer {
    fn default() -> Self {
        Self {
            alias: format!("harmonica_{}", unique_id().replace('-', "")),
            loaded: false,
            duration_ms: 0,
            path: None,
            staged: None,
            sender: None,
        }
    }
}
impl AudioPlayer {
    pub fn with_sender(sender: MciSender) -> Self {
        let mut value = Self::default();
        value.sender = Some(sender);
        value
    }
    fn send(&mut self, command: &str) -> Result<String> {
        if let Some(sender) = &mut self.sender {
            return sender(command);
        }
        native_mci(command)
    }
    fn milliseconds(&self, seconds: f64) -> Result<u64> {
        ensure!(seconds.is_finite(), "播放位置必须为有限数字。");
        Ok((seconds.clamp(0., self.duration()) * 1000.).round() as u64)
    }
    fn require_loaded(&self) -> Result<()> {
        ensure!(self.loaded, "请先生成或打开一份试听音频。");
        Ok(())
    }
    fn clean_staged(&mut self) {
        if let Some(path) = self.staged.take() {
            let _ = fs::remove_file(path.join("audio.wav"));
            let _ = fs::remove_dir(path);
        }
    }
}
impl AudioBackend for AudioPlayer {
    fn load(&mut self, path: &Path) -> Result<()> {
        self.close()?;
        let target = crate::paths::absolute(path)?;
        let text = target.to_string_lossy();
        ensure!(
            !text.chars().any(|c| ['"', '\0', '\r', '\n'].contains(&c)),
            "试听音频路径包含无效字符。"
        );
        let _ = wav_duration(&target)?;
        let result = self.send(&format!(
            "open \"{}\" type waveaudio alias {}",
            target.display(),
            self.alias
        ));
        if let Err(e) = result {
            if e.downcast_ref::<AudioDeviceError>().map(|e| e.code) != Some(304) {
                return Err(e);
            }
            let dir = std::env::temp_dir().join(format!("hs-{}", unique_id()));
            fs::create_dir(&dir)?;
            self.staged = Some(dir.clone());
            let fallback = (|| {
                fs::copy(&target, dir.join("audio.wav"))?;
                self.send(&format!(
                    "open \"{}\" type waveaudio alias {}",
                    dir.join("audio.wav").display(),
                    self.alias
                ))?;
                Ok(())
            })();
            if let Err(e) = fallback {
                self.clean_staged();
                return Err(e);
            }
        }
        self.loaded = true;
        let result = (|| {
            self.send(&format!("set {} time format milliseconds", self.alias))?;
            self.duration_ms = self
                .send(&format!("status {} length", self.alias))?
                .parse()
                .context("音频播放器无法读取时长")?;
            self.path = Some(target);
            Ok(())
        })();
        if result.is_err() {
            let _ = self.close();
        }
        result
    }
    fn duration(&self) -> f64 {
        self.duration_ms as f64 / 1000.
    }
    fn position(&mut self) -> Result<f64> {
        if !self.loaded {
            return Ok(0.);
        }
        let pos: u64 = self
            .send(&format!("status {} position", self.alias))?
            .parse()
            .context("音频播放器返回了无法识别的播放位置")?;
        Ok((pos as f64 / 1000.).min(self.duration()))
    }
    fn playing(&mut self) -> Result<bool> {
        if !self.loaded {
            return Ok(false);
        }
        Ok(self
            .send(&format!("status {} mode", self.alias))?
            .eq_ignore_ascii_case("playing"))
    }
    fn play(&mut self, start: f64) -> Result<()> {
        self.seek(start, true)
    }
    fn pause(&mut self) -> Result<()> {
        if self.playing()? {
            self.send(&format!("pause {}", self.alias))?;
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        if self.loaded {
            self.send(&format!("seek {} to start wait", self.alias))?;
        }
        Ok(())
    }
    fn seek(&mut self, seconds: f64, resume: bool) -> Result<()> {
        self.require_loaded()?;
        let ms = self.milliseconds(seconds)?.min(self.duration_ms);
        let target = if ms == self.duration_ms {
            "end".into()
        } else {
            ms.to_string()
        };
        self.send(&format!("seek {} to {} wait", self.alias, target))?;
        if resume && ms < self.duration_ms {
            self.send(&format!("play {}", self.alias))?;
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        if self.loaded {
            self.send(&format!("close {}", self.alias))?;
        }
        self.loaded = false;
        self.duration_ms = 0;
        self.path = None;
        self.clean_staged();
        Ok(())
    }
}
impl Drop for AudioPlayer {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
#[cfg(windows)]
fn native_mci(command: &str) -> Result<String> {
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn mciSendStringW(
            command: *const u16,
            result: *mut u16,
            length: u32,
            callback: isize,
        ) -> u32;
        fn mciGetErrorStringW(code: u32, text: *mut u16, length: u32) -> i32;
    }
    let cmd: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
    let mut answer = [0u16; 256];
    let code = unsafe { mciSendStringW(cmd.as_ptr(), answer.as_mut_ptr(), answer.len() as u32, 0) };
    if code != 0 {
        let mut message = [0u16; 512];
        unsafe { mciGetErrorStringW(code, message.as_mut_ptr(), message.len() as u32) };
        return Err(AudioDeviceError {
            code,
            detail: String::from_utf16_lossy(
                &message[..message
                    .iter()
                    .position(|x| *x == 0)
                    .unwrap_or(message.len())],
            ),
        }
        .into());
    }
    Ok(String::from_utf16_lossy(
        &answer[..answer.iter().position(|x| *x == 0).unwrap_or(answer.len())],
    )
    .trim()
    .into())
}
#[cfg(not(windows))]
fn native_mci(_: &str) -> Result<String> {
    bail!("内置试听目前支持 Windows。")
}
pub fn wav_duration(path: &Path) -> Result<f64> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = fs::File::open(path).context("找不到试听音频，请重新生成曲谱")?;
    let mut header = [0u8; 12];
    f.read_exact(&mut header)?;
    ensure!(
        &header[..4] == b"RIFF" && &header[8..] == b"WAVE",
        "需要有效的 PCM WAV 文件"
    );
    let mut rate = 0u32;
    let mut data = 0u64;
    let len = f.metadata()?.len();
    while f.stream_position()? + 8 <= len {
        let mut h = [0u8; 8];
        f.read_exact(&mut h)?;
        let size = u32::from_le_bytes(h[4..8].try_into().unwrap()) as u64;
        let at = f.stream_position()?;
        ensure!(at + size <= len, "WAV 数据不完整");
        if &h[..4] == b"fmt " {
            ensure!(size >= 16, "WAV 格式无效");
            let mut fmt = [0u8; 16];
            f.read_exact(&mut fmt)?;
            ensure!(
                u16::from_le_bytes(fmt[..2].try_into().unwrap()) == 1,
                "试听需要 PCM WAV 文件"
            );
            rate = u32::from_le_bytes(fmt[8..12].try_into().unwrap());
        } else if &h[..4] == b"data" {
            data = size;
        }
        f.seek(SeekFrom::Start(at + size + (size % 2)))?;
    }
    ensure!(rate > 0 && data > 0, "试听需要包含音频的 PCM WAV 文件");
    Ok(data as f64 / rate as f64)
}
#[derive(Default)]
pub struct FakeAudio {
    duration: f64,
    position: f64,
    started: Option<Instant>,
}
impl AudioBackend for FakeAudio {
    fn load(&mut self, path: &Path) -> Result<()> {
        self.duration = wav_duration(path)?;
        self.position = 0.;
        self.started = None;
        Ok(())
    }
    fn duration(&self) -> f64 {
        self.duration
    }
    fn position(&mut self) -> Result<f64> {
        Ok((self.position
            + self
                .started
                .map(|s| s.elapsed().as_secs_f64())
                .unwrap_or(0.))
        .min(self.duration))
    }
    fn playing(&mut self) -> Result<bool> {
        Ok(self.started.is_some() && self.position()? < self.duration)
    }
    fn play(&mut self, start: f64) -> Result<()> {
        self.seek(start, true)
    }
    fn pause(&mut self) -> Result<()> {
        self.position = self.position()?;
        self.started = None;
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        self.position = 0.;
        self.started = None;
        Ok(())
    }
    fn seek(&mut self, seconds: f64, resume: bool) -> Result<()> {
        ensure!(seconds.is_finite(), "播放位置必须为有限数字");
        self.position = seconds.clamp(0., self.duration);
        self.started = if resume { Some(Instant::now()) } else { None };
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        *self = Self::default();
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameStatus {
    pub state: String,
    pub position: f64,
    pub duration: f64,
    pub message: String,
}
impl Default for GameStatus {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            position: 0.,
            duration: 0.,
            message: "演奏器未启动。".into(),
        }
    }
}
pub trait GameBackend {
    fn active(&self) -> bool {
        false
    }
    fn start(&mut self, script: &Path, start_seconds: f64) -> Result<()>;
    fn play(&mut self, script: &Path, start_seconds: f64) -> Result<()>;
    fn stop_playback(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn reap(&mut self) -> Result<()>;
    fn status(&mut self) -> GameStatus;
}
#[derive(Default)]
pub struct SilentGame;
impl GameBackend for SilentGame {
    fn start(&mut self, _: &Path, _: f64) -> Result<()> {
        bail!("隔离模式禁止启动按键演奏器")
    }
    fn play(&mut self, _: &Path, _: f64) -> Result<()> {
        bail!("隔离模式禁止发送游戏按键")
    }
    fn stop_playback(&mut self) -> Result<()> {
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        Ok(())
    }
    fn reap(&mut self) -> Result<()> {
        Ok(())
    }
    fn status(&mut self) -> GameStatus {
        GameStatus::default()
    }
}
// Command files are polled by another process. Retry only transient Windows
// access/sharing conflicts, with a short bound so UI callers cannot wait forever.
fn write_command_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let started = Instant::now();
    loop {
        match atomic_write(path, bytes) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let transient = cfg!(windows)
                    && error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|e| matches!(e.raw_os_error(), Some(5 | 32 | 33)));
                if !transient || started.elapsed() >= std::time::Duration::from_millis(100) {
                    return Err(error);
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

pub struct ScriptPlayer {
    control_dir: PathBuf,
    process: Option<Child>,
    instance: Option<PathBuf>,
    signature: Option<(PathBuf, String)>,
    command_id: u64,
    start_ms: u64,
    stopping: bool,
    pending: Option<(PathBuf, f64)>,
    last: GameStatus,
}
// Struct serialization also preserves compatibility with exported protocol-1
// scripts whose parser requires id before action. New parsers accept any order.
#[derive(Serialize)]
struct ScriptCommand<'a> {
    id: u64,
    action: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_ms: Option<u64>,
}
impl ScriptPlayer {
    pub fn new(control_dir: PathBuf) -> Self {
        Self {
            control_dir,
            process: None,
            instance: None,
            signature: None,
            command_id: 0,
            start_ms: 0,
            stopping: false,
            pending: None,
            last: GameStatus::default(),
        }
    }
    fn alive(&mut self) -> bool {
        self.process
            .as_mut()
            .is_some_and(|p| p.try_wait().map(|s| s.is_none()).unwrap_or(true))
    }
    fn signature(script: &Path) -> Result<(PathBuf, String)> {
        let path = crate::paths::absolute(script)?;
        Ok((
            path.clone(),
            format!(
                "{:x}",
                Sha256::digest(fs::read(&path).context("找不到演奏脚本，请先生成曲谱")?)
            ),
        ))
    }
    fn start_offset(script: &Path, seconds: f64) -> Result<u64> {
        ensure!(
            seconds.is_finite() && (0.0..=86400.).contains(&seconds),
            "演奏起点超出有效范围"
        );
        let ms = (seconds * 1000.).round() as u64;
        if ms > 0 {
            ensure!(
                String::from_utf8_lossy(&fs::read(script)?)
                    .contains("; Harmonica Studio start offset: 1"),
                "这份演奏脚本不支持心动片段起点，请重新导出"
            );
        }
        Ok(ms)
    }
    fn cleanup(&mut self) {
        if let Some(dir) = self.instance.take() {
            for name in ["exit.stop", "command.json", "status.json"] {
                let _ = fs::remove_file(dir.join(name));
            }
            let _ = fs::remove_dir(dir);
        }
        self.signature = None;
    }
    fn command(&mut self, action: &str, start: Option<u64>) -> Result<()> {
        if !self.alive() || self.stopping {
            return Ok(());
        }
        let Some(dir) = &self.instance else {
            return Ok(());
        };
        ensure!(["play", "stop"].contains(&action), "无效演奏命令");
        ensure!(
            start.is_none_or(|ms| ms <= 86_400_000),
            "演奏起点超出有效范围"
        );
        ensure!(
            self.command_id < 9_007_199_254_740_991,
            "演奏命令编号已耗尽，请重新启动演奏器"
        );
        let id = self.command_id + 1;
        let payload = ScriptCommand {
            id,
            action,
            start_ms: start,
        };
        write_command_file(&dir.join("command.json"), &serde_json::to_vec(&payload)?)?;
        self.command_id = id;
        if let Some(ms) = start {
            self.start_ms = ms;
        }
        self.last.state = "ready".into();
        self.last.message = if action == "play" {
            "正在请求开始演奏。"
        } else {
            "已请求停止演奏。"
        }
        .into();
        Ok(())
    }
}
impl GameBackend for ScriptPlayer {
    fn active(&self) -> bool {
        self.process.is_some() || self.pending.is_some()
    }
    fn start(&mut self, script: &Path, start_seconds: f64) -> Result<()> {
        ensure!(!self.alive(), "已有演奏器在运行，请先停止它。");
        let signature = Self::signature(script)?;
        let ms = Self::start_offset(&signature.0, start_seconds)?;
        let exe = resource_root().join("third_party/AutoHotkey/AutoHotkey64.exe");
        ensure!(exe.is_file(), "缺少 AutoHotkey 运行文件");
        self.cleanup();
        self.pending = None;
        self.stopping = false;
        self.command_id = 0;
        self.start_ms = ms;
        fs::create_dir_all(&self.control_dir)?;
        let dir = self.control_dir.join(unique_id());
        fs::create_dir(&dir)?;
        self.instance = Some(dir.clone());
        let mut cmd = Command::new(exe);
        cmd.arg("/ErrorStdOut")
            .arg(&signature.0)
            .arg(dir.join("exit.stop"))
            .arg(dir.join("command.json"))
            .arg(dir.join("status.json"))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if ms > 0 {
            cmd.arg(ms.to_string());
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        match cmd.spawn() {
            Ok(child) => self.process = Some(child),
            Err(e) => {
                self.cleanup();
                return Err(e).context("无法启动演奏器");
            }
        }
        self.signature = Some(signature);
        self.last = GameStatus {
            state: "ready".into(),
            message: "演奏器已就绪；切到口琴界面按 F6 或从手机开始。".into(),
            ..GameStatus::default()
        };
        Ok(())
    }
    fn play(&mut self, script: &Path, start_seconds: f64) -> Result<()> {
        let sig = Self::signature(script)?;
        let ms = Self::start_offset(script, start_seconds)?;
        ensure!(
            String::from_utf8_lossy(&fs::read(script)?)
                .contains("; Harmonica Studio remote protocol: 1"),
            "旧版演奏脚本不支持遥控，请重新导出"
        );
        if self.alive() && (self.stopping || self.signature.as_ref() != Some(&sig)) {
            self.stop()?;
            self.pending = Some((sig.0, start_seconds));
            return Ok(());
        }
        if !self.alive() {
            self.start(script, start_seconds)?;
        }
        self.command(
            "play",
            if ms > 0 || self.start_ms > 0 {
                Some(ms)
            } else {
                None
            },
        )
    }
    fn stop_playback(&mut self) -> Result<()> {
        self.pending = None;
        self.command("stop", None)
    }
    fn stop(&mut self) -> Result<()> {
        self.pending = None;
        if self.alive() {
            if let Some(dir) = &self.instance {
                fs::write(dir.join("exit.stop"), b"")?;
                self.stopping = true;
            }
        }
        self.last.state = "idle".into();
        self.last.message = "已请求停止并退出演奏器。".into();
        Ok(())
    }
    fn reap(&mut self) -> Result<()> {
        if self.process.is_some() && !self.alive() {
            let failed = self
                .process
                .as_mut()
                .and_then(|p| p.try_wait().ok().flatten())
                .is_some_and(|s| !s.success());
            let pending = self.pending.take();
            self.process = None;
            self.cleanup();
            self.stopping = false;
            self.last.state = "idle".into();
            self.last.message = if failed {
                "演奏器异常退出，请重新生成曲谱后再试。"
            } else {
                "演奏器已退出。"
            }
            .into();
            if let Some((path, seconds)) = pending {
                self.play(&path, seconds)?;
            }
        }
        Ok(())
    }
    fn status(&mut self) -> GameStatus {
        if !self.alive() || self.stopping {
            let mut status = self.last.clone();
            status.state = if self.pending.is_some() {
                "ready"
            } else {
                "idle"
            }
            .into();
            return status;
        }
        if let Some(dir) = &self.instance {
            let path = dir.join("status.json");
            if fs::metadata(&path).is_ok_and(|m| m.len() <= 16384) {
                if let Ok(text) = fs::read_to_string(path) {
                    if let Ok(v) =
                        serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}'))
                    {
                        let state = v["state"].as_str().unwrap_or("");
                        let pos = v["position"].as_f64().unwrap_or(-1.);
                        let dur = v["duration"].as_f64().unwrap_or(-1.);
                        if v["request_id"].as_u64() == Some(self.command_id)
                            && ["idle", "ready", "countdown", "playing"].contains(&state)
                            && pos.is_finite()
                            && dur.is_finite()
                            && pos >= 0.
                            && dur >= 0.
                        {
                            self.last = GameStatus {
                                state: state.into(),
                                position: pos.min(dur),
                                duration: dur,
                                message: v["message"]
                                    .as_str()
                                    .unwrap_or("")
                                    .chars()
                                    .take(500)
                                    .collect(),
                            };
                        }
                    }
                }
            }
        }
        self.last.clone()
    }
}
impl Drop for ScriptPlayer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    fn wav(path: &Path) {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let (events, _) = crate::schedule::build_events(&[crate::models::Note {
            pitch: 60,
            start: 0.,
            end: 0.1,
            velocity: 90,
        }])
        .unwrap();
        crate::preview::render_wav(&events, path, &cancel, 22050).unwrap();
    }
    fn player(log: Arc<Mutex<Vec<String>>>) -> AudioPlayer {
        AudioPlayer::with_sender(Box::new(move |command| {
            log.lock().unwrap().push(command.into());
            Ok(if command.ends_with(" length") {
                "100"
            } else if command.ends_with(" position") {
                "0"
            } else if command.ends_with(" mode") {
                "stopped"
            } else {
                ""
            }
            .into())
        }))
    }
    #[test]
    fn seek_at_end_never_restarts_audio_and_nonfinite_position_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("中文试听.wav");
        wav(&path);
        let calls = Arc::new(Mutex::new(vec![]));
        let mut audio = player(calls.clone());
        audio.load(&path).unwrap();
        assert_eq!(audio.duration(), 0.1);
        audio.seek(8., true).unwrap();
        let cmds = calls.lock().unwrap().clone();
        assert!(cmds.iter().any(|c| c.contains(" to end wait")));
        assert!(!cmds.iter().any(|c| c.starts_with("play ")));
        assert!(audio.seek(f64::NAN, false).is_err());
        audio.close().unwrap();
    }
    #[test]
    fn legacy_path_failure_stages_private_copy_and_removes_only_that_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("音乐.wav");
        wav(&path);
        let opens = Arc::new(Mutex::new(vec![]));
        let observed = opens.clone();
        let mut audio = AudioPlayer::with_sender(Box::new(move |command| {
            if command.starts_with("open ") {
                let mut log = observed.lock().unwrap();
                log.push(command.to_string());
                if log.len() == 1 {
                    return Err(AudioDeviceError {
                        code: 304,
                        detail: "long path".into(),
                    }
                    .into());
                }
            }
            Ok(if command.ends_with(" length") {
                "100"
            } else {
                ""
            }
            .into())
        }));
        audio.load(&path).unwrap();
        let stage = audio.staged.clone().unwrap();
        assert!(stage.join("audio.wav").is_file());
        assert_eq!(opens.lock().unwrap().len(), 2);
        audio.close().unwrap();
        assert!(!stage.exists());
        assert!(path.is_file());
    }
    #[test]
    fn device_failure_does_not_masquerade_as_path_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sound.wav");
        wav(&path);
        let opens = Arc::new(Mutex::new(0));
        let observed = opens.clone();
        let mut audio = AudioPlayer::with_sender(Box::new(move |_| {
            *observed.lock().unwrap() += 1;
            Err(AudioDeviceError {
                code: 263,
                detail: "device".into(),
            }
            .into())
        }));
        let error = audio.load(&path).unwrap_err();
        assert_eq!(error.downcast_ref::<AudioDeviceError>().unwrap().code, 263);
        assert_eq!(*opens.lock().unwrap(), 1);
        assert!(audio.staged.is_none());
    }
    #[test]
    fn script_content_change_invalidates_signature_even_at_same_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.ahk");
        fs::write(&path, b"first").unwrap();
        let a = ScriptPlayer::signature(&path).unwrap();
        fs::write(&path, b"other").unwrap();
        let b = ScriptPlayer::signature(&path).unwrap();
        assert_eq!(a.0, b.0);
        assert_ne!(a.1, b.1);
    }
    #[test]
    fn old_scripts_cannot_accept_nonzero_marker_and_offsets_must_be_finite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.ahk");
        fs::write(&path, b"old template").unwrap();
        assert_eq!(ScriptPlayer::start_offset(&path, 0.).unwrap(), 0);
        assert!(ScriptPlayer::start_offset(&path, 1.).is_err());
        assert!(ScriptPlayer::start_offset(&path, f64::INFINITY).is_err());
        assert!(ScriptPlayer::start_offset(&path, -1.).is_err());
        fs::write(&path, b"; Harmonica Studio start offset: 1").unwrap();
        assert_eq!(ScriptPlayer::start_offset(&path, 1.001).unwrap(), 1001);
    }
}
