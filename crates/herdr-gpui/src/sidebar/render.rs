//! Laying out the sidebar: the two lists, their headings, and the drag handle
//! that resizes the panel. Render works from prepared state and the bounded
//! caches only.

use super::{
    DEVICE_FOOTER_HEIGHT, SidebarDrag,
    agents::Indicators,
    agents_sort, label_text,
    layout::{self, SidebarLook},
    line_height, reorder, sidebar_width, sticky,
};
use crate::{
    Command, HerdrWindow,
    config::{FontConfig, Theme},
    fonts::StyledFont,
};
use gpui::{prelude::*, *};

mod agent_rows;
mod device_tree;
mod spaces;

impl HerdrWindow {
    pub(crate) fn render_sidebar(
        &self,
        indicators: Indicators,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let width = sidebar_width(self.sidebar_width, f32::from(window.viewport_size().width));
        let split = self.sidebar_split.unwrap_or(0.5).clamp(0.1, 0.9);
        let look = layout::for_config(&self.config, self.fill());
        let layout = look.density;
        let content_x = look.content_x();
        let view = cx.entity().downgrade();
        let font = &self.config.sidebar;
        let theme = &self.theme;
        self.sidebar_search.input.update(cx, |input, cx| {
            input.set_appearance(self.config.ui.clone(), theme.clone(), cx);
        });
        let field = self
            .fill()
            .over(theme.sidebar_background(), theme.background);
        self.sidebar_search
            .input
            .update(cx, |input, cx| input.set_fill(field, cx));
        // The Devices layout heads every device, even a lone one, and lists
        // its agents under it in place of its workspaces. Its own field
        // narrows that tree in place, so the sidebar's search, whose results
        // replace the list, gives way to it; one setting hides either.
        let devices = !self.config.layout.mode.lists_workspaces();
        // A hidden field keeps whatever it held, but never filters the list.
        let searchable = self.config.show_sidebar_search && !devices;
        let tree_search = self.config.show_sidebar_search && devices;
        let search = (searchable && self.sidebar_search.query().is_some())
            .then(|| self.sidebar_search_results(self.sidebar_search_hits(), indicators, look, cx));
        let spaces = div()
            .id("spaces-scroll")
            .debug_selector(|| "spaces-scroll".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        let mut agents = div()
            .id("agents-scroll")
            .debug_selector(|| "agents-scroll".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        let spaces = spaces.track_scroll(&self.sidebar_scroll[0]);
        agents = agents.track_scroll(&self.sidebar_scroll[1]);
        let multi = self.endpoints.len() > 1;
        let headed = multi || devices;
        let agents_panel = self.config.show_agents && !devices;
        let query = if tree_search {
            self.device_tree_query(cx)
        } else {
            String::new()
        };
        let mut agent_count = 0;
        // Child positions of the highlighted rows, for the one-time reveal below.
        // Agent rows are counted by `agent_count`, which indexes that list.
        let mut highlighted = [None; 2];
        let spaces::SpaceList {
            list: spaces,
            host_rows,
            focused,
            drop_rows,
            drop_requests,
            drop_dragged,
            sliding,
            // A plugin view hid every agent, rather than there being none.
            filtered,
        } = self.append_space_rows(
            spaces,
            devices.then_some(query.as_str()),
            indicators,
            look,
            width,
            cx,
        );
        highlighted[0] = focused;
        if agents_panel {
            (agents, agent_count, highlighted[1]) =
                self.append_agent_rows(agents, indicators, look, width, cx);
        }
        if sliding {
            window.request_animation_frame();
        }
        let mut headers = if headed {
            self.host_headers(&host_rows)
        } else {
            Vec::new()
        };
        if let Some(row) = self.sidebar_pin_reveal.take()
            && self.uncover_revealed(row, &headers)
        {
            headers = self.host_headers(&host_rows);
        }
        // Rows under the pinned header are hidden, so a reveal must clear it.
        let cover = px(sticky::cover(&headers));
        let pinned_host = sticky::pinned(&headers)
            .and_then(|pinned| self.pinned_host_row(pinned, look, width, cx));
        // Follow the selection, but only once a frame has measured the viewport:
        // the handle resolves the request against the previous frame's bounds, so
        // an unmeasured list would scroll to a meaningless offset. Recording what
        // was revealed keeps later frames from undoing the user's own scrolling.
        for (list, row) in highlighted.iter().enumerate() {
            let Some(row) = *row else { continue };
            if self.sidebar_revealed[list].get() != Some(row)
                && self.sidebar_scroll[list].bounds().size.height > px(0.)
            {
                let scroll = &self.sidebar_scroll[list];
                let cover = if list == 0 { cover } else { px(0.) };
                let visible = scroll.bounds_for_item(row).is_some_and(|bounds| {
                    let offset = scroll.offset().y;
                    bounds.bottom() + offset > scroll.bounds().top() + cover
                        && bounds.top() + offset < scroll.bounds().bottom()
                });
                // Even a partially visible worktree is already seen. GPUI's
                // reveal also moves clipped rows, so only request it off-screen.
                if !visible {
                    scroll.scroll_to_item(row);
                    // The scroll lands in prepaint, after this render pinned a
                    // header for the old offset, and puts a row from above at
                    // the top edge, under the pinned header. Another frame pins
                    // for the new offset and moves the row out from under it.
                    if list == 0 && headed {
                        window.request_animation_frame();
                        self.sidebar_pin_reveal.set(Some(row));
                    }
                }
                self.sidebar_revealed[list].set(Some(row));
            }
        }
        if agent_count == 0 {
            agents = agents.child(
                div()
                    .px(px(content_x))
                    .text_color(rgb(theme.muted))
                    .truncate()
                    .child(label_text(if filtered {
                        "no matching agents"
                    } else {
                        "no agents"
                    })),
            );
        }
        div()
            .id("sidebar")
            .debug_selector(|| "sidebar".into())
            .relative()
            .w(px(width))
            .flex_none()
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(line_height(font)))
            .text_color(rgb(theme.foreground))
            .bg(self.fill().panel(theme.sidebar_background()))
            .border_r_1()
            .border_color(rgb(theme.active))
            // Zero flex bases keep long workspace lists from displacing agents.
            .child(
                div()
                    .debug_selector(|| "spaces-section".into())
                    .flex()
                    .flex_col()
                    .flex_1()
                    .map(|mut section| {
                        section.style().flex_grow = Some(if agents_panel { split } else { 1. });
                        section
                    })
                    .min_h_0()
                    .overflow_hidden()
                    .child(header(
                        if devices { "devices" } else { "spaces" },
                        font,
                        theme,
                        look,
                    ))
                    .when(searchable, |section| {
                        section.child(self.sidebar_search_field(look, cx))
                    })
                    .children(tree_search.then(|| self.device_tree_search(look)).flatten())
                    // The wrapper clips the pinned header as the next host's
                    // pushes it up, so it never paints over the title above.
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .map(|list| match search {
                                Some(results) => list.child(results),
                                None => list.child(spaces).children(pinned_host),
                            }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .h(px(line_height(font) + 2. * layout.footer_padding()))
                            .px(px(content_x))
                            .flex()
                            .items_center()
                            // Menu hugs the sidebar's edge, as in the terminal client.
                            .justify_between()
                            .text_color(rgb(theme.muted))
                            .gap(px(20.))
                            .child(
                                div()
                                    .id("new-workspace")
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(rgb(theme.foreground)))
                                    .child("new")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.command(Command::Workspace, window, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("sidebar-menu")
                                    .debug_selector(|| "sidebar-menu".into())
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(rgb(theme.foreground)))
                                    .child(label_text("menu"))
                                    .on_click(cx.listener(
                                        |this, event: &ClickEvent, window, cx| {
                                            this.menu.anchor = event.position();
                                            this.open_menu(window, cx);
                                        },
                                    )),
                            ),
                    ),
            )
            .when(agents_panel, |sidebar| {
                sidebar
                    .child(
                        div()
                            .id("sidebar-split-resize")
                            .debug_selector(|| "sidebar-split-resize".into())
                            .h(px(6.))
                            .flex_none()
                            .cursor(CursorStyle::ResizeUpDown)
                            .border_t_1()
                            .border_color(rgb(theme.active))
                            .hover(|s| s.bg(rgba(crate::panel_resize::RESIZE_HOVER)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.sidebar_split_modified = true;
                                    if event.click_count == 2 {
                                        this.sidebar_drag = None;
                                        this.sidebar_split = None;
                                        this.save_chrome();
                                    } else {
                                        this.sidebar_drag = Some(SidebarDrag::Split);
                                    }
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "agents-section".into())
                            .flex()
                            .flex_col()
                            .flex_1()
                            .map(|mut section| {
                                section.style().flex_grow = Some(1. - split);
                                section
                            })
                            .min_h_0()
                            .overflow_hidden()
                            .child(
                                header("agents", font, theme, look)
                                    .justify_between()
                                    .child(agents_sort(self, cx)),
                            )
                            .child(agents),
                    )
            })
            .child(self.render_device_footer(cx))
            .child(
                div()
                    .id("sidebar-resize")
                    .debug_selector(|| "sidebar-resize".into())
                    .absolute()
                    .right_0()
                    .top_0()
                    .h_full()
                    .w(px(6.))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .hover(|s| s.bg(rgba(crate::panel_resize::RESIZE_HOVER)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.sidebar_modified = true;
                            if event.click_count == 2 {
                                this.sidebar_drag = None;
                                this.sidebar_width = None;
                                this.save_sidebar_width();
                            } else {
                                this.sidebar_drag = Some(SidebarDrag::Width {
                                    start: f32::from(event.position.x),
                                    width,
                                });
                            }
                            cx.notify();
                        }),
                    ),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        // Capture globally so dragging continues outside the narrow divider,
                        // and terminal handlers never receive the resize gesture's release.
                        let moving = view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = moving.update(cx, |this, cx| {
                                    let scroll = this.sidebar_scroll[0].clone();
                                    if this.move_workspace_drag(
                                        event.position,
                                        event.pressed_button == Some(MouseButton::Left),
                                        |lift| {
                                            reorder::resolve(
                                                &drop_rows,
                                                &drop_requests,
                                                drop_dragged,
                                                &scroll,
                                                lift,
                                            )
                                        },
                                        cx,
                                    ) {
                                        cx.stop_propagation();
                                    } else if let Some(drag) = this.sidebar_drag {
                                        match drag {
                                            SidebarDrag::Width { start, width } => {
                                                this.sidebar_width = Some(sidebar_width(
                                                    Some(
                                                        width + f32::from(event.position.x) - start,
                                                    ),
                                                    f32::from(window.viewport_size().width),
                                                ));
                                            }
                                            SidebarDrag::Split => {
                                                let height = (f32::from(bounds.size.height)
                                                    - 6.
                                                    - DEVICE_FOOTER_HEIGHT)
                                                    .max(1.);
                                                this.sidebar_split = Some(
                                                    ((f32::from(
                                                        event.position.y - bounds.origin.y,
                                                    ) - 3.)
                                                        / height)
                                                        .clamp(0.1, 0.9),
                                                );
                                            }
                                        }
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                });
                            }
                        });
                        let released = view.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture && event.button == MouseButton::Left
                            {
                                let _ = released.update(cx, |this, cx| {
                                    // A lifted row's release is its drop, not a click.
                                    if this.release_workspace_drag(cx) {
                                        cx.stop_propagation();
                                    } else if this.sidebar_drag.take().is_some() {
                                        this.save_chrome();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

pub(super) fn header(
    label: &'static str,
    font: &FontConfig,
    theme: &Theme,
    look: SidebarLook,
) -> Div {
    div()
        .debug_selector(|| format!("header-{label}"))
        .flex_none()
        .h(px(line_height(font) + 2. * look.density.header_padding()))
        .px(px(look.content_x()))
        .flex()
        .items_center()
        .text_size(px(font.size))
        .text_color(rgb(theme.muted))
        .child(
            div()
                .debug_selector(|| format!("header-label-{label}"))
                .child(label_text(&look.header_label(label))),
        )
}
