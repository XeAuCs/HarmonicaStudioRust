use super::*;

fn visual_view(styled: bool) -> View {
    let border = Border::new();
    let text = TextBlock::new().text("Card");
    if styled {
        border
            .padding(Thickness::uniform(24.0))
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::uniform(8.0))
            .content(text.font_size(28.0))
    } else {
        border.content(text)
    }
}

fn theme_view(background: Option<ThemeBrush>, border_brush: Option<ThemeBrush>) -> View {
    Border::new()
        .background_optional(background)
        .border_brush_optional(border_brush)
        .content(TextBlock::new().text("Card"))
}

fn brush_view(background: Option<Brush>) -> View {
    Border::new()
        .background_optional(background)
        .content(TextBlock::new().text("Card"))
}

#[test]
fn visual_values_reject_invalid_components_before_mount() {
    for invalid in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            std::panic::catch_unwind(|| Border::new().padding(Thickness::uniform(invalid)))
                .is_err()
        );
        assert!(
            std::panic::catch_unwind(|| {
                Border::new().corner_radius(CornerRadius::uniform(invalid))
            })
            .is_err()
        );
        assert!(std::panic::catch_unwind(|| TextBlock::new().font_size(invalid)).is_err());
        assert!(std::panic::catch_unwind(|| Border::new().scale(invalid)).is_err());
    }

    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            std::panic::catch_unwind(|| Border::new().margin(Thickness::uniform(invalid))).is_err()
        );
    }

    let _ = Border::new()
        .margin(Thickness::uniform(-1.0))
        .padding(Thickness::uniform(0.0))
        .border_thickness(Thickness::uniform(0.0))
        .corner_radius(CornerRadius::uniform(0.0));
}

#[test]
fn implicit_visual_transitions_mount_update_and_clear() {
    let duration = std::time::Duration::from_secs(1);
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount(
        Border::new()
            .opacity(1.0)
            .opacity_transition(duration)
            .scale(1.0)
            .scale_transition(duration)
            .into(),
    )
    .unwrap();
    let border = pump.root().unwrap();
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderOpacityTransition),
        Some(&PropertyValue::Duration(duration))
    );
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderScaleTransition),
        Some(&PropertyValue::Duration(duration))
    );

    pump.update(
        Border::new()
            .opacity(0.2)
            .opacity_transition(duration)
            .scale(1.3)
            .scale_transition(duration)
            .into(),
    )
    .unwrap();
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::Opacity),
        Some(&PropertyValue::F64(0.2))
    );
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderScale),
        Some(&PropertyValue::F64(1.3))
    );

    pump.update(Border::new().into()).unwrap();
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderOpacityTransition),
        None
    );
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderScaleTransition),
        None
    );
}

#[test]
fn border_mount_records_struct_values_content_and_typography() {
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(visual_view(true)).unwrap();
    let border = pump.root().unwrap();
    let text = pump.runtime().node(border).unwrap().children()[0];

    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderPadding),
        Some(&PropertyValue::Thickness(Thickness::uniform(24.0)))
    );
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderCornerRadius),
        Some(&PropertyValue::CornerRadius(CornerRadius::uniform(8.0)))
    );
    assert_eq!(
        pump.runtime()
            .node(text)
            .unwrap()
            .property(PropertyId::TextBlockFontSize),
        Some(&PropertyValue::F64(28.0))
    );
}

#[test]
fn border_preserves_independent_corner_radii() {
    let radius = CornerRadius::new(1.0, 2.0, 3.0, 4.0);
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(Border::new().corner_radius(radius.clone()).into())
        .unwrap();
    let border = pump.root().unwrap();

    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderCornerRadius),
        Some(&PropertyValue::CornerRadius(radius))
    );
}

#[test]
fn visual_property_clear_and_no_op_use_the_shared_property_path() {
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(visual_view(true)).unwrap();
    let border = pump.root().unwrap();
    let text = pump.runtime().node(border).unwrap().children()[0];

    pump.update_view(visual_view(false)).unwrap();
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderPadding),
        None
    );
    assert_eq!(
        pump.runtime()
            .node(text)
            .unwrap()
            .property(PropertyId::TextBlockFontSize),
        None
    );
    let batches = pump.runtime().batches();

    pump.update_view(visual_view(false)).unwrap();
    assert_eq!(pump.runtime().batches(), batches);
}

#[test]
fn failed_visual_update_does_not_publish_struct_values() {
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(visual_view(true)).unwrap();
    let border = pump.root().unwrap();
    pump.runtime_mut().fail_at(0);

    assert!(matches!(
        pump.update_view(
            Border::new()
                .padding(Thickness::uniform(32.0))
                .content(TextBlock::new().text("Card"))
        ),
        Err(PumpError::NativeApplyFailed(_))
    ));
    assert_eq!(
        pump.tree
            .native(border)
            .properties
            .get(&PropertyId::BorderPadding),
        Some(&PropertyValue::Thickness(Thickness::uniform(24.0)))
    );
}

#[test]
fn theme_brushes_mount_as_one_grouped_style() {
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(theme_view(
        Some(ThemeBrush::CardBackground),
        Some(ThemeBrush::CardStroke),
    ))
    .unwrap();
    let border = pump.root().unwrap();
    let style = ThemeStyle::new([
        Some(ThemeBrush::CardBackground),
        Some(ThemeBrush::CardStroke),
        None,
        None,
    ]);

    assert_eq!(pump.runtime().node(border).unwrap().theme_style(), style);
    assert_eq!(
        pump.runtime()
            .commands()
            .iter()
            .flatten()
            .filter(|command| matches!(command, Command::SetThemeStyle { .. }))
            .count(),
        1
    );
    assert!(
        pump.runtime()
            .commands()
            .iter()
            .flatten()
            .all(|command| !matches!(
                command,
                Command::SetProperty {
                    property: PropertyId::BorderBackground | PropertyId::BorderBorderBrush,
                    ..
                }
            ))
    );
}

#[test]
fn theme_style_update_clear_and_no_op_are_transactional() {
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(theme_view(
        Some(ThemeBrush::CardBackground),
        Some(ThemeBrush::CardStroke),
    ))
    .unwrap();
    let border = pump.root().unwrap();

    pump.update_view(theme_view(
        Some(ThemeBrush::SolidBackground),
        Some(ThemeBrush::CardStroke),
    ))
    .unwrap();
    assert_eq!(
        pump.runtime().node(border).unwrap().theme_style(),
        ThemeStyle::new([
            Some(ThemeBrush::SolidBackground),
            Some(ThemeBrush::CardStroke),
            None,
            None,
        ])
    );

    pump.update_view(theme_view(None, None)).unwrap();
    assert_eq!(
        pump.runtime().node(border).unwrap().theme_style(),
        ThemeStyle::default()
    );
    let batches = pump.runtime().batches();
    pump.update_view(theme_view(None, None)).unwrap();
    assert_eq!(pump.runtime().batches(), batches);
}

#[test]
fn brush_transitions_switch_between_theme_property_and_inherited_values() {
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(brush_view(Some(ThemeBrush::CardBackground.into())))
        .unwrap();
    let border = pump.root().unwrap();
    let color = Color::rgb(20, 40, 60);

    pump.update_view(brush_view(Some(color.into()))).unwrap();
    assert_eq!(
        pump.runtime().node(border).unwrap().theme_style(),
        ThemeStyle::default()
    );
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderBackground),
        Some(&PropertyValue::Brush(Brush::Solid(color)))
    );
    assert!(
        pump.runtime()
            .commands()
            .last()
            .unwrap()
            .iter()
            .any(|command| matches!(
                command,
                Command::SetThemeStyle { style, .. } if style.is_empty()
            ))
    );

    let batches = pump.runtime().batches();
    pump.update_view(brush_view(Some(color.into()))).unwrap();
    assert_eq!(pump.runtime().batches(), batches);

    pump.update_view(brush_view(Some(ThemeBrush::SolidBackground.into())))
        .unwrap();
    assert_eq!(
        pump.runtime().node(border).unwrap().theme_style(),
        ThemeStyle::new([Some(ThemeBrush::SolidBackground), None, None, None])
    );
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderBackground),
        None
    );
    assert!(
        pump.runtime()
            .commands()
            .last()
            .unwrap()
            .iter()
            .any(|command| matches!(
                command,
                Command::ClearProperty {
                    property: PropertyId::BorderBackground,
                    ..
                }
            ))
    );

    pump.update_view(brush_view(None)).unwrap();
    assert_eq!(
        pump.runtime().node(border).unwrap().theme_style(),
        ThemeStyle::default()
    );
    assert_eq!(
        pump.runtime()
            .node(border)
            .unwrap()
            .property(PropertyId::BorderBackground),
        None
    );
}

#[test]
fn failed_theme_style_update_does_not_publish() {
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(theme_view(
        Some(ThemeBrush::CardBackground),
        Some(ThemeBrush::CardStroke),
    ))
    .unwrap();
    let border = pump.root().unwrap();
    let original = pump.tree.native(border).desired.theme_style();
    pump.runtime_mut().fail_at(0);

    assert!(matches!(
        pump.update_view(theme_view(
            Some(ThemeBrush::SolidBackground),
            Some(ThemeBrush::CardStroke),
        )),
        Err(PumpError::NativeApplyFailed(_))
    ));
    assert_eq!(pump.tree.native(border).desired.theme_style(), original);
    assert_eq!(pump.runtime().node(border).unwrap().theme_style(), original);
}

#[test]
fn font_family_mount_diff_no_op_and_optional_clear_share_property_path() {
    let view = |family: Option<&str>| {
        TextBlock::new()
            .text("口琴工坊")
            .font_family_optional(family)
            .font_size(14.0)
            .into()
    };
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(view(Some("Microsoft YaHei UI"))).unwrap();
    let root = pump.root().unwrap();
    let property = PropertyId::TextBlockFontFamily;
    assert_eq!(
        pump.runtime().node(root).unwrap().property(property),
        Some(&PropertyValue::Str("Microsoft YaHei UI".into()))
    );
    let batches = pump.runtime().batches();
    pump.update_view(view(Some("Microsoft YaHei UI"))).unwrap();
    assert_eq!(pump.runtime().batches(), batches);
    pump.update_view(view(Some("Segoe Fluent Icons"))).unwrap();
    assert_eq!(pump.root(), Some(root));
    assert_eq!(
        pump.runtime().node(root).unwrap().property(property),
        Some(&PropertyValue::Str("Segoe Fluent Icons".into()))
    );
    let commands = pump.runtime().commands().last().unwrap();
    assert_eq!(commands.len(), 1);
    assert!(
        matches!(&commands[0], Command::SetProperty { property: PropertyId::TextBlockFontFamily, value: PropertyValue::Str(value), .. } if value == "Segoe Fluent Icons")
    );
    pump.update_view(view(None)).unwrap();
    assert_eq!(pump.runtime().node(root).unwrap().property(property), None);
    assert_eq!(
        pump.runtime()
            .node(root)
            .unwrap()
            .property(PropertyId::TextBlockFontSize),
        Some(&PropertyValue::F64(14.0))
    );
    assert!(
        pump.runtime()
            .commands()
            .last()
            .unwrap()
            .iter()
            .any(|command| matches!(
                command,
                Command::ClearProperty {
                    property: PropertyId::TextBlockFontFamily,
                    ..
                }
            ))
    );
    let batches = pump.runtime().batches();
    pump.update_view(view(None)).unwrap();
    assert_eq!(pump.runtime().batches(), batches);
}

#[test]
fn omitted_font_family_clears_explicit_font_without_replacing_text_block() {
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount(TextBlock::new().text("Font").font_family("Consolas").into())
        .unwrap();
    let root = pump.root().unwrap();
    pump.update(TextBlock::new().text("Font").into()).unwrap();
    assert_eq!(pump.root(), Some(root));
    assert_eq!(
        pump.runtime()
            .node(root)
            .unwrap()
            .property(PropertyId::TextBlockFontFamily),
        None
    );
}

#[test]
fn failed_font_family_update_does_not_publish_changed_font() {
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount(TextBlock::new().font_family("Microsoft YaHei UI").into())
        .unwrap();
    let root = pump.root().unwrap();
    pump.runtime_mut().fail_at(0);
    assert!(matches!(
        pump.update(TextBlock::new().font_family("Segoe Fluent Icons").into()),
        Err(PumpError::NativeApplyFailed(_))
    ));
    let expected = PropertyValue::Str("Microsoft YaHei UI".into());
    assert_eq!(
        pump.tree
            .native(root)
            .properties
            .get(&PropertyId::TextBlockFontFamily),
        Some(&expected)
    );
    assert_eq!(
        pump.runtime()
            .node(root)
            .unwrap()
            .property(PropertyId::TextBlockFontFamily),
        Some(&expected)
    );
}

#[test]
fn number_box_small_change_mount_replace_clear_and_no_op() {
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(std::panic::catch_unwind(|| NumberBox::new().small_change(invalid)).is_err());
    }
    let view = |step: Option<f64>| NumberBox::new().value(Some(1.0)).small_change(step).into();
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(view(Some(0.05))).unwrap();
    let root = pump.root().unwrap();
    let property = PropertyId::NumberBoxSmallChange;
    assert_eq!(
        pump.runtime().node(root).unwrap().property(property),
        Some(&PropertyValue::F64(0.05))
    );
    let batches = pump.runtime().batches();
    pump.update_view(view(Some(0.05))).unwrap();
    assert_eq!(pump.runtime().batches(), batches);
    pump.update_view(view(Some(1.0))).unwrap();
    assert_eq!(pump.root(), Some(root));
    assert_eq!(
        pump.runtime().node(root).unwrap().property(property),
        Some(&PropertyValue::F64(1.0))
    );
    let commands = pump.runtime().commands().last().unwrap();
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0],
        Command::SetProperty {
            property: PropertyId::NumberBoxSmallChange,
            value: PropertyValue::F64(1.0),
            ..
        }
    ));
    pump.update_view(view(None)).unwrap();
    assert_eq!(pump.runtime().node(root).unwrap().property(property), None);
    assert!(
        pump.runtime()
            .commands()
            .last()
            .unwrap()
            .iter()
            .any(|command| matches!(
                command,
                Command::ClearProperty {
                    property: PropertyId::NumberBoxSmallChange,
                    ..
                }
            ))
    );
    let batches = pump.runtime().batches();
    pump.update_view(view(None)).unwrap();
    assert_eq!(pump.runtime().batches(), batches);
    pump.update_view(view(Some(0.05))).unwrap();
    pump.update(NumberBox::new().value(Some(1.0)).into())
        .unwrap();
    assert_eq!(pump.runtime().node(root).unwrap().property(property), None);
}
