use super::*;

#[test]
fn resource_overrides_mount_update_and_clear() {
    let initial = ResourceOverrides::new()
        .set("ButtonBackground", Color::rgb(178, 34, 34))
        .set("ButtonBorderThemeThickness", Thickness::uniform(0.0));
    let replacement = ResourceOverrides::new()
        .set("ButtonForeground", Color::rgb(255, 255, 255))
        .set("ControlCornerRadius", CornerRadius::uniform(8.0));
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount(Button::new().resource_overrides(initial.clone()).into())
        .unwrap();
    let root = pump.root().unwrap();
    assert_eq!(
        pump.runtime()
            .node(root)
            .unwrap()
            .property(PropertyId::ButtonResources),
        Some(&PropertyValue::ResourceOverrides(initial))
    );

    pump.update(Button::new().resource_overrides(replacement.clone()).into())
        .unwrap();
    assert_eq!(
        pump.runtime()
            .node(root)
            .unwrap()
            .property(PropertyId::ButtonResources),
        Some(&PropertyValue::ResourceOverrides(replacement))
    );

    pump.update(Button::new().into()).unwrap();
    assert_eq!(
        pump.runtime()
            .node(root)
            .unwrap()
            .property(PropertyId::ButtonResources),
        None
    );
}

#[test]
fn border_font_and_dimension_resources_mount_replace_clear_and_no_op() {
    let initial = ResourceOverrides::new()
        .set(
            "ContentControlThemeFontFamily",
            ResourceValue::FontFamily("Microsoft YaHei UI".into()),
        )
        .set("ControlContentThemeFontSize", ResourceValue::Double(13.0))
        .set("ButtonMinHeight", ResourceValue::Double(0.0));
    let replacement = ResourceOverrides::new()
        .set(
            "ContentControlThemeFontFamily",
            ResourceValue::FontFamily("Segoe UI".into()),
        )
        .set("ControlContentThemeFontSize", ResourceValue::Double(15.0));
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount_view(
        Border::new()
            .resource_overrides(initial.clone())
            .content(TextBlock::new().text("口琴工坊")),
    )
    .unwrap();
    let root = pump.root().unwrap();
    let child = pump.runtime().node(root).unwrap().children()[0];
    assert_eq!(
        pump.runtime()
            .node(root)
            .unwrap()
            .property(PropertyId::BorderResources),
        Some(&PropertyValue::ResourceOverrides(initial.clone()))
    );
    let batches = pump.runtime().batches();
    pump.update_view(
        Border::new()
            .resource_overrides(initial)
            .content(TextBlock::new().text("口琴工坊")),
    )
    .unwrap();
    assert_eq!(pump.runtime().batches(), batches);
    pump.update_view(
        Border::new()
            .resource_overrides(replacement.clone())
            .content(TextBlock::new().text("口琴工坊")),
    )
    .unwrap();
    assert_eq!(pump.root(), Some(root));
    assert_eq!(pump.runtime().node(root).unwrap().children(), &[child]);
    assert_eq!(
        pump.runtime()
            .node(root)
            .unwrap()
            .property(PropertyId::BorderResources),
        Some(&PropertyValue::ResourceOverrides(replacement))
    );
    let commands = pump.runtime().commands().last().unwrap();
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0],
        Command::SetProperty {
            property: PropertyId::BorderResources,
            ..
        }
    ));
    pump.update_view(Border::new().content(TextBlock::new().text("口琴工坊")))
        .unwrap();
    assert_eq!(
        pump.runtime()
            .node(root)
            .unwrap()
            .property(PropertyId::BorderResources),
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
                    property: PropertyId::BorderResources,
                    ..
                }
            ))
    );
    let batches = pump.runtime().batches();
    pump.update_view(Border::new().content(TextBlock::new().text("口琴工坊")))
        .unwrap();
    assert_eq!(pump.runtime().batches(), batches);
}

#[test]
fn resource_dimensions_reject_negative_or_non_finite_values_before_mount() {
    for invalid in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            std::panic::catch_unwind(
                || ResourceOverrides::new().set("ButtonMinHeight", ResourceValue::Double(invalid))
            )
            .is_err()
        );
    }
    assert!(
        std::panic::catch_unwind(|| ResourceOverrides::new().set(
            "ContentControlThemeFontFamily",
            ResourceValue::FontFamily(String::new())
        ))
        .is_err()
    );
    let resources = ResourceOverrides::new()
        .set("ButtonMinHeight", ResourceValue::Double(0.0))
        .set("ControlContentThemeFontSize", ResourceValue::Double(13.5));
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount(Border::new().resource_overrides(resources).into())
        .unwrap();
}

#[test]
fn failed_border_resource_update_keeps_previous_font_and_dimensions() {
    let initial = ResourceOverrides::new()
        .set("ControlContentThemeFontSize", ResourceValue::Double(13.0))
        .set(
            "ContentControlThemeFontFamily",
            ResourceValue::FontFamily("Microsoft YaHei UI".into()),
        );
    let mut pump = Pump::new(RecordingRuntime::default());
    pump.mount(Border::new().resource_overrides(initial.clone()).into())
        .unwrap();
    let root = pump.root().unwrap();
    pump.runtime_mut().fail_at(0);
    assert!(matches!(
        pump.update(
            Border::new()
                .resource_overrides(
                    ResourceOverrides::new()
                        .set("ControlContentThemeFontSize", ResourceValue::Double(18.0))
                )
                .into()
        ),
        Err(PumpError::NativeApplyFailed(_))
    ));
    let expected = PropertyValue::ResourceOverrides(initial);
    assert_eq!(
        pump.tree
            .native(root)
            .properties
            .get(&PropertyId::BorderResources),
        Some(&expected)
    );
    assert_eq!(
        pump.runtime()
            .node(root)
            .unwrap()
            .property(PropertyId::BorderResources),
        Some(&expected)
    );
}
