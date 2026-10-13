//! Space group headers and dividers as the spaces list paints and hit-tests
//! them, in every layout and with several hosts. Which rows there are is
//! `rows.rs`; this is where they land and what a click on each does.

use crate::sidebar::layout_tests::*;
use crate::{
    HerdrWindow,
    config::LayoutMode,
    controls::Command,
    sidebar::space_groups::{SidebarRow, sidebar_rows},
    state::ConnectionStatus,
};
use gpui::{
    AppContext as _, Bounds, Entity, Modifiers, MouseButton, Pixels, Point, VisualTestContext,
    point, px, size,
};
use herdr_client::{ConnectTarget, protocol::*};
use std::{collections::HashSet, sync::Arc};

const REMOTE: &str = "ssh:test";

/// One space `{prefix}1..` per entry of `groups`, flat, in that group. The
/// first is focused.
fn listing(prefix: &str, groups: &[Option<&str>]) -> ClientShellSnapshot {
    let mut listing = snapshot(groups.len());
    for (index, (workspace, group)) in listing.workspaces.iter_mut().zip(groups).enumerate() {
        workspace.label = format!("{prefix}{}", index + 1);
        workspace.branch = Some("main".into());
        workspace.worktree = None;
        workspace.agent_status = AgentStatus::Idle;
        workspace.tokens = group
            .map(|group| ("space_group".to_owned(), group.to_owned()))
            .into_iter()
            .collect();
    }
    listing
}

/// Two groups and two ungrouped spaces:
///
/// ```text
/// 0 ▾ work
/// 1   space-1
/// 2   space-2
/// 3 ▾ play
/// 4   space-3
/// 5 ─────
/// 6   space-4
/// 7   space-5
/// ```
fn local_listing() -> ClientShellSnapshot {
    listing(
        "space-",
        &[Some("work"), Some("work"), Some("play"), None, None],
    )
}

fn grouped_window(
    cx: &mut gpui::TestAppContext,
    mode: LayoutMode,
    multi: bool,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.layout.mode = mode;
            view.config.show_agents = false;
            view.live.snapshot = Some(Arc::new(local_listing()));
            view.live.status = ConnectionStatus::Connected;
            if multi {
                let mut remote = crate::endpoint::Endpoint::new(
                    REMOTE.into(),
                    "Remote".into(),
                    ConnectTarget::Ssh {
                        target: "unused".into(),
                        session: "default".into(),
                    },
                    true,
                );
                remote.live.snapshot = Some(Arc::new(listing(
                    "remote-",
                    &[Some("ops"), Some("ops"), None],
                )));
                remote.live.status = ConnectionStatus::Connected;
                view.endpoints.push(remote);
            }
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(1400.)));
    cx.run_until_parked();
    redraw(cx);
    (view, cx)
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

/// The selector each of a host's rows is painted under, in list order, as the
/// model names the rows. A space's own row answers to its label while one
/// host is listed and to its id once hosts need telling apart.
fn selectors(view: &HerdrWindow, host: usize) -> Vec<String> {
    let multi = view.endpoints.len() > 1;
    let endpoint = &view.endpoints[host];
    let snapshot = view.host_live(host).snapshot.as_deref().unwrap();
    let id = &endpoint.id;
    let collapsed = if host == 0 {
        &view.collapsed_repos
    } else {
        &endpoint.collapsed_repos
    };
    sidebar_rows(&snapshot.workspaces, collapsed)
        .iter()
        .enumerate()
        .map(|(position, row)| match row {
            SidebarRow::Marker(marker) if marker.group.is_some() => {
                format!("space-group-{id}-{position}")
            }
            SidebarRow::Marker(_) => format!("space-divider-{id}-{position}"),
            SidebarRow::Workspace(entry) if multi => format!(
                "workspace-{id}-{}",
                snapshot.workspaces[entry.index].workspace_id
            ),
            SidebarRow::Workspace(entry) => {
                format!("row-{}", snapshot.workspaces[entry.index].label)
            }
        })
        .collect()
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Bounds<Pixels> {
    cx.debug_bounds(selector.to_owned().leak())
        .unwrap_or_else(|| panic!("missing {selector}"))
}

fn painted(cx: &mut VisualTestContext, selector: &str) -> bool {
    cx.debug_bounds(selector.to_owned().leak()).is_some()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let at = bounds(cx, selector).center();
    cx.simulate_click(at, Modifiers::default());
    redraw(cx);
}

/// Every layout that lists spaces, each alone and under host headers. The
/// Devices layout lists each host's agents instead, so it has no groups to
/// draw.
fn every_layout() -> impl Iterator<Item = (LayoutMode, bool)> {
    LayoutMode::ALL
        .into_iter()
        .filter(|mode| mode.lists_workspaces())
        .flat_map(|mode| [(mode, false), (mode, true)])
}

#[gpui::test]
fn headers_dividers_and_rows_stack_in_list_order_in_every_layout(cx: &mut gpui::TestAppContext) {
    for (mode, multi) in every_layout() {
        let (view, cx) = grouped_window(cx, mode, multi);
        let what = format!("{mode} multi={multi}");
        let look = crate::sidebar::layout::for_mode(mode);
        let font = view.read_with(cx, |view, _| view.config.sidebar.clone());
        let sidebar = bounds(cx, "sidebar");
        let hosts = if multi { 2 } else { 1 };
        let mut stacked = Vec::new();
        for host in 0..hosts {
            let (id, rows) = view.read_with(cx, |view, _| {
                (view.endpoints[host].id.clone(), selectors(view, host))
            });
            if host == 0 {
                assert_eq!(rows.len(), 8, "{what}");
                assert_eq!(rows[0], "space-group-local-0", "{what}");
                assert_eq!(rows[3], "space-group-local-3", "{what}");
                assert_eq!(rows[5], "space-divider-local-5", "{what}");
            } else {
                assert_eq!(
                    rows,
                    [
                        "space-group-ssh:test-0",
                        "workspace-ssh:test-w0",
                        "workspace-ssh:test-w1",
                        "space-divider-ssh:test-3",
                        "workspace-ssh:test-w2",
                    ],
                    "{what}"
                );
            }
            if multi {
                stacked.push(format!("host-{id}"));
            }
            stacked.extend(rows);
        }
        let nest = if multi { look.nest_indent() } else { 0. };
        let mut above: Option<(String, Bounds<Pixels>)> = None;
        for selector in &stacked {
            let row = bounds(cx, selector);
            // Rows stay inside the sidebar, and each sits below the last
            // without overlapping it: what is painted is what is hit. A
            // header or divider runs the list's width in every layout, also
            // where a space's row is an inset card.
            assert!(row.left() >= sidebar.left(), "{what}: {selector}");
            assert!(row.right() <= sidebar.right(), "{what}: {selector}");
            if selector.starts_with("space-") {
                assert_eq!(row.left(), sidebar.left(), "{what}: {selector}");
                assert_eq!(row.right(), sidebar.right() - px(1.), "{what}: {selector}");
            }
            assert!(row.size.height > px(0.), "{what}: {selector}");
            if let Some((previous, previous_row)) = &above {
                assert!(
                    row.top() >= previous_row.bottom(),
                    "{what}: {selector} overlaps {previous}"
                );
            }
            if selector.starts_with("space-group-") {
                assert_eq!(
                    row.size.height,
                    px(crate::sidebar::line_height(&font) + look.chrome_height()),
                    "{what}: {selector}"
                );
                // Arrow, name, and count share the header's line: the arrow
                // where the host's rows start, the count ending at their edge.
                let arrow = bounds(cx, &format!("{selector}-arrow"));
                let name = bounds(cx, &format!("{selector}-name"));
                let count = bounds(cx, &format!("{selector}-count"));
                assert_eq!(
                    arrow.left(),
                    sidebar.left() + px(look.content_x() + nest),
                    "{what}: {selector}"
                );
                assert_eq!(
                    name.left(),
                    arrow.right() + px(look.density.host_gap()),
                    "{what}: {selector}"
                );
                assert_eq!(
                    count.right(),
                    sidebar.left()
                        + px(look.content_x() + look.content_width(f32::from(sidebar.size.width))),
                    "{what}: {selector}"
                );
                assert!(name.right() <= count.left(), "{what}: {selector}");
                for part in [arrow, name, count] {
                    assert!(
                        row.top() <= part.top() && part.bottom() <= row.bottom(),
                        "{what}: {selector}"
                    );
                }
                // Expanded, a header shows no status of its own.
                assert!(!painted(cx, &format!("{selector}-status")), "{what}");
            }
            if selector.starts_with("space-divider-") {
                assert!(
                    row.size.height < px(crate::sidebar::line_height(&font)),
                    "{what}"
                );
            }
            above = Some((selector.clone(), row));
        }
        cx.update(|window, _| window.remove_window());
    }
}

#[gpui::test]
fn clicking_a_header_folds_its_group_on_its_own_host(cx: &mut gpui::TestAppContext) {
    for (mode, multi) in every_layout() {
        let (view, cx) = grouped_window(cx, mode, multi);
        let what = format!("{mode} multi={multi}");
        let own = |number: usize| {
            if multi {
                format!("workspace-local-w{}", number - 1)
            } else {
                format!("row-space-{number}")
            }
        };
        let folded = |view: &HerdrWindow| {
            let mut keys: Vec<_> = view.collapsed_repos.iter().cloned().collect();
            keys.sort();
            keys
        };
        // Blocked work hides behind the header that folds it.
        view.update(cx, |view, cx| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).workspaces[1].agent_status =
                AgentStatus::Blocked;
            cx.notify();
        });
        redraw(cx);

        // The group `work` holds the focused space-1 and space-2.
        click(cx, "space-group-local-0");
        view.read_with(cx, |view, _| {
            assert_eq!(folded(view), ["group:work"], "{what}")
        });
        // Its focused space stays, the other goes, and the header counts
        // both and stands in for them with their most urgent status.
        assert!(painted(cx, &own(1)), "{what}");
        assert!(!painted(cx, &own(2)), "{what}");
        let header = bounds(cx, "space-group-local-0");
        let status = bounds(cx, "space-group-local-0-status");
        let count = bounds(cx, "space-group-local-0-count");
        assert!(
            header.contains(&status.center()) && status.right() <= count.left(),
            "{what}"
        );
        // The next header moved up into the room, and navigation skips what
        // is hidden, as the rows do.
        assert_eq!(
            bounds(cx, "space-group-local-2").top(),
            bounds(cx, &own(1)).bottom(),
            "{what}"
        );
        view.read_with(cx, |view, _| {
            let stepped: Vec<_> = view
                .sidebar_workspaces()
                .into_iter()
                .filter(|&(host, _)| host == 0)
                .map(|(_, id)| id)
                .collect();
            assert_eq!(stepped, ["w0", "w2", "w3", "w4"], "{what}");
        });

        // Clicking again unfolds.
        click(cx, "space-group-local-0");
        view.read_with(cx, |view, _| assert!(folded(view).is_empty(), "{what}"));
        assert!(painted(cx, &own(2)), "{what}");

        if multi {
            // Another host's header folds that host's list only, and never
            // selects the host.
            click(cx, "space-group-ssh:test-0");
            view.read_with(cx, |view, _| {
                assert!(view.collapsed_repos.is_empty(), "{what}");
                assert_eq!(
                    view.endpoints[1].collapsed_repos,
                    HashSet::from(["group:ops".to_owned()]),
                    "{what}"
                );
                assert_eq!(view.selected_endpoint, 0, "{what}");
            });
            // Its focused space stays; the rest of the group goes.
            assert!(painted(cx, "workspace-ssh:test-w0"), "{what}");
            assert!(!painted(cx, "workspace-ssh:test-w1"), "{what}");
            assert!(painted(cx, "workspace-ssh:test-w2"), "{what}");
            assert!(painted(cx, &own(1)), "{what}");
        }
        cx.update(|window, _| window.remove_window());
    }
}

#[gpui::test]
fn workspace_shortcuts_step_through_each_space_once(cx: &mut gpui::TestAppContext) {
    let (view, cx) = grouped_window(cx, LayoutMode::default(), true);
    view.read_with(cx, |view, _| {
        let rows = view.sidebar_workspaces();
        // Headers and dividers are not stops: every space of every host
        // once, in the order its rows are painted.
        assert_eq!(
            rows,
            [
                (0, "w0"),
                (0, "w1"),
                (0, "w2"),
                (0, "w3"),
                (0, "w4"),
                (1, "w0"),
                (1, "w1"),
                (1, "w2"),
            ]
        );
        let step = |current, command| {
            crate::sidebar::sidebar_step(&rows, current, 0, command).map(|index| rows[index])
        };
        // Stepping from a group's only space crosses its header and the
        // divider.
        assert_eq!(step(Some(2), Command::NextWorkspace), Some((0, "w3")));
        assert_eq!(step(Some(2), Command::PreviousWorkspace), Some((0, "w1")));
        assert_eq!(step(Some(7), Command::NextWorkspace), Some((0, "w0")));
        // The numbered shortcut counts the selected host's own rows.
        assert_eq!(step(None, Command::WorkspaceNumber(1)), Some((0, "w0")));
        assert_eq!(step(None, Command::WorkspaceNumber(3)), Some((0, "w2")));
        assert_eq!(step(None, Command::WorkspaceNumber(5)), Some((0, "w4")));
        assert_eq!(step(None, Command::WorkspaceNumber(6)), None);
    });
    // The focused space's row is the one the list reveals.
    view.update(cx, |view, cx| {
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        for (index, workspace) in snapshot.workspaces.iter_mut().enumerate() {
            workspace.focused = index == 2;
        }
        snapshot.focused_workspace_id = Some("w2".into());
        cx.notify();
    });
    redraw(cx);
    view.read_with(cx, |view, _| {
        // Host header, header, 2 rows, header: its row is the sixth child of
        // the list.
        assert_eq!(view.sidebar_revealed[0].get(), Some(5));
    });
}

#[gpui::test]
fn the_rail_marks_each_space_once_and_hides_a_folded_group(cx: &mut gpui::TestAppContext) {
    let (view, cx) = grouped_window(cx, LayoutMode::default(), false);
    view.update(cx, |view, cx| {
        view.sidebar_visible = false;
        cx.notify();
    });
    redraw(cx);
    let marks: Vec<_> = (0..5)
        .map(|index| bounds(cx, &format!("rail-workspace-local-w{index}")))
        .collect();
    // One mark per space, top to bottom in server order: no header or
    // divider takes a place between them.
    for pair in marks.windows(2) {
        assert_eq!(pair[1].top(), pair[0].bottom());
    }
    view.update(cx, |view, cx| {
        view.collapsed_repos.insert("group:work".into());
        cx.notify();
    });
    redraw(cx);
    // The folded group keeps its focused space, as the expanded list does.
    assert!(painted(cx, "rail-workspace-local-w0"));
    assert!(!painted(cx, "rail-workspace-local-w1"));
    assert_eq!(
        bounds(cx, "rail-workspace-local-w2").top(),
        bounds(cx, "rail-workspace-local-w0").bottom()
    );
}

/// Presses `row`, lets it lift, and carries it to `y`. Two moves, as the gap
/// resolves against the lifted frame's layout.
fn carry(cx: &mut VisualTestContext, row: Bounds<Pixels>, y: Pixels) {
    cx.simulate_mouse_down(row.center(), MouseButton::Left, Modifiers::default());
    cx.executor().advance_clock(crate::reorder::LIFT_DELAY);
    cx.run_until_parked();
    for _ in 0..2 {
        cx.simulate_mouse_move(
            point(row.center().x, y),
            MouseButton::Left,
            Modifiers::default(),
        );
        redraw(cx);
    }
}

/// Lets go of a carried row at `at`. Without a daemon nothing is sent, so
/// the drag just ends.
fn release(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, at: Point<Pixels>, what: &str) {
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    redraw(cx);
    view.update(cx, |view, _| {
        assert!(view.workspace_drag.is_none(), "{what}");
        view.pending_navigation = None;
    });
}

fn drop_target(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> serde_json::Value {
    view.read_with(cx, |view, _| {
        let drag = view.workspace_drag.as_ref().unwrap();
        assert!(drag.lifted);
        drag.target.as_ref().map(|target| target.params()).unwrap()
    })
}

/// Dragging a space's own row works as in a flat list with headers present:
/// the gap is found among the spaces, the daemon regroups what lands, and
/// the headers the row passes slide with the rows around them instead of
/// being painted over.
#[gpui::test]
fn a_row_dragged_past_a_header_moves_the_header_with_the_rows(cx: &mut gpui::TestAppContext) {
    for mode in [
        LayoutMode::default(),
        LayoutMode::new(
            crate::config::Density::Compact,
            crate::config::Style::Rounded,
        ),
        LayoutMode::Superset,
        LayoutMode::Orca,
        LayoutMode::Minimal,
    ] {
        let (view, cx) = grouped_window(cx, mode, false);
        let what = mode.to_string();
        let names = view.read_with(cx, |view, _| selectors(view, 0));
        let rest: Vec<_> = names.iter().map(|name| bounds(cx, name)).collect();
        // 0 ▾ work, 1 space-1, 2 space-2, 3 ▾ play, 4 space-3, 5 divider,
        // 6 space-4, 7 space-5.
        let at = |cx: &mut VisualTestContext, position: usize| bounds(cx, &names[position]);

        // Down, out of `work` and past `play`'s header and only space.
        carry(cx, rest[1], rest[4].bottom() - px(2.));
        assert_eq!(
            drop_target(&view, cx),
            serde_json::json!({"workspace_ids": ["w0"], "before_workspace_id": "w3"}),
            "{what}"
        );
        let hole = rest[1].size.height;
        // The carried row follows the pointer.
        assert_eq!(
            at(cx, 1).top() - rest[1].top(),
            rest[4].bottom() - px(2.) - rest[1].center().y,
            "{what}"
        );
        // What it passed closed its place together, the header included.
        for position in [2, 3, 4] {
            assert_eq!(
                at(cx, position).top(),
                rest[position].top() - hole,
                "{what}: row {position}"
            );
        }
        // Its own header, the divider, and what follows stayed where they
        // were.
        for position in [0, 5, 6, 7] {
            assert_eq!(at(cx, position), rest[position], "{what}: row {position}");
        }
        // Nothing was sent without a daemon, so every row is back in place.
        release(
            &view,
            cx,
            point(rest[1].center().x, rest[4].bottom() - px(2.)),
            &what,
        );
        for (position, row) in rest.iter().enumerate() {
            assert_eq!(at(cx, position), *row, "{what}: row {position}");
        }

        // Up, out of the ungrouped spaces to the top of the list: it lands
        // above `work`'s header, which slides down with everything passed.
        carry(cx, rest[6], rest[0].top() + px(2.));
        assert_eq!(
            drop_target(&view, cx),
            serde_json::json!({"workspace_ids": ["w3"], "before_workspace_id": "w0"}),
            "{what}"
        );
        let hole = rest[6].size.height;
        for position in [0, 1, 2, 3, 4, 5] {
            assert_eq!(
                at(cx, position).top(),
                rest[position].top() + hole,
                "{what}: row {position}"
            );
        }
        assert_eq!(at(cx, 7), rest[7], "{what}");

        // Up inside its own run, it stays below the run's divider.
        release(
            &view,
            cx,
            point(rest[6].center().x, rest[0].top() + px(2.)),
            &what,
        );
        carry(cx, rest[7], rest[6].top() + px(2.));
        assert_eq!(
            drop_target(&view, cx),
            serde_json::json!({"workspace_ids": ["w4"], "before_workspace_id": "w3"}),
            "{what}"
        );
        assert_eq!(at(cx, 5), rest[5], "{what}");
        assert_eq!(
            at(cx, 6).top(),
            rest[6].top() + rest[7].size.height,
            "{what}"
        );
        cx.update(|window, _| window.remove_window());
    }
}

/// With the daemon's `row_gap` configured, a header or divider clears the
/// row above it and hugs the row below.
#[gpui::test]
fn a_configured_row_gap_attaches_each_marker_to_the_row_below(cx: &mut gpui::TestAppContext) {
    let (view, cx) = grouped_window(cx, LayoutMode::default(), false);
    view.update(cx, |view, cx| {
        view.config.sidebar_layout.spaces.row_gap = 1;
        cx.notify();
    });
    redraw(cx);
    let (names, gap) = view.read_with(cx, |view, _| {
        (
            selectors(view, 0),
            px(crate::sidebar::line_height(&view.config.sidebar)),
        )
    });
    let rows: Vec<_> = names.iter().map(|name| bounds(cx, name)).collect();
    let space = |position: usize| rows[position].top() - rows[position - 1].bottom();
    // 0 ▾ work, 1 space-1, 2 space-2, 3 ▾ play, 4 space-3, 5 divider,
    // 6 space-4, 7 space-5.
    for position in [1, 4, 6] {
        assert_eq!(space(position), px(0.), "row {position} hugs its marker");
    }
    for position in [2, 3, 5, 7] {
        assert_eq!(space(position), gap, "row {position} clears the row above");
    }
}
