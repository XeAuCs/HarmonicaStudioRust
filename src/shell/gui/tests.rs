use super::*;
#[test]
fn compact_and_full_modes_remember_their_independent_resized_windows() {
    let mut sizes = WindowSizes::default();
    sizes.remember_transition(false, true, Some((1180.0, 910.0)));
    assert_eq!(sizes.for_mode(true), (900.0, 650.0));
    sizes.remember_transition(true, false, Some((850.0, 690.0)));
    assert_eq!(sizes.for_mode(false), (1180.0, 910.0));
    sizes.remember_transition(false, true, Some((1300.0, 940.0)));
    assert_eq!(sizes.for_mode(true), (850.0, 690.0));
    sizes.remember_transition(true, true, Some((1.0, 1.0)));
    sizes.remember_transition(true, false, None);
    assert_eq!(sizes.for_mode(true), (850.0, 690.0));
    assert_eq!(sizes.for_mode(false), (1300.0, 940.0));
}
#[test]
fn timeline_drag_clamps_endpoints_and_preserves_subsecond_position() {
    let timeline = Timeline {
        duration: 12.5,
        width: 114.0,
        ..Timeline::default()
    };
    assert_eq!(timeline.at(-10.0), 0.0);
    assert_eq!(timeline.at(300.0), 12.5);
    assert_eq!(timeline.at(57.0), 6.25);
    let empty = Timeline::default();
    assert_eq!(empty.at(100.0), 0.0);
}
#[test]
fn score_key_labels_match_exported_modifier_commands() {
    assert_eq!(key_label(48), "↓Z");
    assert_eq!(key_label(61), "Z♯");
    assert_eq!(key_label(72), "↑Z");
    assert_eq!(key_label(85), "↑,♯");
    assert_eq!(key_label(47), "");
}
#[test]
fn sampled_screenshot_colors_map_to_single_theme_sources() {
    // Regression for the green/purple/blue mix: each sampled value must
    // come from exactly one theme's Palette field, proving that mixed
    // pixels are stale surfaces rather than a new design color.
    assert_eq!(Palette::for_theme("plum").background, Rgb::hex(0xF2EEEF));
    assert_eq!(Palette::for_theme("forest").accent, Rgb::hex(0x386C5F));
    assert_eq!(Palette::for_theme("blue").accent, Rgb::hex(0x4A6084));
    assert_eq!(Palette::for_theme("plum").accent, Rgb::hex(0x805369));
    // The four themes must stay distinct; sharing one accent would hide a
    // stale-canvas bug.
    let accents: Vec<_> = Palette::all_themes()
        .iter()
        .map(|t| Palette::for_theme(t).accent)
        .collect();
    for i in 0..accents.len() {
        for j in (i + 1)..accents.len() {
            assert_ne!(
                accents[i], accents[j],
                "themes must not share one accent color"
            );
        }
    }
}
#[test]
fn all_themes_derive_every_visible_surface_from_one_palette() {
    let required = [
        "SystemControlForegroundAccentBrush",
        "SystemControlBackgroundAccentBrush",
        "SystemControlHighlightAccentBrush",
        "SystemControlHighlightAltAccentBrush",
        "SystemControlDisabledAccentBrush",
        "AccentButtonBackground",
        "AccentButtonBackgroundPointerOver",
        "AccentButtonBackgroundPressed",
        "AccentButtonBackgroundDisabled",
        "AccentButtonForeground",
        "AccentButtonForegroundPointerOver",
        "AccentButtonForegroundPressed",
        "AccentButtonForegroundDisabled",
        "AccentButtonBorderBrush",
        "AccentButtonBorderBrushPointerOver",
        "AccentButtonBorderBrushPressed",
        "AccentButtonBorderBrushDisabled",
        "SliderThumbBackground",
        "SliderThumbBackgroundPointerOver",
        "SliderThumbBackgroundPressed",
        "SliderThumbBackgroundDisabled",
        "SliderTrackValueFill",
        "SliderTrackValueFillPointerOver",
        "SliderTrackValueFillPressed",
        "SliderTrackValueFillDisabled",
        "ComboBoxItemPillFillBrush",
    ];
    // ScrollViewer/ProgressBar live under the themed Border, not inside a
    // Button, so they are required in the root table only.
    let root_only = [
        "ScrollBarThumbFill",
        "ScrollBarThumbFillPointerOver",
        "ScrollBarThumbFillPressed",
        "ScrollBarThumbFillDisabled",
        "ScrollBarTrackFill",
        "ProgressBarBackground",
        "ProgressBarForeground",
        "ProgressBarForegroundDisabled",
    ];
    for theme in Palette::all_themes() {
        let p = Palette::for_theme(theme);
        let root = format!("{:?}", theme_resources(&p));
        let accent_button = format!("{:?}", button_resources(&p, true));
        let plain_button = format!("{:?}", button_resources(&p, false));
        for key in required {
            assert!(
                root.contains(key),
                "{theme}: root theme_resources is missing {key}"
            );
            assert!(
                accent_button.contains(key),
                "{theme}: per-Button resources are missing {key}"
            );
            assert!(
                plain_button.contains(key),
                "{theme}: plain Button resources are missing {key}"
            );
        }
        for key in root_only {
            assert!(
                root.contains(key),
                "{theme}: root theme_resources is missing {key}"
            );
        }
        // The accent brush must encode the theme's own accent RGB.
        let accent_marker = format!("r: {}, g: {}, b: {}", p.accent.0, p.accent.1, p.accent.2);
        assert!(
            root.contains(&accent_marker),
            "{theme}: root resources do not use the shared accent {accent_marker}"
        );
        assert!(
            accent_button.contains(&accent_marker),
            "{theme}: Accent Button does not use the shared accent"
        );
        // No WinUI default blue may survive in any theme table.
        for system_blue in ["r: 0, g: 120, b: 215", "r: 0, g: 103, b: 192"] {
            assert!(
                !root.contains(system_blue),
                "{theme}: system blue {system_blue} leaked into theme_resources"
            );
            assert!(
                !accent_button.contains(system_blue),
                "{theme}: system blue leaked into button_resources"
            );
        }
    }
    // Unknown theme names fall back to paper so every surface still shares
    // one defined palette instead of mixing a half-initialized theme.
    assert_eq!(Palette::for_theme("unknown"), Palette::for_theme("paper"));
}
#[test]
fn theme_change_log_records_single_palette_source() {
    // The log line is the field evidence for a reported mix: it pins the
    // exact version, transition, and palette hexes without touching user
    // library content.
    for theme in Palette::all_themes() {
        let p = Palette::for_theme(theme);
        let line = theme_log_line(7, "preview", "paper", theme, &p);
        assert!(line.contains("version=7"), "{theme}: version missing");
        assert!(line.contains("source=preview"), "{theme}: source missing");
        assert!(line.contains("paper->"), "{theme}: old theme missing");
        assert!(
            line.contains(&format!("accent={}", p.accent.css_hex())),
            "{theme}: accent hex missing"
        );
    }
    let plum = Palette::for_theme("plum");
    let line = theme_log_line(8, "sync", "forest", "plum", &plum);
    assert!(line.contains("forest->plum"));
    assert!(line.contains("accent=#805369"));
    assert!(line.contains("bg=#F2EEEF"));
}
#[test]
fn theme_change_log_appends_without_user_library_content() {
    let home = tempfile::tempdir().unwrap();
    let key = "HARMONICA_STUDIO_HOME";
    let previous = std::env::var_os(key);
    unsafe { std::env::set_var(key, home.path()) };
    let body = theme_log_line(9, "preview", "paper", "plum", &Palette::for_theme("plum"));
    append_theme_log(&body);
    match previous {
        Some(value) => unsafe { std::env::set_var(key, value) },
        None => unsafe { std::env::remove_var(key) },
    }
    let text = std::fs::read_to_string(home.path().join("logs/theme-changes.log")).unwrap();
    assert!(text.contains("version=9"));
    assert!(text.contains("paper->plum"));
    assert!(text.contains("accent=#805369"));
}
