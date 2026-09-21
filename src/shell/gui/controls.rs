use super::*;

pub(super) fn label(value: impl Into<String>, size: f64, p: &Palette) -> TextBlock {
    TextBlock::new()
        .text(value)
        .font_family("Microsoft YaHei UI, Segoe UI")
        .font_size(size)
        .foreground(p.ink.native())
}
pub(super) fn logo(size: f64) -> View {
    // WinUI file-URI loading can silently fail after portable relocation.
    // Keep the small branding bitmap with the executable, independent of its path.
    Image::new()
        .source_data(EncodedImage::from_static(include_bytes!(
            "../../../assets/studio.png"
        )))
        .width(size)
        .height(size)
        .into()
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
pub(super) fn table_row(values: [String; 5], p: &Palette, heading: bool) -> View {
    Grid::new()
        .columns([
            GridLength::STAR,
            GridLength::Pixel(64.0),
            GridLength::Pixel(112.0),
            GridLength::Pixel(88.0),
            GridLength::Pixel(80.0),
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
                    .text_trimming(if i == 0 {
                        TextTrimming::CharacterEllipsis
                    } else {
                        TextTrimming::None
                    })
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
    let active = editor == selected;
    let transparent = Color::argb(0, 0, 0, 0);
    Grid::new().children((
        Button::new()
            .style(ButtonStyle::Subtle)
            .height(42.0)
            .resource_overrides(
                button_resources(p, false)
                    .set("ButtonBackground", transparent)
                    .set("ButtonBackgroundPointerOver", p.surface.native())
                    .set("ButtonBackgroundPressed", p.selection.native())
                    .set("ButtonBorderBrush", transparent)
                    .set("ButtonBorderBrushPointerOver", transparent)
                    .set("ButtonBorderBrushPressed", transparent)
                    .set("ButtonBorderThemeThickness", Thickness::uniform(0.0))
                    .set("ButtonBorderThickness", Thickness::uniform(0.0))
                    .set("ButtonPadding", Thickness::xy(16.0, 8.0)),
            )
            .on_click(context.message(Message::SelectTab(editor)))
            .content(
                label(text, 14.0, p)
                    .font_weight(if active {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .foreground(if active {
                        p.ink.native()
                    } else {
                        p.muted.native()
                    }),
            ),
        Border::new()
            .width(24.0)
            .height(3.0)
            .corner_radius(1.5)
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Bottom)
            .background(if active {
                p.accent.native()
            } else {
                transparent
            }),
    ))
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
