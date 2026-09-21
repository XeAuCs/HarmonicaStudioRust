use super::*;

impl AppController {
    pub(super) fn autosave_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.home.join("autosave.hstudio")];
        if let Some(p) = &self.state.auto_project_path {
            if !paths.contains(p) {
                paths.push(p.clone());
            }
        }
        paths
    }
    pub(super) fn submit_save(&mut self, paths: Vec<PathBuf>, kind: SaveKind) -> Result<u64> {
        self.saves.submit(
            self.state.document_id,
            self.state.revision,
            self.state.project.clone(),
            paths,
            kind,
        )
    }
    pub fn autosave(&mut self) -> Result<()> {
        if !self.state.closed && self.state.transition.is_none() && self.state.project.is_some() {
            self.submit_save(self.autosave_paths(), SaveKind::Auto)?;
        }
        Ok(())
    }
    pub(super) fn preserve_and_do(
        &mut self,
        action: Action,
        transition: &str,
        follow: FollowUp,
    ) -> Result<()> {
        let request = if self.state.project_dirty() {
            let mut paths = vec![
                self.home
                    .join("recovery")
                    .join(format!("{}.hstudio", paths::unique_id())),
            ];
            paths.extend(self.autosave_paths());
            Some(self.submit_save(paths, SaveKind::Preserve)?)
        } else if self.saving() {
            Some(self.submit_save(vec![], SaveKind::Preserve)?)
        } else {
            None
        };
        if let Some(request) = request {
            self.pending = Some(PendingAction {
                request,
                action,
                follow,
            });
            self.state.transition = Some(transition.into());
            self.status("正在保存当前工程…");
            Ok(())
        } else {
            self.execute(action, follow)
        }
    }
    pub fn open_path(&mut self, path: &Path) -> Result<()> {
        if path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("hstudio"))
        {
            self.open_project(path)
        } else {
            self.load_file(path, None, true, false)
        }
    }
    pub fn open_project(&mut self, path: &Path) -> Result<()> {
        self.idle()?;
        ensure!(path.is_file(), "工程文件不存在。");
        self.preserve_and_do(Action::Open(paths::absolute(path)?), "open", FollowUp::None)
    }
    pub fn load_file(
        &mut self,
        path: &Path,
        options: Option<Options>,
        prepare: bool,
        autoplay: bool,
    ) -> Result<()> {
        self.idle()?;
        ensure!(path.is_file(), "MIDI 文件不存在。");
        self.preserve_and_do(
            Action::Load(paths::absolute(path)?, options, prepare),
            "load",
            if autoplay {
                FollowUp::Play
            } else {
                FollowUp::None
            },
        )
    }
    pub fn set_project(&mut self, project: Project) -> Result<()> {
        self.idle()?;
        let project = validate_project(&project)?;
        self.preserve_and_do(Action::New(project), "new", FollowUp::None)
    }
    pub fn save_project(&mut self, path: &Path) -> Result<()> {
        self.idle()?;
        ensure!(self.state.project.is_some(), "请先打开或创建工程。");
        let mut target = paths::absolute(path)?;
        if target
            .extension()
            .and_then(|s| s.to_str())
            .is_none_or(|s| !s.eq_ignore_ascii_case("hstudio"))
        {
            target.set_extension("hstudio");
        }
        self.submit_save(vec![target], SaveKind::Manual)?;
        self.submit_save(vec![self.home.join("autosave.hstudio")], SaveKind::Auto)?;
        self.status("正在保存工程…");
        Ok(())
    }
    pub fn set_notes(&mut self, notes: Vec<Note>) -> Result<()> {
        self.idle()?;
        ensure!(
            self.state.transport != Transport::Playing,
            "请先暂停试听再编辑。"
        );
        let mut project = self.state.project.clone().context("请先打开或创建工程")?;
        project.notes = notes;
        if project
            .highlight
            .is_some_and(|h| !project.notes.iter().any(|n| n.end > h))
        {
            project.highlight = None;
        }
        let project = validate_project(&project)?;
        self.state.set_project(Some(project), false, false);
        self.invalidate()?;
        self.status("修改已记录。点试听即可听到修改后的旋律。");
        self.autosave()?;
        Ok(())
    }
    pub fn set_highlight(&mut self, seconds: Option<f64>) -> Result<()> {
        self.idle()?;
        ensure!(
            self.state.transport != Transport::Playing,
            "请先暂停试听再添加标记。"
        );
        let mut project = self.state.project.clone().context("请先打开曲谱")?;
        project.highlight = seconds;
        let project = validate_project(&project)?;
        let position = self.state.logical_seek;
        self.state.set_project(Some(project), false, false);
        self.invalidate()?;
        self.seek(position)?;
        self.manual_seek = false;
        self.status(if seconds.is_some() {
            "心动片段标记已设置。"
        } else {
            "心动片段标记已清除。"
        });
        self.autosave()?;
        Ok(())
    }
    pub fn update_preferences(&mut self, mut preferences: Preferences) -> Result<()> {
        ensure!(
            !self.state.closed && self.state.transition.is_none(),
            "正在保存或关闭工程，请稍候。"
        );
        let timing = preferences.skip_long_rests != self.preferences.skip_long_rests;
        ensure!(!timing || !self.busy(), "曲谱正在准备，请稍候。");
        preferences.library_folder = paths::library_setting(&preferences.library_folder);
        preferences.validate()?;
        save_preferences(&self.home.join("preferences.json"), &preferences)?;
        let library = preferences.library_folder != self.preferences.library_folder;
        self.preferences = preferences;
        if library {
            self.refresh_library()?;
        }
        if timing && self.state.project.is_some() {
            let position = self.state.logical_seek;
            let mut project = self.state.project.clone().unwrap();
            Self::set_rest_option(&mut project, self.preferences.skip_long_rests);
            self.state.set_project(Some(project), false, false);
            self.invalidate()?;
            self.seek(position)?;
            self.manual_seek = false;
            self.autosave()?;
        }
        self.events.push(ControllerEvent::Changed);
        Ok(())
    }
    pub(super) fn set_rest_option(project: &mut Project, skip: bool) {
        let options = project.options.get_or_insert_with(|| json!({}));
        if !options.is_object() {
            *options = json!({});
        }
        options["skip_long_rests"] = json!(skip);
    }
    pub(super) fn poll_saves(&mut self) {
        while let Some(done) = self.saves.take_completed() {
            let r = done.request;
            if let Some(e) = done.error {
                self.pending = None;
                self.state.transition = None;
                self.state.save_error = format!("{e:#}");
                self.report_error(e);
            } else {
                if r.document == self.state.document_id {
                    self.state.save_error.clear();
                    if r.kind == SaveKind::Manual {
                        self.state.project_path = r.paths.first().cloned();
                        self.state.saved_revision = r.revision;
                        self.status("工程已保存。");
                    }
                    if r.paths.contains(&self.home.join("autosave.hstudio")) {
                        self.state.autosave_revision = Some(r.revision);
                    }
                    if r.kind != SaveKind::Manual
                        && self.state.auto_project_path.as_ref().is_some_and(|p| {
                            r.paths.contains(p) && self.state.project_path.as_ref() == Some(p)
                        })
                    {
                        self.state.saved_revision = r.revision;
                    }
                }
                if self.pending.as_ref().is_some_and(|p| p.request == r.number) {
                    let pending = self.pending.take().unwrap();
                    self.state.transition = None;
                    if (r.document, r.revision) != (self.state.document_id, self.state.revision) {
                        self.report_error(anyhow::anyhow!("工程已变化，请重新执行操作。"));
                    } else if let Err(e) = self.execute(pending.action, pending.follow) {
                        self.report_error(e);
                    }
                }
            }
            self.events.push(ControllerEvent::Changed);
        }
    }
}
