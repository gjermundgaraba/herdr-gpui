//! The header of a space group or pinned section in the spaces list, and the
//! divider before ungrouped spaces. Like a host's header, they are the same
//! in every row layout: only the layout's spacing differs.
//!
//! Counterpart of `render_marker` in the Herdr fork's
//! `src/client/shell/space_groups.rs`, which draws the header as
//! `▾ name ──── 2`. Here the name fills the row and the count sits at its
//! end, with the members' most urgent status before it while collapsed.

use super::{
    HOST_ARROW_WIDTH,
    agents::status_indicator,
    cell::RowContext,
    glyph_width, label_text, line_height,
    space_groups::{Group, Marker},
};
use crate::{HerdrWindow, config::sidebar::parse_color};
use gpui::{prelude::*, *};
use herdr_client::protocol::ClientShellSnapshot;

impl HerdrWindow {
    /// Toggles a fold of one host's spaces list: a worktree repository, a
    /// space group, or a pinned section, by its collapse key. Folding is this
    /// client's own view of the list, never a daemon request.
    pub(super) fn toggle_collapsed(&mut self, endpoint: &str, key: &str) {
        let Some(index) = self.endpoints.iter().position(|e| e.id == endpoint) else {
            return;
        };
        let collapsed = self.collapsed_for_mut(index);
        if !collapsed.remove(key) {
            collapsed.insert(key.to_owned());
        }
    }

    /// The row a marker draws: a group or pinned section's header, or the
    /// divider. `position` is the row's place among its host's rows, which
    /// keeps the headers of a split group apart.
    pub(super) fn marker_row(
        &self,
        marker: &Marker<'_>,
        snapshot: &ClientShellSnapshot,
        endpoint_id: &str,
        position: usize,
        row_cx: &RowContext<'_>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let look = row_cx.look;
        let (font, theme) = (row_cx.font, row_cx.theme);
        let left = look.content_x() + row_cx.nest;
        let Some(group) = &marker.group else {
            let key = format!("space-divider-{endpoint_id}-{position}");
            return div()
                .id(SharedString::from(key.clone()))
                .debug_selector(|| key.clone())
                .relative()
                .h(px((line_height(font) / 2.).round()))
                .flex_none()
                .w_full()
                .flex()
                .items_center()
                .pl(px(left))
                .pr(px(look.content_x()))
                .map(|row| row_cx.mark.apply(row, &key, &look))
                .child(div().h(px(1.)).w_full().bg(rgb(theme.active)));
        };
        let Group {
            name,
            collapsed,
            pinned: _,
            fg,
        } = *group;
        let key = format!("space-group-{endpoint_id}-{position}");
        let gap = look.density.host_gap();
        let count = marker.members.len().to_string();
        let status = collapsed.then(|| marker.status(&snapshot.workspaces));
        // The name yields room to the arrow, the count, and while collapsed
        // the status before it. Counts are digits, which an em-fraction
        // bounds well enough in any sidebar font.
        let count_width = (count.len() as f32 * glyph_width(font)).ceil();
        let status_width = status.map_or(0., |_| row_cx.indicators.width(font) + gap);
        let name_width = (look.content_width(row_cx.width)
            - row_cx.nest
            - HOST_ARROW_WIDTH
            - 2. * gap
            - count_width
            - status_width)
            .max(0.);
        // The daemon sends `#rrggbb`; the color reads like a token's, as in
        // the terminal client.
        let color = fg
            .and_then(parse_color)
            .map_or_else(|| theme.subtext(), |color| theme.ink(color));
        let (toggle_endpoint, toggle_key) = (endpoint_id.to_owned(), group.key());
        div()
            .id(SharedString::from(key.clone()))
            .debug_selector(|| key.clone())
            .relative()
            .h(px(line_height(font) + look.chrome_height()))
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(px(gap))
            .pl(px(left))
            .pr(px(look.content_x()))
            .overflow_hidden()
            .cursor_pointer()
            .map(|row| row_cx.mark.apply(row, &key, &look))
            .child(
                div()
                    .debug_selector(|| format!("{key}-arrow"))
                    .w(px(HOST_ARROW_WIDTH))
                    .flex_none()
                    .text_color(rgb(theme.primary()))
                    .child(label_text(if collapsed { "\u{25b8}" } else { "\u{25be}" })),
            )
            .child(
                div()
                    // As with workspace labels, avoid zero-basis text measurement.
                    .w(px(name_width))
                    .flex_none()
                    .overflow_hidden()
                    .child(
                        div()
                            .debug_selector(|| format!("{key}-name"))
                            .w(px(name_width))
                            .truncate()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(color))
                            .child(label_text(name)),
                    ),
            )
            // The count ends at the row's edge whatever its digits measure.
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(gap))
                    .when_some(status, |trailing, status| {
                        trailing.child(
                            div()
                                .debug_selector(|| format!("{key}-status"))
                                .h(px(line_height(font)))
                                .flex_none()
                                .child(status_indicator(status, font, row_cx.indicators)),
                        )
                    })
                    .child(
                        div()
                            .debug_selector(|| format!("{key}-count"))
                            .flex_none()
                            .text_color(rgb(theme.muted))
                            .child(label_text(&count)),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_collapsed(&toggle_endpoint, &toggle_key);
                cx.notify();
            }))
    }
}

#[cfg(test)]
mod tests;
