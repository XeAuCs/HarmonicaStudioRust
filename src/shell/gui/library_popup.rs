use super::*;

impl Studio {
    pub(super) fn library_popup(&self, context: &ViewContext<Self>, p: &Palette) -> View {
        let c = self.controller.as_ref().unwrap();
        let filtered = self.filtered_library();
        let count = filtered.len();
        let list_height = read_client_size(self.native_window.get())
            .map_or(300.0, |(_, height)| (height - 370.0).clamp(100.0, 300.0));
        let mut rows = Vec::new();
        for index in filtered {
            let item = &c.library[index];
            let current = c.state.source.as_ref() == Some(&item.path);
            let duration = item
                .duration_seconds
                .filter(|d| d.is_finite() && *d >= 0.0)
                .map(|d| {
                    let secs = d.round() as u64;
                    format!("{}:{:02}", secs / 60, secs % 60)
                })
                .unwrap_or_else(|| "—".into());
            let content = Grid::new()
                .columns([GridLength::STAR, GridLength::Pixel(65.0)])
                .children((
                    label(&item.title, 13.0, p)
                        .text_wrapping(TextWrapping::Wrap)
                        .margin(Thickness::new(0.0, 0.0, 12.0, 0.0)),
                    label(
                        if current {
                            "当前曲目".into()
                        } else {
                            duration
                        },
                        11.0,
                        p,
                    )
                    .foreground(if current {
                        p.accent.native()
                    } else {
                        p.muted.native()
                    })
                    .horizontal_alignment(HorizontalAlignment::Right)
                    .vertical_alignment(VerticalAlignment::Center)
                    .grid_column(1),
                ));
            rows.push((
                item.path.to_string_lossy().into_owned(),
                Border::new()
                    .background(if current {
                        p.selection.native()
                    } else {
                        p.surface.native()
                    })
                    .corner_radius(4.0)
                    .content(
                        Button::new()
                            .style(ButtonStyle::Subtle)
                            .resource_overrides(
                                button_resources(p, false)
                                    .set("ButtonBackground", Color::argb(0, 0, 0, 0))
                                    .set("ButtonBackgroundPointerOver", p.accent_soft.native())
                                    .set("ButtonBorderBrush", Color::argb(0, 0, 0, 0))
                                    .set("ButtonBorderBrushPointerOver", Color::argb(0, 0, 0, 0)),
                            )
                            .min_height(42.0)
                            .horizontal_alignment(HorizontalAlignment::Stretch)
                            .horizontal_content_alignment(HorizontalAlignment::Stretch)
                            .is_enabled(c.capabilities().can_open)
                            .on_click(
                                context.message(Message::SelectLibraryPath(item.path.clone())),
                            )
                            .content(content)
                            .tooltip(&item.title),
                    ),
            ));
        }
        let list: View = if rows.is_empty() {
            Border::new()
                .padding(Thickness::xy(12.0, 28.0))
                .content(
                    label(
                        if c.library_refreshing() {
                            "正在读取曲库…"
                        } else if !c.library_root().is_dir() {
                            "曲库文件夹不存在，请在设置中重新选择。"
                        } else if self.library_query.trim().is_empty() {
                            "曲库还是空的，放入 MIDI 后点击刷新。"
                        } else {
                            "没有找到匹配曲目，试试其他关键词。"
                        },
                        13.0,
                        p,
                    )
                    .foreground(p.muted.native())
                    .text_wrapping(TextWrapping::Wrap),
                )
                .into()
        } else {
            ScrollViewer::new()
                .max_height(list_height)
                .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
                .content(StackPanel::new().spacing(2.0).keyed_children(rows))
                .into()
        };
        let action = |title: &str, message: Message, enabled: bool| {
            Button::new()
                .style(ButtonStyle::Subtle)
                .resource_overrides(button_resources(p, false))
                .is_enabled(enabled)
                .on_click(context.message(message))
                .content(label(title, 12.0, p))
        };
        card(p)
            .width(440.0)
            .padding(14.0)
            .corner_radius(6.0)
            .horizontal_alignment(HorizontalAlignment::Right)
            .vertical_alignment(VerticalAlignment::Top)
            .margin(Thickness::new(0.0, 146.0, 138.0, 0.0))
            .resource_overrides(theme_resources(p))
            .on_preview_key_down(context.routed_callback(|info: KeyEventInfo| {
                if info.key == VirtualKey::ESCAPE {
                    RoutedMessage::handled(Message::DismissLibrary)
                } else {
                    RoutedMessage::bubble_without_message()
                }
            }))
            .content(
                StackPanel::new().spacing(10.0).children((
                    Grid::new()
                        .columns([GridLength::STAR, GridLength::Auto])
                        .children((
                            label("选择曲目", 17.0, p).font_weight(FontWeight::SEMI_BOLD),
                            Border::new().grid_column(1).content(action(
                                "收起",
                                Message::DismissLibrary,
                                true,
                            )),
                        )),
                    Grid::new()
                        .columns([GridLength::STAR, GridLength::Auto])
                        .children((
                            TextBox::new()
                                .text(&self.library_query)
                                .placeholder_text("搜索曲名或文件名")
                                .background(p.surface.native())
                                .border_brush(p.line.native())
                                .on_text_changed(context.callback(Message::LibrarySearch)),
                            Border::new()
                                .grid_column(1)
                                .margin(Thickness::new(6.0, 0.0, 0.0, 0.0))
                                .content(action(
                                    "清空",
                                    Message::LibrarySearch(String::new()),
                                    !self.library_query.is_empty(),
                                )),
                        )),
                    label(
                        if c.library_refreshing() {
                            "正在刷新…".into()
                        } else if self.library_query.trim().is_empty() {
                            format!("{} 首曲目 · 点击打开", c.library.len())
                        } else {
                            format!("找到 {count} / {} 首曲目", c.library.len())
                        },
                        11.0,
                        p,
                    )
                    .foreground(p.muted.native()),
                    list,
                    Border::new()
                        .height(1.0)
                        .background(p.line.native())
                        .content(View::empty()),
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            action("打开文件夹", Message::LibraryFolder, true),
                            action("刷新", Message::Refresh, !c.library_refreshing()),
                            action("曲库设置", Message::Settings, true),
                        )),
                )),
            )
    }
}
