use super::*;

impl Studio {
    pub(super) fn handle_message(&mut self, message: Message, context: &ComponentContext<Self>) {
        let mut should_redraw = true;
        match message {
            Message::Tick => {
                if let Some(c) = self.controller.as_mut() {
                    let events = c.poll();
                    if events
                        .iter()
                        .any(|e| matches!(e, crate::controller::ControllerEvent::Parts))
                        && c.state.project.is_none()
                    {
                        if let Ok(options) = c.default_options() {
                            self.options = options;
                            if let Some(&(track, channel)) = c.state.keys.first() {
                                self.options.track = Some(track);
                                self.options.channel = Some(channel);
                            }
                        }
                    }
                    should_redraw = !events.is_empty() || c.state.transport == Transport::Playing;
                    if !c.state.closed && self.library_watch.poll(&c.library_root()) {
                        if let Err(e) = c.refresh_library() {
                            self.error = e.to_string();
                        }
                    }
                }
                if self.controller.as_ref().is_some_and(|c| c.state.closed) {
                    self.close_allowed.set(true);
                    let _ = context.window().request_close();
                    return;
                }
                if self
                    .smoke_ms
                    .is_some_and(|ms| self.started.elapsed().as_millis() >= ms as u128)
                {
                    self.smoke_ms = None;
                    let _ = context.sender().send(Message::Close);
                }
                self.schedule(context);
            }
            Message::Open => {
                if !OpenFilePicker::new()
                    .title("打开 MIDI 或口琴工程")
                    .filter_extensions("MIDI 与口琴工程", ["mid", "midi", "kar", "rmi", "hstudio"])
                    .filter_all()
                    .request(context, |r| {
                        Message::PickedOpen(r.map_err(|e| e.to_string()))
                    })
                {
                    self.error = "请先关闭当前文件对话框。".into();
                }
            }
            Message::PickedOpen(Ok(Some(path))) => self.perform(|c| open_document(c, &path, None)),
            Message::PickedOpen(Err(e))
            | Message::PickedSave(Err(e))
            | Message::PickedLibrary(Err(e)) => self.error = e,
            Message::Save => self.save_picker(context),
            Message::PickedSave(Ok(Some(path))) => {
                if self.controller.as_ref().is_some_and(|c| {
                    self.save_version == Some((c.state.document_id, c.state.revision))
                }) {
                    self.perform(|c| c.save_project(&path));
                } else {
                    self.error =
                        "选择保存位置期间曲谱已改变，请重新按 Ctrl+S 保存当前工程。".into();
                }
                self.save_version = None;
            }
            Message::SelectLibraryPath(path) => {
                if !self
                    .controller
                    .as_ref()
                    .is_some_and(|c| c.capabilities().can_open)
                {
                    return;
                }
                if let Some(entry) = self
                    .controller
                    .as_ref()
                    .and_then(|c| c.library.iter().find(|e| e.path == path))
                    .cloned()
                {
                    self.library_menu = false;
                    let options = entry.options.and_then(|v| serde_json::from_value(v).ok());
                    self.perform(|c| open_document(c, &entry.path, options));
                } else {
                    self.error = "这首曲目已移出曲库，请刷新后重试。".into();
                }
            }
            Message::LibrarySearch(query) => self.library_query = query,
            Message::LibraryMenu => {
                self.library_menu = !self.library_menu;
                if self.library_menu {
                    self.library_query.clear();
                    self.perform(|c| c.refresh_library());
                }
            }
            Message::DismissLibrary => self.library_menu = false,
            Message::LibraryFolder => {
                self.library_menu = false;
                if let Some(c) = &self.controller {
                    let _ = std::process::Command::new("explorer.exe")
                        .arg(c.library_root())
                        .spawn();
                }
            }
            Message::SelectTab(editor) => self.editor_tab = editor,
            Message::Refresh => self.perform(|c| c.refresh_library()),
            Message::SelectPart(index) => {
                if self
                    .controller
                    .as_ref()
                    .is_some_and(|c| !c.capabilities().can_open)
                {
                    return;
                }
                let selected =
                    index.and_then(|i| self.controller.as_ref()?.state.keys.get(i).copied());
                self.options.track = selected.map(|p| p.0);
                self.options.channel = selected.map(|p| p.1);
            }
            Message::Convert => {
                let mut options = self.options.clone();
                if let Some(c) = &self.controller {
                    options.skip_long_rests = c.preferences.skip_long_rests;
                }
                self.perform(|c| c.convert(options));
            }
            Message::Listen => {
                if self
                    .controller
                    .as_ref()
                    .is_some_and(|c| c.state.transport == Transport::Playing)
                {
                    self.perform(|c| c.toggle_preview());
                } else if self.controller.as_ref().is_some_and(|c| {
                    c.preferences.compact && !c.state.parts.is_empty() && c.state.project.is_none()
                }) {
                    let options = self.options.clone();
                    self.perform(|c| c.convert(options));
                } else {
                    self.perform(|c| c.toggle_preview());
                }
            }
            Message::Game(arm) => {
                if !self.smoke {
                    self.perform(|c| c.game_play(arm));
                }
            }
            Message::StopGame => self.perform(|c| c.stop_game()),
            Message::Cancel => self.perform(|c| c.cancel()),
            Message::FilesDropped(DroppedData::StorageItems(items)) => {
                if let Some(item) = items.into_iter().find(|item| {
                    PathBuf::from(&item.path)
                        .extension()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| {
                            ["mid", "midi", "kar", "rmi", "hstudio"]
                                .contains(&s.to_ascii_lowercase().as_str())
                        })
                }) {
                    let path = PathBuf::from(item.path);
                    self.perform(|c| open_document(c, &path, None));
                } else {
                    self.error = "请拖入 MIDI、RMI 或 .hstudio 工程文件。".into();
                }
            }
            Message::Settings => {
                if self
                    .controller
                    .as_ref()
                    .is_some_and(|c| c.state.transition.is_some())
                {
                    return;
                }
                self.settings = true;
                self.library_menu = false;
                self.settings_draft = self.controller.as_ref().map(|c| c.preferences.clone());
            }
            Message::SettingsSave => {
                if let Some(p) = self.settings_draft.take() {
                    let previous = self
                        .controller
                        .as_ref()
                        .is_some_and(|c| c.preferences.compact);
                    let requested = p.compact;
                    let requested_theme = p.theme.clone();
                    let size = read_client_size(self.native_window.get());
                    self.perform(|c| c.update_preferences(p));
                    let applied = self
                        .controller
                        .as_ref()
                        .is_some_and(|c| c.preferences.compact);
                    if requested == applied {
                        self.window_sizes
                            .remember_transition(previous, applied, size);
                    }
                    // Commit path: the single surface rebuild + log per save.
                    // Previews above only invalidate the shared Rc; rapid clicks
                    // can no longer interleave native mounts.
                    if self
                        .controller
                        .as_ref()
                        .is_some_and(|c| c.preferences.theme == requested_theme)
                        && self.committed_theme != requested_theme
                    {
                        let next = Palette::for_theme(&requested_theme);
                        let old_name = self.canvas_theme.borrow().theme_name().to_string();
                        self.apply_canvas_theme(next.clone());
                        self.committed_theme = requested_theme.clone();
                        self.theme_version = self.theme_version.wrapping_add(1);
                        append_theme_log(&theme_log_line(
                            self.theme_version,
                            "commit",
                            &old_name,
                            &requested_theme,
                            &next,
                        ));
                        self.rebuild_canvases();
                    }
                }
                self.settings = false;
            }
            Message::SettingsCancel => {
                self.settings = false;
                self.settings_draft = None;
            }
            Message::ResetLibrary => {
                if let Some(p) = self.settings_draft.as_mut() {
                    p.library_folder.clear();
                }
            }
            Message::PitchZoom(closer) => {
                if closer {
                    self.editor.borrow_mut().pitch_zoom_in();
                } else {
                    self.editor.borrow_mut().pitch_zoom_out();
                }
            }
            Message::JumpMark => {
                let marker = self.editor.borrow().highlight;
                if let Some(t) = marker {
                    self.perform(|c| c.seek(t));
                }
            }
            Message::TimelineDown(info) => {
                if !info.is_left_button_pressed
                    || !self
                        .controller
                        .as_ref()
                        .is_some_and(|c| c.capabilities().can_play)
                {
                    return;
                }
                self.perform(|c| c.pause());
                let mut t = self.timeline.borrow_mut();
                t.dragging = true;
                t.position = t.at(info.x);
            }
            Message::TimelineMove(info) => {
                let mut t = self.timeline.borrow_mut();
                if t.dragging {
                    t.position = t.at(info.x);
                }
            }
            Message::TimelineUp(info) => {
                if !self.timeline.borrow().dragging {
                    return;
                }
                let value = {
                    let mut t = self.timeline.borrow_mut();
                    t.dragging = false;
                    t.position = t.at(info.x);
                    t.position
                };
                self.perform(|c| c.seek_audio(value));
            }
            Message::RemoteAddress(Some(index)) => self.remote_address = index,
            Message::CopyRemote => {
                if let Some(url) = self.phone_url() {
                    if let Err(error) = copy_text(&url) {
                        self.error = error.to_string();
                    }
                }
            }
            Message::Phone => {
                self.phone = !self.phone;
                if self.phone
                    && self
                        .controller
                        .as_ref()
                        .is_some_and(|c| c.remote_url().is_none())
                {
                    self.perform(|c| c.start_remote(47638).map(|_| ()));
                }
            }
            Message::PhoneClosed => self.phone = false,
            Message::StopRemote => {
                self.phone = false;
                self.perform(|c| {
                    c.stop_remote();
                    Ok(())
                });
            }
            Message::PickLibrary => {
                if !FolderPicker::new()
                    .title("选择 MIDI 曲库文件夹")
                    .request(context, |r| {
                        Message::PickedLibrary(r.map_err(|e| e.to_string()))
                    })
                {
                    self.error = "请先关闭当前文件对话框。".into();
                }
            }
            Message::PickedLibrary(Ok(Some(path))) => {
                if let Some(p) = self.settings_draft.as_mut() {
                    p.library_folder = path.to_string_lossy().into_owned();
                }
            }
            Message::Speed(Some(v)) => self.options.speed = v,
            Message::Transpose(Some(v)) => self.options.transpose = v.round() as i32,
            Message::Mode(Some(i)) => {
                if let Some(mode) = ["sustain", "highest", "continuous"].get(i) {
                    self.options.melody_mode = (*mode).into();
                }
            }
            Message::AutoOctave(value) => self.options.auto_octave = value,
            Message::Trim(value) => self.options.trim_silence = value,
            Message::Phrase(value) => self.options.phrase_octave = value,
            Message::SkipRests(value) => {
                if let Some(p) = self.settings_draft.as_mut() {
                    p.skip_long_rests = value;
                }
            }
            Message::HighlightStart(value) => {
                if let Some(p) = self.settings_draft.as_mut() {
                    p.start_from_highlight = value;
                }
            }
            Message::Compact(value) => {
                if let Some(p) = self.settings_draft.as_mut() {
                    p.compact = value;
                }
            }
            Message::Theme(Some(i)) => {
                if let Some(theme) = ["paper", "forest", "blue", "plum"].get(i) {
                    if let Some(p) = self.settings_draft.as_mut() {
                        p.theme = (*theme).into();
                    }
                    // Preview: update the shared palette and remount native
                    // controls through the revisioned keyed subtree.
                    let next = Palette::for_theme(theme);
                    self.apply_canvas_theme(next);
                }
            }
            Message::Zoom(factor) => self.editor.borrow_mut().zoom_by(factor),
            Message::Fit => {
                let mut e = self.editor.borrow_mut();
                e.fit_pitches();
            }
            Message::Pan(delta) => self.editor.borrow_mut().pan_seconds(delta),
            Message::Undo => {
                let changed = self.editor.borrow_mut().undo();
                self.apply_edit(Ok(changed));
            }
            Message::Redo => {
                let changed = self.editor.borrow_mut().redo();
                self.apply_edit(Ok(changed));
            }
            Message::Delete => {
                let result = self.editor.borrow_mut().delete_selected();
                self.apply_edit(result);
            }
            Message::Mark => {
                let position = self.editor.borrow().position;
                self.perform(|c| {
                    c.pause()?;
                    c.set_highlight(Some(position))
                });
            }
            Message::ClearMark => self.perform(|c| {
                c.pause()?;
                c.set_highlight(None)
            }),
            Message::Nudge(time, pitch, length) => {
                let result = self.editor.borrow_mut().nudge(time, pitch, length);
                self.apply_edit(result);
            }
            Message::Wheel(info) => {
                let mut e = self.editor.borrow_mut();
                // A normal wheel pans vertically; a real horizontal-wheel
                // event (mouse tilt or trackpad) pans the timeline. Neither
                // direction requires Shift/Ctrl or another modifier key.
                if info.is_horizontal_wheel {
                    let seconds = -(info.wheel_delta as f64) / e.zoom;
                    e.pan_seconds(seconds);
                } else {
                    let row = e.row_height();
                    e.pan_pitches(-(info.wheel_delta as f64) / 120.0 * row * 3.0);
                }
            }
            Message::PointerDown(info) => {
                if !self.controller.as_ref().is_some_and(|c| {
                    let caps = c.capabilities();
                    caps.can_play || caps.can_edit
                }) {
                    return;
                }
                let double = self.last_pointer.is_some_and(|(when, x, y)| {
                    when.elapsed() < Duration::from_millis(350)
                        && (x - info.x).abs() < 5.0
                        && (y - info.y).abs() < 5.0
                });
                self.last_pointer = Some((Instant::now(), info.x, info.y));
                if double
                    && info.is_left_button_pressed
                    && info.y > RULER_HEIGHT
                    && !self.editor.borrow().compact
                    && self.editor.borrow().allow_note_edits
                    && info.x >= self.editor.borrow().left()
                    && self.editor.borrow().hit_test(info.x, info.y).is_none()
                {
                    let (time, pitch) = {
                        let e = self.editor.borrow();
                        (e.time_at(info.x), e.pitch_at(info.y))
                    };
                    let result = self.editor.borrow_mut().add_note(time, pitch);
                    self.apply_edit(result);
                } else if (info.is_left_button_pressed || info.is_middle_button_pressed)
                    && self
                        .controller
                        .as_ref()
                        .is_some_and(|c| c.capabilities().can_play)
                {
                    self.editor.borrow_mut().begin_pointer(
                        info.x,
                        info.y,
                        info.is_middle_button_pressed,
                    );
                }
            }
            Message::PointerMove(info) => {
                if info.is_left_button_pressed || info.is_middle_button_pressed {
                    let action = self.editor.borrow_mut().move_pointer(info.x, info.y);
                    if action.pause {
                        self.perform(|c| c.pause());
                    }
                }
            }
            Message::PointerUp(info) => {
                let result = self.editor.borrow_mut().end_pointer(info.x);
                match result {
                    Ok(action) => {
                        if let Some(position) = action.seek {
                            self.perform(|c| c.seek(position));
                        }
                        self.apply_edit(Ok(action.changed));
                    }
                    Err(e) => self.error = e.to_string(),
                }
            }
            Message::PointerCancel => self.editor.borrow_mut().cancel_drag(),
            Message::Close => {
                self.editor.borrow_mut().cancel_drag();
                if let Some(c) = self.controller.as_mut() {
                    match c.close() {
                        Ok(true) => {
                            self.close_allowed.set(true);
                            let _ = context.window().request_close();
                        }
                        Ok(false) => {}
                        Err(e) => self.error = e.to_string(),
                    }
                } else {
                    self.close_allowed.set(true);
                    let _ = context.window().request_close();
                }
            }
            Message::CloseHookResult(Err(e)) | Message::CanvasError(e) => self.error = e,
            _ => {}
        }
        self.sync();
        if should_redraw {
            self.invalidator.invalidate();
            self.timeline_invalidator.invalidate();
        }
    }
}
