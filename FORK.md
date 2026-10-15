# Fork context

This checkout is `gjermundgaraba/herdr-gpui`, a thin personal fork of
`penso/herdr-gpui`. It runs fixes that are still on their way into an upstream
release, and will carry local changes on top of them. Machine-specific paths
live in `FORK.local.md`, which is gitignored and may be absent.

## Remotes and branches

- `origin`: `gjermundgaraba/herdr-gpui` (`remote.pushDefault`)
- `upstream`: `penso/herdr-gpui`
- `main`: an unmodified mirror of upstream `main`
- `custom`: the branch that gets built and run: the latest upstream release tag
  plus a short stack of commits
- `fix/*`, `feat/*` created here: contribution branches, one per upstream pull
  request, based on `upstream/main`
- Every other branch on `origin` is a copy of an upstream branch from when the
  fork was created; do not modify or delete them

Upstream's docs and recipes call its repository `origin`. Here that remote is
`upstream`, so pass the base explicitly: `just check-commits upstream/main` on a
contribution branch, `just check-commits <tag>` on `custom`.

Release tags are `vYYYYMMDD.N`, are cut almost daily, and sit on `main` in
order. List the stack with:

```sh
git log --oneline "$(git describe --tags --abbrev=0 --match 'v*' custom)..custom"
```

GitHub Actions are disabled on `origin`. The inherited workflows belong to
upstream, whose CI already covers pull requests, and none may run from the
fork.

## Pending upstream

Copies of upstream pull requests that no release contains yet. They sit at the
bottom of the stack, directly above the tag.

- Remote transport and public probe for applications:
  `feat/app-remote-transport` (copied in order), no pull request yet.
  Herdr Mobile pins it (see Consumers). `Transport::Remote(Stream)` lets an
  application hand `herdr-client` a stream it connected to a remote daemon
  itself, with health checks and
  `check_liveness` as for SSH. `probe_command` and `probe_candidates` expose
  the discovery probe, whose `ProbeCandidate` names its path, with the
  probe's bounds `PROBE_TIMEOUT` and `PROBE_OUTPUT_LIMIT`
  (`crates/herdr-client/src/connect.rs`,
  `crates/herdr-client/src/ssh/probe.rs`)

## What the fork adds

Describe each local change here, with pointers to its code, when it is added.
What each feature does for a user is in `crates/herdr-gpui/FORK-FEATURES.md`,
which takes the fork's text that upstream's READMEs would otherwise carry.
Keep it in step with this list.

- Fork slots: upstream types carry the fork through one slot each, so its
  code stays in fork files. `herdr-client` forwards the named controls it
  does not handle as `ClientEvent::Control`, and the fork's lane methods are
  `Method::Fork`. In Herdr GPUI the fork's commands are `Command::Fork`, a
  connection has one fork mailbox, which decodes the fork's controls, and a
  window has one `fork` field and one tick call
  (`crates/herdr-client/src/method/fork.rs`, `crates/herdr-gpui/src/fork.rs`,
  `crates/herdr-gpui/src/controls/fork.rs`)
- Sidebar `separator_before`: a token table in `[ui.sidebar.*]` rows may set
  the separator placed before that token, as the Herdr fork's config allows.
  Without it, one such token made the whole sidebar layout fall back to the
  default rows (`crates/herdr-gpui/src/config/sidebar.rs`,
  `crates/herdr-gpui/src/config/sidebar/separator.rs`,
  `crates/herdr-gpui/src/sidebar/tokens.rs`)
- Native plugin picker: the daemon's `endpoint.pick.open.v1` and
  `endpoint.pick.close.v1` controls open and dismiss a filterable list, and
  the answer goes back as `ui.pick.resolve` on the connection that asked. The
  list model is separate from the plugin wiring so other pickers can reuse it
  (`crates/herdr-protocol/src/pick.rs`, `crates/herdr-gpui/src/picker.rs`,
  `crates/herdr-gpui/src/picker/`)
- Contract types: `herdr-protocol` decodes the Herdr fork's `priority_key`
  and `priority_mark`, with the fork's fixtures, and `herdr-client` names the
  fork's lane methods
  (`crates/herdr-protocol/src/wire.rs`, `crates/herdr-client/src/method/fork.rs`)
- Plugin popup selection: a plugin key the daemon advertises as `Popup`,
  because its entrypoint opens a popup, is sent the targeted pane's
  selection as a plugin action is, as the TUI does
  (`crates/herdr-gpui/src/palette.rs`,
  `crates/herdr-gpui/src/palette/tests/popup_selection.rs`)
- Agent picker and workspace list: the `agent_picker` and `workspace_list`
  actions of the daemon's `[keys]` table open two navigators over every
  enabled host, built on the picker's list
  (`crates/herdr-gpui/src/picker/navigator.rs`,
  `crates/herdr-gpui/src/picker/navigator/rows.rs`)
- Client-side keys stay local: `agent_picker`, `workspace_list`,
  `history_back`, `history_forward`, and `mark_unread` are read from the local
  Herdr config even under a host's key profile. The daemon `[keys]` actions
  `history_back` and `history_forward` run upstream's Back and Forward
  (`crates/herdr-gpui/src/keymap/daemon.rs`,
  `crates/herdr-gpui/src/keymap/daemon/client_actions.rs`,
  `crates/herdr-gpui/src/window/server_keys/local.rs`)
- Agent order: under upstream's Priority sort, agents with a daemon
  `priority_key` sort by it, connected hosts first, then by host and
  snapshot order on equal keys, with no client-side recency among keyed
  agents. Upstream's Devices layout and its agents nested under their
  workspaces list each host's agents by the same key. A host that sends no
  keys ranks its agents by upstream's status and recency order, after every
  keyed agent, where the TUI keeps listing order; every host runs the fork's
  daemon, so this rarely shows
  (`crates/herdr-gpui/src/sidebar/agents/priority.rs`, hooked into
  `crates/herdr-gpui/src/sidebar/agents.rs`)
- Space groups: the spaces sidebar starts a collapsible header where the
  `space_group` token changes. Collapse state is per host and in memory, and
  headers do not drag. The TUI's pinned sections are not shown. Upstream's
  spaces loop in `sidebar/render.rs` reads `sidebar_rows`, draws each header
  through `marker_row`, and previews a drop with `preview_shifts`; `sidebar.rs`
  imports the fork's `visible_workspace_entries`, which drops the headers,
  in place of upstream's, so navigation and the rail follow the groups
  (`crates/herdr-gpui/src/sidebar/space_groups.rs`,
  `crates/herdr-gpui/src/sidebar/space_groups/`)
- Mark unread: the daemon `[keys]` action `mark_unread` sends
  `agent.mark_unread` for the focused pane, keeps no local record, and shows
  "Marked unread" when the daemon accepts it
  (`crates/herdr-gpui/src/mark_unread.rs`)
- Color fonts in the terminal: on macOS, the codepoints an `sbix` or `COLR`
  font in `[terminal] fallback` wins are drawn in its own colors, through
  GPUI's emoji path. Config resolution reads each color fallback's cmap into
  `Config.color_glyphs`, off the UI thread, and the painter draws those
  characters in color. It is a local change, not offered upstream for now,
  and edits upstream's `config.rs`, `config/fonts.rs`, `terminal_painter.rs`,
  and `window/render.rs` directly. Go To's preview paints with its own
  painter, so `palette/preview.rs` and `palette/render.rs` pass it the same
  `set_color_glyphs` call as the window
  (`crates/herdr-gpui/src/config/color_fonts.rs`,
  `crates/herdr-gpui/src/terminal_painter/color_glyphs.rs`)

## Consumers

Other projects build against this fork's crates. Keep what they pin alive.

- Herdr Mobile (`~/ws/pers/herdr-mobile`, local only) depends on
  `herdr-client` and `herdr-protocol` by git at a local tag
  `mobile-pin-<date>[-<n>]` on `custom`. A rebase leaves a tagged commit
  alive, so the app keeps building. A new pin is a new tag, never a moved
  one, and older tags stay so earlier app commits still build. The tags and
  `custom` must be pushed before anyone else builds the app. It uses
  `Transport::Remote`, `probe_command`, `probe_candidates`, `ProbeCandidate`,
  `PROBE_TIMEOUT`, and `PROBE_OUTPUT_LIMIT` from the pending upstream copy
  above.

## Sibling fork: Herdr

Local changes on `custom` exist to work with the user's Herdr fork,
`gjermundgaraba/herdr` on branch `custom`, which carries the daemon and the
TUI client. The goal is the same features in both clients. Its path is in
`FORK.local.md`, and its `FORK.md` has the matching section; read that before
changing anything of the first two kinds below.

- Contract: the wire shapes a daemon and its clients exchange, such as snapshot
  fields, endpoint control kinds, and client methods. The Herdr fork's
  `src/protocol` defines them. `crates/herdr-protocol` copies them and never
  invents one. Copy the fixture for a fork addition from the Herdr fork's
  `tests/fixtures/` into `crates/herdr-protocol/tests/fixtures/`, as the
  `endpoint-*-v1.json` fixtures already are, so a shape that changed on one
  side fails a test.
- Mirrored client logic: behavior both clients implement, listed in the table
  below. Port the logic and its test cases, not the file: the sibling checkout
  is a reference, never a build dependency. Name the counterpart in the
  module's header comment.
- Single owner: whatever the daemon computes or a plugin does. It lives in the
  Herdr fork or the plugins repository only. Render what the snapshot says
  instead of recomputing it here.

| Feature | Herdr fork (TUI) | Here | In step |
| --- | --- | --- | --- |
| Sidebar `separator_before` | `src/config/sidebar.rs`, `src/config/sidebar/separator.rs`, `src/ui/sidebar/tokens.rs` | `crates/herdr-gpui/src/config/sidebar.rs`, `crates/herdr-gpui/src/config/sidebar/separator.rs`, `crates/herdr-gpui/src/sidebar/tokens.rs` | yes |
| Plugin picker | `src/client/shell/picks.rs`, `src/client/shell/pick_overlay.rs` | `crates/herdr-gpui/src/picker.rs`, `crates/herdr-gpui/src/picker/` | yes, except a picker that arrives while a menu, a dialog, or another picker or navigator is open: Herdr GPUI answers `cancelled`, the TUI replaces its overlay and shows the picker; and Herdr GPUI sends a picker's answer only after the request its connection holds is answered |
| Agent priority order | `src/client/shell/aggregate_navigation/agent_order.rs` | `crates/herdr-gpui/src/sidebar/agents/priority.rs`, `crates/herdr-gpui/src/sidebar/agents.rs` | yes: both sort by the daemon's `priority_key`, whose `space_priority` lift neither client configures, and break ties between equal keys by host, then snapshot order, except for agents without a key, from a host that sends none: Herdr GPUI ranks them by upstream's status and recency order, the TUI keeps their listing order; under a plugin agent view: Herdr GPUI keeps each host's own view order, the TUI applies the active host's view across hosts; and for a host whose connection is up but that is not online (a moment during a refused handshake or an outage): Herdr GPUI's panel ranks it as connected, while the TUI ranks it as stale |
| Space groups and pinned sections | `src/client/shell/space_groups.rs`, `src/space_order.rs` | `crates/herdr-gpui/src/sidebar/space_groups.rs`, `crates/herdr-gpui/src/sidebar/space_groups/group_row.rs` | no: Herdr GPUI does not show pinned sections, which the TUI resolves from its own config; group headers do not drag in Herdr GPUI, and it keeps collapse state in memory where the TUI saves it to its preferences file |
| Mark unread | `src/client/shell/mark_unread.rs` | `crates/herdr-gpui/src/mark_unread.rs` | yes |
| Agent and workspace pickers | `src/client/shell/aggregate_navigation/agent_picker.rs`, `src/client/shell/overlay_input.rs` | `crates/herdr-gpui/src/picker/navigator.rs`, `crates/herdr-gpui/src/picker/navigator/rows.rs` | no: Herdr GPUI lacks the Tree layout, the second detail line, and the current marker, and under a plugin agent view its agent picker lists each host's own view where the TUI applies the active host's view across hosts |
| Back/Forward history | `src/client/shell/history.rs` | `crates/herdr-gpui/src/endpoint/history.rs` (upstream's), `crates/herdr-gpui/src/keymap/daemon.rs` (the fork's `history_back` and `history_forward` keys) | yes: the TUI ports upstream's per-host trail, except that after a pane opened from an activity-bar section, Back in Herdr GPUI first returns to that section, which the TUI does not have |
| Client-side keys stay local | `src/config/keybinds/client_actions.rs`, `src/client/shell/client_actions.rs` | `crates/herdr-gpui/src/keymap/daemon/client_actions.rs` | yes, except a direct key equal to the host profile's prefix key (the TUI runs the action, Herdr GPUI arms the prefix) and a local config that does not parse (the TUI keeps the previous five bindings, Herdr GPUI unbinds them) |
| Plugin popup selection | `src/client/shell/actions.rs` | `crates/herdr-gpui/src/palette.rs` | yes: both send the pane selection with a plugin key the daemon advertises as a popup |

Add a row, in both `FORK.md` files, when a mirrored feature lands in either
client.

A change to contract or mirrored code is finished only when the other side
matches. The Herdr fork stays a separate task the user asks for (see Upstream
guardrails), so do not edit it from here: tell the user which change it now
needs, and mark the row as behind until that lands.

Local changes may require the Herdr fork's daemon. A change meant for upstream
may not: it has to work against stock Herdr, so it uses nothing from the fork's
contract, and any field stock Herdr does not send defaults when absent.

## Working on the commit stack

- The stack has three layers, bottom to top: copies of upstream pull requests,
  the fork context commit (`docs: add fork context for agents`), then local
  changes, one commit per feature. Fold follow-ups into the commit they belong
  to with `git commit --fixup <commit>` and
  `GIT_SEQUENCE_EDITOR=: git rebase -i --autosquash <tag>`. Keep generated files
  and unrelated formatting out of those commits.
- A change meant for upstream is written on a contribution branch from
  `upstream/main`, never on `custom`, so the pull request carries no fork
  files. Copy it into the stack with `git cherry-pick -x` and add a row above.
  To offer a local change upstream, cherry-pick its commit onto a new branch
  from `upstream/main`.
- The contribution branch is the source of truth while its pull request is
  open. A copy is stale when the commit in its `cherry picked from` line is no
  longer the branch tip; replace it in place with `git rebase -i <tag>`,
  changing the old copy's line to `exec git cherry-pick -x <new-sha>`.
- Move to a new release with `git rebase vNEW`. Never merge `main` into the
  stack, and do not rebase `custom` onto `main` to get a merged pull request
  sooner. A copy the release contains is skipped, or conflicts if it differs
  from what was merged; confirm the release has it, then `git rebase --skip`:

  ```sh
  git merge-base --is-ancestor \
    "$(gh pr view N --repo penso/herdr-gpui --json mergeCommit --jq .mergeCommit.oid)" \
    vNEW && echo released
  ```

  Then remove its row, and delete its contribution branch and worktree.
- A clean rebase does not prove compatibility. If an upstream change interacts
  with a customization, changes existing behavior, or leaves you unsure whether
  a customization still works, stop and ask the user. Do not remove, adapt, or
  disable a customization on your own.
- Only the tip of the stack has to build and pass checks.
- Push only when asked, with `git push --force-with-lease origin custom`.

## Upstream guardrails

Upstream's `AGENTS.md` still applies to code changes, and its pull request
rules apply to every contribution branch. Its release instructions are for the
upstream owner: never run `just release`, `just sign-release`, or the `release`
skill, and never dispatch upstream workflows.

- Never push to `upstream`. Pull requests go from a contribution branch on
  `origin`, and only when the user asks for one.
- Never open a pull request from `custom` or `main`, and never let `FORK.md`,
  the fork line in `AGENTS.md`, or the `.gitignore` entry into one.
- No `gh` default repository is set, on purpose. Name the repository in every
  `gh` command that writes:

  ```sh
  gh pr create --repo penso/herdr-gpui --head gjermundgaraba:<branch>
  ```

- Upstream's rule against modifying Herdr holds here too. The user's own Herdr
  fork is a separate project with its own `FORK.md`; a change there is a
  separate task the user asks for, never a side effect of work in this
  repository.

## Build and install

Check, build, and replace the installed app:

```sh
just ci
just bundle release   # target/release/Herdr.app
pgrep -lx Herdr       # if it prints a process, the app is running: see below
ditto target/release/Herdr.app /Applications/.Herdr.app.installing
find /Applications -maxdepth 1 -name 'Herdr.app.bak-*' -exec rm -rf {} +
mv /Applications/Herdr.app /Applications/Herdr.app.bak-<label>
mv /Applications/.Herdr.app.installing /Applications/Herdr.app
codesign --verify --deep --strict /Applications/Herdr.app
cmp target/release/Herdr.app/Contents/MacOS/Herdr /Applications/Herdr.app/Contents/MacOS/Herdr
```

Always copy next to the target and rename, never copy over the installed
bundle or symlink it into `target/`. Keep at most one backup, and delete it
once the user confirms the new build works.

The local bundle differs from an upstream release, on purpose:

- It is ad hoc signed with the bundle ID `so.pen.herdr-gpui.dev`, so macOS
  treats it as a different app from the release and asks again for permissions
  such as notifications. There is no Developer ID for this fork; do not sign
  with another identity.
- It reports the version `dev` and has no update key, so the in-app updater is
  disabled. `--build-info` does not name the commit; verify an install with
  `cmp`.
- It saves a GitHub token in a private file beside the GUI config, not in the
  Keychain, so a token saved by a release build has to be entered again.

`just run` builds the same bundle with the QA menu and launches it from
`target/`; it does not install anything.

Herdr GPUI must not be installed through Homebrew alongside the fork build:
`brew install penso/tap/herdr-gpui` puts the upstream release at the same path.
That command is also the rollback to the upstream release.

## Activating a new build

A running Herdr app keeps the old build until it is relaunched. Do not quit or
relaunch it unless the user asks; tell them a relaunch is needed. Closing the
app leaves the Herdr daemon and its terminals running; never stop or restart
the daemon to activate a build.
