# menubar

In-window GPUI application menus, adapted from termua for Tern.

## Behavior

- Linux/Windows: the application label and top-level menu bar remain visible.
- Clicking the application label opens the first menu; dismissing a popup clears
  its selection and keeps the bar visible.
- Hover switches menus only when a menu is already open.
- macOS: the helper uses the native application menu instead of in-window triggers.

`FoldableAppMenuBar` is the retained public type name. Tern's current behavior
is an always-visible bar, rather than the older icon-only collapsed default.

## Integration

Initialize `gpui_kit::component` and `menubar` once, then set application menus
with `App::set_menus`. `menus[0]` provides the application menu and the remaining
entries provide the other top-level menus.

Inside a GPUI render implementation, use `menubar::MenubarTitleBar::build(window, cx)`
to obtain a title bar containing the menu. The enclosing window should have a
`gpui_kit::component::Root` for popups and notifications. See the runnable
[example](examples/menubar.rs) and [Tern window integration](../../src/workspace/window.rs).

Tests use GPUI's simulated platform and are included by the
[headless script](../../tools/check-headless.sh). Run `./tools/check-headless.sh`
from the repository root.
