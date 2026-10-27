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

## What the fork adds

Nothing in the code, for now. The stack ends in a commit whose subject starts
with `revert: return custom to stock`, which returns the code, tests, and docs
to the release tag. The tree differs from the tag only in this file, the fork
line in `AGENTS.md`, and the `FORK.local.md` entry in `.gitignore`.

The earlier features, the pending upstream copies, and how features were split
between this fork and Herdr are in the history before that commit:

```sh
git show "$(git log --format=%h -1 --grep='^revert: return custom to stock' custom)^:FORK.md"
```

## Consumers

Other projects build against this fork's crates. Keep what they pin alive.

- Herdr Mobile (`~/ws/pers/herdr-mobile`, local only) depends on
  `herdr-client` and `herdr-protocol` by git at a local tag
  `mobile-pin-<date>[-<n>]` on `custom`. A rebase leaves a tagged commit
  alive, so the app keeps building. A new pin is a new tag, never a moved
  one, and older tags stay so earlier app commits still build. The tags and
  `custom` must be pushed before anyone else builds the app. It uses
  `Transport::Remote`, `probe_command`, `probe_candidates`, `ProbeCandidate`,
  `PROBE_TIMEOUT`, and `PROBE_OUTPUT_LIMIT` from the pending upstream copy,
  which the revert removed from `custom`. Its pin tags still build, because
  the revert adds a commit and rewrites none.

## Sibling fork: Herdr

`gjermundgaraba/herdr` on branch `custom` is the user's fork of the Herdr
daemon and TUI, which this client attaches to. It is back to stock as well.
When a feature returns, restore the contract, mirrored-client, and
parity-table rules from the earlier `FORK.md` along with it.

## Working on the commit stack

- The stack has three layers, bottom to top: copies of upstream pull requests,
  the fork context commit (`docs: add fork context for agents`), then local
  changes, one commit per feature. Fold follow-ups into the commit they belong
  to with `git commit --fixup <commit>` and
  `GIT_SEQUENCE_EDITOR=: git rebase -i --autosquash <tag>`. Keep generated files
  and unrelated formatting out of those commits.
- A change meant for upstream is written on a contribution branch from
  `upstream/main`, never on `custom`, so the pull request carries no fork
  files. Copy it into the stack with `git cherry-pick -x` and list it in this file.
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

  Then remove it from this file, and delete its contribution branch and worktree.
- A clean rebase does not prove compatibility. If an upstream change interacts
  with a customization, changes existing behavior, or leaves you unsure whether
  a customization still works, stop and ask the user. Do not remove, adapt, or
  disable a customization on your own.
- Only the tip of the stack has to build and pass checks.
- While the code is stock, a new release needs no rebase: start `custom` again
  from the new tag with only the files named under "What the fork adds", and
  move the old branch to `backup/custom-<date>`. Keep the `mobile-pin-*` tags.
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
