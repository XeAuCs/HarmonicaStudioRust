use super::drawing::pitch_name;
use super::*;

impl Studio {
    pub(super) fn import_page(&self, context: &ViewContext<Self>, p: &Palette) -> View {
        let c = self.controller.as_ref().unwrap();
        let enabled = c.capabilities().can_open;
        let recommended = c.state.keys.first().copied();
        let selected = self
            .options
            .track
            .zip(self.options.channel)
            .or(c.state.selected_part);
        let mut cache = self.part_rows.borrow_mut();
        if cache.0 != c.state.document_id || cache.1 != c.state.keys.len() {
            cache.0 = c.state.document_id;
            cache.1 = c.state.keys.len();
            cache.2 = c
                .state
                .keys
                .iter()
                .map(|key| {
                    let notes = &c.state.parts[key];
                    let count = notes.len();
                    let min = notes.iter().map(|n| n.pitch).min().unwrap_or(0);
                    let max = notes.iter().map(|n| n.pitch).max().unwrap_or(0);
                    let duration = notes.iter().map(|n| n.end).fold(0.0, f64::max)
                        - notes.iter().map(|n| n.start).fold(f64::INFINITY, f64::min);
                    let name = format!(
                        "{}{} · 轨 {} / 通道 {}",
                        if Some(*key) == recommended {
                            "★ "
                        } else {
                            ""
                        },
                        c.state
                            .names
                            .get(&key.0)
                            .map(String::as_str)
                            .unwrap_or("未命名"),
                        key.0,
                        key.1 + 1
                    );
                    [
                        name,
                        count.to_string(),
                        format!("{} – {}", pitch_name(min), pitch_name(max)),
                        format!("{duration:.1} 秒"),
                    ]
                })
                .collect();
        }
        let rows = cache
            .2
            .iter()
            .enumerate()
            .map(|(index, values)| {
                (
                    index.to_string(),
                    ListViewItem::new().content(table_row(values.clone(), p, false)),
                )
            })
            .collect::<Vec<_>>();
        drop(cache);
        let table = Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .children((
                Border::new()
                    .background(p.background.native())
                    .border_brush(p.line.native())
                    .border_thickness(Thickness::new(0.0, 0.0, 0.0, 1.0))
                    .content(table_row(
                        [
                            "音轨 / 声部".into(),
                            "音符".into(),
                            "音域".into(),
                            "时长".into(),
                        ],
                        p,
                        true,
                    )),
                ListView::new()
                    .items(rows)
                    .selection_mode(ListViewSelectionMode::Single)
                    .selected_index(
                        selected.and_then(|key| c.state.keys.iter().position(|k| *k == key)),
                    )
                    .on_selection_changed(context.callback(Message::SelectPart))
                    .grid_row(1),
            ));
        let left = card(p).content(
            Grid::new()
                .rows([GridLength::Auto, GridLength::Auto, GridLength::STAR])
                .children((
                    label("声部", 14.0, p).font_weight(FontWeight::SEMI_BOLD),
                    label(
                        if c.state.parts.is_empty() {
                            "支持 MIDI / KAR / RMID。带 ★ 的是推荐声部，已排除打击乐。".into()
                        } else {
                            format!(
                                "{} 个声部 · 已排除打击乐 · 原文件保持不变",
                                c.state.parts.len()
                            )
                        },
                        12.0,
                        p,
                    )
                    .foreground(p.muted.native())
                    .text_wrapping(TextWrapping::Wrap)
                    .grid_row(1)
                    .margin(Thickness::new(0.0, 12.0, 0.0, 12.0)),
                    Border::new().grid_row(2).content(table),
                )),
        );
        let mode = ["sustain", "highest", "continuous"]
            .iter()
            .position(|m| *m == self.options.melody_mode)
            .unwrap_or(0);
        let form = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .rows([GridLength::Auto, GridLength::Auto, GridLength::Auto])
            .children((
                label("播放速度", 13.0, p)
                    .vertical_alignment(VerticalAlignment::Center)
                    .margin(Thickness::new(0.0, 0.0, 12.0, 12.0)),
                NumberBox::new()
                    .minimum(0.25)
                    .maximum(2.0)
                    .small_change(0.05)
                    .value(self.options.speed)
                    .is_enabled(enabled)
                    .on_value_changed(context.callback(Message::Speed))
                    .grid_column(1)
                    .margin(Thickness::new(0.0, 0.0, 0.0, 12.0)),
                label("整体移调", 13.0, p)
                    .grid_row(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .margin(Thickness::new(0.0, 0.0, 12.0, 12.0)),
                NumberBox::new()
                    .minimum(-24.0)
                    .maximum(24.0)
                    .small_change(1.0)
                    .value(self.options.transpose as f64)
                    .is_enabled(enabled)
                    .on_value_changed(context.callback(Message::Transpose))
                    .grid_row(1)
                    .grid_column(1)
                    .margin(Thickness::new(0.0, 0.0, 0.0, 12.0)),
                label("提取方式", 13.0, p)
                    .grid_row(2)
                    .vertical_alignment(VerticalAlignment::Center)
                    .margin(Thickness::new(0.0, 0.0, 12.0, 0.0)),
                ComboBox::new()
                    .items_source(["长音保护（原方式）", "同刻最高音", "连续旋律（兼顾前后音）"])
                    .selected_index(mode)
                    .is_enabled(enabled)
                    .on_selection_changed(context.callback(Message::Mode))
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .grid_row(2)
                    .grid_column(1),
            ));
        let options = StackPanel::new().spacing(12.0).children((
            label("调音", 14.0, p).font_weight(FontWeight::SEMI_BOLD),
            form,
            check(
                context,
                "自动调整八度，适配口琴音域",
                self.options.auto_octave,
                Message::AutoOctave,
                enabled,
                p,
            ),
            check(
                context,
                "超出音域时，允许按乐句调整八度",
                self.options.phrase_octave,
                Message::Phrase,
                enabled,
                p,
            ),
            check(
                context,
                "去掉开头的空白等待",
                self.options.trim_silence,
                Message::Trim,
                enabled,
                p,
            ),
            label(
                "参数用于重新提取旋律，生成后可在音符图中继续修改。",
                12.0,
                p,
            )
            .foreground(p.muted.native())
            .text_wrapping(TextWrapping::Wrap),
        ));
        let right = card(p).content(
            Grid::new()
                .rows([GridLength::STAR, GridLength::Auto])
                .children((
                    options,
                    Border::new()
                        .grid_row(1)
                        .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                        .content(ui_button(
                            context,
                            "生成曲谱",
                            Message::Convert,
                            c.capabilities().can_convert,
                            true,
                            p,
                        )),
                )),
        );
        Grid::new()
            .columns([GridLength::Star(3.0), GridLength::Star(2.0)])
            .children((
                left,
                Border::new()
                    .grid_column(1)
                    .margin(Thickness::new(14.0, 0.0, 0.0, 0.0))
                    .content(right),
            ))
    }
}
