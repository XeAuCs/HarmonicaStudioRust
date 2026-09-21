use super::*;

impl Rgb {
    pub(super) fn native(self) -> Color {
        let (r, g, b) = self.channels();
        Color::rgb(r, g, b)
    }
    pub(super) fn canvas(self) -> ColorF {
        let (r, g, b) = self.channels();
        ColorF::from_rgb8(r, g, b)
    }
    pub(super) fn scaled(self, f: f32) -> ColorF {
        let (r, g, b) = self.channels();
        ColorF::from_rgb8(
            (f32::from(r) * f).min(255.0) as u8,
            (f32::from(g) * f).min(255.0) as u8,
            (f32::from(b) * f).min(255.0) as u8,
        )
    }
}
pub(super) fn theme_resources(p: &Palette) -> ResourceOverrides {
    let transparent = Color::argb(0, 0, 0, 0);
    let mut resources = ResourceOverrides::new()
        .set(
            "ContentControlThemeFontFamily",
            ResourceValue::FontFamily("Microsoft YaHei UI, Segoe UI".into()),
        )
        .set("ControlContentThemeFontSize", ResourceValue::Double(13.0))
        .set("ControlCornerRadius", CornerRadius::uniform(2.0))
        .set("ControlBorderThemeThickness", Thickness::uniform(1.0))
        .set("TextControlCornerRadius", CornerRadius::uniform(2.0))
        .set("TextControlThemePadding", Thickness::xy(8.0, 5.0))
        .set("ComboBoxThemePadding", Thickness::xy(8.0, 5.0))
        .set("CheckBoxBorderThemeThickness", ResourceValue::Double(1.0))
        // WinUI 3's current templates use this key (the older ThemeThickness
        // name is retained above for compatibility with older templates).
        .set("CheckBoxBorderThickness", ResourceValue::Double(1.0));

    // Most WinUI controls are driven by the common Fluent resource ramp.  The
    // app has its own palette, so override that ramp as well as the
    // control-specific aliases below.  Without these aliases, a template can
    // silently fall back to the system blue/purple accent.
    //
    // All brushes below come from the shared Palette. No fixed blue, purple,
    // or green values are used here; Canvas drawing uses the same Palette.
    for key in [
        "AccentFillColorDefaultBrush",
        "AccentFillColorSecondaryBrush",
        "AccentFillColorTertiaryBrush",
        "AccentFillColorSelectedTextBackgroundBrush",
        "AccentControlElevationBorderBrush",
        "SystemControlForegroundAccentBrush",
        "SystemControlBackgroundAccentBrush",
        "SystemControlHighlightAccentBrush",
        "SystemControlHighlightAltAccentBrush",
        "AccentButtonBackground",
        "AccentButtonBorderBrush",
        "AccentButtonBorderBrushPointerOver",
        "AccentButtonBorderBrushPressed",
        "ProgressBarForeground",
        "ProgressBarForegroundPointerOver",
        "ProgressBarForegroundPressed",
        "CheckBoxCheckBackgroundFillChecked",
        "CheckBoxCheckBackgroundFillCheckedPointerOver",
        "CheckBoxCheckBackgroundFillIndeterminate",
        "CheckBoxCheckBackgroundFillIndeterminatePointerOver",
        "CheckBoxCheckBackgroundStrokeChecked",
        "CheckBoxCheckBackgroundStrokeCheckedPointerOver",
        "CheckBoxCheckBackgroundStrokeIndeterminate",
        "CheckBoxCheckBackgroundStrokeIndeterminatePointerOver",
        "SliderTrackValueFill",
        "SliderTrackValueFillPointerOver",
        "SliderThumbBackground",
        "SliderThumbBackgroundPointerOver",
        "FocusStrokeColorOuterBrush",
        // WinUI 3 renders the selected ComboBox item with this narrow pill
        // on the left. It is independent from the selected-item background.
        "ComboBoxItemPillFillBrush",
        // ListViewItem uses a separate left selection indicator brush too.
        "ListViewItemSelectionIndicatorBrush",
    ] {
        resources = resources.set(key, p.accent.native());
    }
    for key in [
        "AccentFillColorDisabledBrush",
        "AccentTextFillColorDisabledBrush",
        "SystemControlDisabledAccentBrush",
        "AccentButtonForegroundDisabled",
        "ControlFillColorDisabledBrush",
        "ControlStrongFillColorDisabledBrush",
        "ControlAltFillColorDisabledBrush",
        "ControlStrongStrokeColorDisabledBrush",
        "TextFillColorDisabledBrush",
        "TextOnAccentFillColorDisabledBrush",
        "CheckBoxCheckBackgroundFillCheckedDisabled",
        "CheckBoxCheckBackgroundFillIndeterminateDisabled",
        "CheckBoxCheckBackgroundStrokeCheckedDisabled",
        "CheckBoxCheckBackgroundStrokeIndeterminateDisabled",
        "SliderTrackValueFillDisabled",
        "SliderThumbBackgroundDisabled",
    ] {
        resources = resources.set(key, p.muted.native());
    }
    for key in [
        "ControlFillColorDefaultBrush",
        "ControlFillColorInputActiveBrush",
        "ControlStrongFillColorDefaultBrush",
        "ControlSolidFillColorDefaultBrush",
        "ControlAltFillColorSecondaryBrush",
        "LayerFillColorDefaultBrush",
        "CardBackgroundFillColorDefaultBrush",
        "SolidBackgroundFillColorBaseBrush",
        "AccentButtonBackgroundDisabled",
        "TextControlBackground",
        "TextControlBackgroundFocused",
        "ComboBoxBackground",
        "ComboBoxBackgroundUnfocused",
        "ComboBoxBackgroundFocused",
        "ListViewItemBackground",
        "ListViewBackground",
        "CheckBoxCheckBackgroundFillUnchecked",
        "CheckBoxCheckBackgroundFillUncheckedPointerOver",
        "CheckBoxCheckBackgroundFillUncheckedPressed",
        "CheckBoxCheckBackgroundFillUncheckedDisabled",
        "TextControlBackgroundDisabled",
        "ComboBoxBackgroundDisabled",
    ] {
        resources = resources.set(key, p.surface.native());
    }
    for key in [
        "ControlFillColorSecondaryBrush",
        "ControlAltFillColorTertiaryBrush",
        "SubtleFillColorSecondaryBrush",
        "AccentButtonBackgroundPointerOver",
        "ListViewItemBackgroundPointerOver",
        "ComboBoxItemBackgroundPointerOver",
        "ComboBoxItemBackgroundSelectedPointerOver",
        "ComboBoxBackgroundPointerOver",
        "TextControlBackgroundPointerOver",
        "CheckBoxCheckBackgroundFillCheckedPointerOver",
        "CheckBoxCheckBackgroundFillIndeterminatePointerOver",
        "SliderTrackValueFillPointerOver",
        "SliderThumbBackgroundPointerOver",
    ] {
        resources = resources.set(key, p.accent_soft.native());
    }
    for key in [
        "ControlFillColorTertiaryBrush",
        "ControlFillColorQuarternaryBrush",
        "ControlAltFillColorQuarternaryBrush",
        "SubtleFillColorTertiaryBrush",
        "AccentButtonBackgroundPressed",
        "ComboBoxBackgroundPressed",
        "CheckBoxCheckBackgroundFillCheckedPressed",
        "CheckBoxCheckBackgroundFillIndeterminatePressed",
        "SliderTrackValueFillPressed",
        "SliderThumbBackgroundPressed",
    ] {
        resources = resources.set(key, p.selection.native());
    }
    for key in [
        "ControlStrongStrokeColorDefaultBrush",
        "ControlStrokeColorDefaultBrush",
        "AccentButtonBorderBrushDisabled",
        "ScrollBarThumbFill",
        "SliderTickBarFill",
        "TextControlBorderBrush",
        "TextControlBorderBrushPointerOver",
        "TextControlBorderBrushFocused",
        "ComboBoxBorderBrush",
        "ComboBoxBorderBrushPointerOver",
        "ComboBoxBorderBrushPressed",
        "ComboBoxBorderBrushDisabled",
        "CheckBoxCheckBackgroundStrokeUnchecked",
        "CheckBoxCheckBackgroundStrokeUncheckedPointerOver",
        "CheckBoxCheckBackgroundStrokeUncheckedPressed",
        "SliderTrackFill",
        "SliderTrackFillPointerOver",
        "SliderTrackFillPressed",
    ] {
        resources = resources.set(key, p.line.native());
    }
    for key in [
        "TextFillColorPrimaryBrush",
        "AccentButtonForegroundPointerOver",
        "AccentButtonForegroundPressed",
        "TextControlForeground",
        "TextControlForegroundPointerOver",
        "TextControlForegroundFocused",
        "ComboBoxForeground",
        "ComboBoxForegroundPointerOver",
        "ComboBoxForegroundFocused",
        "ComboBoxItemForeground",
        "ComboBoxItemForegroundPointerOver",
        "ComboBoxItemForegroundSelected",
        "ComboBoxItemForegroundSelectedUnfocused",
        "ListViewItemForeground",
        "CheckBoxForegroundUnchecked",
        "CheckBoxForegroundUncheckedPointerOver",
        "CheckBoxForegroundUncheckedPressed",
        "CheckBoxForegroundChecked",
        "CheckBoxForegroundCheckedPointerOver",
        "CheckBoxForegroundCheckedPressed",
        "CheckBoxForegroundIndeterminate",
        "CheckBoxForegroundIndeterminatePointerOver",
        "CheckBoxForegroundIndeterminatePressed",
    ] {
        resources = resources.set(key, p.ink.native());
    }
    for key in [
        "TextFillColorSecondaryBrush",
        "TextFillColorTertiaryBrush",
        "TextFillColorDisabledBrush",
    ] {
        resources = resources.set(key, p.muted.native());
    }
    for key in [
        "TextFillColorInverseBrush",
        "AccentButtonForeground",
        "AccentTextFillColorPrimaryBrush",
        "AccentTextFillColorSecondaryBrush",
        "AccentTextFillColorTertiaryBrush",
        "TextOnAccentFillColorSelectedTextBrush",
        "TextOnAccentFillColorPrimaryBrush",
        "TextOnAccentFillColorSecondaryBrush",
        "CheckBoxCheckGlyphForegroundChecked",
        "CheckBoxCheckGlyphForegroundCheckedPointerOver",
        "CheckBoxCheckGlyphForegroundCheckedPressed",
        "CheckBoxCheckGlyphForegroundIndeterminate",
        "CheckBoxCheckGlyphForegroundIndeterminatePointerOver",
        "CheckBoxCheckGlyphForegroundIndeterminatePressed",
    ] {
        resources = resources.set(key, p.accent_ink.native());
    }
    for key in [
        "TextControlForegroundDisabled",
        "ProgressBarForegroundDisabled",
        "ScrollBarThumbFillPointerOver",
        "ScrollBarThumbFillPressed",
        "ScrollBarThumbFillDisabled",
        "TextControlPlaceholderForeground",
        "TextControlPlaceholderForegroundPointerOver",
        "TextControlPlaceholderForegroundFocused",
        "TextControlPlaceholderForegroundDisabled",
        "ComboBoxForegroundPressed",
        "ComboBoxForegroundDisabled",
        "ComboBoxPlaceHolderForeground",
        "ComboBoxPlaceHolderForegroundPointerOver",
        "ComboBoxPlaceHolderForegroundPressed",
        "ComboBoxPlaceHolderForegroundDisabled",
        "ComboBoxPlaceHolderForegroundFocused",
        "ComboBoxDropDownGlyphForeground",
        "ComboBoxEditableDropDownGlyphForeground",
        "ComboBoxItemForegroundPressed",
        "ComboBoxItemForegroundDisabled",
        "ComboBoxItemForegroundSelectedPressed",
        "ComboBoxItemForegroundSelectedDisabled",
        "CheckBoxForegroundUncheckedDisabled",
        "CheckBoxForegroundCheckedDisabled",
        "CheckBoxForegroundIndeterminateDisabled",
        "CheckBoxCheckGlyphForegroundUncheckedDisabled",
        "CheckBoxCheckGlyphForegroundCheckedDisabled",
        "CheckBoxCheckGlyphForegroundIndeterminateDisabled",
    ] {
        resources = resources.set(key, p.muted.native());
    }
    for key in [
        "TextControlButtonBackgroundPointerOver",
        "TextControlButtonBackgroundPressed",
        "TextControlButtonBorderBrush",
        "TextControlButtonBorderBrushPointerOver",
        "TextControlButtonBorderBrushPressed",
        "TextControlButtonForeground",
    ] {
        resources = resources.set(key, p.muted.native());
    }
    for key in [
        "ControlFillColorTransparentBrush",
        "ControlAltFillColorTransparentBrush",
        "SubtleFillColorTransparentBrush",
        "CheckBoxBackgroundUnchecked",
        "CheckBoxBackgroundUncheckedPointerOver",
        "CheckBoxBackgroundUncheckedPressed",
        "CheckBoxBackgroundUncheckedDisabled",
        "CheckBoxBackgroundChecked",
        "CheckBoxBackgroundCheckedPointerOver",
        "CheckBoxBackgroundCheckedPressed",
        "CheckBoxBackgroundCheckedDisabled",
        "CheckBoxBackgroundIndeterminate",
        "CheckBoxBackgroundIndeterminatePointerOver",
        "CheckBoxBackgroundIndeterminatePressed",
        "CheckBoxBackgroundIndeterminateDisabled",
        "CheckBoxBorderBrushUnchecked",
        "CheckBoxBorderBrushUncheckedPointerOver",
        "CheckBoxBorderBrushUncheckedPressed",
        "CheckBoxBorderBrushUncheckedDisabled",
        "CheckBoxBorderBrushChecked",
        "CheckBoxBorderBrushCheckedPointerOver",
        "CheckBoxBorderBrushCheckedPressed",
        "CheckBoxBorderBrushCheckedDisabled",
        "CheckBoxBorderBrushIndeterminate",
        "CheckBoxBorderBrushIndeterminatePointerOver",
        "CheckBoxBorderBrushIndeterminatePressed",
        "CheckBoxBorderBrushIndeterminateDisabled",
        "ComboBoxItemBackground",
        "ComboBoxItemBackgroundDisabled",
        "ComboBoxItemBorderBrush",
        "ComboBoxItemBorderBrushDisabled",
    ] {
        resources = resources.set(key, transparent);
    }
    for key in [
        "ComboBoxItemBackgroundPressed",
        "ComboBoxItemBackgroundSelected",
        "ComboBoxItemBackgroundSelectedUnfocused",
        "ComboBoxItemBackgroundSelectedPressed",
        "ComboBoxItemBorderBrushPressed",
        "ComboBoxItemBorderBrushSelected",
        "ComboBoxItemBorderBrushSelectedPressed",
        "ComboBoxItemBorderBrushSelectedPointerOver",
        "ListViewItemBackgroundSelected",
        "ListViewItemBackgroundSelectedPointerOver",
        "ListViewItemBackgroundSelectedPressed",
        "ListViewItemSelectedBackgroundThemeBrush",
        "TextControlSelectionHighlightColor",
        "ComboBoxSelectedBackgroundThemeBrush",
        "ComboBoxSelectedPointerOverBackgroundThemeBrush",
    ] {
        resources = resources.set(key, p.selection.native());
    }
    for key in [
        "FocusStrokeColorInnerBrush",
        "TextControlElevationBorderBrushFocused",
        "TextControlElevationBorderBrush",
    ] {
        resources = resources.set(key, p.accent_soft.native());
    }
    // Progress and page surfaces also stay inside the shared Palette so a
    // stale system brush cannot reintroduce blue on determinate bars.
    // ScrollViewer tracks follow the page so a missing key cannot fall back
    // to the previous theme or the system accent.
    for (key, value) in [
        ("ProgressBarBackground", p.grid.native()),
        ("ProgressBarBackgroundDisabled", p.grid.native()),
        ("ScrollBarTrackFill", p.background.native()),
        ("ScrollBarTrackFillPointerOver", p.background.native()),
        ("ScrollBarTrackFillPressed", p.background.native()),
        ("ScrollBarTrackFillDisabled", p.background.native()),
        ("SystemControlPageBackgroundBrush", p.background.native()),
    ] {
        resources = resources.set(key, value);
    }
    // Legacy aliases are still used by a few WinUI 3 templates and by the
    // down-level NumberBox/TextBox parts.
    for (key, value) in [
        ("ApplicationPageBackgroundThemeBrush", p.background.native()),
        ("ButtonBackgroundThemeBrush", p.surface.native()),
        ("ButtonBorderThemeBrush", p.line.native()),
        ("ButtonDisabledBackgroundThemeBrush", p.background.native()),
        ("ButtonDisabledBorderThemeBrush", p.line.native()),
        ("ButtonDisabledForegroundThemeBrush", p.muted.native()),
        ("ButtonForegroundThemeBrush", p.ink.native()),
        (
            "ButtonPointerOverBackgroundThemeBrush",
            p.accent_soft.native(),
        ),
        ("ButtonPointerOverForegroundThemeBrush", p.ink.native()),
        ("ButtonPressedBackgroundThemeBrush", p.selection.native()),
        ("ButtonPressedForegroundThemeBrush", p.ink.native()),
        ("TextBoxBackgroundThemeBrush", p.surface.native()),
        ("TextBoxBorderThemeBrush", p.line.native()),
        ("TextBoxDisabledBackgroundThemeBrush", p.background.native()),
        ("TextBoxDisabledBorderThemeBrush", p.line.native()),
        ("TextBoxDisabledForegroundThemeBrush", p.muted.native()),
        ("TextBoxForegroundThemeBrush", p.ink.native()),
    ] {
        resources = resources.set(key, value);
    }
    resources
}
pub(super) fn button_resources(p: &Palette, accent: bool) -> ResourceOverrides {
    let bg = if accent { p.accent } else { p.surface };
    let fg = if accent { p.accent_ink } else { p.ink };
    // A Button resolves ThemeResource first in its own Resources dictionary.
    // Popups such as the library list live outside the root themed Border, so
    // every Button carries the same accent/system aliases as theme_resources().
    // All values come from the shared Palette; no fixed system blue is used.
    ResourceOverrides::new()
        .set("ButtonBackground", bg.native())
        .set("ButtonBackgroundPointerOver", p.accent_soft.native())
        .set("ButtonBackgroundPressed", p.selection.native())
        .set("ButtonBackgroundDisabled", p.surface.native())
        .set("ButtonForeground", fg.native())
        .set("ButtonForegroundPointerOver", p.ink.native())
        .set("ButtonForegroundPressed", p.ink.native())
        .set("ButtonForegroundDisabled", p.muted.native())
        .set(
            "ButtonBorderBrush",
            if accent {
                p.accent.native()
            } else {
                p.line.native()
            },
        )
        .set("ButtonBorderBrushPointerOver", p.accent.native())
        .set("ButtonBorderBrushPressed", p.accent.native())
        .set("ButtonBorderBrushDisabled", p.line.native())
        .set("ButtonBorderThemeThickness", Thickness::uniform(1.0))
        .set("ButtonBorderThickness", Thickness::uniform(1.0))
        .set("ButtonCornerRadius", CornerRadius::uniform(2.0))
        .set("ButtonPadding", Thickness::xy(11.0, 6.0))
        .set("SystemControlForegroundAccentBrush", p.accent.native())
        .set("SystemControlBackgroundAccentBrush", p.accent.native())
        .set("SystemControlHighlightAccentBrush", p.accent.native())
        .set("SystemControlHighlightAltAccentBrush", p.accent.native())
        .set("SystemControlDisabledAccentBrush", p.muted.native())
        .set("AccentButtonBackground", p.accent.native())
        .set("AccentButtonBackgroundPointerOver", p.accent_soft.native())
        .set("AccentButtonBackgroundPressed", p.selection.native())
        .set("AccentButtonBackgroundDisabled", p.surface.native())
        .set("AccentButtonForeground", p.accent_ink.native())
        .set("AccentButtonForegroundPointerOver", p.ink.native())
        .set("AccentButtonForegroundPressed", p.ink.native())
        .set("AccentButtonForegroundDisabled", p.muted.native())
        .set("AccentButtonBorderBrush", p.accent.native())
        .set("AccentButtonBorderBrushPointerOver", p.accent.native())
        .set("AccentButtonBorderBrushPressed", p.accent.native())
        .set("AccentButtonBorderBrushDisabled", p.line.native())
        .set("AccentFillColorDefaultBrush", p.accent.native())
        .set("AccentFillColorSecondaryBrush", p.accent.native())
        .set("AccentFillColorTertiaryBrush", p.accent.native())
        .set(
            "AccentFillColorSelectedTextBackgroundBrush",
            p.accent.native(),
        )
        .set("AccentFillColorDisabledBrush", p.muted.native())
        .set("SliderTrackValueFill", p.accent.native())
        .set("SliderTrackValueFillPointerOver", p.accent_soft.native())
        .set("SliderTrackValueFillPressed", p.selection.native())
        .set("SliderTrackValueFillDisabled", p.muted.native())
        .set("SliderThumbBackground", p.accent.native())
        .set("SliderThumbBackgroundPointerOver", p.accent_soft.native())
        .set("SliderThumbBackgroundPressed", p.selection.native())
        .set("SliderThumbBackgroundDisabled", p.muted.native())
        .set("FocusStrokeColorOuterBrush", p.accent.native())
        .set("FocusStrokeColorInnerBrush", p.accent_soft.native())
        .set("ComboBoxItemPillFillBrush", p.accent.native())
        .set("ListViewItemSelectionIndicatorBrush", p.accent.native())
}
pub(super) fn dialog_resources(p: &Palette, width: f64) -> ResourceOverrides {
    theme_resources(p)
        .set("ContentDialogBackground", p.background.native())
        .set("ContentDialogBorderBrush", p.line.native())
        .set("ContentDialogBorderThickness", Thickness::uniform(0.0))
        .set("ContentDialogCornerRadius", CornerRadius::uniform(0.0))
        .set("ContentDialogPadding", Thickness::uniform(0.0))
        .set("ContentDialogContentPadding", Thickness::uniform(0.0))
        .set("ContentDialogMinWidth", ResourceValue::Double(width))
        .set("ContentDialogMaxWidth", ResourceValue::Double(width))
}
