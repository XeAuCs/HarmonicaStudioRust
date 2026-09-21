use super::*;

impl Studio {
    pub(super) fn editor_page(&self, context: &ViewContext<Self>, p: &Palette) -> View {
        let c = self.controller.as_ref().unwrap();
        let compact = c.preferences().compact;
        let has = c.state().has_notes();
        let caps = c.capabilities();
        let ready = !c.busy() && c.state().transition.is_none();
        let retry = compact && !c.state().parts.is_empty() && c.state().project.is_none();
        let summary = if let Some(project) = &c.state().project {
            let mut s = format!(
                "{} 音  /  {:.1} 秒",
                project.notes.len(),
                c.state().score_duration
            );
            if let Some(report) = project.report.as_ref() {
                let adjusted = report
                    .get("phrase_adjusted_notes")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let dropped = report
                    .get("dropped_out_of_range")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let removed = report
                    .get("removed_rest_seconds")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if removed > 0.0 && !c.state().export_dirty() {
                    s += &format!("  /  已缩短 {removed:.1} 秒空白");
                }
                if adjusted > 0 {
                    s += &format!("  /  提取时按句调整 {adjusted} 音");
                }
                if dropped > 0 {
                    s += &if report.get("out_of_range_notes").is_some() {
                        format!("  /  超音域 {dropped} 音")
                    } else {
                        format!("  /  超音域丢弃 {dropped} 音（重新提取可显示）")
                    };
                }
            }
            if c.state().export_dirty() {
                s += "  /  待生成";
            }
            s
        } else if compact {
            "从曲库选歌，自动生成旋律。".into()
        } else {
            "生成曲谱后，在这里编辑旋律。".into()
        };
        let tools = if compact {
            View::empty()
        } else {
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(6.0)
                .children((
                    ui_button(context, "音域 −", Message::PitchZoom(false), true, false, p),
                    ui_button(context, "音域 +", Message::PitchZoom(true), true, false, p),
                    ui_button(context, "适配音域", Message::Fit, true, false, p),
                    ui_button(context, "时间 −", Message::Zoom(1.0 / 1.2), true, false, p),
                    ui_button(context, "时间 +", Message::Zoom(1.2), true, false, p),
                ))
        };
        let heading = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((
                label(summary, 13.0, p)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .vertical_alignment(VerticalAlignment::Center)
                    .tooltip("工程和标记自动保存。需要备份或分享时，按 Ctrl+S 另存工程副本。"),
                Border::new()
                    .grid_column(1)
                    .margin(Thickness::new(20.0, 0.0, 0.0, 0.0))
                    .content(tools),
            ));
        let roll = Border::new()
            .is_tab_stop(true)
            .capture_pointer_on_press(true)
            .focus_on_pointer_release(true)
            .on_pointer_wheel_changed(context.callback(Message::Wheel))
            .on_pointer_pressed(context.callback(Message::PointerDown))
            .on_pointer_moved(context.callback(Message::PointerMove))
            .on_pointer_released(context.callback(Message::PointerUp))
            .on_pointer_capture_lost(context.message(Message::PointerCancel))
            .on_pointer_canceled(context.message(Message::PointerCancel))
            .on_preview_key_down(context.routed_callback(editor_key))
            .min_height(if compact { 170.0 } else { 250.0 })
            .content(self.score_view.clone());
        let e = self.editor.borrow();
        // Panning is handled directly by the editor wheel gesture. Do not
        // reserve a column for a visible scrollbar/slider here.
        let rollrow = roll;
        drop(e);
        let marker = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((
                if compact {
                    View::empty()
                } else {
                    label(
                        "拖动音符编辑 · 双击空白添加\n滚轮上下移动，横向滚轮左右移动\n拖动空白 / 时间尺移动并暂停",
                        12.0,
                        p,
                    )
                    .foreground(p.muted.native())
                    .into()
                },
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(12.0)
                    .grid_column(1)
                    .children((
                        ui_button(
                            context,
                            "标记心动片段",
                            Message::Mark,
                            caps.can_edit && has && c.state().logical_seek < c.state().score_duration,
                            false,
                            p,
                        ),
                        ui_button(
                            context,
                            "跳到标记",
                            Message::JumpMark,
                            ready
                                && c.state()
                                    .project
                                    .as_ref()
                                    .is_some_and(|p| p.highlight.is_some()),
                            false,
                            p,
                        ),
                        ui_button(
                            context,
                            "清除标记",
                            Message::ClearMark,
                            caps.can_edit
                                && c.state()
                                    .project
                                    .as_ref()
                                    .is_some_and(|p| p.highlight.is_some()),
                            false,
                            p,
                        ),
                    )),
            ));
        let timeline = Grid::new()
            .columns([
                GridLength::Pixel(55.0),
                GridLength::STAR,
                GridLength::Pixel(120.0),
            ])
            .children((
                label(
                    if c.state().position_label.is_empty() {
                        "未播放"
                    } else {
                        &c.state().position_label
                    },
                    12.0,
                    p,
                )
                .foreground(p.muted.native())
                .vertical_alignment(VerticalAlignment::Center),
                Border::new()
                    .grid_column(1)
                    .height(36.0)
                    .capture_pointer_on_press(true)
                    .on_pointer_pressed(context.callback(Message::TimelineDown))
                    .on_pointer_moved(context.callback(Message::TimelineMove))
                    .on_pointer_released(context.callback(Message::TimelineUp))
                    .content(self.timeline_view.clone()),
                label(
                    format!(
                        "{} / {}",
                        format_time(self.timeline.borrow().position),
                        format_time(self.timeline.borrow().duration)
                    ),
                    12.0,
                    p,
                )
                .foreground(p.muted.native())
                .horizontal_alignment(HorizontalAlignment::Right)
                .vertical_alignment(VerticalAlignment::Center)
                .grid_column(2),
            ));
        let transport = Grid::new()
            .columns([GridLength::STAR, GridLength::STAR])
            .children((
                ui_button(
                    context,
                    if retry {
                        "重新生成"
                    } else if c.state().transport == Transport::Playing {
                        "暂停"
                    } else if c.state().transport == Transport::Paused {
                        "继续试听"
                    } else {
                        "试听"
                    },
                    Message::Listen,
                    c.state().transport == Transport::Playing
                        || caps.can_play
                        || (retry && caps.can_open),
                    true,
                    p,
                ),
                Border::new()
                    .grid_column(1)
                    .margin(Thickness::new(6.0, 0.0, 0.0, 0.0))
                    .content(ui_button(
                        context,
                        if c.game_active() {
                            "结束演奏"
                        } else {
                            "演奏"
                        },
                        if c.game_active() {
                            Message::StopGame
                        } else {
                            Message::Game(true)
                        },
                        c.game_active() || (has && ready && !self.smoke),
                        false,
                        p,
                    )),
            ));
        card(p).content(
            Grid::new()
                .rows([
                    GridLength::Auto,
                    GridLength::STAR,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                ])
                .children((
                    heading,
                    Border::new()
                        .grid_row(1)
                        .margin(Thickness::new(0.0, 12.0, 0.0, 12.0))
                        .content(rollrow),
                    Border::new().grid_row(2).content(marker),
                    Border::new()
                        .grid_row(3)
                        .margin(Thickness::new(0.0, 12.0, 0.0, 12.0))
                        .content(timeline),
                    Border::new().grid_row(4).content(transport),
                )),
        )
    }
}
