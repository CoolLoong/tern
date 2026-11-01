use gpui_kit::component::{
    ActiveTheme, Root, TitleBar,
    button::{Button, ButtonVariants},
    input::Input,
    scroll::ScrollableElement,
    switch::Switch,
};
use gpui_kit::{
    App, AppContext, Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
    WindowBounds, WindowDecorations, WindowKind, WindowOptions, div, prelude::FluentBuilder, px,
    size,
};

use crate::settings::{SettingsFile, Theme};

pub(crate) struct SettingsWindow {
    settings: SettingsFile,
    shortcut_inputs: Vec<(
        String,
        String,
        gpui_kit::Entity<gpui_kit::component::input::InputState>,
    )>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsWindow {
    fn new(settings: SettingsFile, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut shortcut_inputs = Vec::new();
        for (id, label, default, _) in crate::app::keybindings::KEYBINDINGS {
            let input = cx.new(|cx| {
                gpui_kit::component::input::InputState::new(window, cx).default_value(
                    settings
                        .keybindings
                        .get(*id)
                        .cloned()
                        .unwrap_or_else(|| (*default).into()),
                )
            });
            shortcut_inputs.push(((*id).into(), (*label).into(), input));
        }
        Self {
            settings,
            shortcut_inputs,
            _subscriptions: Vec::new(),
        }
    }

    pub(crate) fn open(cx: &mut App) -> anyhow::Result<gpui_kit::WindowHandle<Root>> {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(620.0), px(720.0)), cx)),
                titlebar: Some(TitleBar::title_bar_options()),
                window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
                window_min_size: Some(size(px(520.0), px(520.0))),
                kind: WindowKind::Floating,
                ..Default::default()
            },
            |window, cx| {
                window.set_window_title("Tern Settings");
                let settings = crate::settings::load_settings_from_disk().unwrap_or_default();
                let view = cx.new(|cx| Self::new(settings, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (id, _, input) in &self.shortcut_inputs {
            self.settings
                .keybindings
                .insert(id.clone(), input.read(cx).value().to_string());
        }
        crate::app::keybindings::register(&self.settings, cx);
        self.settings.apply_to_app(Some(window), cx);
        if let Err(err) = crate::settings::save_settings_to_disk(&self.settings) {
            log::warn!("failed to save settings: {err:#}");
        }
    }

    fn set_theme(&mut self, theme: Theme, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.appearance.theme = theme;
        self.save(window, cx);
        cx.notify();
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let selected = cx.theme().selection;
        let current_theme = self.settings.appearance.theme;
        let this = cx.entity();
        let theme_window = this.clone();

        let theme_button = move |id, label, theme| {
            Button::new(id)
                .label(label)
                .compact()
                .when(current_theme == theme, |button| button.primary())
                .when(current_theme != theme, |button| button.outline())
                .on_click({
                    let this = theme_window.clone();
                    move |_, window, cx| {
                        this.update(cx, |this, cx| this.set_theme(theme, window, cx))
                    }
                })
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .p(px(24.0))
            .gap(px(20.0))
            .child(div().text_lg().child("Settings"))
            .child(div().text_sm().text_color(muted).child("Appearance"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div().flex().flex_col().gap(px(4.0)).child("Theme").child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child("Choose the application appearance"),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(theme_button(
                                "settings-theme-system",
                                "System",
                                Theme::System,
                            ))
                            .child(theme_button("settings-theme-light", "Light", Theme::Light))
                            .child(theme_button("settings-theme-dark", "Dark", Theme::Dark)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child("Show hidden files")
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child("Show dot-prefixed entries in new browsers"),
                            ),
                    )
                    .child({
                        let this = this.clone();
                        Switch::new("settings-show-hidden")
                            .checked(self.settings.appearance.show_hidden_files)
                            .on_click(move |checked, window, cx| {
                                this.update(cx, |this, cx| {
                                    this.settings.appearance.show_hidden_files = *checked;
                                    this.save(window, cx);
                                    cx.notify();
                                });
                            })
                    }),
            )
            .child(div().h(px(1.0)).bg(selected.opacity(0.3)))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Keyboard Shortcuts"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .overflow_y_scrollbar()
                    .children(self.shortcut_inputs.iter().map(|(_, label, input)| {
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().child(label.clone()))
                            .child(div().w(px(180.0)).child(Input::new(input).cleanable(true)))
                    })),
            )
            .child({
                let this = this;
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Button::new("settings-save-shortcuts")
                            .label("Save Shortcuts")
                            .primary()
                            .on_click(move |_, window, cx| {
                                this.update(cx, |this, cx| {
                                    this.save(window, cx);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(Button::new("settings-close").label("Close").on_click(
                        |_, window, cx| {
                            // Settings save on every change, so there is
                            // nothing to confirm before closing.
                            cx.global_mut::<crate::workspace::state::WindowState>()
                                .settings_window = None;
                            window.remove_window();
                        },
                    ))
            })
    }
}
