use super::*;

impl AppController {
    pub fn default_options(&self) -> Result<Options> {
        let (track, channel) = self.state.selected_part.context("请先选择一个声部。")?;
        Ok(Options {
            track: Some(track),
            channel: Some(channel),
            skip_long_rests: self.preferences.skip_long_rests,
            ..self
                .source_options
                .clone()
                .unwrap_or_else(|| self.saved_options.clone())
        })
    }
    pub fn conversion_options(&self) -> Options {
        Options {
            skip_long_rests: self.preferences.skip_long_rests,
            ..self.saved_options.clone()
        }
    }
    pub fn convert(&mut self, options: Options) -> Result<()> {
        self.idle()?;
        options.validate()?;
        ensure!(self.state.source.is_some(), "请先打开 MIDI 文件。");
        crate::preferences::save_options(&self.home.join("settings.json"), &options)?;
        self.saved_options = options.clone();
        self.preserve_and_do(Action::Convert(options), "convert", FollowUp::None)
    }
    pub(super) fn execute(&mut self, action: Action, follow: FollowUp) -> Result<()> {
        match action {
            Action::Load(path, options, prepare) => {
                let _timing = crate::performance::Span::new("selection.dispatch");
                self.invalidate()?;
                self.state.set_project(None, true, true);
                self.state.source = Some(path.clone());
                self.state.project_path = None;
                self.state.auto_project_path = None;
                self.state.parts.clear();
                self.state.names.clear();
                self.state.keys.clear();
                self.state.recommendations.clear();
                self.source_options = options.clone();
                self.state.selected_part = options.and_then(|o| o.track.zip(o.channel));
                let root = self.home.join("song-projects");
                self.jobs.start(
                    JobKind::Load,
                    self.state.revision,
                    self.state.document_id,
                    follow,
                    move |cancel| {
                        let cache_timing =
                            crate::performance::Span::new("selection.restore_project");
                        let (auto, project) = find_song_project(&root, &path, cancel)?;
                        drop(cache_timing);
                        let loaded = service::load_ranked_midi(&path, cancel)?;
                        Ok(WorkResult::Load(loaded, auto, project, prepare))
                    },
                )?;
                self.status("正在读取曲谱…");
                self.events.push(ControllerEvent::Document);
            }
            Action::Open(path) => {
                self.jobs.start(
                    JobKind::Load,
                    self.state.revision,
                    self.state.document_id,
                    follow,
                    move |cancel| {
                        crate::library::check_cancel(cancel)?;
                        let project = load_project(&path)?;
                        crate::library::check_cancel(cancel)?;
                        Ok(WorkResult::Open(project, path))
                    },
                )?;
                self.status("正在打开工程…");
            }
            Action::Convert(options) => {
                self.invalidate()?;
                let source = self.state.source.clone().context("请先打开 MIDI")?;
                let root = self.home.join("exports");
                self.jobs.start(
                    JobKind::Convert,
                    self.state.revision,
                    self.state.document_id,
                    follow,
                    move |cancel| {
                        Ok(WorkResult::Export(service::convert(
                            &source, &root, &options, cancel,
                        )?))
                    },
                )?;
                self.status("正在提取旋律并制作试听…");
            }
            Action::New(project) => {
                self.invalidate()?;
                self.state.set_project(Some(project), false, true);
                self.state.source = None;
                self.state.project_path = None;
                self.state.auto_project_path = None;
                self.state.parts.clear();
                self.state.names.clear();
                self.state.keys.clear();
                self.state.recommendations.clear();
                self.events.push(ControllerEvent::Document);
                self.status("新曲谱已建立。");
            }
            Action::Close => self.finish_close()?,
        }
        Ok(())
    }
    pub fn export(&mut self) -> Result<()> {
        self.begin_export(FollowUp::None)
    }
    pub(super) fn begin_export(&mut self, follow: FollowUp) -> Result<()> {
        self.idle()?;
        let mut project = validate_project(self.state.project.as_ref().context("请先打开曲谱")?)?;
        ensure!(!project.notes.is_empty(), "请先在曲谱上添加音符。");
        Self::set_rest_option(&mut project, self.preferences.skip_long_rests);
        let seek = self.state.logical_seek;
        let manual = self.manual_seek;
        self.stop()?;
        self.state.logical_seek = seek;
        self.manual_seek = manual;
        self.player.stop()?;
        let root = self.home.join("exports");
        self.jobs.start(
            JobKind::Export,
            self.state.revision,
            self.state.document_id,
            follow,
            move |cancel| {
                Ok(WorkResult::Export(service::export_project(
                    &project, &root, cancel,
                )?))
            },
        )?;
        self.status("正在生成试听、MIDI 和演奏脚本…");
        Ok(())
    }
    pub(super) fn invalidate(&mut self) -> Result<()> {
        let _timing = crate::performance::Span::new("selection.stop_previous");
        self.state.result = None;
        self.state.export_revision = None;
        self.state.preview_duration = 0.;
        self.state.transport = Transport::Ready;
        self.state.position_label = "未播放".into();
        self.state.logical_seek = 0.;
        self.state.position = 0.;
        self.state.show_cursor = false;
        self.manual_seek = false;
        self.playback_clock.reset(0., false);
        let audio = self.audio.close();
        let player = self.player.stop();
        audio.and(player)
    }
    pub fn cancel(&mut self) -> Result<()> {
        self.jobs.cancel();
        self.status("正在取消任务…");
        Ok(())
    }
    pub(super) fn install_export(&mut self, result: ExportResult, replace: bool) -> Result<()> {
        let _timing = crate::performance::Span::new("selection.install_result");
        if replace {
            self.state
                .set_project(Some(result.project.clone()), false, true);
            self.state.project_path = None;
            self.state.logical_seek = 0.;
            if let Some(source) = &result.project.source {
                if let (Some(path), Some(digest)) =
                    (source["path"].as_str(), source["sha256"].as_str())
                {
                    let auto = song_project_path(
                        &self.home.join("song-projects"),
                        Path::new(path),
                        digest,
                    );
                    self.state.auto_project_path = Some(auto.clone());
                    self.state.project_path = Some(auto);
                }
            }
            self.events.push(ControllerEvent::Document);
        } else {
            self.state.project = Some(result.project.clone());
        }
        self.state.export_revision = Some(self.state.revision);
        self.state.result = Some(result);
        self.events.push(ControllerEvent::Result);
        self.autosave()?;
        self.load_preview()?;
        self.status("曲谱已准备好，可以试听或演奏。");
        Ok(())
    }
    pub(super) fn poll_work(&mut self) -> Result<()> {
        let Some(done) = self.jobs.take_completed() else {
            return Ok(());
        };
        let _context = crate::performance::job_context(done.job.timing_id);
        let _timing = crate::performance::Span::new("job.apply_result");
        if done.job.cancel.load(Ordering::Relaxed) {
            self.status("已取消；可以继续编辑或重新导出。");
            return Ok(());
        }
        if (done.job.document, done.job.revision) != (self.state.document_id, self.state.revision) {
            self.status("工程已变化，已忽略旧任务结果。");
            return Ok(());
        }
        let follow = done.job.follow_up;
        match done.result? {
            WorkResult::Open(mut project, path) => {
                Self::set_rest_option(&mut project, self.preferences.skip_long_rests);
                self.invalidate()?;
                self.state.set_project(Some(project), true, true);
                self.state.project_path = Some(path.clone());
                self.state.auto_project_path = Some(path);
                self.state.source = None;
                self.state.parts.clear();
                self.state.names.clear();
                self.state.keys.clear();
                self.state.recommendations.clear();
                self.events.push(ControllerEvent::Document);
                self.status("工程已打开，可以继续编辑。");
            }
            WorkResult::Load(loaded, auto, project, prepare) => {
                self.state.parts = loaded.parts;
                self.state.names = loaded.names;
                self.state.keys = loaded.keys;
                self.state.recommendations = loaded.recommendations;
                if !self
                    .state
                    .selected_part
                    .is_some_and(|key| self.state.keys.contains(&key))
                {
                    self.state.selected_part = self.state.keys.first().copied();
                }
                self.state.auto_project_path = Some(auto.clone());
                self.events.push(ControllerEvent::Parts);
                if let Some(mut project) = project {
                    Self::set_rest_option(&mut project, self.preferences.skip_long_rests);
                    self.state.set_project(Some(project), true, true);
                    self.state.project_path = Some(auto);
                    self.events.push(ControllerEvent::Document);
                    self.status("已恢复自动保存的曲谱和标记。");
                    if prepare {
                        self.begin_export(follow)?;
                    }
                } else if prepare {
                    let options = self.default_options()?;
                    self.execute(Action::Convert(options), follow)?;
                } else {
                    self.status("已推荐主旋律，请选择声部并生成曲谱。");
                }
            }
            WorkResult::Export(result) => {
                self.install_export(result, done.job.kind == JobKind::Convert)?;
                match follow {
                    FollowUp::Play => self.listen()?,
                    FollowUp::Game => self.game_play(false)?,
                    FollowUp::Arm => self.game_play(true)?,
                    FollowUp::None => {}
                }
            }
        }
        Ok(())
    }
}
