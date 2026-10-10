//! The window's picker: one filterable list on a menu page, answered with a
//! single choice.
//!
//! The list itself ([`list::List`]) knows nothing about who asked: it holds
//! items with an id, label, detail and badge, the query, the selection, an
//! optional "create" row, and the choice Enter makes. [`Source`] is the only
//! thing that differs between the modes sharing this component: it says what
//! opened the picker and so where its answer goes. Today that is a plugin's
//! `ui.pick` ([`plugin`]); a mode fed by client-side data adds a variant and
//! builds a `List` the same way.
//!
//! The picker is a menu page, so input isolation, focus, and what the page
//! covers are the menu's. Its filter field is the palette's `SearchInput`, so
//! text editing and composition behave as they do there.

mod list;
pub(crate) mod plugin;
mod render;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

use crate::{
    HerdrWindow,
    menu::Page,
    search_input::{Changed, SearchInput},
};
use gpui::{prelude::*, *};
use list::{Choice, List, Row};

pub(crate) struct Picker {
    title: SharedString,
    pub(crate) search: Entity<SearchInput>,
    list: List,
    scroll: UniformListScrollHandle,
    source: Source,
    _subscription: Subscription,
}

/// What opened a picker, which is where its answer goes.
pub(crate) enum Source {
    /// A plugin's `ui.pick`, answered on the connection that asked.
    Plugin(plugin::PluginPick),
}

impl HerdrWindow {
    /// Opens the picker page with `list`. Gives `source` back when the menu
    /// would not open, so the caller can still answer whoever asked.
    pub(crate) fn open_picker(
        &mut self,
        title: &str,
        list: List,
        source: Source,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Source> {
        if !self.open_menu(window, cx) {
            return Err(source);
        }
        self.menu.page = Some(Page::Picker);
        let search = cx.new(SearchInput::new);
        let subscription = cx.subscribe(&search, |this, search, _: &Changed, cx| {
            let query = search.read(cx).text().to_owned();
            if let Some(picker) = &mut this.picker
                && picker.list.set_query(&query)
            {
                picker.scroll.scroll_to_item(0, ScrollStrategy::Top);
                cx.notify();
            }
        });
        search.update(cx, |input, cx| {
            input.set_placeholder("Type to filter...", cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            window.focus(&input.focus, cx);
        });
        let scroll = UniformListScrollHandle::new();
        scroll.scroll_to_item(list.selected(), ScrollStrategy::Center);
        self.picker = Some(Picker {
            title: crate::notifications::safe_text(title, render::TITLE_CHARS).into(),
            search,
            list,
            scroll,
            source,
            _subscription: subscription,
        });
        cx.notify();
        Ok(())
    }

    fn answer_picker(&mut self, source: Source, choice: Option<Choice>) {
        match source {
            Source::Plugin(pick) => self.answer_plugin_pick(pick, plugin::outcome(choice)),
        }
    }

    /// Answers the picker with `choice`, or as cancelled with none, and
    /// closes its page.
    fn finish_picker(
        &mut self,
        choice: Option<Choice>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        self.answer_picker(picker.source, choice);
        if self.menu.page == Some(Page::Picker) {
            self.dismiss_menu(window, cx);
        }
    }

    /// Closes the picker without answering: whoever asked no longer waits.
    pub(crate) fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.take().is_some() && self.menu.page == Some(Page::Picker) {
            self.dismiss_menu(window, cx);
        }
    }

    /// Answers the picker as cancelled because its page is going away: the
    /// menu was dismissed, or another page is opening over it. Whoever asked
    /// must never be left waiting on a picker that is no longer on screen.
    pub(crate) fn cancel_picker(&mut self) {
        if let Some(picker) = self.picker.take() {
            self.answer_picker(picker.source, None);
        }
    }

    /// The window's periodic picker work.
    pub(crate) fn poll_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // `dismiss_menu` and `open_menu` cancel a picker at once. The menu
        // has other ways to leave a page, such as the selected connection
        // being replaced, and this catches those.
        if let Some(picker) = &self.picker
            && self.menu.page != Some(Page::Picker)
        {
            let focused = picker.search.read(cx).focus.is_focused(window);
            self.cancel_picker();
            // The filter field is gone; the keyboard goes back to the pane.
            if focused && self.menu.page.is_none() {
                window.focus(&self.focus, cx);
            }
            cx.notify();
        }
        self.poll_plugin_picks(window, cx);
        // A dismissal the menu refused leaves an answered picker's page up.
        if self.picker.is_none() && self.menu.page == Some(Page::Picker) {
            self.dismiss_menu(window, cx);
        }
    }

    /// Chooses `row`, as clicking it does.
    fn choose_picker_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        if picker.list.select(row)
            && let Some(choice) = picker.list.choice()
        {
            self.finish_picker(Some(choice), window, cx);
        }
    }

    /// Keys the list owns while the picker has the keyboard. Everything else
    /// is left for the filter field and the platform's text input, and while
    /// a composition is open so are these.
    pub(crate) fn picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        if picker.search.read(cx).is_composing() {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        let control_only =
            modifiers.control && !modifiers.alt && !modifiers.shift && !modifiers.platform;
        let step = match event.keystroke.key.as_str() {
            "up" => Some(-1),
            "down" => Some(1),
            "p" if control_only => Some(-1),
            "n" if control_only => Some(1),
            _ => None,
        };
        match event.keystroke.key.as_str() {
            "escape" => self.finish_picker(None, window, cx),
            // Enter on a list the query emptied chooses nothing and waits.
            "enter" => {
                if let Some(choice) = picker.list.choice() {
                    self.finish_picker(Some(choice), window, cx);
                }
            }
            _ if step.is_some() => {
                if picker.list.step(step.unwrap_or_default()) {
                    picker
                        .scroll
                        .scroll_to_item(picker.list.selected(), ScrollStrategy::Nearest);
                    cx.notify();
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }
}
