//! The picker paints its prepared list; keys and answers live elsewhere.

use super::{Picker, Row};
use crate::{HerdrWindow, notifications::safe_text};
use gpui::{prelude::*, *};

/// Characters of a title the header shows.
pub(super) const TITLE_CHARS: usize = 200;
/// Rows shown before the list scrolls.
const VISIBLE_ROWS: usize = 12;

impl HerdrWindow {
    fn picker_row_height(&self) -> f32 {
        self.config.ui.line_height() + 14.
    }

    fn picker_row(&self, picker: &Picker, index: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.theme;
        let row = div()
            .id(index)
            .debug_selector(move || format!("picker-row-{index}"))
            .w_full()
            .h(px(self.picker_row_height()))
            .px(px(16.))
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .when(index == picker.list.selected(), |row| {
                row.bg(rgb(theme.active))
            })
            .hover(|row| row.bg(rgb(theme.active)))
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.choose_picker_row(index, window, cx);
            }));
        let item = match picker.list.rows().get(index) {
            Some(Row::Item(item)) => picker.list.item(*item),
            Some(Row::Create) => {
                let label = picker
                    .list
                    .create_label()
                    .map_or("Create", |label| label.as_ref());
                // The query is as long as the user made it; a row shows a line.
                let text = safe_text(picker.list.create_text(), TITLE_CHARS);
                return row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .italic()
                        .child(format!("+ {label} \"{text}\"")),
                );
            }
            None => None,
        };
        let Some(item) = item else {
            return row;
        };
        row.when(!item.badge.is_empty(), |row| {
            row.child(
                div()
                    .flex_none()
                    .text_color(rgb(theme.primary()))
                    .child(item.badge.clone()),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(item.label.clone()),
        )
        .when(!item.detail.is_empty(), |row| {
            row.child(
                div()
                    .flex_none()
                    .max_w(relative(0.5))
                    .truncate()
                    .text_color(rgb(theme.muted))
                    .child(item.detail.clone()),
            )
        })
    }

    pub(crate) fn render_picker(&self, cx: &mut Context<Self>) -> Div {
        let Some(picker) = &self.picker else {
            return div();
        };
        let theme = &self.theme;
        let rows = picker.list.rows().len();
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_h_0()
            .child(
                div()
                    .flex_none()
                    .p(px(16.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .debug_selector(|| "picker-title".into())
                            .truncate()
                            .text_size(px(self.config.ui.size * 1.35))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(picker.title.clone()),
                    )
                    .child(div().pt(px(12.)).child(picker.search.clone())),
            )
            .when(rows == 0, |panel| {
                panel.child(
                    div()
                        .debug_selector(|| "picker-empty".into())
                        .flex_none()
                        .p(px(16.))
                        .text_color(rgb(theme.muted))
                        .child(if picker.list.len() == 0 {
                            "Nothing to choose from."
                        } else {
                            "No matches."
                        }),
                )
            })
            .when(rows > 0, |panel| {
                panel.child(
                    uniform_list(
                        "picker-rows",
                        rows,
                        cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                            let Some(picker) = &this.picker else {
                                return Vec::new();
                            };
                            range
                                .map(|index| this.picker_row(picker, index, cx))
                                .collect()
                        }),
                    )
                    .track_scroll(&picker.scroll)
                    // A short list takes only its own height; a long one
                    // scrolls, and a short window may still shrink it.
                    .h(px(rows.min(VISIBLE_ROWS) as f32 * self.picker_row_height()))
                    .min_h_0(),
                )
            })
            .child(
                div()
                    .debug_selector(|| "picker-status".into())
                    .flex_none()
                    .px(px(16.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child("↑ / ↓ navigate · Enter choose · Esc cancel"),
            )
    }
}
