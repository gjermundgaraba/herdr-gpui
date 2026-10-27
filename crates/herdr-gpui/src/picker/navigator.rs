//! The agent picker and the workspace list: two modes of the window's picker
//! that are fed by the hosts' snapshots instead of a plugin.
//!
//! They are the Herdr fork's Navigator in its `Agents` and `Workspaces`
//! layouts (`agent_picker` and `workspace_list` in its `[keys]`). [`rows`]
//! is that client's row logic; this module holds one open navigator, turns
//! its keys into the fork's (`src/client/shell/overlay_input.rs` there), and
//! opens the chosen row with the window's own cross-host navigation, the one
//! a click on a sidebar row uses.
//!
//! The picker's filter field is the search. Where the keyboard is decides
//! what a key means, as in the fork: on the rows `j`, `k`, and the status
//! letters act on the list, and in the search they are text.

mod rows;
mod seen;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub(crate) use rows::Layout;

use super::{
    Source,
    list::{Item, List},
};
use crate::{HerdrWindow, navigation::NavigationTarget};
use gpui::{prelude::*, *};
use rows::{Host, Row, StatusFilter, Target, View};
use seen::Fingerprint;

/// Rows Ctrl-D and Ctrl-U move by.
const PAGE: isize = 8;

pub(crate) struct Navigator {
    view: View,
    /// The rows listed, one per list item and in the same order.
    rows: Vec<Row>,
    /// What the rows were built from, so they follow the hosts while open.
    seen: Fingerprint,
    /// Why Enter opened nothing.
    notice: Option<SharedString>,
}

/// What a key asks of the navigator.
enum Act {
    Close,
    LeaveSearch,
    Search,
    Accept,
    Step(isize),
    /// To the next or previous workspace that lists panes.
    Jump(bool),
    First,
    Last,
    /// Narrow to one status, or to none, and drop the search.
    Filter(Option<StatusFilter>),
    /// Drop the status filter and keep the search.
    Unfilter,
    /// A key the rows have no use for; it still goes nowhere else.
    Ignore,
}

impl Navigator {
    pub(super) fn layout(&self) -> Layout {
        self.view.layout
    }

    pub(super) fn filter_label(&self) -> Option<&'static str> {
        self.view.filter.map(StatusFilter::label)
    }

    pub(super) fn notice(&self) -> Option<&SharedString> {
        self.notice.as_ref()
    }

    /// Whether a search or a status filter narrows the list.
    pub(super) fn narrowed(&self) -> bool {
        self.view.filter.is_some() || !self.view.query.trim().is_empty()
    }

    /// The picker list for the rows, with the view's selection. A selected
    /// row that is no longer listed leaves nothing selected, as in the fork:
    /// Enter then opens nothing rather than whatever took its place.
    fn list(&self) -> List {
        let items = self
            .rows
            .iter()
            .map(|row| {
                let mut item = Item::new(
                    String::new(),
                    &row.label,
                    (!row.meta.is_empty()).then_some(row.meta.as_str()),
                    None,
                );
                item.indent = row.depth;
                item.status = row.status;
                item.dimmed = row.stale;
                item
            })
            .collect();
        let mut list = List::new(items, None, None);
        match rows::selected_index(&self.rows, self.view.selected.as_ref()) {
            Some(row) => {
                list.select(row);
            }
            None => list.deselect(),
        }
        list
    }

    /// Where the row `list` selects leads.
    fn target_at(&self, list: &List) -> Option<Target> {
        list.selected_item()
            .and_then(|index| self.rows.get(index))
            .map(|row| row.target.clone())
    }

    /// Keeps the selection where `list` has it.
    fn follow(&mut self, list: &List) {
        self.view.selected = self.target_at(list);
    }
}

impl HerdrWindow {
    /// Every agent of every enabled host, in the agents panel's priority
    /// order. The device filter narrows the sidebar, not what can be reached.
    fn navigator_rows(&self, view: &View) -> Vec<Row> {
        let listed: Vec<usize> = (0..self.endpoints.len())
            .filter(|index| self.endpoints[*index].enabled)
            .collect();
        let hosts: Vec<Host<'_>> = listed
            .iter()
            .map(|&index| {
                let endpoint = &self.endpoints[index];
                Host {
                    id: &endpoint.id,
                    label: &endpoint.label,
                    local: index == 0,
                    online: endpoint.online(),
                    snapshot: self.host_live(index).snapshot.as_deref(),
                }
            })
            .collect();
        let agents: Vec<_> = self
            .fleet_agents()
            .into_iter()
            .filter_map(|(endpoint, agent)| {
                Some((listed.iter().position(|index| *index == endpoint)?, agent))
            })
            .collect();
        rows::rows(&hosts, &agents, view)
    }

    /// Opens the agent picker or the workspace list. The workspace list
    /// starts in its search, so typing filters at once; the agent picker
    /// starts on its rows, so `j` and `k` move at once and `/` searches.
    pub(crate) fn open_navigator(
        &mut self,
        layout: Layout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut view = View::new(layout);
        let rows = self.navigator_rows(&view);
        let active = &self.endpoints[self.selected_endpoint].id;
        let focused_workspace = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_workspace_id.as_ref());
        view.selected = match layout {
            // The top row is the highest-priority agent, not the current one.
            Layout::Agents => rows.first(),
            Layout::Workspaces => rows.iter().find(|row| {
                matches!(&row.target, Target::Workspace { endpoint, workspace }
                    if endpoint == active && Some(workspace) == focused_workspace)
            }),
        }
        .map(|row| row.target.clone());
        let navigator = Navigator {
            view,
            rows,
            seen: self.navigator_fingerprint(),
            notice: None,
        };
        let list = navigator.list();
        let title = match layout {
            Layout::Agents => "Agents",
            Layout::Workspaces => "Workspaces",
        };
        if self
            .open_picker(
                title,
                list,
                Source::Navigator(Box::new(navigator)),
                window,
                cx,
            )
            .is_err()
        {
            return;
        }
        if let Some(picker) = &self.fork.picker {
            picker
                .search
                .update(cx, |input, cx| input.set_placeholder("Search...", cx));
        }
        if layout == Layout::Agents {
            window.focus(&self.menu.focus, cx);
        }
    }

    /// Lists the rows again from what the window holds now, keeping the
    /// selection on the row it was on. Returns whether that row is now at
    /// another place in the list.
    fn relist_navigator(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(mut picker) = self.fork.picker.take() else {
            return false;
        };
        let before = picker.list.selected();
        if let Source::Navigator(navigator) = &mut picker.source {
            navigator.rows = self.navigator_rows(&navigator.view);
            navigator.seen = self.navigator_fingerprint();
            picker.list = navigator.list();
        }
        let moved = picker.list.selected() != before;
        self.fork.picker = Some(picker);
        cx.notify();
        moved
    }

    /// Scrolls the list to its selected row.
    fn reveal_navigator_selection(&self) {
        if let Some(picker) = &self.fork.picker
            && let Some(row) = picker.list.selected()
        {
            picker.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
        }
    }

    /// The user changed what is listed: the rows are listed again and the
    /// selection is brought into view.
    fn rebuild_navigator(&mut self, cx: &mut Context<Self>) {
        self.relist_navigator(cx);
        self.reveal_navigator_selection();
    }

    /// Follows the hosts while a navigator is open: a snapshot or a status
    /// that changed lists its rows again. Called once per window tick. Any
    /// host's snapshot gets here, so the list is scrolled only when the
    /// selected row moved; otherwise it stays where the user scrolled it.
    pub(super) fn refresh_navigator(&mut self, cx: &mut Context<Self>) {
        let Some(Source::Navigator(navigator)) =
            self.fork.picker.as_ref().map(|picker| &picker.source)
        else {
            return;
        };
        if navigator.seen != self.navigator_fingerprint() && self.relist_navigator(cx) {
            self.reveal_navigator_selection();
        }
    }

    /// The search changed. Returns whether the picker is a navigator, which
    /// filters its own rows.
    pub(super) fn navigator_query(&mut self, query: &str, cx: &mut Context<Self>) -> bool {
        let Some(Source::Navigator(navigator)) =
            self.fork.picker.as_mut().map(|picker| &mut picker.source)
        else {
            return false;
        };
        if navigator.view.query != query {
            navigator.view.query = query.to_owned();
            // A search starts over: every status, and its own first match.
            navigator.view.filter = None;
            navigator.view.selected = None;
            navigator.notice = None;
            self.rebuild_navigator(cx);
        }
        true
    }

    /// Selects `row` as pointing at it does, and opens it.
    pub(super) fn choose_navigator_row(
        &mut self,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &mut self.fork.picker else {
            return;
        };
        let Source::Navigator(navigator) = &mut picker.source else {
            return;
        };
        if picker.list.select(row) {
            navigator.follow(&picker.list);
            self.accept_navigator(window, cx);
        }
    }

    /// Opens the selected row: its host, and on it the workspace or pane.
    fn accept_navigator(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.fork.picker else {
            return;
        };
        let Source::Navigator(navigator) = &mut picker.source else {
            return;
        };
        let Some(target) = navigator.target_at(&picker.list) else {
            return;
        };
        let host = self
            .endpoints
            .iter()
            .enumerate()
            .find(|(_, endpoint)| endpoint.id == target.endpoint() && endpoint.enabled);
        // As in the fork: another machine that is not online is not opened,
        // and the navigator stays for another choice. Local always is.
        let ready = host.filter(|(index, endpoint)| *index == 0 || endpoint.online());
        if ready.is_none() {
            let label = host.map_or("That device", |(_, endpoint)| endpoint.label.as_str());
            navigator.notice = Some(format!("{label} is not ready").into());
            cx.notify();
            return;
        }
        self.finish_picker(None, window, cx);
        // The menu may refuse to close; then nothing is opened under it.
        if self.menu.page.is_some() {
            return;
        }
        match &target {
            Target::Host { endpoint } => {
                self.select_endpoint(endpoint, cx);
            }
            Target::Workspace {
                endpoint,
                workspace,
            } => {
                self.navigate_endpoint(endpoint, NavigationTarget::Workspace(workspace), cx);
            }
            Target::Pane { endpoint, pane } => {
                self.navigate_endpoint(endpoint, NavigationTarget::Pane(pane), cx);
            }
        }
    }

    /// The navigator's keys. In the search everything but the list keys is
    /// left for the filter field and the platform's text input, and so is
    /// every key while a composition is open.
    pub(super) fn navigator_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &self.fork.picker else {
            return;
        };
        if picker.search.read(cx).is_composing() {
            return;
        }
        let searching = picker.search.read(cx).focus.is_focused(window);
        let modifiers = event.keystroke.modifiers;
        let plain = modifiers == Modifiers::none();
        let control = modifiers == Modifiers::control();
        let shift = modifiers == Modifiers::shift();
        let act = match event.keystroke.key.as_str() {
            "escape" if searching => Act::LeaveSearch,
            "escape" => Act::Close,
            "enter" => Act::Accept,
            "up" => Act::Step(-1),
            "down" => Act::Step(1),
            "p" if control => Act::Step(-1),
            "n" if control => Act::Step(1),
            // Anything else typed into the search is text.
            _ if searching => return,
            "k" if plain => Act::Step(-1),
            "j" if plain => Act::Step(1),
            "u" if control => Act::Step(-PAGE),
            "d" if control => Act::Step(PAGE),
            "left" if plain => Act::Jump(false),
            "right" if plain => Act::Jump(true),
            "home" if plain => Act::First,
            "end" if plain => Act::Last,
            "g" if shift => Act::Last,
            "/" if plain => Act::Search,
            "backspace" if plain => Act::Unfilter,
            "b" if plain => Act::Filter(Some(StatusFilter::Blocked)),
            "w" if plain => Act::Filter(Some(StatusFilter::Working)),
            "i" if plain => Act::Filter(Some(StatusFilter::Idle)),
            "d" if plain => Act::Filter(Some(StatusFilter::Done)),
            "a" if plain => Act::Filter(None),
            _ => Act::Ignore,
        };
        cx.stop_propagation();
        window.prevent_default();
        self.navigator_act(act, window, cx);
    }

    fn navigator_act(&mut self, act: Act, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.fork.picker else {
            return;
        };
        let Source::Navigator(navigator) = &mut picker.source else {
            return;
        };
        navigator.notice = None;
        let list = &mut picker.list;
        // Whether the keys that only move the selection moved it; the others
        // list the rows again.
        let moved = match act {
            Act::Close => return self.finish_picker(None, window, cx),
            Act::Accept => return self.accept_navigator(window, cx),
            Act::LeaveSearch => {
                window.focus(&self.menu.focus, cx);
                cx.notify();
                return;
            }
            Act::Ignore => {
                cx.notify();
                return;
            }
            Act::Step(delta) => list.step(delta),
            Act::Jump(forward) => list
                .selected()
                .and_then(|row| rows::workspace_jump(&navigator.rows, row, forward))
                .is_some_and(|row| list.select(row)),
            Act::Last => list.select(navigator.rows.len().saturating_sub(1)),
            Act::First => {
                rows::selected_index(&navigator.rows, None).is_some_and(|row| list.select(row))
            }
            Act::Search => {
                let focus = picker.search.read(cx).focus.clone();
                window.focus(&focus, cx);
                if navigator.view.filter.take().is_none() {
                    cx.notify();
                    return;
                }
                return self.rebuild_navigator(cx);
            }
            Act::Unfilter => {
                if navigator.view.filter.take().is_none() {
                    cx.notify();
                    return;
                }
                navigator.view.selected = None;
                return self.rebuild_navigator(cx);
            }
            Act::Filter(filter) => {
                navigator.view.filter = filter;
                navigator.view.selected = None;
                // Cleared here first, so the field's own change is no news.
                navigator.view.query.clear();
                picker.search.update(cx, |input, cx| input.clear(cx));
                return self.rebuild_navigator(cx);
            }
        };
        if moved {
            navigator.follow(list);
            if let Some(row) = list.selected() {
                picker.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
            }
        }
        cx.notify();
    }
}
