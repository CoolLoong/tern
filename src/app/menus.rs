use super::actions::*;
use gpui_kit::{App, Menu, MenuItem};

pub fn build_menus() -> Vec<Menu> {
    vec![Menu::new("Tern").items(vec![
        MenuItem::action("Settings", OpenSettings),
        MenuItem::action("About Tern", About),
        MenuItem::action("Toggle Theme", ToggleTheme),
        MenuItem::separator(),
        MenuItem::action("Quit", Quit),
    ])]
}

pub fn set_app_menus(cx: &mut App) {
    cx.set_menus(build_menus());
}
