//! The picker paints its prepared list; keys and answers live elsewhere.

use super::{Picker, Row, Source, navigator::Layout};
use crate::{HerdrWindow, notifications::safe_text, sidebar};
use gpui::{prelude::*, *};

/// Characters of a title the header shows.
pub(super) const TITLE_CHARS: usize = 200;
/// Rows shown before the list scrolls.
const VISIBLE_ROWS: usize = 12;
/// How far one step of a nested row is indented.
const INDENT: f32 = 16.;

impl Picker {
    /// What an empty list says.
    fn empty_text(&self) -> &'static str {
        match &self.source {
            Source::Navigator(navigator) if navigator.narrowed() => "No matches.",
            Source::Navigator(navigator) => match navigator.layout() {
                Layout::Agents => "No agents.",
                Layout::Workspaces => "No workspaces.",
            },
            Source::Plugin(_) if self.list.len() == 0 => "Nothing to choose from.",
            Source::Plugin(_) => "No matches.",
        }
    }

    /// The keys the footer names.
    fn hint(&self) -> &'static str {
        match &self.source {
            Source::Plugin(_) => "↑ / ↓ navigate · Enter choose · Esc cancel",
            Source::Navigator(_) => {
                "j / k move · / search · b w i d status · a all · Enter open · Esc close"
            }
        }
    }
}

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
            .when(picker.list.selected() == Some(index), |row| {
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
        row.when(item.indent > 0, |row| {
            row.pl(px(16. + INDENT * f32::from(item.indent)))
        })
        .when(item.dimmed, |row| row.text_color(rgb(theme.muted)))
        .when_some(item.status, |row, status| {
            let light = matches!(
                cx.window_appearance(),
                WindowAppearance::Light | WindowAppearance::VibrantLight
            );
            let indicators = sidebar::Indicators::new(
                self.settings.shared.as_ref(),
                self.config.status_icons,
                self.config.edge_rail,
                light,
                theme,
            );
            row.child(sidebar::status_indicator(
                status,
                &self.config.ui,
                indicators,
            ))
        })
        .when(!item.badge.is_empty(), |row| {
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
                        .child(picker.empty_text()),
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
                    .map(|footer| match &picker.source {
                        Source::Navigator(navigator) => match navigator.notice() {
                            Some(notice) => footer
                                .text_color(rgb(theme.ink(theme.palette[3])))
                                .child(notice.clone()),
                            None => footer
                                .when_some(navigator.filter_label(), |footer, filter| {
                                    footer.child(format!("{filter} only · "))
                                })
                                .child(picker.hint()),
                        },
                        Source::Plugin(_) => footer.child(picker.hint()),
                    }),
            )
    }
}
