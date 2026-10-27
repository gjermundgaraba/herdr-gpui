# Fork features

This build of Herdr GPUI comes from `gjermundgaraba/herdr-gpui`, a personal
fork of `penso/herdr-gpui`. It is meant for the daemon of the
`gjermundgaraba/herdr` fork of Herdr, branch `custom`, and its terminal
client: the two clients show the same things. `README.md` describes
everything the fork leaves as upstream ships it; this file describes only
what the fork adds or changes.

Against stock Herdr the additions stay inert. A daemon that sends none of the
fork's controls or key actions decodes as before, and a method it does not
advertise (`ui.pick.resolve`, `agent.mark_unread`) is refused before it is
sent.

## Sidebar

- **Row separators.** A token table in the daemon's `[ui.sidebar.*]` rows may
  set `separator_before`, the separator drawn before that token when an
  earlier token of the row is visible. `""` joins the two directly; without
  it the default separator stays. Control characters are dropped. Upstream
  rejects the key, so one such token made the whole sidebar layout fall back
  to the default rows.

## Agent shortcuts

The indexed, next, and previous agent shortcuts step through the Agents
panel's rows, passing over the agents of a host that is not online: the
panel keeps showing what that host last sent while it reconnects, but a key
does not land there.

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

## Plugin keys that open a popup

The fork's daemon advertises a plugin key whose entrypoint opens a popup as a
popup command, so input waits for the popup to show. Such a key still gets
the text selected in the pane it targets, as a plugin action does; upstream
sends the selection with plugin actions only. The terminal client sends it
too, and Herdr ignores it on a popup command that is not a plugin's.

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
  top row selected and the agent's status; another host's agents carry its
  name. While any host shows a plugin's agent view, the picker lists what the
  panel does then.
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

## Color fonts in the terminal

On macOS, a color font in the GUI config's `[terminal] fallback`, such as an
`sbix` spinner or icon font, draws the characters it supplies in its own
colors. Upstream draws every glyph that is not Apple Color Emoji's in the
text color. A character is the color font's when neither the terminal's
family nor an earlier fallback has it, as Core Text picks fonts. Linux and
Windows still draw such fonts in the text color. No daemon support is
needed.

## The front window's agent on the command line

For a launcher or another tool that adapts to the agent the user is typing
into. `window list` and `status` give each window `agent`, the agent Herdr
reports in the focused pane on the window's selected host (its `agent`, such
as `claude`, `codex`, or `pi`; null without one or while that host is not
connected), and `front`, set on the main window the user last focused. Unlike
`active`, `front` stays set while another app is in front. The `agent` event
(`events --kind agent`) names the front window and its agent whenever either
changes: another window comes to the front, focus moves to another pane, tab,
or workspace, an agent starts or exits there, or the window selects another
host, SSH hosts included. Nothing else prints it, and closing the last
window prints none. It reads the agent stock Herdr already sends, so no
daemon support is needed.

## Library additions

### `herdr-protocol`

- `pick` holds the JSON shapes of a plugin's `ui.pick`: the `PICK_OPEN_KIND`
  and `PICK_CLOSE_KIND` controls (`EndpointPickOpen`, `EndpointPickClose`),
  an item, the create row, and the outcome and parameters of the
  `ui.pick.resolve` answer, and the daemon's `MAX_ENDPOINT_COMMAND_BYTES`.

Unknown fields are ignored and optional ones default, as generation 1 grows
by optional fields. Each shape is checked against the fork's own fixture in
`crates/herdr-protocol/tests/fixtures/`.

### `herdr-client`

- Named controls the session does not handle itself, other than the health
  pong, are not ignored as upstream ignores them: they reach the owner as
  `ClientEvent::Control { kind: String, data: String }`, unread, and the
  owner ignores the kinds it has no use for. Herdr GPUI decodes
  `endpoint.pick.open.v1` and `endpoint.pick.close.v1` from it, and drops a
  payload it cannot decode like an unknown control instead of treating it as
  a protocol failure.
- `Method::Fork` names the fork's client-lane methods:
  `ForkMethod::UiPickResolve` (`protocol::pick::UiPickResolveParams`) and
  `ForkMethod::AgentMarkUnread` (`{"pane_id"}`). A daemon that does not
  advertise one rejects it before it is sent, like any other unadvertised
  method. A picker's answer goes on the connection that received the
  control; the daemon answers a late one `pick_not_found`, which a client
  expects.
- The daemon closes a connection whose lane request is longer than
  `MAX_ENDPOINT_COMMAND_BYTES` (1 MiB). `ClientHandle::request` writes what
  it is given, so Herdr GPUI measures each fork request that can grow as
  `request` will write it, under the longest request id it gives, before it
  queues it. A picker's answer too long is answered `cancelled` instead (see
  Plugin pickers).
