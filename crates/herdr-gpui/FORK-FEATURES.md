# Fork features

This build of Herdr GPUI comes from `gjermundgaraba/herdr-gpui`, a personal
fork of `penso/herdr-gpui`. It is meant for the daemon of the
`gjermundgaraba/herdr` fork of Herdr, branch `custom`, and its terminal
client: the two clients show the same things. `README.md` describes
everything the fork leaves as upstream ships it; this file describes only
what the fork adds or changes.

Against stock Herdr the additions stay inert. A daemon that sends none of the
fork's snapshot fields, controls, or key actions decodes as before, and a
method it does not advertise (`client.report`, `client.jump.resolve`,
`ui.pick.resolve`, `agent.mark_unread`) is refused before it is sent.

## Sidebar

- **Row separators.** A token table in the daemon's `[ui.sidebar.*]` rows may
  set `separator_before`, the separator drawn before that token when an
  earlier token of the row is visible. `""` joins the two directly; without
  it the default separator stays. Control characters are dropped. Upstream
  rejects the key, so one such token made the whole sidebar layout fall back
  to the default rows.
- **Space groups.** A space joins a group through its `space_group`
  workspace token, which the daemon sets (`workspace.set_group`). The Spaces
  list never reorders spaces: it shows the daemon's order and starts a header
  wherever the group changes, with a divider where ungrouped spaces follow a
  group. A worktree family takes its parent checkout's group. Each header
  shows the group's name and how many spaces it has; click it to fold the
  group, which then shows only its focused space and its members' most
  urgent status. Runs of one group fold together. Folding is this client's
  own, per host like repository folding, and is not saved. Headers and
  dividers are the same in every layout, sit under their host's row when
  several hosts are listed, and have no mark on the collapsed rail, which
  marks each shown space once. The terminal client also shows pinned
  sections from `[[ui.sidebar.spaces.pinned]]`; this client does not.
- **Moving spaces between groups.** Drag a space's row to move it, with
  headers present as in a flat list: the drop sends `workspace.move_block`,
  and the daemon regroups the space by where it lands. Headers and dividers
  the row passes slide with the rows around them. A header itself is not
  draggable here; the terminal client moves a group's whole run that way.

## Agents panel order

`priority` is the daemon's order, which the fork's daemon sends as each
agent's `priority_key`, priority-token lifts included. One host's agents, or
every host's together, sort by that key compared as plain bytes, smallest
first, so this window and the terminal client list the same agent first. The
key is never read.

- Agents with equal keys keep the sidebar's host order, then their daemon's
  listing order.
- The last-known agents of a host whose connection is down follow the others,
  as upstream orders them.
- An agent without a key, which a daemon that computes none sends (stock
  Herdr among them), follows every keyed agent. Among themselves such agents
  keep upstream's order: attention first, then the most recent change on any
  host. The terminal client keeps them in listing order instead.
- The Devices layout, and agents listed under their workspaces
  (`agents_placement = "nested"`), list each host's agents by the same key.
- While any host shows a plugin's agent view, hosts are not merged: the
  view's host lists what the view orders, and every other host its own
  agents by key. The terminal client applies the active host's view across
  hosts.

`grouped` lists each host's agents in turn, as its daemon lists them, and
ignores the keys. The indexed, next, and previous agent shortcuts step
through the panel's rows, passing over the agents of a host that is not
online: the panel keeps showing what that host last sent while it
reconnects, but a key does not land there.

## Plugin pickers

A plugin's `ui.pick` opens a filterable list in this window when the daemon
sends `endpoint.pick.open.v1`. Typing keeps the items whose label or detail
contains the query, ignoring case, in the plugin's order; a create row, when
the plugin offers one, follows them while the query names no item. Enter
answers with the chosen item or the created text, on the connection that
asked, as `ui.pick.resolve`, and Escape answers `cancelled`. The picker also
goes away when the plugin closes it (`endpoint.pick.close.v1`), the
connection is lost, or the daemon restarts; it answers `cancelled` then
unless the daemon has already given up on it. An answer longer than the
daemon reads in one command, such as created text of about a megabyte, is
answered `cancelled` instead, and the window says why.

A picker is shown only for the host the window presents. One that arrives on
another host's connection, or while this window shows a menu, a dialog, or a
picker of its own, is answered `cancelled` and never shown; the terminal
client closes the overlay it has open and shows the picker instead.

## Daemon `[keys]` actions

The fork's daemon config has five more `[keys]` actions, each unset by
default. Each is also in the palette and can be bound under `[keybindings]`.

- **`mark_unread`** (for example `mark_unread = "prefix+u"`; palette **Mark
  Agent Unread**) sends `agent.mark_unread` for the focused pane to the host
  that owns it: the selected one. That daemon keeps the hold, so every client
  attached to it shows the same thing: the completion badge returns once you
  leave the pane, and arriving at it again clears it. Only an acknowledged
  completion, shown as idle, can be held; the daemon refuses a working,
  blocked, or still-badged agent and a pane without one, and its reason
  appears in the status bar like any other daemon error. A press that cannot
  be sent, because the host is disconnected or has not sent its panes yet,
  says so there too; a press that is sent leaves whatever the status bar
  showed. Once the daemon accepts, `Marked unread` shows briefly over the
  terminal, since nothing else changes until you leave the pane. This client
  keeps no unread record of its own: the badge comes only from the snapshot.
- **`agent_picker`** (`prefix+a`; palette **Agent Picker**) opens a flat list
  of every agent on every enabled host, in the Agents panel's `priority`
  order whatever the panel's own sort or the device filter shows, with the
  top row selected, the agent's status, and the daemon's `priority_mark`
  before the label; another host's agents carry its name. While any host
  shows a plugin's agent view, the picker lists what the panel does then.
  The keyboard starts on the rows: `j`/`k` or Up/Down move, Ctrl-D/Ctrl-U
  move eight rows, Home and End or `G` go to either end, `/` moves to the
  search, and Enter opens the row with the same cross-host navigation as a
  click on it. `b`, `w`, `i`, and `d` narrow the list to blocked, working,
  idle, or done agents and drop the search, Backspace drops that filter, and
  `a` drops both; a search starts from every status again.
- **`workspace_list`** (`prefix+shift+l`; palette **Workspace List**) shows
  workspaces alone, under a row per host when there are several, with the
  search focused and the focused workspace selected. Typing searches, every
  word in any order, through host, workspace, branch, tab, pane label,
  directory, agent, and pane id, and matching panes appear under their
  workspaces while a search or a status filter is active. In the search, Up
  and Down or Ctrl-N and Ctrl-P move and every other key is text; Escape
  leaves the search for the rows, where Left and Right step between the
  workspaces that list panes, and a second Escape closes.

  Both navigators are modes of the window's picker; Go To is unchanged beside
  them. Rows follow the hosts while a navigator is open; when the selected
  row goes away nothing is selected and Enter opens nothing, as in the
  terminal client, until a move selects the first row or the row comes back.
  A host that is not online keeps its last-known rows, dimmed; Enter on one
  of another host's says `<host> is not ready` and leaves the navigator open.
  Unlike the terminal client, a row shows no second line with its workspace,
  tab, and pane id, the focused pane is not marked, and the tree layout (the
  fork's `prefix+g` list) is not offered: Go To covers it.
- **`history_back`** and **`history_forward`** run **Back** and **Forward**:
  like the title bar's buttons, they step along the presented host's trail
  and never cross to another host. When the window left an activity-bar
  section, such as Overview, to open a pane, Back first returns to that
  section; the terminal client has no such sections. On macOS Option-Left
  and Option-Right are pane keys by default and a pane key wins over a
  daemon binding, so bind other keys or clear those two under `[pane_keys]`.

These five are performed by this client, so under a server's key profile
(`[devices.<id>] keybindings = "server"`) they always take the local daemon
`[keys]` bindings, and whatever the server's profile says of them is not
read. They keep their keystrokes over the commands the profile binds to the
same ones, but not over its prefix: a direct key of theirs that is the
server's prefix key starts a chord and does not run the action, where the
terminal client runs it. A `prefix+` chord of theirs is typed after the
server's prefix while its profile applies. A local `config.toml` that does
not parse gives the five no `[keys]` binding until it does, where the
terminal client keeps the ones it last read.

## Client report

Each window tells its local daemon what its Agents list shows, with
`client.report`, so tools can read it through that daemon's `client.list`
and `client.changed`.

The report names whether the window is the active one, the host it presents,
and every host with its status (`connecting`, `online`, `reconnecting`,
`attention` for a host that needs an update or a sign-in, `disabled` for one
switched off or detached) and boot. It then lists the panel's rows in the
order the panel shows them under its current sort, each with the snapshot's
agent record unchanged, its space's label and number, and the pane's
directories. The device picker's filter does not narrow the report: it
always lists every enabled host's agents, in the order the panel has at All
Devices. At most one row is focused: the focused pane of the presented host.
An agent whose space the snapshot no longer lists has no row.

The report goes to the local daemon only, never to another host, and only
when its content changed: again after a new local connection, which is also
how a restarted daemon is met, and one at a time, so changes that arrive
while the daemon has not answered become one report. It keeps to the
daemon's limits (64 hosts, 512 rows, ids of 128 bytes, text of 512), and a
report still larger than one 1 MiB command loses rows from its end. Without
a local connection, or with a daemon that does not offer `client.report`,
nothing is sent, and a report the daemon rejects is logged, not retried.

A window holds one connection per editor group that shows another Herdr tab,
each a client of its own to the daemon. It reports on its own connection and
stays with that one while it holds it, even when a group swap parks it, so
the daemon keeps one report per window. A connection the window lets go of
while a jump it carried is still unanswered is kept, no longer shown in any
group, until that answer is sent and acknowledged or the daemon's 20 seconds
are up.

## Client jump

A tool can ask a window to show a pane with `client.jump`, which the daemon
forwards to the client it names. The window navigates the way a click on the
agent's row does, switching host when the pane is on another one, and
answers `client.jump.resolve` on the connection the jump arrived on,
whichever editor group holds it meanwhile.

The answer comes from what the window holds, checked when the jump arrives
and again on every tick while it waits:

- `not_found` for a host it does not list, or a pane the host's snapshot
  does not list
- `unavailable` for a host without a snapshot, or one that is not online
- `stale_boot` when the host now runs another boot than the jump names
- `focused` once the window presents that host with the pane focused: its
  surface active and drawn from the snapshot that shows the pane focused, not
  when the request was accepted or the host merely selected
- `timeout` for a jump that has not got there after 12 seconds, before the
  daemon's own 20

Nothing follows the navigation's own request: a host that refuses it shows
that in the status bar as it would for a click, and the jump times out, as it
does when you select another host before the pane shows. A jump never closes
a menu or dialog: it waits for the window, and times out if the menu stays
open.

The newest jump wins: one that arrives while another is still open replaces
it, and the replaced jump is never answered, because the daemon told its
caller `jump_superseded` when it forwarded the newer one. A jump to the pane
the window already presents is answered `focused` without navigating, unless
it replaced another, in which case it navigates and is answered once its
pane shows like any other. Jumps reach a window on one connection only, the
one that carries its report. A daemon's `jump_not_found` for an answer that
came too late is ignored and never reaches the status bar. An answer whose
connection has since been replaced is dropped, and a jump this client cannot
read is ignored.

## Library additions

### `herdr-protocol`

- `pick` holds the JSON shapes of a plugin's `ui.pick`: the `PICK_OPEN_KIND`
  and `PICK_CLOSE_KIND` controls (`EndpointPickOpen`, `EndpointPickClose`),
  an item, the create row, and the outcome and parameters of the
  `ui.pick.resolve` answer. `decode_open` and `decode_close` return
  `Error::PickPayload` with the `serde_json` source.
- Snapshot additions, both optional: each agent's `priority_key` and
  `priority_mark`. A daemon without them decodes as an empty key and no mark.
  Keys are opaque: order agents by comparing them as bytes, with an empty key
  after every other one.
- `clients` holds what a client sends and receives: `ClientReport` with its
  endpoints and agent rows (the `client.report` parameters), the daemon's
  `MAX_CLIENT_REPORT_*` limits and `MAX_ENDPOINT_COMMAND_BYTES`, the
  `CLIENT_JUMP_KIND` control (`EndpointClientJump`), and
  `ClientJumpResolveParams` with `ClientJumpOutcome`, the answer a client
  sends as `client.jump.resolve`. `decode_jump` returns
  `Error::ClientJumpPayload` with the `serde_json` source. The socket API
  shapes tools read (`client.list`, `client.changed`, `client.jump`) are a
  tool's, not a client's, and are not declared here.
- `ClientReport::normalize` repeats the daemon's own check of a report: it
  rejects a report the daemon would reject, with a typed
  `ClientReportError`, and cuts display text to the daemon's limits. It does
  not bound the encoded request; `herdr-client` measures that before sending.

Unknown fields are ignored and optional ones default, as generation 1 grows
by optional fields. Each shape is checked against the fork's own fixture in
`crates/herdr-protocol/tests/fixtures/`.

### `herdr-client`

- Named controls the session does not handle itself, other than the health
  pong, are not ignored as upstream ignores them: they reach the owner as
  `ClientEvent::Control { kind: String, data: String }`, unread, and the
  owner ignores the kinds it has no use for. Herdr GPUI decodes
  `endpoint.pick.open.v1`, `endpoint.pick.close.v1`, and
  `endpoint.client-jump.v1` from it, and drops a payload it cannot decode
  like an unknown control instead of treating it as a protocol failure.
- `Method::Fork` names the fork's client-lane methods:
  `ForkMethod::ClientReport` (`protocol::clients::ClientReport`),
  `ForkMethod::ClientJumpResolve` (`protocol::clients::ClientJumpResolveParams`),
  `ForkMethod::UiPickResolve` (`protocol::pick::UiPickResolveParams`), and
  `ForkMethod::AgentMarkUnread` (`{"pane_id"}`). A daemon that does not
  advertise one rejects it before it is sent, like any other unadvertised
  method. A picker's or a jump's answer goes on the connection that received
  the control; the daemon answers a late one `pick_not_found` or
  `jump_not_found`, which a client expects.
- The daemon closes a connection whose lane request is longer than
  `MAX_ENDPOINT_COMMAND_BYTES` (1 MiB). `ClientHandle::request` writes what
  it is given, so Herdr GPUI measures each fork request that can grow as
  `request` will write it, under the longest request id it gives, before it
  queues it. A report too long loses rows from the end of its list until it
  fits, and one that does not fit without rows is not sent. A picker's answer
  too long is answered `cancelled` instead (see Plugin pickers). A jump's
  answer carries only the daemon's jump id and a fixed outcome.
