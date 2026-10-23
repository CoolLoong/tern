use gpui_kit::App;

#[derive(Clone, Default)]
pub struct StatusState {
    pub items: usize,
    pub selected: usize,
    pub truncated: bool,
    pub searching: bool,
}

impl gpui_kit::Global for StatusState {}

pub(crate) fn update_status(
    _panel_id: gpui_kit::EntityId,
    status: crate::browser::table::BrowserStatus,
    cx: &mut App,
) {
    let state = cx.try_global::<StatusState>().cloned().unwrap_or_default();
    let mut state = state;
    state.items = status.items;
    state.selected = status.selected;
    state.truncated = status.truncated;
    state.searching = status.searching;
    *cx.global_mut::<StatusState>() = state;
}
