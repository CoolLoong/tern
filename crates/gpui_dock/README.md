# gpui_dock

Dock panels, tabs and resizable layouts used by Tern. The implementation comes
from termua's dock components, originally derived from
[gpui-component](https://github.com/longbridge/gpui-component).

`DockArea` owns the layout; `PanelView` supplies panel behavior and
`DockAreaState` is the persistence representation. Tern composes these in
[workspace/window.rs](../../src/workspace/window.rs) and registers its browser
panel from [browser/panel.rs](../../src/browser/panel.rs).

The `examples/` directory illustrates common layouts. Clippy can type-check all examples with `--all-targets`;
layout/interaction tests run through GPUI TestPlatform without native windows.
