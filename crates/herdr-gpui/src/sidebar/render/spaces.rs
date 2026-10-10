//! The spaces list: per listed host its header, then the rows
//! `space_groups::sidebar_rows` names: pinned sections with their mirrors,
//! group headers and dividers, and each space's own row. Painting, the drop
//! preview, and the reveal all read child positions counted here, so a row is
//! hit where it was painted.

use crate::{
    HerdrWindow, NavigationTarget,
    endpoint::Endpoint,
    sidebar::{
        Indicators, STATUS_WIDTH,
        cell::{Cell, Fold, RowContext, RowData, RowLayout, WorkspaceRow, layout_for},
        drop_preview::{PreviewRow, row_shifts},
        layout::SidebarLook,
        line_height,
        reorder::{MoveBlock, Plan},
        row::{RowIcon, RowLift, RowTree},
        space_groups::{SidebarRow, sidebar_rows},
        tokens::{self, SpaceContext},
        wash,
        workspaces::{
            displayed_workspace_status, workspace_badge, workspace_label, workspace_note,
        },
    },
};
use gpui::{prelude::*, *};
use herdr_client::protocol::{ClientShellSnapshot, ClientShellWorkspace};
use std::collections::HashSet;

/// The spaces list as built, with what the rest of the sidebar reads off it.
pub(super) struct SpaceList {
    pub(super) list: Stateful<Div>,
    /// Each host header's endpoint index and child position in the list, for
    /// the header pinned over it.
    pub(super) host_rows: Vec<(usize, usize)>,
    /// Child position of the selected host's focused space, for the reveal.
    /// Its own row, never a pinned section's mirror of it.
    pub(super) focused: Option<usize>,
    /// A lifted workspace row: each drop unit's rows in the list, and the
    /// move every gap makes, for the pointer handler.
    pub(super) drop_rows: Vec<(usize, usize, Pixels)>,
    pub(super) drop_requests: Vec<Option<MoveBlock>>,
    pub(super) drop_dragged: usize,
    /// A previewed row is still sliding to its place.
    pub(super) sliding: bool,
    /// A listed host shows a plugin agent view.
    pub(super) filtered: bool,
}

/// One host's rows and what every one of them shares.
struct Host<'a> {
    index: usize,
    endpoint: &'a Endpoint,
    selected: bool,
    multi: bool,
    snapshot: &'a ClientShellSnapshot,
    collapsed: &'a HashSet<String>,
    daemon: Option<crate::listening_ports::Daemon>,
    row_cx: RowContext<'a>,
}

/// A space's row as the list places it.
struct Placed<'a> {
    workspace: &'a ClientShellWorkspace,
    /// Position in the snapshot's `workspaces`.
    index: usize,
    tree: RowTree,
    /// The repository this row folds.
    fold: Option<String>,
    /// The pinned section this row mirrors the space for.
    mirror: Option<usize>,
    lift: RowLift,
    shift: Pixels,
    gap: f32,
}

impl HerdrWindow {
    /// Appends every listed host's header and rows to `list`.
    ///
    /// `devices` is the Devices layout's search query while that layout is
    /// shown: it heads every host, even a lone one, lists only the hosts the
    /// query matches, and puts each one's agents in place of its spaces.
    pub(super) fn append_space_rows(
        &self,
        list: Stateful<Div>,
        devices: Option<&str>,
        indicators: Indicators,
        look: SidebarLook,
        width: f32,
        cx: &mut Context<Self>,
    ) -> SpaceList {
        let rows = layout_for(self.config.layout.mode);
        let font = &self.config.sidebar;
        let theme = &self.theme;
        let multi = self.endpoints.len() > 1;
        let headed = multi || devices.is_some();
        let spaces_custom = self.config.usage.inline
            && self.config.sidebar_layout.spaces != crate::config::SpaceLayout::default();
        let row_gap = if spaces_custom {
            f32::from(self.config.sidebar_layout.spaces.row_gap) * line_height(font)
        } else {
            0.
        };
        // The row a workspace menu was opened for keeps looking hovered while
        // the pointer is over the menu.
        let menu_target = self.workspace_menu_target();
        let now = std::time::Instant::now();
        // Child positions in the list: every row pushed is one child.
        let mut children = 0usize;
        let mut built = SpaceList {
            list,
            host_rows: Vec::new(),
            focused: None,
            drop_rows: Vec::new(),
            drop_requests: Vec::new(),
            drop_dragged: 0,
            sliding: false,
            filtered: false,
        };
        for (endpoint_index, endpoint) in self.endpoints.iter().enumerate() {
            if !self.device_visible(&endpoint.id)
                || devices.is_some_and(|query| !self.device_listed(endpoint_index, query))
            {
                continue;
            }
            let selected = endpoint_index == self.selected_endpoint;
            if headed {
                built.host_rows.push((endpoint_index, children));
                built.list = built
                    .list
                    .child(self.host_row(endpoint, selected, look, width, false, cx));
                children += 1;
            }
            if devices.is_some() {
                let (list, rows, focused) = self.append_device_agents(
                    built.list,
                    endpoint_index,
                    indicators,
                    look,
                    width,
                    cx,
                );
                built.list = list;
                if let Some(row) = focused {
                    built.focused = Some(children + row);
                }
                children += rows;
                continue;
            }
            let live = if selected { &self.live } else { &endpoint.live };
            let Some(snapshot) = live.snapshot.as_deref() else {
                continue;
            };
            // Agent rows come after every host: the panel interleaves them.
            // Only a host the panel lists can make the view a filtered one.
            if self.config.show_agents && endpoint.enabled {
                built.filtered |= snapshot.agent_view_label.is_some();
            }
            if multi && endpoint.collapsed {
                continue;
            }
            let host = Host {
                index: endpoint_index,
                endpoint,
                selected,
                multi,
                snapshot,
                collapsed: self.collapsed_for(endpoint_index),
                daemon: crate::listening_ports::Daemon::of(&endpoint.connection.target),
                row_cx: RowContext {
                    indicators,
                    font,
                    theme,
                    look,
                    width,
                    nest: if multi { look.nest_indent() } else { 0. },
                    mark: wash::HostMark::resolve(
                        &self.config.sidebar_style,
                        &endpoint.label,
                        selected,
                        theme,
                    ),
                    host: (multi && endpoint.id != crate::endpoint::LOCAL)
                        .then_some(endpoint.label.as_str()),
                },
            };
            let listed = sidebar_rows(snapshot, host.collapsed);
            // Gap separates one repository from the next, not a parent from
            // the linked worktrees under it, nor a header from its first row.
            // Reset per host: its first row has nothing above to clear.
            let gaps: Vec<f32> = (0..listed.len())
                .map(|position| match position.checked_sub(1) {
                    Some(previous) if listed[position].gap_after(&listed[previous]) => row_gap,
                    _ => 0.,
                })
                .collect();
            let drag = self
                .workspace_drag
                .as_ref()
                .filter(|drag| selected && drag.previewing(&snapshot.workspaces));
            let floating = drag.is_some_and(|drag| drag.floating());
            let plan = drag.and_then(|drag| Plan::new(&snapshot.workspaces, &drag.workspace));
            let unit_of = |row: &SidebarRow<'_>| match row {
                SidebarRow::Workspace(entry) => {
                    plan.as_ref().and_then(|plan| plan.unit_of(entry.index))
                }
                SidebarRow::Marker(_) | SidebarRow::Mirror { .. } => None,
            };
            // Each row's shift for the drop in preview, from the heights the
            // last frame laid out: shifting moves rows, never resizes them.
            let base = children;
            let shifts = plan.as_ref().map_or_else(Vec::new, |plan| {
                built.drop_requests = (0..=plan.len())
                    .map(|slot| plan.request(&snapshot.workspaces, slot))
                    .collect();
                built.drop_dragged = plan.dragged();
                let measured: Vec<_> = listed
                    .iter()
                    .enumerate()
                    .map(|(position, row)| PreviewRow {
                        unit: unit_of(row),
                        marker: matches!(row, SidebarRow::Marker(_)),
                        // A unit keeps the gap above its rows wherever it
                        // lands, the host's first row included.
                        height: self.sidebar_scroll[0]
                            .bounds_for_item(base + position)
                            .map_or(0., |bounds| f32::from(bounds.size.height))
                            + match position {
                                0 if matches!(row, SidebarRow::Workspace(_)) => row_gap,
                                _ => gaps[position],
                            },
                    })
                    .collect();
                let slot = drag
                    .and_then(|drag| drag.target.as_ref())
                    .map_or(plan.dragged(), |target| target.slot);
                row_shifts(&measured, plan.dragged(), slot)
            });
            for (position, row) in listed.iter().enumerate() {
                let to = shifts.get(position).copied().unwrap_or(0.);
                let child = children;
                children += 1;
                let (entry_index, indented, fold, mirror) = match row {
                    SidebarRow::Marker(marker) => {
                        // A header or divider between passed rows slides with them.
                        let shift = match drag {
                            Some(drag) if plan.is_some() => {
                                let (at, moving) =
                                    drag.slide(&format!("\0marker-{position}"), to, now);
                                built.sliding |= moving;
                                px(at)
                            }
                            _ => px(0.),
                        };
                        let element = self
                            .marker_row(marker, snapshot, &endpoint.id, position, &host.row_cx, cx)
                            .when(gaps[position] > 0., |row| row.mt(px(gaps[position])))
                            .when(shift != px(0.), |row| row.top(shift));
                        built.list = built.list.child(element);
                        continue;
                    }
                    SidebarRow::Workspace(entry) => {
                        (entry.index, entry.indented, entry.fold.clone(), None)
                    }
                    SidebarRow::Mirror { index, section } => (*index, false, None, Some(*section)),
                };
                let workspace = &snapshot.workspaces[entry_index];
                let unit = unit_of(row);
                if mirror.is_none() && selected && workspace.focused {
                    built.focused = Some(child);
                }
                // The lifted row and the rows it carries, such as its group's
                // children, follow the pointer together while it floats.
                let carried = unit.is_some_and(|unit| unit == built.drop_dragged) && floating;
                let shift = match (drag, unit) {
                    (Some(drag), Some(_)) if carried => {
                        drag.pin(&workspace.workspace_id, f32::from(drag.offset()), now);
                        drag.offset()
                    }
                    (Some(drag), Some(_)) => {
                        let (at, moving) = drag.slide(&workspace.workspace_id, to, now);
                        built.sliding |= moving;
                        px(at)
                    }
                    _ => px(0.),
                };
                if let Some(unit) = unit {
                    built.drop_rows.push((unit, child, shift));
                }
                // A child closes the group when no child follows it.
                let follows = matches!(
                    listed.get(position + 1),
                    Some(SidebarRow::Workspace(next)) if next.indented
                );
                let element = self.space_row(
                    &host,
                    Placed {
                        workspace,
                        index: entry_index,
                        tree: match (indented, follows) {
                            (false, _) => RowTree::None,
                            (true, true) => RowTree::Child,
                            (true, false) => RowTree::LastChild,
                        },
                        fold,
                        mirror,
                        lift: match (carried, floating) {
                            (true, _) => RowLift::Lifted,
                            (false, true) => RowLift::Passed,
                            (false, false) => RowLift::Resting,
                        },
                        shift,
                        gap: gaps[position],
                    },
                    (rows, spaces_custom, menu_target),
                    cx,
                );
                built.list = if carried {
                    // Painted last so it floats over the rows it passes, while
                    // its layout slot keeps the others' positions stable.
                    built.list.child(deferred(element).with_priority(1))
                } else {
                    built.list.child(element)
                };
            }
        }
        built
    }

    /// One space's row: its own, or with `placed.mirror` a pinned section's
    /// copy of it. A mirror focuses the space and opens its menu like the row
    /// it copies, but is flat, folds nothing, and never lifts for a reorder.
    fn space_row(
        &self,
        host: &Host<'_>,
        placed: Placed<'_>,
        (rows, spaces_custom, menu_target): (&dyn RowLayout, bool, Option<&str>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Placed {
            workspace,
            index,
            tree,
            fold,
            mirror,
            lift,
            shift,
            gap,
        } = placed;
        let Host {
            endpoint,
            selected,
            snapshot,
            collapsed,
            row_cx,
            ..
        } = host;
        let (selected, multi) = (*selected, host.multi);
        let (font, theme, look) = (row_cx.font, row_cx.theme, row_cx.look);
        let layout = look.density;
        let indented = tree != RowTree::None;
        let endpoint_id = endpoint.id.clone();
        let id = workspace.workspace_id.clone();
        // A space two sections pin has a copy in each, so the section tells
        // their element state apart.
        let key = match mirror {
            Some(section) => format!("mirror-{endpoint_id}-{section}-{id}"),
            None => format!("workspace-{endpoint_id}-{id}"),
        };
        let press_id = id.clone();
        let context_id = id.clone();
        let hover_id = id.clone();
        let context_endpoint = endpoint_id.clone();
        let navigate_endpoint = endpoint_id.clone();
        let collapse_endpoint = endpoint_id.clone();
        let grouped = fold.is_some() || indented;
        let fold = fold.map(|key| Fold {
            id: SharedString::from(format!("collapse-{endpoint_id}-{id}")).into(),
            index,
            collapsed: collapsed.contains(&key),
            toggle: Box::new(cx.listener(move |this, _, _, cx| {
                this.toggle_collapsed(&collapse_endpoint, &key);
                cx.notify();
            })),
        });
        let label = workspace_label(workspace, indented);
        // A mirror is one space's copy, so it never stands in for a folded group.
        let shown_status = if self.config.usage.inline && mirror.is_none() {
            displayed_workspace_status(&snapshot.workspaces, workspace, collapsed)
        } else {
            workspace.agent_status
        };
        let lines = if spaces_custom {
            tokens::space_rows(
                &self.config.sidebar_layout.spaces,
                SpaceContext {
                    label,
                    branch: workspace.branch.as_deref(),
                    status: shown_status,
                    ahead_behind: workspace.git_ahead_behind,
                    tokens: &workspace.tokens,
                    indented,
                },
            )
        } else {
            Vec::new()
        };
        let removing_row = selected
            && self.live.status.is_connected()
            && self.removal.as_ref().is_some_and(|removal| {
                removal.pending_for(
                    (self.selection_epoch, endpoint.generation),
                    &snapshot.boot_id,
                    &workspace.workspace_id,
                )
            });
        // Lines under the row start under its label column, clear of the
        // status dot.
        let under_left = look.content_x()
            + row_cx.nest
            + if indented {
                layout.child_indent() + row_cx.indicators.width(font) - STATUS_WIDTH
            } else {
                0.
            }
            + row_cx.indicators.width(font)
            + layout.gap();
        let note = workspace_note(
            workspace,
            crate::worktree_notes::Notes::of(cx),
            &endpoint.id,
        )
        .map(SharedString::from);
        // The note's line and the listening ports ride under the space's own
        // row only; a mirror still shows the note's mark in its badge.
        let note_line = note.clone().filter(|_| mirror.is_none()).map(|note| {
            let endpoint = endpoint_id.clone();
            let workspace = id.clone();
            crate::sidebar::note_line::element(
                &format!("{endpoint_id}-{id}"),
                note,
                (under_left, look.content_x()),
                font,
                theme,
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                cx.stop_propagation();
                this.edit_worktree_note(&endpoint, &workspace, event.position(), window, cx);
            }))
        });
        let ports = (self.config.show_listening_ports && !removing_row && mirror.is_none())
            .then_some(host.daemon.as_ref())
            .flatten()
            .and_then(|daemon| self.listening_ports.get(daemon, &workspace.workspace_id))
            .map(|listed| {
                div()
                    .debug_selector(|| format!("ports-{endpoint_id}-{id}"))
                    .h(px(line_height(font)))
                    .flex_none()
                    .w_full()
                    .min_w_0()
                    .pl(px(under_left))
                    .pr(px(look.content_x()))
                    .text_size(px((font.size * 0.85).round()))
                    .child(crate::listening_ports::chips(
                        listed,
                        (&endpoint_id, &workspace.workspace_id),
                        &self.tunnels,
                        theme,
                        (font.size * 0.85).round(),
                        cx,
                    ))
            });
        let carried = lift == RowLift::Lifted;
        let element = Cell::new(
            rows,
            RowData::Workspace(WorkspaceRow {
                workspace,
                label,
                tree,
                icon: if indented || !self.config.show_repository_icons {
                    RowIcon::None
                } else {
                    self.avatars
                        .as_ref()
                        .filter(|_| host.index == 0)
                        .and_then(|avatars| avatars.image(&workspace.new_workspace_cwd))
                        .map_or(RowIcon::Mark, RowIcon::Avatar)
                },
                fold,
                grouped,
                badge: workspace_badge(
                    workspace,
                    selected.then_some(&self.menu.pr_cache),
                    &self.git,
                    (&self.teleport_marks, &endpoint.id),
                    note.as_deref(),
                    theme,
                ),
                removing: removing_row,
                status: shown_status,
                lines,
            }),
            row_cx,
        )
        .selected(selected && workspace.focused)
        .highlighted(selected && menu_target == Some(id.as_str()))
        .lift(lift)
        .row()
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                if this.navigate_endpoint(
                    &context_endpoint,
                    NavigationTarget::Workspace(&context_id),
                    cx,
                ) {
                    this.open_workspace_menu(&context_id, event.position, window, cx);
                    this.menu.opening_right_click =
                        this.menu.page == Some(crate::menu::Page::Workspace);
                }
            }),
        )
        .when(
            self.menu.page == Some(crate::menu::Page::Workspace),
            |row| {
                let view = cx.entity().downgrade();
                let endpoint = endpoint_id.clone();
                let workspace = id.clone();
                row.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            let bounds = bounds.intersect(&window.content_mask().bounds);
                            window.on_mouse_event(
                                move |event: &MouseDownEvent, phase, window, cx| {
                                    // The overlay dismisses first in bubble order. Use
                                    // clipped row geometry because it occludes our hitbox.
                                    if phase == DispatchPhase::Bubble
                                        && event.button == MouseButton::Right
                                        && bounds.contains(&event.position)
                                    {
                                        let _ = view.update(cx, |this, cx| {
                                            cx.stop_propagation();
                                            if this.navigate_endpoint(
                                                &endpoint,
                                                NavigationTarget::Workspace(&workspace),
                                                cx,
                                            ) {
                                                this.open_workspace_menu(
                                                    &workspace,
                                                    event.position,
                                                    window,
                                                    cx,
                                                );
                                                this.menu.opening_right_click = this.menu.page
                                                    == Some(crate::menu::Page::Workspace);
                                            }
                                        });
                                    }
                                },
                            );
                        },
                    )
                    .absolute()
                    .inset_0()
                    .size_full(),
                )
            },
        )
        .id(SharedString::from(key.clone()))
        // A mirror answers to its own name, so the layout's `row-` selector
        // always names the space's own row.
        .when(multi || mirror.is_some(), |row| {
            row.debug_selector(|| key.clone())
        })
        .on_click(cx.listener(move |this, _, window, cx| {
            this.navigate_endpoint(&navigate_endpoint, NavigationTarget::Workspace(&id), cx);
            window.focus(&this.focus, cx);
        }))
        // Only the selected endpoint's rows arm the hover menu:
        // another endpoint's menu would have to select it first, and
        // resting the pointer must not switch which daemon is shown.
        // The feature is opt-in, so rows stay unarmed without it.
        .when(selected && self.config.features.sidebar_hover_menu, |row| {
            row.on_hover(cx.listener(move |this, hovered: &bool, window, _| {
                this.hover_workspace(&hover_id, *hovered, window);
            }))
        })
        // Holding a press lifts the row for reordering. Another
        // endpoint's rows would have to select it first, and a mirror
        // is a copy with no place of its own to move.
        .when(selected && mirror.is_none(), |row| {
            row.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if event.click_count == 1 {
                        this.press_workspace(&press_id, event.position, cx);
                    }
                }),
            )
        })
        .when(carried, |row| row.cursor_grabbing());
        // The note and ports ride under their row as one list item, so the
        // drop preview still measures one height per workspace.
        if note_line.is_none() && ports.is_none() {
            element
                .when(gap > 0., |row| row.mt(px(gap)))
                .when(shift != px(0.), |row| row.top(shift))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .flex_none()
                .w_full()
                .relative()
                .when(gap > 0., |unit| unit.mt(px(gap)))
                .when(shift != px(0.), |unit| unit.top(shift))
                .child(element)
                .children(note_line)
                .children(ports)
                .into_any_element()
        }
    }
}
