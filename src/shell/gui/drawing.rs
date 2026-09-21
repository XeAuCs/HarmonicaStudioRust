use super::*;

pub(super) fn format_time(seconds: f64) -> String {
    let s = seconds.max(0.0).floor() as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}
pub(super) fn centered_font(size: f32) -> windows_canvas::Result<TextFormat> {
    Ok(TextFormat::new("Microsoft YaHei UI", size)?
        .with_alignment(TextAlignment::Center)
        .with_paragraph_alignment(ParagraphAlignment::Center))
}
pub(super) fn draw_score(
    ctx: &DrawContext,
    e: &EditorModel,
    p: &Palette,
) -> windows_canvas::Result<()> {
    ctx.clear(p.surface.canvas());
    let grid = ctx.create_solid_brush(p.grid.canvas())?;
    let line = ctx.create_solid_brush(p.line.canvas())?;
    let ink = ctx.create_solid_brush(p.ink.canvas())?;
    let muted = ctx.create_solid_brush(p.muted.canvas())?;
    let accent = ctx.create_solid_brush(p.accent.canvas())?;
    let selected = ctx.create_solid_brush(p.selection.canvas())?;
    let paper = ctx.create_solid_brush(p.surface.canvas())?;
    let shade = ctx.create_solid_brush(p.background.canvas())?;
    let noteink = ctx.create_solid_brush(p.accent_ink.canvas())?;
    let playhead = ctx.create_solid_brush(p.playhead.canvas())?;
    let font = TextFormat::new("Microsoft YaHei UI", 11.0)?;
    let centered = centered_font(11.0)?;
    let left = e.left() as f32;
    let row = e.row_height() as f32;
    let ruler = RULER_HEIGHT as f32;
    for pitch in e.low_pitch..=e.high_pitch {
        let y = e.y_at(pitch) as f32;
        let top = y.max(ruler);
        let bottom = (y + row).min(ctx.height);
        if bottom <= top {
            continue;
        }
        if !e.compact && [1, 3, 6, 8, 10].contains(&pitch.rem_euclid(12)) {
            ctx.fill_rect(&CanvasRect::new(left, top, ctx.width, bottom), &shade);
        }
        if pitch % 12 == 0 || (!e.compact && row >= 12.0) {
            ctx.draw_line(
                Vector2::new(left, bottom),
                Vector2::new(ctx.width, bottom),
                if pitch % 12 == 0 { &line } else { &grid },
                if pitch % 12 == 0 { 1.0 } else { 0.65 },
            );
        }
    }
    let step = [0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0]
        .into_iter()
        .find(|s| s * e.zoom >= 55.0)
        .unwrap_or(20.0);
    let start = ((e.offset / step).floor() as i64).max(0);
    let end = ((e.offset + e.visible_seconds()) / step).ceil() as i64;
    for tick in start..=end {
        let x = e.x_at(tick as f64 * step) as f32;
        if x < left || x > ctx.width {
            continue;
        }
        ctx.draw_line(
            Vector2::new(x, ruler),
            Vector2::new(x, ctx.height),
            &grid,
            0.65,
        );
    }
    for (i, note) in e.notes.iter().chain(&e.out_of_range_notes).enumerate() {
        if note.start > e.offset + e.visible_seconds() || note.end < e.offset {
            continue;
        }
        let (raw_x, raw_y, w, h) = e.note_rect(note);
        let true_width = (note.end - note.start) * e.zoom;
        let padding = (true_width * 0.18).min(1.5);
        let x = ((raw_x + padding) as f32).max(left);
        let right = ((raw_x + true_width - padding) as f32).min(ctx.width);
        let y = (raw_y as f32).max(ruler);
        let bottom = ((raw_y + h) as f32).min(ctx.height);
        if right <= x || bottom <= y {
            continue;
        }
        let rect = CanvasRect::new(x, y, right, bottom);
        let chosen = e.selected == Some(i) && !e.compact;
        let playable = (crate::notes::MIN_PITCH..=crate::notes::MAX_PITCH).contains(&note.pitch);
        if playable {
            ctx.fill_rounded_rect(
                &RoundedRect::new(rect, 0.8, 0.8),
                if chosen { &selected } else { &accent },
            );
        }
        ctx.draw_rounded_rect(
            &RoundedRect::new(rect, 0.8, 0.8),
            if chosen { &ink } else { &accent },
            if chosen {
                1.2
            } else if playable {
                0.7
            } else {
                1.0
            },
        );
        if !e.compact
            && w > key_label(note.pitch).chars().count() as f64 * 11.0 + 8.0
            && h >= 13.0
            && raw_x >= e.left()
            && raw_x + w <= ctx.width as f64
        {
            ctx.draw_text(
                &key_label(note.pitch),
                &centered,
                &CanvasRect::from_xywh(raw_x as f32 + 4.0, raw_y as f32, w as f32 - 8.0, h as f32),
                if chosen { &ink } else { &noteink },
            );
        }
        if chosen && w >= 9.0 && h >= 8.0 {
            ctx.draw_line(
                Vector2::new(right - 3.0, y + 3.0),
                Vector2::new(right - 3.0, bottom - 3.0),
                &ink,
                0.8,
            );
        }
    }
    if let Some(time) = e.highlight {
        let x = e.x_at(time) as f32;
        if x >= left && x <= ctx.width {
            let mut y = ruler;
            while y < ctx.height {
                ctx.draw_line(
                    Vector2::new(x, y),
                    Vector2::new(x, (y + 5.0).min(ctx.height)),
                    &accent,
                    1.5,
                );
                y += 9.0;
            }
        }
    }
    if e.has_position() {
        let x = e.x_at(e.position) as f32;
        if x >= left && x <= ctx.width {
            ctx.draw_line(
                Vector2::new(x, ruler),
                Vector2::new(x, ctx.height),
                &playhead,
                1.25,
            );
        }
    }
    if !e.compact {
        let active = e.active_pitch();
        let (white, black) = e.piano_geometry();
        for key in white {
            let y = (key.y as f32).max(ruler);
            let bottom = ((key.y + key.height) as f32).min(ctx.height);
            if bottom <= y {
                continue;
            }
            let on = active == Some(key.pitch);
            let base = if on { p.accent_soft } else { p.surface };
            let end = if on { p.selection } else { p.background };
            let gradient = ctx.create_linear_gradient(
                Vector2::new(0.0, 0.0),
                Vector2::new(96.0, 0.0),
                &[
                    GradientStop::new(0.0, base.canvas()),
                    GradientStop::new(0.82, base.canvas()),
                    GradientStop::new(1.0, end.canvas()),
                ],
            )?;
            let rect = CanvasRect::new(0.0, y, 96.0, bottom);
            ctx.fill_rect(&rect, &gradient);
            ctx.draw_rect(&rect, &line, 0.8);
            if (e.low_pitch..=e.high_pitch).contains(&key.pitch)
                && (row >= 9.0 || key.pitch % 12 == 0)
            {
                ctx.draw_text(
                    &pitch_name(key.pitch),
                    &centered_font(if row >= 18.0 { 12.0 } else { 10.0 })?,
                    &CanvasRect::new(66.0, y, 90.0, bottom),
                    if key.pitch % 12 == 0 || on {
                        &accent
                    } else {
                        &ink
                    },
                );
                if key.pitch % 12 == 0 {
                    let mid = (key.y + key.height / 2.0) as f32;
                    ctx.fill_rect(
                        &CanvasRect::from_xywh(
                            91.0,
                            mid - (row * 0.34).min(7.0),
                            3.0,
                            (row * 0.68).min(14.0),
                        ),
                        &accent,
                    );
                }
            }
        }
        for key in black {
            let y = (key.y as f32).max(ruler);
            let bottom = ((key.y + key.height) as f32).min(ctx.height);
            if bottom <= y {
                continue;
            }
            let base = if active == Some(key.pitch) {
                p.accent
            } else {
                p.ink
            };
            let gradient = ctx.create_linear_gradient(
                Vector2::new(0.0, 0.0),
                Vector2::new(62.0, 0.0),
                &[
                    GradientStop::new(0.0, base.scaled(1.22)),
                    GradientStop::new(0.82, base.canvas()),
                    GradientStop::new(1.0, base.scaled(1.0 / 1.4)),
                ],
            )?;
            let rect = CanvasRect::new(0.0, y, 62.0, bottom);
            ctx.fill_rounded_rect(&RoundedRect::new(rect, 1.0, 1.0), &gradient);
            ctx.draw_rounded_rect(&RoundedRect::new(rect, 1.0, 1.0), &ink, 0.8);
            ctx.draw_line(
                Vector2::new(3.0, y + 1.5),
                Vector2::new(57.0, y + 1.5),
                &muted,
                0.75,
            );
            if row >= 13.0 {
                ctx.draw_text(
                    &pitch_name(key.pitch),
                    &font,
                    &CanvasRect::new(7.0, y + ((bottom - y - 14.0) / 2.0).max(0.0), 56.0, bottom),
                    &paper,
                );
            }
        }
        ctx.fill_rect(&CanvasRect::new(98.0, ruler, left, ctx.height), &shade);
        ctx.draw_line(
            Vector2::new(98.0, ruler),
            Vector2::new(98.0, ctx.height),
            &line,
            0.8,
        );
        for pitch in e.low_pitch..=e.high_pitch {
            let y = e.pitch_center(pitch) as f32;
            if y >= ruler && y <= ctx.height && (row >= 16.0 || pitch % 12 == 0) {
                ctx.draw_text(
                    &key_label(pitch),
                    &centered,
                    &CanvasRect::from_xywh(100.0, y - 8.0, left - 102.0, 16.0),
                    if active == Some(pitch) {
                        &accent
                    } else {
                        &muted
                    },
                );
            }
        }
    } else {
        ctx.fill_rect(&CanvasRect::new(0.0, ruler, left, ctx.height), &paper);
    }
    ctx.fill_rect(&CanvasRect::new(0.0, 0.0, ctx.width, ruler), &paper);
    if !e.compact {
        ctx.draw_text(
            "钢琴",
            &font,
            &CanvasRect::new(8.0, 6.0, 98.0, ruler),
            &muted,
        );
        ctx.draw_text(
            "按键",
            &centered,
            &CanvasRect::new(98.0, 0.0, left, ruler),
            &muted,
        );
    }
    for tick in start..=end {
        let time = tick as f64 * step;
        let x = e.x_at(time) as f32;
        if x < left || x > ctx.width {
            continue;
        }
        ctx.draw_text(
            &format!("{time}"),
            &font,
            &CanvasRect::from_xywh(x + 4.0, 6.0, 54.0, 18.0),
            &muted,
        );
    }
    if let Some(marker) = e.highlight {
        let x = e.x_at(marker) as f32;
        if x >= left && x <= ctx.width {
            ctx.fill_ellipse(
                &Ellipse::new(Vector2::new(x - 2.0, 19.0), 3.0, 2.0),
                &accent,
            );
            ctx.draw_line(
                Vector2::new(x + 1.0, 19.0),
                Vector2::new(x + 1.0, 8.0),
                &accent,
                1.3,
            );
        }
    }
    if e.has_position() {
        let x = e.x_at(e.position) as f32;
        if x >= left && x <= ctx.width {
            ctx.draw_line(Vector2::new(x, 0.0), Vector2::new(x, ruler), &playhead, 1.8);
        }
    }
    ctx.draw_line(
        Vector2::new(left, 0.0),
        Vector2::new(left, ctx.height),
        &line,
        1.0,
    );
    Ok(())
}
pub(super) fn pitch_name(pitch: i32) -> String {
    const N: [&str; 12] = [
        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
    ];
    format!("{}{}", N[pitch.rem_euclid(12) as usize], pitch / 12 - 1)
}
pub(super) fn key_label(pitch: i32) -> String {
    let Ok((code, modifiers)) = crate::schedule::mapping(pitch) else {
        return String::new();
    };
    let key = match code {
        "SC02C" => "Z",
        "SC02D" => "X",
        "SC02E" => "C",
        "SC02F" => "V",
        "SC030" => "B",
        "SC031" => "N",
        "SC032" => "M",
        "SC033" => ",",
        _ => "",
    };
    format!(
        "{}{}{}",
        if modifiers.contains(&"LButton") {
            "↓"
        } else if modifiers.contains(&"RButton") {
            "↑"
        } else {
            ""
        },
        key,
        if modifiers.contains(&"MButton") {
            "♯"
        } else {
            ""
        }
    )
}
#[derive(Default)]
pub(super) struct Timeline {
    pub(super) position: f64,
    pub(super) duration: f64,
    pub(super) width: f64,
    pub(super) highlight: Option<f64>,
    pub(super) dragging: bool,
}
impl Timeline {
    pub(super) fn at(&self, x: f64) -> f64 {
        ((x - 7.0) / (self.width - 14.0).max(1.0)).clamp(0.0, 1.0) * self.duration
    }
}
pub(super) fn draw_timeline(
    ctx: &DrawContext,
    t: &Timeline,
    p: &Palette,
) -> windows_canvas::Result<()> {
    ctx.clear(p.surface.canvas());
    let line = ctx.create_solid_brush(p.line.canvas())?;
    let accent = ctx.create_solid_brush(p.accent.canvas())?;
    let paper = ctx.create_solid_brush(p.surface.canvas())?;
    let left = 7.0;
    let right = (ctx.width - 7.0).max(left);
    let y = ctx.height / 2.0 + 5.0;
    let x = left + (t.position / t.duration.max(0.001)).clamp(0.0, 1.0) as f32 * (right - left);
    ctx.draw_line(Vector2::new(left, y), Vector2::new(right, y), &line, 1.5);
    ctx.draw_line(Vector2::new(left, y), Vector2::new(x, y), &accent, 2.0);
    ctx.fill_ellipse(&Ellipse::circle(Vector2::new(x, y), 5.0), &paper);
    ctx.draw_ellipse(&Ellipse::circle(Vector2::new(x, y), 5.0), &accent, 1.7);
    if let Some(time) = t.highlight {
        let x = left + (time / t.duration.max(0.001)).clamp(0.0, 1.0) as f32 * (right - left);
        ctx.fill_ellipse(
            &Ellipse::new(Vector2::new(x - 2.0, y - 9.0), 3.5, 2.5),
            &accent,
        );
        ctx.draw_line(
            Vector2::new(x + 1.0, y - 9.0),
            Vector2::new(x + 1.0, y - 22.0),
            &accent,
            1.5,
        );
        ctx.draw_line(
            Vector2::new(x + 1.0, y - 22.0),
            Vector2::new(x + 6.0, y - 17.0),
            &accent,
            1.5,
        );
        ctx.draw_line(
            Vector2::new(x + 6.0, y - 17.0),
            Vector2::new(x + 3.0, y - 13.0),
            &accent,
            1.5,
        );
    }
    Ok(())
}
pub(super) fn draw_qr(
    ctx: &DrawContext,
    size: usize,
    pixels: &[bool],
    background: ColorF,
) -> windows_canvas::Result<()> {
    // The WinUI swap-chain host shows white behind transparent clears.
    // Match the surrounding card while preserving the four-module quiet zone.
    ctx.clear(background);
    let ink = ctx.create_solid_brush(ColorF::from_rgb8(0, 0, 0))?;
    let unit = (ctx.width.min(ctx.height) / (size as f32 + 8.0))
        .floor()
        .max(1.0);
    let x = (ctx.width - unit * size as f32) / 2.0;
    let y = (ctx.height - unit * size as f32) / 2.0;
    for row in 0..size {
        for col in 0..size {
            if pixels[row * size + col] {
                ctx.fill_rect(
                    &CanvasRect::from_xywh(
                        x + col as f32 * unit,
                        y + row as f32 * unit,
                        unit,
                        unit,
                    ),
                    &ink,
                );
            }
        }
    }
    Ok(())
}
