use super::*;

pub(super) fn label(value: impl Into<String>, size: f64, p: &Palette) -> TextBlock {
    TextBlock::new()
        .text(value)
        .font_family("Microsoft YaHei UI, Segoe UI")
        .font_size(size)
        .foreground(p.ink.native())
}
pub(super) fn logo(size: f64) -> View {
    Image::new()
        .source_file(crate::paths::resource_root().join("assets/studio.png"))
        .map(|i| i.width(size).height(size).into())
        .unwrap_or_else(|_| View::empty())
}
pub(super) fn card(p: &Palette) -> Border {
    Border::new()
        .background(p.surface.native())
        .border_brush(p.line.native())
        .border_thickness(1.0)
        .corner_radius(2.0)
        .padding(Thickness::new(18.0, 16.0, 18.0, 16.0))
}
pub(super) fn ui_button(
    context: &ViewContext<Studio>,
    title: &str,
    message: Message,
    enabled: bool,
    accent: bool,
    p: &Palette,
) -> View {
    Button::new()
        .resource_overrides(button_resources(p, accent))
        .height(37.0)
        .is_enabled(enabled)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .horizontal_content_alignment(HorizontalAlignment::Center)
        .on_click(context.message(message))
        .content(label(title, 13.0, p).foreground(if !enabled {
            p.muted.native()
        } else if accent {
            p.accent_ink.native()
        } else {
            p.ink.native()
        }))
}
pub(super) fn check(
    context: &ViewContext<Studio>,
    title: &str,
    value: bool,
    message: fn(bool) -> Message,
    enabled: bool,
    p: &Palette,
) -> View {
    CheckBox::new()
        .min_height(25.0)
        .is_checked(value)
        .is_enabled(enabled)
        .on_is_checked_changed(context.callback(message))
        .content(label(title, 13.0, p).text_wrapping(TextWrapping::Wrap))
}
pub(super) fn table_row(values: [String; 4], p: &Palette, heading: bool) -> View {
    Grid::new()
        .columns([
            GridLength::STAR,
            GridLength::Pixel(52.0),
            GridLength::Pixel(72.0),
            GridLength::Pixel(72.0),
        ])
        .keyed_children(values.into_iter().enumerate().map(|(i, v)| {
            (
                i.to_string(),
                label(v, 13.0, p)
                    .font_weight(if heading {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_trimming(TextTrimming::CharacterEllipsis)
                    .grid_column(i as i32)
                    .margin(Thickness::xy(8.0, 7.0)),
            )
        }))
}
pub(super) fn tab_button(
    context: &ViewContext<Studio>,
    text: &str,
    editor: bool,
    selected: bool,
    p: &Palette,
) -> View {
    Border::new()
        .border_brush(if editor == selected {
            p.accent.native()
        } else {
            Color::argb(0, 0, 0, 0)
        })
        .border_thickness(Thickness::new(0.0, 0.0, 0.0, 2.0))
        .content(
            Button::new()
                .resource_overrides(
                    button_resources(p, false)
                        .set("ButtonBackground", p.background.native())
                        .set("ButtonBorderThickness", Thickness::uniform(0.0))
                        .set("ButtonPadding", Thickness::xy(14.0, 8.0)),
                )
                .on_click(context.message(Message::SelectTab(editor)))
                .content(label(text, 13.0, p)),
        )
}
pub(super) fn icon_button(
    context: &ViewContext<Studio>,
    title: &str,
    settings: bool,
    active: bool,
    message: Message,
    p: &Palette,
) -> View {
    let native_icon: View = SymbolIcon::new()
        .symbol(if settings {
            // A wrench is clearer at this small size than the font gear and
            // stays inside the same native icon geometry as the phone icon.
            Symbol::Repair
        } else {
            Symbol::CellPhone
        })
        .width(24.0)
        .height(24.0)
        .into();
    let icon = Grid::new().width(24.0).height(24.0).children((
        native_icon,
        if active {
            windows_reactor::Ellipse::new()
                .width(6.0)
                .height(6.0)
                .margin(Thickness::new(0.0, 0.0, 1.0, 1.0))
                .horizontal_alignment(HorizontalAlignment::Right)
                .vertical_alignment(VerticalAlignment::Top)
                .fill(p.accent.native())
                .into()
        } else {
            View::empty()
        },
    ));
    // Do not put these glyphs inside a WinUI Button template.  At non-100%
    // DPI the template's ContentPresenter can round its content slot smaller
    // than the declared icon size, which clips the right side of both glyphs.
    // The Border keeps the same visual affordance while giving the icon a
    // direct, fixed layout box and routes the mouse release to the component.
    let click_message = message.clone();
    Border::new()
        .width(40.0)
        .height(40.0)
        .background(p.background.native())
        .border_brush(p.line.native())
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(2.0))
        .allow_focus_on_interaction(true)
        .focus_on_pointer_release(true)
        .on_pointer_released(context.callback(move |_| click_message.clone()))
        .content(icon)
        .tooltip(title)
        .into()
}
pub(super) fn ornament(canvas_theme: &Rc<RefCell<Palette>>, invalidator: &Invalidator) -> View {
    let palette = Rc::clone(canvas_theme);
    Border::new()
        .height(24.0)
        .content(windows_canvas::Canvas::invalidated(
            invalidator,
            move |ctx| {
                let p = palette.borrow();
                ctx.clear(p.surface.canvas());
                let line = ctx.create_solid_brush(p.line.canvas())?;
                let accent = ctx.create_solid_brush(p.accent.canvas())?;
                let middle = ctx.width / 2.0;
                for y in [6.0, 12.0, 18.0] {
                    ctx.draw_line(
                        Vector2::new(16.0, y),
                        Vector2::new(middle - 46.0, y),
                        &line,
                        0.8,
                    );
                    ctx.draw_line(
                        Vector2::new(middle + 46.0, y),
                        Vector2::new(ctx.width - 16.0, y),
                        &line,
                        0.8,
                    );
                }
                for (offset, y) in [(-22.0, 16.0), (0.0, 10.0), (22.0, 13.0)] {
                    ctx.fill_ellipse(
                        &Ellipse::new(Vector2::new(middle + offset, y), 3.0, 2.0),
                        &accent,
                    );
                    ctx.draw_line(
                        Vector2::new(middle + offset + 3.0, y),
                        Vector2::new(middle + offset + 3.0, y - 9.0),
                        &accent,
                        1.2,
                    );
                }
                Ok(())
            },
        ))
}
