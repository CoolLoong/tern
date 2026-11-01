//! Breadcrumbs, navigation controls, preview and table composition.
use super::*;

impl BrowserView {
    pub(super) fn render_breadcrumb_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let cwd = self.current_dir(cx).unwrap_or_default();
        let crumbs = crate::files::model::breadcrumbs_for_path(&cwd);
        let total = crumbs.len();

        let mut bc = Breadcrumb::new();
        for (ix, (label, path)) in crumbs.into_iter().enumerate() {
            let is_last = ix + 1 == total;
            if is_last {
                bc = bc.child(BreadcrumbItem::new(label).disabled(true));
            } else {
                let table = self.table.clone();
                bc = bc.child(BreadcrumbItem::new(label).on_click(move |_e, _window, cx| {
                    table.update(cx, |state, cx| state.delegate_mut().cd(path.clone(), cx));
                }));
            }
        }

        let search_button = Self::toolbar_button(
            "browser-search",
            Icon::new(IconName::Search),
            "Search (ctrl-f)",
        )
        .custom(Self::toolbar_variant(cx))
        .on_click(cx.listener(|this, _e, window, cx| this.on_search(&Search, window, cx)));

        let table = self.table.clone();
        let menu_button = Self::toolbar_button(
            "browser-options-menu",
            Icon::new(IconName::EllipsisVertical),
            "View options",
        )
        .custom(Self::toolbar_variant(cx))
        .dropdown_menu(move |menu, _window, _cx| {
            let table_for_element = table.clone();
            let table_for_click = table.clone();
            menu.item(
                PopupMenuItem::element(move |_window, _cx| {
                    let show_hidden = {
                        let d = table_for_element.read(_cx).delegate();
                        d.show_hidden
                    };
                    let (label, icon) = if show_hidden {
                        ("Hide Hidden Files".to_string(), IconName::EyeOff)
                    } else {
                        ("Show Hidden Files".to_string(), IconName::Eye)
                    };
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(Icon::new(icon))
                        .text_sm()
                        .child(label)
                })
                .on_click(move |_e, _window, cx| {
                    table_for_click.update(cx, |state, cx| {
                        let d = state.delegate_mut();
                        d.set_show_hidden(!d.show_hidden(), cx);
                        cx.notify();
                    });
                }),
            )
        });

        div()
            .id("browser-breadcrumb")
            .flex()
            .items_center()
            .justify_between()
            .px(px(6.0))
            .py(px(3.0))
            .bg(cx.theme().popover.opacity(0.55))
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.9))
            .child(
                div()
                    .id("browser-path-area")
                    .debug_selector(|| "browser-path-area".into())
                    .flex_1()
                    .on_key_down(
                        cx.listener(|this, event: &gpui_kit::KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" && this.path_editing {
                                this.on_cancel(&Cancel, window, cx);
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui_kit::MouseDownEvent, window, cx| {
                            if event.click_count == 2 && !this.path_editing && !this.search_open {
                                this.begin_path_edit(window, cx);
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .child(if self.path_editing {
                        Input::new(&self.path_input)
                            .cleanable(true)
                            .into_any_element()
                    } else if self.search_open {
                        Input::new(&self.search_input)
                            .cleanable(true)
                            .into_any_element()
                    } else {
                        bc.into_any_element()
                    }),
            )
            .child(search_button)
            .child(menu_button)
            .into_any_element()
    }

    /// The one recipe every icon button in the tab bar and toolbar uses, so the
    /// two groups cannot drift apart.
    ///
    /// `Button` emits a border only for the default and outline variants, and
    /// the focus style works by colouring that border — on a borderless ghost
    /// button it would therefore paint nothing. A transparent 1px border keeps
    /// the flat look, gives focus something to show, and stays inside the box so
    /// the tab panel's clipping cannot cut it off.
    pub(crate) fn toolbar_button(
        id: impl Into<gpui_kit::ElementId>,
        icon: Icon,
        tip: &'static str,
    ) -> Button {
        Button::new(id)
            .icon(icon)
            .compact()
            .border_1()
            .border_color(gpui_kit::transparent_black())
            .tooltip(gpui_kit::SharedString::from(tip))
            .focus_ring(false)
    }

    pub(crate) fn toolbar_variant(cx: &App) -> ButtonCustomVariant {
        ButtonCustomVariant::new(cx)
            .color(cx.theme().transparent)
            .hover(cx.theme().button_hover)
            .active(cx.theme().button_active)
    }

    pub(crate) fn nav_controls(&self, view: &Entity<Self>, cx: &App) -> AnyElement {
        let (can_back, can_forward, can_up) = (
            !self.back_stack.is_empty(),
            !self.forward_stack.is_empty(),
            self.current_dir(cx)
                .and_then(|dir| fs::parent_dir(&dir))
                .is_some(),
        );
        let in_trash = self.table.read(cx).delegate().in_trash();
        let nav_variant = Self::toolbar_variant(cx);
        let nav_button =
            move |id: &'static str, icon: IconName, enabled: bool, tip: &'static str| {
                Self::toolbar_button(id, Icon::new(icon), tip)
                    .custom(nav_variant)
                    .disabled(!enabled)
            };

        div()
            .id("browser-nav-controls")
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(
                nav_button(
                    "browser-nav-back",
                    IconName::ArrowLeft,
                    can_back,
                    "Back (alt-left)",
                )
                .on_click({
                    let view = view.clone();
                    move |_e, _window, cx| view.update(cx, |this, cx| this.navigate_back(cx))
                }),
            )
            .child(
                nav_button(
                    "browser-nav-forward",
                    IconName::ArrowRight,
                    can_forward,
                    "Forward (alt-right)",
                )
                .on_click({
                    let view = view.clone();
                    move |_e, _window, cx| view.update(cx, |this, cx| this.navigate_forward(cx))
                }),
            )
            .child(
                nav_button("browser-nav-up", IconName::ArrowUp, can_up, "Up (alt-up)").on_click({
                    let view = view.clone();
                    move |_e, _window, cx| view.update(cx, |this, cx| this.navigate_up(cx))
                }),
            )
            // Only meaningful while looking at the trash, and it is destructive,
            // so it never sits next to the navigation buttons otherwise.
            .when(in_trash, |this| {
                this.child(
                    Self::toolbar_button(
                        "browser-empty-trash",
                        Icon::default().path(gpui_common::TermuaIcon::Trash),
                        "Empty Trash",
                    )
                    .custom(nav_variant)
                    .on_click({
                        let clicked = view.clone();
                        move |_e, window, cx| {
                            let view = clicked.clone();
                            window.open_alert_dialog(cx, move |dialog, _window, cx| {
                                let view = view.clone();
                                let count = view.read(cx).table.read(cx).delegate().visible.len();
                                let confirm = DialogButtonProps::default()
                                    .ok_text(gpui_kit::SharedString::from("Empty Trash"))
                                    .ok_variant(ButtonVariant::Danger)
                                    .cancel_text(gpui_kit::SharedString::from("Cancel"))
                                    .show_cancel(true);
                                dialog
                                    .title(format!("Empty Trash? ({count} items)"))
                                    .confirm()
                                    .button_props(confirm)
                                    .on_ok(move |_e, _window, cx| {
                                        crate::browser::transfers::start_file_operation(
                                            "Empty Trash",
                                            || crate::files::operations::empty_trash().len(),
                                            cx,
                                        );
                                        true
                                    })
                            });
                        }
                    }),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_body_root(
        table_panel: AnyElement,
        preview_panel: Option<AnyElement>,
    ) -> AnyElement {
        div()
            .id("browser-body")
            .flex_1()
            .min_h_0()
            .flex()
            .items_stretch()
            .child(table_panel)
            .when_some(preview_panel, |this, panel| this.child(panel))
            .into_any_element()
    }

    pub(super) fn preview_panel_width(&self, viewport: gpui_kit::Size<Pixels>) -> Pixels {
        if self.show_preview {
            px((viewport.width.as_f32() * 0.38).clamp(280.0, 520.0))
        } else {
            px(0.0)
        }
    }

    pub(super) fn render_preview_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.show_preview {
            return None;
        }

        let (preview_title, preview_size) = self
            .preview
            .target
            .as_ref()
            .map(|t| (gpui_kit::SharedString::from(t.name.clone()), t.size))
            .unwrap_or_else(|| ("Preview".to_string().into(), None));

        let viewport = window.viewport_size();
        let preview_w = self.preview_panel_width(viewport);
        let muted_fg = cx.theme().muted_foreground.opacity(0.85);
        let body = self.render_preview_body(preview_w, cx);
        let can_copy = matches!(self.preview.content, PreviewContent::Text { .. });
        let controls = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(8.0))
            .when_some(preview_size, |this, size| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted_fg)
                        .child(human_bytes(size)),
                )
            })
            .when(can_copy, |this| {
                this.child(
                    div()
                        .id("browser-preview-copy-action")
                        .debug_selector(|| "browser-preview-copy".into())
                        .child(
                            Self::toolbar_button(
                                "browser-preview-copy",
                                Icon::new(IconName::Copy),
                                "Copy all text",
                            )
                            .custom(Self::toolbar_variant(cx))
                            .on_click(cx.listener(|this, _, _, cx| this.copy_preview_text(cx))),
                        ),
                )
            });

        let header = div()
            .id("browser-preview-header")
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(6.0))
            .bg(cx.theme().table_head)
            .text_color(cx.theme().table_head_foreground)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(preview_title),
            )
            .child(controls);

        Some(
            div()
                .id("browser-preview-panel")
                .key_context("TernPreview")
                .w(preview_w)
                .min_w(px(240.0))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .min_h_0()
                .bg(cx.theme().popover.opacity(0.32))
                .border_l_1()
                .border_color(cx.theme().border.opacity(0.9))
                .on_any_mouse_down(|event, _window, cx| {
                    if event.button != MouseButton::Left {
                        cx.stop_propagation();
                    }
                })
                .capture_action(cx.listener(
                    |this, _: &gpui_kit::component::input::Copy, window, cx| {
                        this.copy_preview_selection(window, cx);
                    },
                ))
                .child(header)
                .child(
                    div()
                        .id("browser-preview-body")
                        .debug_selector(|| "browser-preview-body".into())
                        .flex_1()
                        .min_h_0()
                        .p(px(8.0))
                        .child(body),
                )
                .into_any_element(),
        )
    }

    pub(super) fn render_preview_body(
        &mut self,
        preview_w: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted_fg = cx.theme().muted_foreground.opacity(0.85);
        match &self.preview.content {
            PreviewContent::Empty => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_color(muted_fg)
                .child(Icon::new(IconName::Search).size_6())
                .child("Select a file")
                .into_any_element(),
            PreviewContent::Loading => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_color(muted_fg)
                .child(Icon::new(IconName::LoaderCircle).size_6())
                .child("Loading…")
                .into_any_element(),
            PreviewContent::Binary => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_color(muted_fg)
                .child(Icon::new(IconName::TriangleAlert).size_6())
                .child("Binary preview unsupported")
                .into_any_element(),
            PreviewContent::Error { message } => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_color(muted_fg)
                .child(Icon::new(IconName::Info).size_6())
                .child(message.clone())
                .into_any_element(),
            PreviewContent::Image { image } => {
                let content_w = (preview_w - px(16.0)).max(px(80.0));
                let img_el = img(gpui_kit::ImageSource::Image(image.clone()))
                    .w(content_w)
                    .object_fit(gpui_kit::ObjectFit::Contain);
                div()
                    .size_full()
                    .overflow_y_scrollbar()
                    .child(img_el)
                    .into_any_element()
            }
            PreviewContent::Text {
                fenced_markdown, ..
            } => gpui_kit::component::text::TextView::markdown(
                "browser-preview-text",
                fenced_markdown.clone(),
            )
            .selectable(true)
            .scrollable(true)
            .w_full()
            .h_full()
            .into_any_element(),
        }
    }

    pub(super) fn render_table_panel(
        &mut self,
        table: Entity<TableState<BrowserDelegate>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let table_for_secondary = table.clone();
        let table_for_down = table.clone();
        let table_for_move = table.clone();
        let table_for_up = table.clone();

        div()
            .id("browser-table-panel")
            .flex_1()
            .min_w_0()
            .relative()
            .capture_any_mouse_down(cx.listener(
                move |_view, ev: &gpui_kit::MouseDownEvent, _window, cx| {
                    table_for_secondary
                        .update(cx, |state, _| state.delegate_mut().clear_press_on_row());
                    if ev.button != MouseButton::Right {
                        return;
                    }
                    let row = marquee_hit_row(&table_for_secondary, ev.position, cx);
                    Self::secondary_press(&table_for_secondary, row, cx);
                },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, ev: &gpui_kit::MouseDownEvent, _window, cx| {
                    view.marquee = None;
                    if table_for_down
                        .update(cx, |state, _| state.delegate_mut().take_press_on_row())
                        || !marquee_allows_start(&table_for_down, ev.position, cx)
                    {
                        cx.notify();
                        return;
                    }
                    let additive = ev.modifiers.control || ev.modifiers.shift;
                    let was_selected = table_for_down.update(cx, |state, cx| {
                        let was_selected = state.delegate_mut().begin_marquee();
                        if !additive {
                            state.delegate_mut().clear_selection();
                        }
                        cx.notify();
                        was_selected
                    });
                    view.marquee = Some(Marquee {
                        anchor: ev.position,
                        current: ev.position,
                        was_selected: if additive {
                            was_selected
                        } else {
                            HashSet::new()
                        },
                    });
                    cx.notify();
                }),
            )
            .on_mouse_move(
                cx.listener(move |view, ev: &gpui_kit::MouseMoveEvent, _window, cx| {
                    if ev.pressed_button != Some(MouseButton::Left) {
                        return;
                    }
                    let Some(marquee) = view.marquee.as_mut() else {
                        return;
                    };
                    marquee.current = ev.position;
                    let marquee = marquee.clone();
                    marquee_apply(&table_for_move, &marquee, cx);
                    cx.notify();
                }),
            )
            .capture_any_mouse_up(cx.listener(
                move |view, _ev: &gpui_kit::MouseUpEvent, _window, cx| {
                    if view.marquee.take().is_some() {
                        table_for_up.update(cx, |state, cx| {
                            state.delegate_mut().end_marquee();
                            cx.notify();
                        });
                    }
                },
            ))
            .context_menu({
                let table_for_menu = table.clone();
                move |menu, window, cx| build_context_menu(menu, &table_for_menu, window, cx)
            })
            .child(DataTable::new(&table).bordered(false))
            .into_any_element()
    }
}

impl Render for BrowserView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let breadcrumb = self.render_breadcrumb_bar(cx);

        let table = self.table.clone();

        div()
            .id("browser-view")
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::on_back))
            .on_action(cx.listener(Self::on_forward))
            .on_action(cx.listener(Self::on_up))
            .on_action(cx.listener(Self::on_refresh))
            .on_action(cx.listener(Self::on_toggle_hidden))
            .on_action(cx.listener(Self::on_search))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_select_up))
            .on_action(cx.listener(Self::on_select_down))
            .on_action(cx.listener(Self::on_toggle_path_entry))
            .on_action(cx.listener(Self::on_new_folder))
            .on_action(cx.listener(Self::on_new_text_file))
            .on_action(cx.listener(Self::on_rename))
            .on_action(cx.listener(Self::on_move_to_trash))
            .on_action(cx.listener(Self::on_open_file))
            .on_action(cx.listener(Self::on_open_in_terminal))
            .on_action(cx.listener(Self::on_open_with))
            .on_action(cx.listener(Self::on_properties))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_copy_path))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_add_to_bookmark))
            .on_action(cx.listener(Self::on_restore_from_trash))
            .on_action(cx.listener(Self::on_delete_permanently))
            .child(breadcrumb)
            .child(Self::render_body_root(
                self.render_table_panel(table, cx),
                self.render_preview_panel(window, cx),
            ))
            .when_some(self.render_op_modal(window, cx), |this, modal| {
                this.child(modal)
            })
    }
}
