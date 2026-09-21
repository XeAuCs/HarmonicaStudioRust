# Microsoft windows-rs

Source: https://github.com/microsoft/windows-rs
Commit: a38689a8b29520db84e3d398e4389d48f18e6346

Only the application dependency closure is included. Workspace member paths are narrowed to the included crates.

Local adaptation (2026-09-19): the reactor TextBlock exposes a string font_family property. The existing property diff / clear pipeline and native WinUI TextBlock FontFamily / FontFamilyProperty slots are extended accordingly. XamlReader constructs the native FontFamily, with XML text escaping. Changes are limited to reactor/src/generated.rs, reactor/src/native/winui/generated.rs and reactor/src/native/winui/bindings.rs. This preserves the original application's Chinese typeface without replacing native text controls. These generated files must retain this patch if regenerated upstream.

See https://learn.microsoft.com/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.markup.xamlreader.load for the native object construction API.
Local adaptation (2026-09-19): Border additionally exposes on_pointer_wheel_changed. PointerEventInfo carries wheel_delta and is_horizontal_wheel, read from native PointerPointProperties. The subscription, revision dispatch and revoker follow PointerMoved. ABI method order was checked against the shipped Microsoft.UI.winmd. This also changes reactor/src/element.rs and reactor/src/native/winui/mod.rs.
Local adaptation (2026-09-19): Border.resource_overrides applies resources to a subtree; ResourceValue adds FontFamily and finite non-negative Double values. NumberBox.small_change preserves the original speed step of 0.05 and transpose step of 1; its native setter and dependency-property ABI positions were checked against Microsoft.UI.Xaml.winmd.

Local fake-pump regression coverage is maintained in reactor/src/core/pump/tests/pointer_events.rs, visual_properties.rs, resource_overrides.rs and content_dialogs.rs. It checks property/event propagation and failed-batch rollback without starting a window. Run the corresponding test filter against the reactor Cargo.toml with a separate target/reactor-tests directory. These tests do not replace native rendering verification.
ContentDialog.resource_overrides uses the same set/clear pipeline as Button and Border, so native modal focus handling can be retained while restoring the original dialog palette and spacing.
Local adaptation (2026-09-20): ResourceValue::CornerRadius uses XamlReader to construct the native boxed value. A live startup A/B test with only ControlCornerRadius reproduced an unhandled E_FAIL with StockReference<CornerRadius>; the identical value created by WinUI loaded successfully, including with the full application theme. Direct property and offscreen Measure tests did not expose this failure.

Local adaptation (2026-09-20): ResourceValue::Thickness also uses native XamlReader boxing. The installed application reproducibly crashed when opening Settings with StockReference<Thickness>; changing only this construction path allowed the same ContentDialog resources to open and cancel successfully. This complements the CornerRadius fix and preserves all four edge values.