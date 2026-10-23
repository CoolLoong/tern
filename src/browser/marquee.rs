use std::collections::HashSet;

use gpui_kit::component::table::TableState;
use gpui_kit::{App, Bounds, Entity, Pixels, Point, px};

use crate::browser::table::BrowserDelegate;

const ROW_HEIGHT: Pixels = px(32.0);

#[derive(Clone, Debug)]
pub(crate) struct Marquee {
    pub anchor: Point<Pixels>,
    pub current: Point<Pixels>,
    pub was_selected: HashSet<String>,
}

fn viewport(
    table: &Entity<TableState<BrowserDelegate>>,
    cx: &App,
) -> Option<(Bounds<Pixels>, Pixels)> {
    let base = table
        .read(cx)
        .vertical_scroll_handle
        .0
        .borrow()
        .base_handle
        .clone();
    let bounds = base.bounds();
    if bounds.size.height <= Pixels::ZERO || bounds.size.width <= Pixels::ZERO {
        return None;
    }
    Some((bounds, base.offset().y))
}

fn row_bounds(viewport: Bounds<Pixels>, offset_y: Pixels, row_ix: usize) -> Bounds<Pixels> {
    let top = viewport.top() + offset_y + row_ix as f32 * ROW_HEIGHT;
    Bounds::from_corners(
        gpui_kit::point(viewport.left(), top),
        gpui_kit::point(viewport.right(), top + ROW_HEIGHT),
    )
}

fn visible_rows(
    table: &Entity<TableState<BrowserDelegate>>,
    cx: &App,
) -> Option<(Bounds<Pixels>, Pixels, std::ops::Range<usize>)> {
    let (viewport, offset_y) = viewport(table, cx)?;
    let count = table.read(cx).delegate().visible.len();
    if count == 0 {
        return None;
    }
    let scrolled = -offset_y;
    let first = (scrolled / ROW_HEIGHT).floor().max(0.0) as usize;
    let last = ((scrolled + viewport.size.height) / ROW_HEIGHT).ceil() as usize;
    let first = first.min(count - 1);
    let last = last.clamp(first + 1, count);
    Some((viewport, offset_y, first..last))
}

pub(crate) fn marquee_allows_start(
    table: &Entity<TableState<BrowserDelegate>>,
    pos: Point<Pixels>,
    cx: &App,
) -> bool {
    viewport(table, cx).is_some_and(|(viewport, _)| viewport.contains(&pos))
}

pub(crate) fn marquee_hit_row(
    table: &Entity<TableState<BrowserDelegate>>,
    pos: Point<Pixels>,
    cx: &App,
) -> Option<usize> {
    let (viewport, offset_y, range) = visible_rows(table, cx)?;
    range
        .into_iter()
        .find(|row_ix| row_bounds(viewport, offset_y, *row_ix).contains(&pos))
}

pub(crate) fn marquee_apply(
    table: &Entity<TableState<BrowserDelegate>>,
    marquee: &Marquee,
    cx: &mut App,
) {
    let rect = Bounds::from_corners(
        gpui_kit::point(
            marquee.anchor.x.min(marquee.current.x),
            marquee.anchor.y.min(marquee.current.y),
        ),
        gpui_kit::point(
            marquee.anchor.x.max(marquee.current.x),
            marquee.anchor.y.max(marquee.current.y),
        ),
    );

    let Some((viewport, offset_y, range)) = visible_rows(table, cx) else {
        return;
    };
    let hits: Vec<String> = {
        let state = table.read(cx);
        range
            .filter(|row_ix| rect.intersects(&row_bounds(viewport, offset_y, *row_ix)))
            .filter_map(|row_ix| state.delegate().row(row_ix).map(|row| row.id.clone()))
            .collect()
    };

    let mut ids = marquee.was_selected.clone();
    for id in hits {
        if !ids.remove(&id) {
            ids.insert(id);
        }
    }

    table.update(cx, |state, cx| {
        state.delegate_mut().set_marquee_selection(ids);
        cx.notify();
    });
}
