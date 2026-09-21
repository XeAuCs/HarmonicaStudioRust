use super::*;

impl Studio {
    pub(super) fn library_popup(&self, context: &ViewContext<Self>, p: &Palette) -> View {
        let c = self.controller.as_ref().unwrap();
        let mut rows = Vec::new();
        for (index, item) in c.library.iter().enumerate() {
            let title = item.duration_seconds.map_or(item.title.clone(), |d| {
                format!("{}  /  {d:.0} 秒", item.title)
            });
            rows.push(ui_button(
                context,
                &title,
                Message::SelectSong(Some(index)),
                true,
                false,
                p,
            ));
        }
        if rows.is_empty() {
            rows.push(
                label(
                    if c.library_root().is_dir() {
                        "文件夹中还没有 MIDI"
                    } else {
                        "曲库文件夹不存在"
                    },
                    13.0,
                    p,
                )
                .foreground(p.muted.native())
                .into(),
            );
        }
        let songs = ScrollViewer::new()
            .max_height(360.0)
            .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
            .content(
                StackPanel::new().spacing(1.0).keyed_children(
                    rows.into_iter()
                        .enumerate()
                        .map(|(i, v)| (i.to_string(), v)),
                ),
            );
        card(p)
            .width(370.0)
            .padding(6.0)
            .horizontal_alignment(HorizontalAlignment::Right)
            .vertical_alignment(VerticalAlignment::Top)
            .margin(Thickness::new(0.0, 146.0, 138.0, 0.0))
            // The popup overlays `main` as a sibling of the themed body Border,
            // so it carries the same resource table instead of relying on
            // inheritance. Buttons inside additionally carry per-Button aliases.
            .resource_overrides(theme_resources(p))
            .content(
                StackPanel::new().spacing(4.0).children((
                    songs,
                    Border::new()
                        .height(1.0)
                        .background(p.line.native())
                        .content(View::empty()),
                    ui_button(
                        context,
                        "打开曲库文件夹",
                        Message::LibraryFolder,
                        true,
                        false,
                        p,
                    ),
                    ui_button(context, "刷新曲库", Message::Refresh, true, false, p),
                    ui_button(context, "曲库设置…", Message::Settings, true, false, p),
                )),
            )
    }
    pub(super) fn settings_view(&self, context: &ViewContext<Self>, p: &Palette) -> View {
        let c = self.controller.as_ref().unwrap();
        let prefs = self.settings_draft.as_ref().unwrap_or(&c.preferences);
        let theme = ["paper", "forest", "blue", "plum"]
            .iter()
            .position(|t| *t == prefs.theme)
            .unwrap_or(0);
        let enabled = !c.busy();
        let content=StackPanel::new().spacing(16.0).children((
            label("设置",20.0,p).font_weight(FontWeight::SEMI_BOLD),label("主题色",13.0,p),
            ComboBox::new().items_source(["暖纸 · 朱砂","雾绿 · 松石","灰蓝 · 靛墨","素绢 · 梅紫"]).selected_index(theme).on_selection_changed(context.callback(Message::Theme)).horizontal_alignment(HorizontalAlignment::Stretch),
            check(context,"精简模式 · 选歌后自动生成，隐藏调音与编辑",prefs.compact,Message::Compact,enabled,p),
            check(context,"跳过长空白",prefs.skip_long_rests,Message::SkipRests,enabled,p),
            label("音符之间超过 3 秒的空白缩短为 0.6 秒。保留正常停顿和长音，试听与游戏演奏同步生效。",12.0,p).foreground(p.muted.native()).text_wrapping(TextWrapping::Wrap),
            check(context,"从心动片段开始播放",prefs.start_from_highlight,Message::HighlightStart,true,p),
            label("试听和游戏演奏均生效。没有标记时正常播放；暂停后继续或手动定位试听时保留当前位置。",12.0,p).foreground(p.muted.native()).text_wrapping(TextWrapping::Wrap),
            label("曲库文件夹",13.0,p),
            Grid::new().columns([GridLength::STAR,GridLength::Auto]).children((
                Border::new().border_brush(p.line.native()).border_thickness(1.0).padding(8.0).content(label(crate::paths::library_path(&prefs.library_folder).display().to_string(),13.0,p).is_text_selection_enabled(true).text_trimming(TextTrimming::CharacterEllipsis)),
                Border::new().grid_column(1).margin(Thickness::new(6.0,0.0,0.0,0.0)).content(ui_button(context,"选择…",Message::PickLibrary,true,false,p)),
            )),
            label("将 MIDI 放入这个文件夹，曲库会自动更新。支持 .mid / .midi / .kar / .rmi，不扫描子文件夹。",12.0,p).foreground(p.muted.native()).text_wrapping(TextWrapping::Wrap),
            ui_button(context,"恢复默认曲库文件夹",Message::ResetLibrary,true,false,p),
            StackPanel::new().orientation(Orientation::Horizontal).spacing(6.0).horizontal_alignment(HorizontalAlignment::Right).children((ui_button(context,"保存",Message::SettingsSave,true,false,p),ui_button(context,"取消",Message::SettingsCancel,true,false,p))),
        ));
        Border::new()
            .width(540.0)
            .background(p.background.native())
            .border_brush(p.line.native())
            .border_thickness(1.0)
            .padding(Thickness::new(28.0, 26.0, 28.0, 24.0))
            .content(content)
    }
    pub(super) fn phone_url(&self) -> Option<String> {
        let c = self.controller.as_ref()?;
        c.remote_addresses()
            .get(self.remote_address)
            .and_then(|(_, ip)| c.remote_url_for(ip))
            .or_else(|| c.remote_url())
    }
    pub(super) fn phone_view(&self, context: &ViewContext<Self>, p: &Palette) -> View {
        let c = self.controller.as_ref().unwrap();
        let url = self.phone_url();
        let addresses = c.remote_addresses();
        let qr = url
            .as_ref()
            .and_then(|u| {
                qrcode::QrCode::with_error_correction_level(u.as_bytes(), qrcode::EcLevel::M).ok()
            })
            .map(|code| {
                let size = code.width();
                let pixels = code
                    .to_colors()
                    .iter()
                    .map(|v| *v == qrcode::Color::Dark)
                    .collect::<Vec<_>>();
                let palette = Rc::clone(&self.canvas_theme);
                let invalidator = self.invalidator.clone();
                let artistic = self.qr_artistic;
                Border::new()
                    .height(300.0)
                    .content(windows_canvas::Canvas::invalidated(
                        &invalidator,
                        move |ctx| draw_qr(ctx, size, &pixels, artistic, &palette.borrow()),
                    ))
            })
            .unwrap_or_else(|| label("遥控已关闭", 13.0, p).into());
        let pairing = card(p)
            .padding(Thickness::new(18.0, 12.0, 18.0, 12.0))
            .content(
                StackPanel::new().children((
                    Grid::new()
                        .columns([GridLength::Pixel(30.0), GridLength::STAR, GridLength::Auto])
                        .children((
                            logo(30.0),
                            label("口琴工坊", 14.0, p)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .grid_column(1)
                                .margin(Thickness::new(8.0, 0.0, 0.0, 0.0))
                                .vertical_alignment(VerticalAlignment::Center),
                            label("随手选曲 · 随时演奏", 10.0, p)
                                .foreground(p.muted.native())
                                .grid_column(2)
                                .vertical_alignment(VerticalAlignment::Center),
                        )),
                    qr,
                    ornament(&self.canvas_theme, &self.invalidator),
                )),
            );
        Border::new().width(468.0).background(p.background.native()).border_brush(p.line.native()).border_thickness(1.0).padding(Thickness::new(24.0,20.0,24.0,22.0)).content(StackPanel::new().spacing(12.0).children((
            Grid::new().columns([GridLength::STAR,GridLength::Auto]).children((label("手机遥控",20.0,p).font_weight(FontWeight::SEMI_BOLD),label(if c.remote_client_connected(){"手机已连接"}else if c.remote_url().is_some(){"等待手机连接"}else{"遥控已关闭"},12.0,p).foreground(p.muted.native()).grid_column(1).vertical_alignment(VerticalAlignment::Center))),
            label("扫码，在手机浏览器中选曲与控制播放。",12.0,p).foreground(p.muted.native()),pairing,
            Grid::new().columns([GridLength::STAR,GridLength::Auto]).children((label("二维码样式",12.0,p).foreground(p.muted.native()).vertical_alignment(VerticalAlignment::Center),StackPanel::new().orientation(Orientation::Horizontal).spacing(6.0).grid_column(1).children((ui_button(context,"线条码",Message::QrStyle(true),true,self.qr_artistic,p),ui_button(context,"标准码",Message::QrStyle(false),true,!self.qr_artistic,p))))),
            Grid::new().columns([GridLength::Auto,GridLength::STAR]).children((label("连接网络",12.0,p).foreground(p.muted.native()).vertical_alignment(VerticalAlignment::Center).margin(Thickness::new(0.0,0.0,12.0,0.0)),ComboBox::new().items_source(addresses.iter().map(|(name,ip)|format!("{name}  /  {ip}")).collect::<Vec<_>>()).selected_index(self.remote_address).on_selection_changed(context.callback(Message::RemoteAddress)).horizontal_alignment(HorizontalAlignment::Stretch).grid_column(1))),
            label("手机与电脑需在同一局域网，声音由电脑播放。\n识别不顺时可切换标准码；重新开启遥控后请重新扫码。",12.0,p).foreground(p.muted.native()).text_wrapping(TextWrapping::Wrap),
            Grid::new().columns([GridLength::STAR,GridLength::STAR,GridLength::STAR]).children((ui_button(context,"复制连接",Message::CopyRemote,url.is_some(),false,p),Border::new().grid_column(1).margin(Thickness::new(6.0,0.0,0.0,0.0)).content(ui_button(context,"收起",Message::Phone,true,true,p)),Border::new().grid_column(2).margin(Thickness::new(6.0,0.0,0.0,0.0)).content(ui_button(context,"关闭遥控",Message::StopRemote,true,false,p)))),
        )))
    }
}
