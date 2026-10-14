# Vendored Protocol Attribution

The wire declarations in `src/wire.rs`, handshake declarations in
`src/endpoint.rs`, and endpoint JSON fixtures in `tests/fixtures/` are adapted
from Herdr, https://github.com/herdrdev/herdr, licensed under Apache-2.0.
The license is reproduced in `LICENSE-APACHE` in this directory.

Source: Herdr 0.9.1, commit
[`856b64b9bfd41b9d3d82e2375ed5b4bf941fda28`](https://github.com/herdrdev/herdr/commit/856b64b9bfd41b9d3d82e2375ed5b4bf941fda28).
Sources: `src/protocol/{wire,endpoint}.rs`, `src/input/model.rs`,
`src/api/schema/common.rs`, `src/config/model.rs`, and
`tests/fixtures/endpoint-{hello,welcome,snapshot}-v1.json`.
No upstream NOTICE file was present.

Modifications: removed crossterm, ratatui, server/config conversion methods,
internal-only input types, native bincode Decode derives, and server builders;
inlined WindowsKeyRecord, AgentStatus, and ToastHerdrPosition; reformatted
declarations. All retained bincode field and variant orders are unchanged.
Framing was reimplemented with outbound bounds, decoder limits and strict
payload consumption. Surface validation and patch application are client code.

The optional surface encodings in `src/surface_{scroll,delta,reuse}.rs` adapt
`src/protocol/surface_{scroll,delta,reuse}.rs` and
`src/protocol/surface_delta/decode.rs` from Herdr commit
[`65e35a38`](https://github.com/herdrdev/herdr/commit/65e35a38) (scroll
encoding introduced in `80c0c072`, #4711), same license. Only the receivers
are retained. The scroll byte layout, the `SurfaceDelta`/`GridUpdate` field and
variant order, the reuse JSON shape, message kinds, capability names, and
decoder limits are unchanged. Delta decoding uses serde under the shared
bincode byte limit instead of Herdr's native allocation-bounded decoder, then
applies the same structural limits before touching any baseline. The
`surface-*-v1.bin` fixtures in `tests/fixtures/` were produced by Herdr's own
encoders at that commit.

Fork additions adapt the `gjermundgaraba/herdr` fork of Herdr, branch
`custom`, same license. That branch is rebased, so its commits are named
by subject, not hash. From "feat: adopt the client contract in the tui and
drop plugin menu entries": `src/pick.rs` adapts the picker controls in its
`src/protocol/endpoint.rs` and the picker types in its `src/api/schema/ui.rs`.
From "feat: add the client contract for reports, jumps, shared order, and an
unread hold": the agent's `priority_key` and `priority_mark` in `src/wire.rs`
adapt its `src/protocol/wire.rs`, and `MAX_ENDPOINT_COMMAND_BYTES` in
`src/pick.rs` is the limit of that name in its
`src/server/client_commands.rs`. Message kinds, JSON field names and order,
optional fields, serde defaults, and outcome tags are unchanged; the
`schemars` derives and the server-side `ui.pick` and `ui.pick.close`
parameters are not retained.
`tests/fixtures/endpoint-{pick-open,pick-close,pick-resolve,agent-mark-unread,snapshot-shared}-v1.json`
are verbatim copies of that fork's current `tests/fixtures/`.

The client discovery rules also adapt `src/server/socket_paths.rs`,
`src/session.rs`, and `src/config/io.rs` from the same source/license.
The native client's build mode does not implicitly select Herdr's dev session.

