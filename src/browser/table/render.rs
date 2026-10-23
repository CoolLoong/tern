//! GPUI table rendering, separate from the listing and selection state.
use super::*;

impl TableDelegate for BrowserDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.visible.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let (label, sort_col) = match col_ix {
            0 => ("Name".to_string(), SortColumn::Name),
            1 => ("Size".to_string(), SortColumn::Size),
            2 => ("Modified".to_string(), SortColumn::Modified),
            ORIGINAL_COL => (
                if self.in_trash() {
                    "Deleted from"
                } else {
                    "Folder"
                }
                .to_string(),
                SortColumn::Name,
            ),
            _ => (String::new(), SortColumn::Name),
        };

        let active = self.sort.column == sort_col;
        let icon = if active {
            Some(if self.sort.direction == SortDirection::Asc {
                IconName::ArrowUp
            } else {
                IconName::ArrowDown
            })
        } else {
            None
        };

        let mut row = div().flex().items_center().gap(px(6.0)).child(label);
        if let Some(icon) = icon {
            row = row.child(Icon::new(icon).xsmall());
        }

        row.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |table, _e: &MouseDownEvent, _window, cx| {
                table.delegate_mut().set_sort(sort_col, cx);
                cx.stop_propagation();
            }),
        )
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let Some(row) = self.visible.get(row_ix) else {
            return div().id(("row", row_ix)).border_0();
        };

        let is_selected = self.interaction.selected_ids.contains(row.id.as_str());
        div()
            .id(("row", row_ix))
            .border_0()
            .py(px(1.0))
            .when(is_selected, |this| {
                this.bg(cx.theme().table_active.opacity(0.65))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |table, ev: &MouseDownEvent, _window, cx| {
                    if table.delegate_mut().row_press(row_ix, ev.modifiers) {
                        cx.emit(TableEvent::SelectRow(row_ix));
                    }
                    // Row selection is off, so the component's own click handler
                    // no longer emits this; a double click must still navigate.
                    if ev.click_count == 2 {
                        cx.emit(TableEvent::DoubleClickedRow(row_ix));
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |table, ev: &MouseMoveEvent, _window, cx| {
                if ev.pressed_button != Some(MouseButton::Left) {
                    return;
                }
                if table.delegate_mut().drag_select_row(row_ix) {
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |table, ev: &MouseUpEvent, _window, cx| {
                    let modifiers = ev.modifiers;
                    let collapsed = table.delegate_mut().row_release(row_ix, modifiers);
                    if let Some(ix) = collapsed {
                        cx.emit(TableEvent::SelectRow(ix));
                    }
                    cx.notify();
                }),
            )
            .when(row.kind != EntryKind::Dir, |this| {
                let drag = LocalDragEntry::new(row.id.clone(), row.name.clone(), false);
                let table_for_drag = cx.entity();
                this.on_drag(drag, move |drag, _, _, cx| {
                    table_for_drag.update(cx, |state, _| {
                        let delegate = state.delegate_mut();
                        delegate.set_dragging(true);
                        delegate.defer_row_click(row_ix);
                    });
                    cx.new(|_| drag.clone())
                })
            })
            .when(
                row.kind == EntryKind::Dir && !crate::files::recent::is_recent(&row.id),
                |this| {
                    let drag_id = row.id.clone();
                    let target_dir_external = row.id.clone();
                    let target_dir_local = row.id.clone();
                    let table_for_drag = cx.entity();
                    this.on_drag(
                        LocalDragEntry::new(drag_id, row.name.clone(), true),
                        move |drag, _, _, cx| {
                            table_for_drag.update(cx, |state, _| {
                                let delegate = state.delegate_mut();
                                delegate.set_dragging(true);
                                delegate.defer_row_click(row_ix);
                            });
                            cx.new(|_| drag.clone())
                        },
                    )
                    .can_drop(move |any, _window, _cx| {
                        any.downcast_ref::<gpui_kit::ExternalPaths>().is_some()
                            || any.downcast_ref::<LocalDragEntry>().is_some()
                    })
                    .drag_over::<gpui_kit::ExternalPaths>(|mut style, _paths, _window, cx| {
                        let t = cx.theme();
                        style = style
                            .bg(t.table_active)
                            .border_1()
                            .border_color(t.table_active_border)
                            .rounded_md();
                        style
                    })
                    .drag_over::<LocalDragEntry>(|mut style, _drag, _window, cx| {
                        let t = cx.theme();
                        style = style
                            .bg(t.table_active)
                            .border_1()
                            .border_color(t.table_active_border)
                            .rounded_md();
                        style
                    })
                    .on_drop(cx.listener(
                        move |_table, paths: &gpui_kit::ExternalPaths, _window, cx| {
                            let dst = target_dir_external.clone();
                            crate::browser::transfers::start_transfer(
                                paths.paths().to_vec(),
                                dst,
                                crate::browser::transfers::next_group_id(),
                                "Copying…",
                                crate::files::operations::copy_path,
                                cx,
                            );
                        },
                    ))
                    .on_drop(cx.listener(
                        move |_table, drag: &LocalDragEntry, _window, cx| {
                            let dst = target_dir_local.clone();
                            if drag.id == dst {
                                return;
                            }
                            crate::browser::transfers::start_transfer(
                                vec![std::path::PathBuf::from(&drag.id)],
                                dst,
                                crate::browser::transfers::next_group_id(),
                                "Moving…",
                                crate::files::operations::move_path,
                                cx,
                            );
                        },
                    ))
                },
            )
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        use crate::browser::format::{format_modified, format_size};
        let Some(row) = self.visible.get(row_ix) else {
            return div();
        };

        let (muted, fg, accent) = {
            let t = cx.theme();
            (t.muted_foreground, t.foreground, t.accent)
        };

        match col_ix {
            0 => div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .pl(px(8.0))
                .child(file_icon(row, accent, muted))
                .child(
                    div()
                        .text_sm()
                        .text_color(if row.kind == EntryKind::Dir {
                            fg
                        } else {
                            fg.opacity(0.95)
                        })
                        .child(row.name.clone()),
                )
                .when(self.loading.contains(&row.id), |this| {
                    this.child(
                        div()
                            .ml(px(6.0))
                            .child(Icon::new(IconName::LoaderCircle).small().text_color(muted)),
                    )
                }),
            1 => div().text_xs().text_color(muted).child(format_size(row)),
            2 => div()
                .text_xs()
                .text_color(muted)
                .child(format_modified(row.modified)),
            ORIGINAL_COL => div()
                .text_xs()
                .text_color(muted)
                .child(row.original_path.clone().unwrap_or_else(|| "—".to_string())),
            _ => div(),
        }
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground.opacity(0.75);
        // During a search an empty list means "nothing matched", not "nothing
        // here" — saying the folder is empty there is simply wrong.
        let searching = self.searching && !self.query.trim().is_empty();
        let searchable = self
            .directory
            .as_deref()
            .is_some_and(|dir| !crate::files::recent::is_recent(dir));
        let message = if !self.loading.is_empty() && self.query.trim().is_empty() {
            "Loading…".to_string()
        } else if self.query.trim().is_empty() {
            "This folder is empty".to_string()
        } else if !searchable {
            // Recent has no directory tree behind it; saying "no matches" there
            // would blame the query for something it cannot do.
            "Search only covers real folders".to_string()
        } else if searching {
            format!(
                "Searching for \u{201c}{}\u{201d}\u{2026}",
                self.query.trim()
            )
        } else {
            format!("No matches for \u{201c}{}\u{201d}", self.query.trim())
        };
        div()
            .id("browser-empty")
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_color(muted)
            .child(
                Icon::new(if self.query.trim().is_empty() {
                    IconName::Folder
                } else if searching {
                    IconName::LoaderCircle
                } else {
                    IconName::Search
                })
                .size_6(),
            )
            .child(div().text_sm().child(message))
    }

    fn render_last_empty_col(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div().flex_1()
    }
}
