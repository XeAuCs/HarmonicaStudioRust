//! One application owner for desktop, CLI diagnostics and the phone adapter.
use crate::transport::PlaybackClock;
use crate::{
    jobs::{FollowUp, JobKind, JobRunner, SaveKind, SaveQueue, SaveWriter},
    library::{LibraryEntry, sample_entries, song_id},
    midi::{Parts, TrackNames},
    models::{Note, Options},
    paths,
    playback::{AudioBackend, AudioPlayer, FakeAudio, GameBackend, ScriptPlayer, SilentGame},
    preferences::{Preferences, load_preferences, save_preferences},
    project::{Project, load_project, validate_project},
    remote::RemoteServer,
    service::{self, ExportResult, LoadedParts},
    song_projects::{find_song_project, song_project_path},
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
#[path = "controller/state.rs"]
mod state;
pub use state::{AppState, Capabilities, ControllerEvent, Transport};
#[path = "controller/document.rs"]
mod document;
#[path = "controller/export.rs"]
mod export;
#[cfg(test)]
#[path = "controller/lifecycle_tests.rs"]
mod lifecycle_tests;
#[path = "controller/playback.rs"]
mod playback;
#[path = "controller/remote.rs"]
mod remote;
#[cfg(test)]
#[path = "controller/state_guard_tests.rs"]
mod state_guard_tests;
enum WorkResult {
    Load(LoadedParts, PathBuf, Option<Project>, bool),
    Open(Project, PathBuf),
    Export(ExportResult),
}
enum Action {
    Load(PathBuf, Option<Options>, bool),
    Open(PathBuf),
    Convert(Options),
    New(Project),
    Close,
}
struct PendingAction {
    request: u64,
    action: Action,
    follow: FollowUp,
    queued_at: Instant,
}
pub struct AppController {
    pub home: PathBuf,
    state: AppState,
    preferences: Preferences,
    library: Vec<LibraryEntry>,
    jobs: JobRunner<WorkResult>,
    library_jobs: JobRunner<(PathBuf, Vec<LibraryEntry>)>,
    saves: SaveQueue,
    pending: Option<PendingAction>,
    events: Vec<ControllerEvent>,
    audio: Box<dyn AudioBackend>,
    player: Box<dyn GameBackend>,
    remote: Option<RemoteServer>,
    manual_seek: bool,
    library_generation: u64,
    library_pending: Option<(PathBuf, u64)>,
    source_options: Option<Options>,
    saved_options: Options,
    playback_clock: PlaybackClock,
    last_audio_poll: Instant,
    last_remote_publish: Instant,
    last_poll: Instant,
    score_cache: Value,
    score_key: Option<(u64, u64, Option<u64>, bool)>,
    closing_resources: bool,
}
impl AppController {
    /// Read-only business state; mutations must go through controller operations.
    /// ```compile_fail
    /// fn bypass(c: &mut harmonica_studio::controller::AppController) {
    ///     c.state().saved_revision = 0;
    /// }
    /// ```
    pub fn state(&self) -> &AppState {
        &self.state
    }
    /// ```compile_fail
    /// fn bypass(c: &mut harmonica_studio::controller::AppController) {
    ///     c.preferences().compact = true;
    /// }
    /// ```
    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }
    /// ```compile_fail
    /// fn bypass(c: &mut harmonica_studio::controller::AppController) {
    ///     c.library()[0].title.clear();
    /// }
    /// ```
    pub fn library(&self) -> &[LibraryEntry] {
        &self.library
    }
    pub fn new(home: Option<PathBuf>) -> Result<Self> {
        let home = home.unwrap_or_else(paths::data_root);
        let control = home.join("control");
        Self::with_backends(
            home,
            Box::<AudioPlayer>::default(),
            Box::new(ScriptPlayer::new(control)),
        )
    }
    pub fn silent(home: PathBuf) -> Result<Self> {
        Self::with_backends(
            home,
            Box::<FakeAudio>::default(),
            Box::<SilentGame>::default(),
        )
    }
    pub fn with_backends(
        home: PathBuf,
        audio: Box<dyn AudioBackend>,
        player: Box<dyn GameBackend>,
    ) -> Result<Self> {
        std::fs::create_dir_all(&home)?;
        let preferences = load_preferences(&home.join("preferences.json"));
        let saved_options = crate::preferences::load_options(&home.join("settings.json"));
        Ok(Self {
            home,
            state: AppState::default(),
            preferences,
            library: vec![],
            jobs: JobRunner::default(),
            library_jobs: JobRunner::default(),
            saves: SaveQueue::default(),
            pending: None,
            events: vec![],
            audio,
            player,
            remote: None,
            manual_seek: false,
            library_generation: 0,
            library_pending: None,
            source_options: None,
            saved_options,
            playback_clock: PlaybackClock::default(),
            last_audio_poll: Instant::now(),
            last_remote_publish: Instant::now(),
            last_poll: Instant::now(),
            score_cache: Value::Null,
            score_key: None,
            closing_resources: false,
        })
    }
    /// Injectable persistence transport for deterministic failure/order regression tests.
    pub fn set_save_writer(&mut self, writer: SaveWriter) -> Result<()> {
        ensure!(!self.saving(), "请等待保存完成");
        self.saves = SaveQueue::with_writer(writer);
        Ok(())
    }
    pub fn busy(&self) -> bool {
        self.jobs.busy()
    }
    pub fn saving(&self) -> bool {
        self.saves.busy()
    }
    pub fn library_refreshing(&self) -> bool {
        self.library_jobs.busy() || self.library_pending.is_some()
    }
    pub fn capabilities(&self) -> Capabilities {
        let available = !self.busy() && !self.state.closed && self.state.transition.is_none();
        Capabilities {
            can_open: available,
            can_save: available && self.state.project.is_some(),
            can_export: available && self.state.has_notes(),
            can_play: available && self.state.has_notes(),
            can_edit: available
                && self.state.project.is_some()
                && self.state.transport != Transport::Playing,
            can_convert: available && !self.state.parts.is_empty(),
            current_export: available && self.state.result.is_some() && !self.state.export_dirty(),
        }
    }
    fn idle(&self) -> Result<()> {
        ensure!(!self.state.closed, "应用已关闭。");
        ensure!(
            self.state.transition.is_none(),
            "正在保存当前工程，请稍候。"
        );
        ensure!(!self.busy(), "曲谱正在准备，请稍候。");
        Ok(())
    }
    pub fn status(&mut self, text: impl Into<String>) {
        self.state.message = text.into();
        self.events.push(ControllerEvent::Changed);
    }
    pub fn report_error(&mut self, error: anyhow::Error) {
        let text = format!("{error:#}");
        self.status(format!("未完成：{text}"));
        self.events.push(ControllerEvent::Error(text));
    }
    pub fn library_root(&self) -> PathBuf {
        paths::library_path(&self.preferences.library_folder)
    }
    pub fn refresh_library(&mut self) -> Result<()> {
        ensure!(!self.state.closed, "应用已关闭。");
        self.library_generation += 1;
        let root = self.library_root();
        self.state.library_error.clear();
        if self.library_jobs.busy() {
            self.library_jobs.cancel();
            self.library_pending = Some((root, self.library_generation));
        } else {
            self.start_library(root, self.library_generation)?;
        }
        Ok(())
    }
    fn start_library(&mut self, root: PathBuf, generation: u64) -> Result<()> {
        self.library_jobs.start(
            JobKind::Library,
            generation,
            0,
            FollowUp::None,
            move |cancel| {
                let rows = sample_entries(&root, cancel)?;
                Ok((root, rows))
            },
        )
    }
    fn poll_library(&mut self) {
        if let Some(done) = self.library_jobs.take_completed() {
            if done.job.revision == self.library_generation
                && !done.job.cancel.load(Ordering::Relaxed)
            {
                match done.result {
                    Ok((root, entries)) if root == self.library_root() => {
                        self.library = entries;
                        self.state.library_revision = done.job.revision;
                        self.state.library_root = Some(root);
                        self.state.library_error.clear();
                        self.events.push(ControllerEvent::Library);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        self.state.library_error = format!("{e:#}");
                        self.report_error(e);
                    }
                }
            }
            if let Some((root, generation)) = self.library_pending.take() {
                if let Err(e) = self.start_library(root, generation) {
                    self.report_error(e);
                }
            }
        }
    }
    pub fn poll(&mut self) -> Vec<ControllerEvent> {
        let poll_started = Instant::now();
        let gap = self.last_poll.elapsed();
        self.last_poll = Instant::now();
        if gap >= Duration::from_millis(500) {
            crate::performance::elapsed("controller.poll_gap", gap, 0);
        }
        if !self.state.closed {
            self.poll_saves();
            if self.closing_resources {
                self.poll_closing();
            }
            if !self.state.closed && !self.closing_resources {
                self.poll_library();
                if self.state.transition.is_none() {
                    if let Err(e) = self.poll_work() {
                        self.report_error(e);
                    }
                }
                if let Err(e) = self.player.reap() {
                    self.report_error(e);
                }
                if let Err(e) = self.update_playback() {
                    self.state.transport = Transport::Ready;
                    self.state.position_label = "未播放".into();
                    self.report_error(e);
                }
                let commands = self
                    .remote
                    .as_ref()
                    .map(|r| r.take_commands())
                    .unwrap_or_default();
                for c in commands {
                    if !c.expired.load(Ordering::Relaxed) {
                        crate::performance::elapsed(
                            "remote.command_queue",
                            c.queued_at.elapsed(),
                            0,
                        );
                        let _timing = crate::performance::Span::new("remote.command_apply");
                        let reply = crate::remote::handle_command(self, c.value);
                        let _ = c.reply.try_send(reply);
                    }
                }
                if self.remote.is_some()
                    && self.last_remote_publish.elapsed() >= Duration::from_millis(100)
                {
                    let publish_start = Instant::now();
                    let state = self.remote_state();
                    if let Some(remote) = &self.remote {
                        let _ = remote.publish(&state, &self.score_cache);
                    }
                    self.last_remote_publish = Instant::now();
                    if publish_start.elapsed() >= Duration::from_millis(50) {
                        crate::performance::elapsed(
                            "remote.publish_slow",
                            publish_start.elapsed(),
                            0,
                        );
                    }
                }
            }
        }
        if poll_started.elapsed() >= Duration::from_millis(50) {
            crate::performance::elapsed("controller.poll_slow", poll_started.elapsed(), 0);
        }
        std::mem::take(&mut self.events)
    }
    pub fn close(&mut self) -> Result<bool> {
        if self.state.closed {
            return Ok(true);
        }
        if self.closing_resources {
            return Ok(false);
        }
        if self.state.transition.as_deref() != Some("close") {
            if self.state.project.is_none() && !self.saving() {
                self.pending = None;
                self.state.transition = None;
                self.finish_close()?;
                return Ok(self.state.closed);
            }
            let request = self.submit_save(
                if self.state.project.is_some() {
                    self.autosave_paths()
                } else {
                    vec![]
                },
                SaveKind::Close,
            )?;
            self.pending = Some(PendingAction {
                request,
                action: Action::Close,
                follow: FollowUp::None,
                queued_at: Instant::now(),
            });
            self.state.transition = Some("close".into());
            self.status("正在保存工程并退出…");
        }
        Ok(self.state.closed)
    }
    fn finish_close(&mut self) -> Result<()> {
        self.jobs.cancel();
        self.library_jobs.cancel();
        self.library_pending = None;
        self.stop_remote();
        let audio = self.audio.close();
        let player = self.player.stop();
        audio.and(player)?;
        self.closing_resources = true;
        self.state.transition = Some("closing".into());
        self.poll_closing();
        Ok(())
    }
    fn poll_closing(&mut self) {
        self.jobs.take_completed();
        self.library_jobs.take_completed();
        if !self.jobs.busy() && !self.library_jobs.busy() && !self.saving() {
            self.closing_resources = false;
            self.state.closed = true;
            self.state.transition = None;
            self.events.push(ControllerEvent::CloseReady);
        }
    }
    pub fn wait_idle(&mut self, timeout: Duration) -> Result<()> {
        let until = Instant::now() + timeout;
        let mut error = None;
        loop {
            for event in self.poll() {
                if let ControllerEvent::Error(e) = event {
                    error.get_or_insert(e);
                }
            }
            if !self.busy() && !self.saving() && !self.library_refreshing() {
                break;
            }
            ensure!(Instant::now() < until, "等待后台任务超时");
            std::thread::sleep(Duration::from_millis(2));
        }
        if let Some(error) = error {
            bail!(error);
        }
        Ok(())
    }
}
impl Drop for AppController {
    fn drop(&mut self) {
        if !self.state.closed {
            if let Err(e) = self.close() {
                eprintln!("关闭前保存未能提交：{e:#}");
            }
            let until = Instant::now() + Duration::from_secs(30);
            while (self.saving()
                || self.busy()
                || self.library_refreshing()
                || self.closing_resources)
                && Instant::now() < until
            {
                self.poll();
                std::thread::sleep(Duration::from_millis(2));
            }
            if !self.state.closed {
                self.jobs.cancel();
                self.library_jobs.cancel();
                self.stop_remote();
                let _ = self.audio.close();
                let _ = self.player.stop();
            }
        }
    }
}
