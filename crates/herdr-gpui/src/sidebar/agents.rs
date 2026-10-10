//! The agents list: how it is sorted, and how each agent's place and status
//! are labelled. Status comes from the daemon's snapshot, never from guessing
//! at terminal output. The Priority order mirrors the Herdr fork's
//! `src/client/shell/aggregate_navigation.rs`.

use super::{STATUS_DOT_UNKNOWN, STATUS_WIDTH, first_text, label_text, line_height};
use crate::{
    HerdrWindow,
    config::{FontConfig, StatusIcons, Theme},
    herdr_settings::Settings,
};
use gpui::{prelude::*, *};
use herdr_client::protocol::{AgentStatus, ClientShellAgent, ClientShellSnapshot};

pub(super) fn agents_sort(window: &HerdrWindow, cx: &mut Context<HerdrWindow>) -> Stateful<Div> {
    let theme = &window.theme;
    let view = window
        .live
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.agent_view_label.clone());
    let label = view
        .clone()
        .unwrap_or_else(|| window.agent_sort.to_string());
    div()
        .id("agents-sort")
        .debug_selector(|| "agents-sort".into())
        // A plugin's view only names itself; the sort is a button.
        .map(|sort| match &view {
            Some(view) => sort.role(Role::Label).aria_value(format!("Agents: {view}")),
            None => window.chrome_stop(
                crate::window::chrome_focus::Region::Sidebar,
                sort.role(Role::Button)
                    .aria_label(format!("Sort agents: {}", window.agent_sort)),
            ),
        })
        .flex_none()
        .min_w_0()
        .truncate()
        .text_color(rgb(theme.muted))
        .when(view.is_none(), |sort| {
            sort.cursor_pointer()
                .hover(|style| style.text_color(rgb(theme.foreground)))
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.agent_sort = this.agent_sort.toggled();
                    this.agent_sort_modified = true;
                    this.save_chrome();
                    cx.notify();
                }))
        })
        .child(label_text(&label))
}

/// One host's agents before any sort: the daemon's own listing, or under a
/// plugin's agent view (`agent.view.set`) exactly the panes in `agent_order`,
/// in that order.
///
/// A view is authoritative while the daemon names it, so agents its filter
/// excluded stay hidden and an empty order means none match. Upstream's client
/// keys this on the label, not on the order being non-empty, and drops ids
/// the snapshot has no agent for.
fn host_agents(snapshot: &ClientShellSnapshot) -> Vec<&ClientShellAgent> {
    if snapshot.agent_view_label.is_none() {
        return snapshot.agents.iter().collect();
    }
    snapshot
        .agent_order
        .iter()
        .filter_map(|pane_id| {
            snapshot
                .agents
                .iter()
                .find(|agent| agent.pane_id == *pane_id)
        })
        .collect()
}

/// One host's agents in the order the Devices layout lists them under its
/// header: what [`host_agents`] lists, by the daemon's key under Priority
/// unless a plugin view orders them.
pub(super) fn sorted_agents(
    snapshot: &ClientShellSnapshot,
    sort: crate::preferences::AgentSort,
) -> Vec<&ClientShellAgent> {
    let mut agents = host_agents(snapshot);
    if sort == crate::preferences::AgentSort::Priority && snapshot.agent_view_label.is_none() {
        agents.sort_by(|left, right| by_priority_key(left, right));
    }
    agents
}

/// The daemon's `priority` order between two agents: their keys as plain byte
/// strings, smallest first, and an empty key, which a daemon that computes
/// none sends, after every other one. A key is opaque and never read.
fn by_priority_key(left: &ClientShellAgent, right: &ClientShellAgent) -> std::cmp::Ordering {
    let (left, right) = (&left.priority_key, &right.priority_key);
    left.is_empty()
        .cmp(&right.is_empty())
        .then_with(|| left.as_bytes().cmp(right.as_bytes()))
}

/// Every listed host's agents in the order the panel paints them, with the
/// host's endpoint index. `hosts` come in sidebar order, each with whether it
/// is online ([`crate::endpoint::Endpoint::online`]).
///
/// Grouped lists one host's agents after another's, each in its daemon's
/// listing order. Priority is the daemon's order: each agent's `priority_key`
/// is its place in it, with the priority-token lift applied, and keys from
/// different daemons compare the same way. So the panel only compares keys, as
/// plain byte strings, smallest first, and never reads one; an empty key
/// comes after every other. Equal keys keep the sidebar's host order, then
/// the agents' order in their snapshot, which the stable sort gives for free.
/// This client's own rule comes before any key: the last-known agents of a
/// host that is not online follow those of the online ones.
///
/// A plugin view orders only its own host's agents, so while any host shows
/// one, hosts are not merged: the view's host lists what the view orders, and
/// every other host its own agents, by key under Priority.
///
/// Mirrors `priority_order` in the Herdr fork's
/// `src/client/shell/aggregate_navigation.rs`; its cases are ported in
/// `tests/agent_order.rs`.
pub(super) fn panel_agents<'a>(
    hosts: impl IntoIterator<Item = (usize, bool, &'a crate::state::LiveState)>,
    sort: crate::preferences::AgentSort,
) -> Vec<(usize, &'a ClientShellAgent)> {
    let by_key = sort == crate::preferences::AgentSort::Priority;
    let hosts: Vec<_> = hosts
        .into_iter()
        .filter_map(|(index, online, live)| Some((index, online, live.snapshot.as_deref()?)))
        .collect();
    let viewed = hosts
        .iter()
        .any(|(_, _, snapshot)| snapshot.agent_view_label.is_some());
    let mut rows = Vec::new();
    for &(index, online, snapshot) in &hosts {
        let mut agents = host_agents(snapshot);
        if by_key && viewed && snapshot.agent_view_label.is_none() {
            agents.sort_by(|left, right| by_priority_key(left, right));
        }
        rows.extend(agents.into_iter().map(|agent| (index, online, agent)));
    }
    if by_key && !viewed {
        rows.sort_by(|(_, left_online, left), (_, right_online, right)| {
            (!left_online)
                .cmp(&!right_online)
                .then_with(|| by_priority_key(left, right))
        });
    }
    rows.into_iter()
        .map(|(index, _, agent)| (index, agent))
        .collect()
}

/// What an agent is called wherever it is listed.
pub(crate) fn agent_name(agent: &ClientShellAgent) -> &str {
    first_text(agent_names(agent), "agent")
}

pub(super) fn agent_names(agent: &ClientShellAgent) -> [Option<&str>; 4] {
    [
        agent.display_agent.as_deref(),
        agent.name.as_deref(),
        agent.agent.as_deref(),
        agent.title.as_deref(),
    ]
}

/// Where an agent runs: its workspace, and its tab when that earns a place,
/// which upstream decides by the workspace having several tabs or the user
/// naming it. `None` once the agent's workspace has gone.
pub(super) fn agent_place<'a>(
    agent: &ClientShellAgent,
    snapshot: &'a ClientShellSnapshot,
) -> Option<(&'a str, Option<&'a str>)> {
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == agent.workspace_id)?;
    let tabs = snapshot
        .tabs
        .iter()
        .filter(|tab| tab.workspace_id == agent.workspace_id)
        .count();
    let tab = snapshot
        .tabs
        .iter()
        .find(|tab| tab.tab_id == agent.tab_id)
        .filter(|tab| tabs > 1 || tab.custom_label)
        .map(|tab| tab.label.as_str());
    Some((workspace.label.as_str(), tab))
}

/// Upstream's default agent rows: host, workspace and tab on the first line,
/// the agent itself on the second. A pane whose workspace has gone leaves the
/// agent to name the row.
pub(super) fn agent_labels<'a>(
    name: &'a str,
    place: Option<(&'a str, Option<&'a str>)>,
    host: Option<&'a str>,
) -> (Vec<(&'a str, bool)>, &'a str) {
    let Some((workspace, tab)) = place else {
        return (vec![(name, true)], "");
    };
    // Only the workspace carries the row's weight: upstream paints the host and
    // tab around it in its secondary color.
    let segments = [(host, false), (Some(workspace), true), (tab, false)]
        .into_iter()
        .filter_map(|(text, primary)| Some((text?, primary)))
        .filter(|(text, _)| !text.is_empty())
        .collect();
    (segments, name)
}

/// Prepared, comparable props for the cached sidebar, never loaded during render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Indicators {
    /// The set every status is drawn with.
    pub(super) icons: StatusIcons,
    /// Whether busy and finished rows carry a bar on their leading edge.
    pub(super) edge_rail: bool,
    colors: [u32; 5],
    /// The opacity of the row a mark sits in, as `f32` bits: moving marks
    /// draw in the window's mark layer, outside the row's own opacity.
    fade: u32,
}

impl Indicators {
    /// Herdr's settings supply only the status colors; its indicator style
    /// is the terminal client's, and this app draws `icons` instead.
    pub(crate) fn new(
        settings: Option<&Settings>,
        icons: StatusIcons,
        edge_rail: bool,
        light: bool,
        theme: &Theme,
    ) -> Self {
        Self {
            icons,
            edge_rail,
            colors: [
                AgentStatus::Unknown,
                AgentStatus::Idle,
                AgentStatus::Working,
                AgentStatus::Done,
                AgentStatus::Blocked,
            ]
            .map(|status| {
                settings.map_or_else(
                    || status_style(status, theme).2,
                    |settings| theme.ink(settings.status_color(status, light)),
                )
            }),
            fade: 1f32.to_bits(),
        }
    }

    /// The same marks in a row drawn at `opacity`.
    pub(super) fn faded(self, opacity: f32) -> Self {
        Self {
            fade: opacity.to_bits(),
            ..self
        }
    }

    pub(super) fn fade(self) -> f32 {
        f32::from_bits(self.fade)
    }

    /// The same colors drawn with another set, as a preview of it.
    pub(crate) fn with_icons(self, icons: StatusIcons) -> Self {
        Self { icons, ..self }
    }

    pub(crate) fn color(self, status: AgentStatus) -> u32 {
        self.colors[match status {
            AgentStatus::Unknown => 0,
            AgentStatus::Idle => 1,
            AgentStatus::Working => 2,
            AgentStatus::Done => 3,
            AgentStatus::Blocked => 4,
        }]
    }

    pub(crate) fn width(self, font: &FontConfig) -> f32 {
        match self.icons {
            StatusIcons::Dots => STATUS_WIDTH,
            StatusIcons::Tiles => tile_size(font),
            _ => icon_size(font),
        }
    }

    /// Whether a row centers the mark across all its lines. Drawn icons are
    /// taller than a line and read as the row's status, not its first line's.
    pub(crate) fn centered(self) -> bool {
        self.icons != StatusIcons::Dots
    }

    /// The edge rail's bar on a row's leading edge, for the statuses worth
    /// noticing. Absolutely placed, so the row must be `relative`.
    pub(super) fn rail(self, status: AgentStatus, key: &str) -> Option<Div> {
        if !self.edge_rail
            || !matches!(
                status,
                AgentStatus::Working | AgentStatus::Done | AgentStatus::Blocked
            )
        {
            return None;
        }
        Some(
            div()
                .debug_selector(|| format!("rail-{key}"))
                .absolute()
                .left_0()
                .top(relative(RAIL_INSET))
                .bottom(relative(RAIL_INSET))
                .w(px(RAIL_WIDTH))
                .rounded_r(px(RAIL_WIDTH))
                .bg(rgb(self.color(status))),
        )
    }
}

const RAIL_WIDTH: f32 = 3.;
/// The bar's inset from the row's top and bottom, as a share of its height,
/// so even a one-line Compact row at the smallest font keeps a visible bar.
const RAIL_INSET: f32 = 0.2;

/// A drawn icon's side: as tall as a line fits, since compact rows are one
/// line with no padding to spill into, and even so its 16-unit grid lands on
/// whole device pixels at 1x and 2x.
fn icon_size(font: &FontConfig) -> f32 {
    (line_height(font) / 2.).floor().max(5.) * 2.
}

/// A tile is the same size as the other icons; its mark leaves it two pixels
/// of tint on every side.
fn tile_size(font: &FontConfig) -> f32 {
    icon_size(font)
}

pub(crate) fn status_indicator(
    status: AgentStatus,
    font: &FontConfig,
    indicators: Indicators,
) -> Div {
    status_mark(status, font, indicators, indicators.color(status), false)
}

/// Configured tokens can override color and weight without changing the set;
/// only dots have a weight to change.
pub(super) fn status_mark(
    status: AgentStatus,
    font: &FontConfig,
    indicators: Indicators,
    color: u32,
    bold: bool,
) -> Div {
    if indicators.icons != StatusIcons::Dots {
        return status_icon(status, font, indicators, color);
    }
    // Upstream dots: blocked/done filled, idle hollow, unknown a small dot.
    // Working spins instead, so busy agents read apart from settled ones.
    let (diameter, filled) = match status {
        AgentStatus::Unknown => (STATUS_DOT_UNKNOWN, true),
        AgentStatus::Idle => (STATUS_WIDTH, false),
        _ => (STATUS_WIDTH, true),
    };
    div()
        .w(px(indicators.width(font)))
        .h(px(STATUS_WIDTH))
        .mt(px((line_height(font) - STATUS_WIDTH) / 2.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .when(status == AgentStatus::Working, |slot| {
            slot.child(icon_mark(
                StatusIcons::Dots,
                status,
                STATUS_WIDTH,
                color,
                indicators.fade(),
            ))
        })
        .when(status != AgentStatus::Working, |slot| {
            slot.child(
                div()
                    .size(px(if bold { STATUS_WIDTH } else { diameter }))
                    .rounded_full()
                    .border_1()
                    .when(bold, |dot| dot.border_2())
                    .border_color(rgb(color))
                    .when(filled, |dot| dot.bg(rgb(color))),
            )
        })
}

/// A working dot: a faint ring with a spinning arc, `size` across. Rows draw
/// it at the dot's size, so they keep their geometry.
fn working_orbit(size: f32, color: u32) -> impl IntoElement {
    spin(
        svg()
            .debug_selector(|| "status-orbit".into())
            .path("icons/status-orbit.svg")
            .size(px(size))
            .flex_none()
            .text_color(rgb(color)),
        "status-orbit",
        900,
    )
}

/// Turns `mark` once every `period_ms`. Every spinner of one kind shares an
/// id and the app's synced clock, so all working agents turn in step, and
/// repaints are capped because a sidebar can show many at once. GPUI holds
/// it still under reduced motion.
fn spin(mark: Svg, id: &'static str, period_ms: u64) -> impl IntoElement {
    mark.with_animation(
        id,
        Animation::new(std::time::Duration::from_millis(period_ms))
            .repeat_synced()
            .with_max_fps(30.),
        |mark, delta| mark.with_transformation(Transformation::rotate(percentage(delta))),
    )
}

/// A drawn status icon, centered in its slot. Configured weights do not
/// apply: the icon's strokes are fixed by its asset.
fn status_icon(status: AgentStatus, font: &FontConfig, indicators: Indicators, color: u32) -> Div {
    let (icons, fade) = (indicators.icons, indicators.fade());
    let size = icon_size(font);
    let slot = div().flex_none().flex().items_center().justify_center();
    if icons != StatusIcons::Tiles {
        return slot
            .size(px(size))
            .child(icon_mark(icons, status, size, color, fade));
    }
    slot.size(px(tile_size(font)))
        .rounded(px(crate::config::corners::SMALL))
        .bg(rgba((color << 8) | 0x30))
        .border_1()
        .border_color(rgba((color << 8) | 0x70))
        .child(icon_mark(icons, status, size - 4., color, fade))
}

/// One set's mark for `status` at `size`: its asset, turning while working,
/// or the mark drawn for the sets without assets. The working assets are
/// rings around a turnable center, so spinning the whole icon keeps its
/// outline still. A mark that moves draws in the window's mark layer, so its
/// frames never rebuild the sidebar; `fade` is the opacity of its row.
pub(super) fn icon_mark(
    icons: StatusIcons,
    status: AgentStatus,
    size: f32,
    color: u32,
    fade: f32,
) -> AnyElement {
    // Working always moves; of the settled statuses only a drawn blocked
    // mark can.
    let moves = status == AgentStatus::Working
        || (status == AgentStatus::Blocked && crate::icons::status_icon(icons, status).is_none());
    if !moves {
        return drawn_mark(icons, status, size, color);
    }
    crate::mark_layer::layered(size, fade, move || drawn_mark(icons, status, size, color))
        .into_any_element()
}

fn drawn_mark(icons: StatusIcons, status: AgentStatus, size: f32, color: u32) -> AnyElement {
    if icons == StatusIcons::Dots && status == AgentStatus::Working {
        return working_orbit(size, color).into_any_element();
    }
    let Some(path) = crate::icons::status_icon(icons, status) else {
        return super::animated::mark(icons, status, size, color);
    };
    let mark = svg()
        .path(path)
        .size(px(size))
        .flex_none()
        .text_color(rgb(color));
    if status != AgentStatus::Working {
        return mark.into_any_element();
    }
    spin(
        mark.debug_selector(|| "status-spin".into()),
        "status-spin",
        1400,
    )
    .into_any_element()
}

/// The word the daemon's `state_text` token shows for a status when its
/// sidebar config asks for it. Lowercase, matching the daemon's status names
/// and what the terminal client prints.
pub(crate) fn status_text(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Working => "working",
        AgentStatus::Blocked => "blocked",
        AgentStatus::Done => "done",
        AgentStatus::Idle => "idle",
        AgentStatus::Unknown => "unknown",
    }
}

/// Herdr's own cap on a state label, so a longer one came from a peer that
/// ignored it and is cut where the terminal client would have cut it.
const STATE_LABEL_LIMIT: usize = 80;

/// The words an agent's integration chose for its current status, as Herdr's
/// `state_labels` metadata, else `fallback`. Labels are untrusted display
/// text: bounded and stripped of controls and bidi overrides.
pub(crate) fn state_label<'a>(
    agent: &ClientShellAgent,
    fallback: &'a str,
) -> std::borrow::Cow<'a, str> {
    let key = status_text(agent.agent_status);
    agent
        .state_labels
        .iter()
        .find(|(state, _)| state == key)
        .map(|(_, label)| crate::notifications::safe_text(label, STATE_LABEL_LIMIT))
        .map(|label| label.trim().to_owned())
        .filter(|label| !label.is_empty())
        .map_or(
            std::borrow::Cow::Borrowed(fallback),
            std::borrow::Cow::Owned,
        )
}

/// Upstream draws status from its own palette, defaulting to Catppuccin Mocha,
/// and never from the terminal's ANSI colors. Matching those literals keeps a
/// dot the same color in both clients whatever terminal theme is loaded, where
/// ANSI slots would drift: Xcode Dark paints its cyan purple. Mocha's pastels
/// vanish on light chrome, so [`Theme::ink`] darkens them there, keeping hue.
pub(super) fn status_style(status: AgentStatus, theme: &Theme) -> (f32, bool, u32) {
    let (diameter, filled, color) = match status {
        AgentStatus::Working => (STATUS_WIDTH, true, 0xf9e2af),
        AgentStatus::Blocked => (STATUS_WIDTH, true, 0xf38ba8),
        AgentStatus::Done => (STATUS_WIDTH, true, 0x94e2d5),
        AgentStatus::Idle => (STATUS_WIDTH, false, 0xa6e3a1),
        AgentStatus::Unknown => (STATUS_DOT_UNKNOWN, true, 0x6c7086),
    };
    (diameter, filled, theme.ink(color))
}

#[cfg(test)]
mod tests {
    use super::{Indicators, state_label};
    use crate::config::StatusIcons;
    use herdr_client::protocol::AgentStatus;

    /// Herdr's settings supply the status colors; its indicator style is the
    /// terminal client's, so the chosen set is drawn whatever it says.
    #[test]
    fn shared_palette_props_keep_the_set_and_follow_chrome_contrast() -> anyhow::Result<()> {
        use crate::{config::Theme, contrast::Contrast, herdr_settings::Settings};
        use anyhow::Context as _;
        let settings = Settings::parse_text(
            "[ui]\nstatus_indicators = 'symbols'\n[theme.custom]\nyellow = '#123456'\n",
        )?;
        for light in [false, true] {
            for name in ["Default", "Catppuccin Latte"] {
                let theme = Theme::builtin(name)
                    .context("builtin theme")?
                    .with_contrast(Contrast::High);
                let indicators =
                    Indicators::new(Some(&settings), StatusIcons::Wave, true, light, &theme);
                assert_eq!(
                    indicators,
                    Indicators::new(Some(&settings), StatusIcons::Wave, true, light, &theme)
                );
                assert_eq!(indicators.icons, StatusIcons::Wave);
                for status in [
                    AgentStatus::Unknown,
                    AgentStatus::Idle,
                    AgentStatus::Working,
                    AgentStatus::Done,
                    AgentStatus::Blocked,
                ] {
                    assert_eq!(
                        indicators.color(status),
                        theme.ink(settings.status_color(status, light))
                    );
                    for background in [theme.background, theme.surface, theme.active] {
                        assert!(
                            crate::contrast::ratio(indicators.color(status), background)
                                >= Contrast::High.mark_ratio()
                        );
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn prepared_custom_palette_keeps_each_authoritative_status_color() {
        let indicators = Indicators {
            icons: StatusIcons::Dots,
            edge_rail: false,
            colors: [0x112233, 0x223344, 0x334455, 0x445566, 0x556677],
            fade: 1f32.to_bits(),
        };
        for (status, expected) in [
            (AgentStatus::Unknown, 0x112233),
            (AgentStatus::Idle, 0x223344),
            (AgentStatus::Working, 0x334455),
            (AgentStatus::Done, 0x445566),
            (AgentStatus::Blocked, 0x556677),
        ] {
            assert_eq!(indicators.color(status), expected);
            assert_eq!(
                indicators.with_icons(StatusIcons::Orca).color(status),
                expected
            );
        }
    }

    /// An integration's label names the agent's current status; a label for
    /// another status, a blank one, or none leaves the fallback. Labels are
    /// untrusted: controls and bidi overrides go, and length is bounded.
    #[test]
    fn state_labels_name_the_current_status_as_safe_bounded_text() {
        let agent = |status, labels: &[(&str, &str)]| herdr_client::protocol::ClientShellAgent {
            pane_id: "p".into(),
            workspace_id: "w".into(),
            tab_id: "t".into(),
            name: None,
            display_agent: None,
            agent: None,
            title: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_status: status,
            state_change_seq: 0,
            state_labels: labels
                .iter()
                .map(|(state, label)| ((*state).into(), (*label).into()))
                .collect(),
            tokens: Vec::new(),
            focused: false,
            priority_key: String::new(),
            priority_mark: None,
        };
        let labels = [("working", "deep in the mines"), ("blocked", "  ")];
        for (status, fallback, expected) in [
            (AgentStatus::Working, "working", "deep in the mines"),
            (AgentStatus::Blocked, "blocked", "blocked"),
            (AgentStatus::Done, "done", "done"),
            (AgentStatus::Unknown, "", ""),
        ] {
            assert_eq!(state_label(&agent(status, &labels), fallback), expected);
        }
        let hostile = agent(
            AgentStatus::Working,
            &[("working", "\u{1b}[31mred\u{202e}dlrow\n")],
        );
        assert_eq!(state_label(&hostile, "working"), "[31mreddlrow");
        let long = "x".repeat(10_000);
        let long = agent(AgentStatus::Idle, &[("idle", &long)]);
        assert_eq!(state_label(&long, "idle").chars().count(), 80);
    }
}
