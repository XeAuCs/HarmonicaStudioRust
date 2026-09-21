use super::*;

impl Studio {
    pub(super) fn shell_view(&self, _input: &LaunchInput, context: &mut ViewContext<Self>) -> View {
        let compact = self
            .controller
            .as_ref()
            .is_some_and(|c| c.preferences.compact);
        context.window_title(
            if self
                .controller
                .as_ref()
                .is_some_and(|c| c.state.project_dirty())
            {
                "● 口琴工坊 · Harmonica Studio"
            } else {
                "口琴工坊 · Harmonica Studio"
            },
        );
        let (window_width, window_height) = self.window_sizes.for_mode(compact);
        context.window_visuals(
            WindowVisuals::new()
                .theme(WindowTheme::Light)
                .client_size(window_width, window_height)
                .constraints(WindowConstraints {
                    min_width: Some(if compact { 800.0 } else { 1080.0 }),
                    min_height: Some(if compact { 640.0 } else { 870.0 }),
                    max_width: None,
                    max_height: None,
                }),
        );
        let Some(c) = self.controller.as_ref() else {
            return Border::new()
                .padding(28.0)
                .content(TextBlock::new().text(&self.error));
        };
        // Native controls and the self-drawn score/timeline must use the same
        // palette. The settings draft can change during a message update, so
        // read the already-synchronised shared palette instead of deriving a
        // second copy from preferences here.
        let p = self.canvas_theme.borrow().clone();
        let enabled = c.capabilities().can_open;
        let header = Grid::new()
            .columns([GridLength::Pixel(46.0), GridLength::STAR, GridLength::Auto])
            .children((
                logo(44.0),
                StackPanel::new()
                    .spacing(3.0)
                    .grid_column(1)
                    .margin(Thickness::new(16.0, 0.0, 0.0, 0.0))
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        label("口琴工坊", 22.0, &p).font_weight(FontWeight::SEMI_BOLD),
                        label("HARMONICA  /  曲谱与演奏", 10.0, &p).foreground(p.muted.native()),
                    )),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(16.0)
                    .grid_column(2)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        icon_button(
                            context,
                            "手机遥控",
                            false,
                            c.remote_url().is_some(),
                            Message::Phone,
                            &p,
                        ),
                        icon_button(context, "设置", true, false, Message::Settings, &p),
                        label(format!("v{}", env!("CARGO_PKG_VERSION")), 12.0, &p)
                            .foreground(p.muted.native())
                            .vertical_alignment(VerticalAlignment::Center),
                    )),
            ));
        let filename = c
            .state
            .project
            .as_ref()
            .map(|v| v.title.clone())
            .or_else(|| {
                c.state
                    .source
                    .as_ref()
                    .and_then(|f| f.file_name())
                    .map(|f| f.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "选择一首曲谱".into());
        let filecard = card(&p).content(
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
                .children((
                    label(filename, 14.0, &p)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_trimming(TextTrimming::CharacterEllipsis)
                        .vertical_alignment(VerticalAlignment::Center),
                    Border::new()
                        .grid_column(1)
                        .margin(Thickness::new(12.0, 0.0, 12.0, 0.0))
                        .content(ui_button(
                            context,
                            &format!("曲库 · {}  ⌄", c.library.len()),
                            Message::LibraryMenu,
                            enabled,
                            false,
                            &p,
                        )),
                    Border::new().grid_column(2).content(
                        ui_button(context, "打开 MIDI", Message::Open, enabled, true, &p)
                            .tooltip("选歌时自动恢复工程；也可将 .hstudio 工程文件拖入窗口打开。"),
                    ),
                )),
        );
        let tabbar = if compact {
            View::empty()
        } else {
            Border::new()
                .border_brush(p.line.native())
                .border_thickness(Thickness::new(0.0, 0.0, 0.0, 1.0))
                .content(
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .children((
                            tab_button(context, "曲谱", false, self.editor_tab, &p),
                            tab_button(context, "编辑与试听", true, self.editor_tab, &p),
                        )),
                )
        };
        let page = if compact || self.editor_tab {
            self.editor_page(context, &p)
        } else {
            self.import_page(context, &p)
        };
        let tabs = Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .children((
                tabbar,
                Border::new()
                    .grid_row(1)
                    .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                    .content(page),
            ));
        let status = if !self.error.is_empty() {
            self.error.clone()
        } else if !c.state.save_error.is_empty() {
            c.state.save_error.clone()
        } else if !c.state.library_error.is_empty() {
            c.state.library_error.clone()
        } else {
            c.state.message.clone()
        };
        let bottom = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((
                label(status, 12.0, &p)
                    .foreground(p.muted.native())
                    .text_trimming(TextTrimming::CharacterEllipsis)
                    .vertical_alignment(VerticalAlignment::Center),
                Border::new().grid_column(1).content(if c.busy() {
                    ui_button(context, "取消转换", Message::Cancel, true, false, &p)
                } else {
                    View::empty()
                }),
            ));
        let body = Border::new()
            .background(p.background.native())
            .padding(Thickness::new(28.0, 22.0, 28.0, 18.0))
            .resource_overrides(theme_resources(&p))
            .on_preview_key_down(context.routed_callback(global_key))
            .drop_policy(if enabled {
                Some(DragDropPolicy::new().storage_items(
                    DragDropAction::new(DragDropOperation::Copy).caption("打开曲谱"),
                ))
            } else {
                None
            })
            .on_drop(context.callback(Message::FilesDropped))
            .content(
                Grid::new()
                    .rows([
                        GridLength::Auto,
                        GridLength::Auto,
                        GridLength::STAR,
                        GridLength::Auto,
                        GridLength::Auto,
                        GridLength::Auto,
                    ])
                    .children((
                        header,
                        Border::new()
                            .grid_row(1)
                            .margin(Thickness::new(0.0, 16.0, 0.0, 16.0))
                            .content(filecard),
                        Border::new().grid_row(2).content(tabs),
                        Border::new()
                            .grid_row(3)
                            .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
                            .content(bottom),
                        Border::new()
                            .grid_row(4)
                            .margin(Thickness::new(
                                0.0,
                                if c.busy() { 16.0 } else { 0.0 },
                                0.0,
                                0.0,
                            ))
                            .content(if c.busy() {
                                ProgressBar::new().is_indeterminate(true).height(4.0).into()
                            } else {
                                View::empty()
                            }),
                        label(
                            "试听为合成音色  /  F6 开始、停止演奏 · F8 退出  /  游戏兼容性尚未实测",
                            12.0,
                            &p,
                        )
                        .foreground(p.muted.native())
                        .grid_row(5)
                        .margin(Thickness::new(0.0, 16.0, 0.0, 0.0)),
                    )),
            );
        // ResourceDictionary replacement is not sufficient to refresh every
        // WinUI control template. Change the body key with the palette
        // revision so all native controls are recreated with current brushes.
        let mut layers = vec![(format!("body-theme-{}", self.theme_revision), body)];
        if self.library_menu {
            layers.push((
                "library-dismiss".into(),
                Border::new()
                    .background(Color::argb(0, 0, 0, 0))
                    .on_pointer_pressed(context.callback(|_| Message::DismissLibrary))
                    .content(View::empty()),
            ));
            layers.push(("library-popup".into(), self.library_popup(context, &p)));
        }
        let main = Grid::new().keyed_children(layers);
        // Native ContentDialog owns modal input, traps Tab focus, restores focus on
        // closing, and reports Escape as a close without accepting settings.
        let settings_content: View = if self.settings {
            Grid::new()
                .keyed_children([(
                    format!("settings-theme-{}", self.theme_revision),
                    self.settings_view(context, &p),
                )])
                .into()
        } else {
            View::empty()
        };
        let settings = ContentDialog::new()
            .is_open(self.settings)
            .resource_overrides(dialog_resources(&p, 540.0))
            .on_closed(context.callback(|_| Message::SettingsCancel))
            .content(settings_content);
        let phone_content: View = if self.phone && !self.settings {
            Grid::new()
                .keyed_children([(
                    format!("phone-theme-{}", self.theme_revision),
                    self.phone_view(context, &p),
                )])
                .into()
        } else {
            View::empty()
        };
        let phone = ContentDialog::new()
            .is_open(self.phone && !self.settings)
            .resource_overrides(dialog_resources(&p, 468.0))
            .on_closed(context.callback(|_| Message::PhoneClosed))
            .content(phone_content);
        // Dialog overlays must belong to a native children container. A root
        // fragment can be published through a component content slot, which is
        // not a supported dialog owner in Reactor's topology planner.
        Grid::new().children((main, settings, phone))
    }
}
