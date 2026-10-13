//! A forwarded jump over real connections: the navigation the window starts,
//! and the one answer it sends, on the connection the jump arrived on. The
//! cases follow the Herdr fork's `src/client/shell/tests/client_jump.rs` and
//! the jump cases of its `src/client/client_contract/tests.rs`.

use super::mailbox::{snapshot, welcome};
use super::wiring::{BOOT, answer, received};
use super::*;
use crate::{
    client_contract::jump::NAVIGATION_SETTLE_DEADLINE, connection::ConnectionBridge, menu::Page,
    navigation::NavigationTarget, window::MockPeer,
};
use herdr_client::Method;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const RESOLVE: &str = "client.jump.resolve";

/// What a connection's reader leaves in its contract mailbox once the
/// daemon's welcome and first snapshot arrived.
fn greet(connection: &ConnectionBridge, methods: &[&str]) {
    let mut clients = connection.clients.lock().unwrap();
    clients.apply(welcome(methods));
    clients.apply(snapshot(BOOT));
}

/// Local over `local`, presented and ready for input, with `pane_1` focused
/// and `pane_2` beside it; Build over `remote`, online, with `pane_1` focused
/// and `pane_7` beside it. Both daemons advertise `methods`.
fn attach(
    view: &mut HerdrWindow,
    local: &MockPeer,
    remote: &MockPeer,
    methods: &[&str],
    cx: &mut gpui::Context<HerdrWindow>,
) {
    fleet(view, &["Build"]);
    local.prepare(view, cx);
    view.live.snapshot = Some(Arc::new(machine(
        BOOT,
        vec![
            agent("pane_1", AgentStatus::Idle, 1),
            agent("pane_2", AgentStatus::Done, 2),
        ],
    )));
    view.live.supports_surface = true;
    view.live.error = None;
    view.endpoints[0].live = view.live.clone();
    *view.endpoints[0].connection.inbox.lock().unwrap() = view.live.clone();
    greet(&view.endpoints[0].connection, methods);
    view.endpoints[1].connection.handle = Some(remote.client.handle.clone());
    greet(&view.endpoints[1].connection, methods);
    accept(
        view,
        1,
        machine(
            BOOT,
            vec![
                agent("pane_1", AgentStatus::Idle, 1),
                agent("pane_7", AgentStatus::Blocked, 3),
            ],
        ),
    );
    assert!(view.input_ready());
}

pub(super) fn client(
    cx: &mut gpui::TestAppContext,
) -> (
    MockPeer,
    MockPeer,
    Entity<HerdrWindow>,
    &mut VisualTestContext,
) {
    let local = MockPeer::advertising(&[RESOLVE]);
    let remote = MockPeer::advertising(&[RESOLVE]);
    // Attached after the window's first frame, which measures the terminal
    // and would otherwise leave the prepared surface the wrong size.
    let (view, cx) = window(cx, |_| {});
    view.update(cx, |view, cx| attach(view, &local, &remote, &[RESOLVE], cx));
    (local, remote, view, cx)
}

pub(super) fn jump(
    id: &str,
    endpoint_id: &str,
    boot_id: &str,
    pane_id: &str,
) -> EndpointClientJump {
    EndpointClientJump {
        jump_id: id.into(),
        endpoint_id: endpoint_id.into(),
        boot_id: boot_id.into(),
        pane_id: pane_id.into(),
    }
}

/// Host `index`'s daemon forwards `jump`, as that connection's reader files it.
pub(super) fn arrive(
    view: &Entity<HerdrWindow>,
    index: usize,
    jump: EndpointClientJump,
    cx: &mut VisualTestContext,
) {
    view.read_with(cx, |view, _| {
        view.endpoints[index]
            .connection
            .mailboxes()
            .deliver(ClientEvent::ClientJump(jump));
    });
}

/// One window tick's worth of contract work at `now`.
pub(super) fn turn(view: &Entity<HerdrWindow>, now: Instant, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.tick_client_contract(now, cx));
}

/// Everything host `index`'s `peer` was asked since the last call. Each
/// request is answered as accepted, which lets the one queued behind it
/// reach the wire, until nothing more comes.
pub(super) fn asked(
    peer: &mut MockPeer,
    view: &Entity<HerdrWindow>,
    index: usize,
    cx: &mut VisualTestContext,
) -> Vec<Value> {
    let mut all = Vec::new();
    loop {
        let batch = received(peer);
        if batch.is_empty() {
            return all;
        }
        for (boot_id, request) in batch {
            assert_eq!(boot_id, BOOT, "a lane request names this daemon's boot");
            let result = if request["method"] == Method::ClientShellSurfaceSet.as_str() {
                json!({
                    "type": "client_shell_surface_set",
                    "active": request["params"]["active"],
                    "projection_revision": 1,
                })
            } else {
                json!({"type": "ok"})
            };
            answer(peer, view, index, &request, json!({"result": result}), cx);
            all.push(request);
        }
    }
}

/// The jump answers among `requests`, as `(jump id, outcome)`.
pub(super) fn answers(requests: &[Value]) -> Vec<(String, ClientJumpOutcome)> {
    requests
        .iter()
        .filter(|request| request["method"] == RESOLVE)
        .map(|request| {
            let params: ClientJumpResolveParams =
                serde_json::from_value(request["params"].clone()).expect("a jump answer");
            (params.jump_id, params.outcome)
        })
        .collect()
}

/// The panes `requests` asked to focus.
pub(super) fn focused(requests: &[Value]) -> Vec<&str> {
    requests
        .iter()
        .filter(|request| request["method"] == "pane.focus")
        .filter_map(|request| request["params"]["pane_id"].as_str())
        .collect()
}

/// Host `index`'s next snapshot shows `pane` focused. A presented host's
/// surface follows, as the daemon projects one with every snapshot.
pub(super) fn focus(
    view: &Entity<HerdrWindow>,
    index: usize,
    pane: &str,
    cx: &mut VisualTestContext,
) {
    view.update(cx, |view, _| {
        update(view, index, |snapshot| {
            snapshot.focused_pane_id = Some(pane.into());
        });
        if index == view.selected_endpoint
            && let Some(snapshot) = &view.live.snapshot
        {
            view.live.surface = Some(surface_for(snapshot));
        }
    });
}

pub(super) fn answer_of(outcome: ClientJumpOutcome) -> Vec<(String, ClientJumpOutcome)> {
    vec![("jump-1".to_owned(), outcome)]
}

#[gpui::test]
fn a_jump_on_the_presented_host_focuses_the_pane_and_answers_once_it_shows(
    cx: &mut gpui::TestAppContext,
) {
    let (mut local, mut remote, view, cx) = client(cx);
    let start = Instant::now();

    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_2"), cx);
    turn(&view, start, cx);

    // The same request a click on the agent's row sends, and the daemon's
    // acceptance of it proves nothing yet.
    let sent = asked(&mut local, &view, 0, cx);
    assert_eq!(focused(&sent), ["pane_2"]);
    assert!(answers(&sent).is_empty());
    turn(&view, start, cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());

    focus(&view, 0, "pane_2", cx);
    turn(&view, start + Duration::from_millis(40), cx);
    let sent = asked(&mut local, &view, 0, cx);
    assert_eq!(answers(&sent), answer_of(ClientJumpOutcome::Focused));
    assert!(focused(&sent).is_empty(), "asked to focus once");

    turn(&view, start + Duration::from_secs(1), cx);
    assert!(asked(&mut local, &view, 0, cx).is_empty(), "answered once");
    assert!(received(&mut remote).is_empty());
}

#[gpui::test]
fn a_jump_to_the_pane_already_presented_is_answered_at_once(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let now = Instant::now();

    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_1"), cx);
    turn(&view, now, cx);

    let sent = asked(&mut local, &view, 0, cx);
    assert_eq!(answers(&sent), answer_of(ClientJumpOutcome::Focused));
    assert_eq!(sent.len(), 1, "nothing was navigated: {sent:?}");
}

#[gpui::test]
fn a_jump_to_another_host_selects_it_with_the_pane_as_target(cx: &mut gpui::TestAppContext) {
    let (mut local, mut remote, view, cx) = client(cx);
    let start = Instant::now();
    let build = host_id(1);

    // Both machines have a `pane_1`; the jump names Build's `pane_7`.
    arrive(&view, 0, jump("jump-1", &build, BOOT, "pane_7"), cx);
    turn(&view, start, cx);

    // The window's own cross-host navigation: Build is presented, and the
    // focus request waits for its surface.
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 1);
        assert_eq!(
            view.pending_navigation
                .as_ref()
                .map(|pending| pending.as_deref()),
            Some(NavigationTarget::Pane("pane_7"))
        );
    });
    // Presenting Build is not enough: its focus is still elsewhere.
    view.update(cx, |view, _| presented(view));
    turn(&view, start, cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());

    focus(&view, 1, "pane_7", cx);
    turn(&view, start + Duration::from_secs(1), cx);

    // Local's daemon asked, so Local's daemon is answered.
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::Focused)
    );
    assert!(answers(&asked(&mut remote, &view, 1, cx)).is_empty());
}

#[gpui::test]
fn a_refused_jump_is_answered_in_the_same_turn_on_its_own_connection(
    cx: &mut gpui::TestAppContext,
) {
    let (mut local, mut remote, view, cx) = client(cx);
    let now = Instant::now();

    // Build's daemon forwards a jump that names a Local pane under a boot
    // Local no longer runs.
    arrive(&view, 1, jump("jump-1", "local", "boot-0", "pane_2"), cx);
    turn(&view, now, cx);

    assert_eq!(
        answers(&asked(&mut remote, &view, 1, cx)),
        answer_of(ClientJumpOutcome::StaleBoot)
    );
    assert!(asked(&mut local, &view, 0, cx).is_empty());
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 0, "a refused jump moves nothing");
        assert_eq!(view.local_error, None, "a tool asked, not the user");
    });
}

#[gpui::test]
fn every_refusal_before_navigating_has_its_outcome(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let now = Instant::now();
    let build = host_id(1);
    view.update(cx, |view, _| reconnecting(view, 1));

    for (index, (jump, expected)) in [
        (
            jump("", "ssh:ffffffffffffffffffffffffffffffff", BOOT, "pane_1"),
            ClientJumpOutcome::NotFound,
        ),
        (
            jump("", "local", BOOT, "pane_7"),
            ClientJumpOutcome::NotFound,
        ),
        (
            jump("", &build, BOOT, "pane_7"),
            ClientJumpOutcome::Unavailable,
        ),
        // A restarted machine is stale before it is anything else.
        (
            jump("", &build, "old-boot", "pane_7"),
            ClientJumpOutcome::StaleBoot,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("jump-{index}");
        arrive(
            &view,
            0,
            EndpointClientJump {
                jump_id: id.clone(),
                ..jump
            },
            cx,
        );
        turn(&view, now, cx);
        assert_eq!(answers(&asked(&mut local, &view, 0, cx)), [(id, expected)]);
    }
    view.read_with(cx, |view, _| assert_eq!(view.selected_endpoint, 0));
}

#[gpui::test]
fn a_jump_that_never_shows_its_pane_times_out_before_the_daemon_does(
    cx: &mut gpui::TestAppContext,
) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();

    arrive(&view, 0, jump("jump-1", &host_id(1), BOOT, "pane_7"), cx);
    turn(&view, start, cx);
    view.read_with(cx, |view, _| assert_eq!(view.selected_endpoint, 1));

    turn(
        &view,
        start + NAVIGATION_SETTLE_DEADLINE - Duration::from_millis(1),
        cx,
    );
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());

    turn(&view, start + NAVIGATION_SETTLE_DEADLINE, cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::Timeout)
    );

    turn(&view, start + NAVIGATION_SETTLE_DEADLINE * 2, cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());
}

#[gpui::test]
fn a_refused_focus_request_is_the_users_to_see_and_the_jump_times_out(
    cx: &mut gpui::TestAppContext,
) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();
    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_2"), cx);
    turn(&view, start, cx);
    let sent = received(&mut local);
    assert_eq!(sent.len(), 1, "one request is in flight at a time");
    assert_eq!(sent[0].1["method"], "pane.focus");

    answer(
        &mut local,
        &view,
        0,
        &sent[0].1,
        json!({"error": {"code": "pane_focus_failed", "message": "the pane cannot take focus"}}),
        cx,
    );
    turn(&view, start + Duration::from_secs(1), cx);
    // Nothing follows the request: the refusal is shown as a click's is,
    // and only what the window presents answers the jump.
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());
    view.read_with(cx, |view, _| {
        let inbox = view.endpoints[0].connection.inbox.lock().unwrap();
        assert!(inbox.error.is_some());
    });

    turn(&view, start + NAVIGATION_SETTLE_DEADLINE, cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::Timeout)
    );
}

#[gpui::test]
fn a_jump_whose_host_the_user_leaves_stays_open_until_its_time_is_up(
    cx: &mut gpui::TestAppContext,
) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();
    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_2"), cx);
    turn(&view, start, cx);
    let sent = asked(&mut local, &view, 0, cx);
    assert_eq!(focused(&sent), ["pane_2"]);

    // The user selects Build with the focus request on its way. The window
    // takes no request back, so nothing ends the jump but its pane or time.
    view.update(cx, |view, cx| {
        assert!(view.select_endpoint(&host_id(1), cx))
    });
    turn(&view, start + Duration::from_secs(1), cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());

    turn(&view, start + NAVIGATION_SETTLE_DEADLINE, cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::Timeout)
    );
}

#[gpui::test]
fn a_jump_ends_when_its_host_restarts_or_goes_away_meanwhile(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();
    let build = host_id(1);
    let replace = |view: &mut HerdrWindow, snapshot: ClientShellSnapshot| {
        let snapshot = Some(Arc::new(snapshot));
        view.endpoints[1].live.snapshot = snapshot.clone();
        view.live.snapshot = snapshot;
    };

    arrive(&view, 0, jump("jump-1", &build, BOOT, "pane_7"), cx);
    turn(&view, start, cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());
    view.update(cx, |view, _| reconnecting(view, 1));
    turn(&view, start, cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::Unavailable)
    );

    view.update(cx, |view, _| {
        let live = &mut view.endpoints[1].live;
        live.status = ConnectionStatus::Connected;
        live.error = None;
    });
    arrive(&view, 0, jump("jump-1", &build, BOOT, "pane_7"), cx);
    turn(&view, start, cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());
    view.update(cx, |view, _| {
        replace(
            view,
            machine("build-boot-2", vec![agent("pane_7", AgentStatus::Idle, 1)]),
        );
    });
    // Build restarted with the same pane ids: the jump's `pane_7` may be
    // another pane now, although the new snapshot shows one focused.
    turn(&view, start, cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::StaleBoot)
    );

    view.update(cx, |view, _| {
        replace(
            view,
            machine(
                "build-boot-2",
                vec![
                    agent("pane_1", AgentStatus::Idle, 1),
                    agent("pane_7", AgentStatus::Idle, 1),
                ],
            ),
        );
    });
    arrive(
        &view,
        0,
        jump("jump-1", &build, "build-boot-2", "pane_7"),
        cx,
    );
    turn(&view, start, cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());
    view.update(cx, |view, _| {
        replace(
            view,
            machine("build-boot-2", vec![agent("pane_1", AgentStatus::Idle, 1)]),
        );
    });
    turn(&view, start, cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::NotFound)
    );
}

#[gpui::test]
fn an_answer_is_dropped_when_its_connection_is_gone(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();
    arrive(&view, 0, jump("jump-1", &host_id(1), BOOT, "pane_7"), cx);
    turn(&view, start, cx);
    asked(&mut local, &view, 0, cx);

    // Local reconnected before the jump settled: the daemon behind the new
    // connection never asked, and the old connection is no longer held.
    let mut reconnected = MockPeer::advertising(&[RESOLVE]);
    view.update(cx, |view, _| {
        let connection = &mut view.endpoints[0].connection;
        connection.handle = None;
        connection.detach(false);
        connection.handle = Some(reconnected.client.handle.clone());
        greet(connection, &[RESOLVE]);
    });
    focus(&view, 1, "pane_7", cx);
    turn(&view, start, cx);
    turn(&view, start + NAVIGATION_SETTLE_DEADLINE, cx);

    assert!(received(&mut local).is_empty());
    assert!(received(&mut reconnected).is_empty());
}

#[gpui::test]
fn a_jump_waits_for_an_open_menu_and_does_not_close_it(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();
    view.update(cx, |view, _| view.menu.page = Some(Page::About));

    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_2"), cx);
    arrive(&view, 0, jump("jump-2", "local", BOOT, "pane_2"), cx);
    turn(&view, start, cx);
    assert!(asked(&mut local, &view, 0, cx).is_empty());
    view.read_with(cx, |view, _| assert_eq!(view.menu.page, Some(Page::About)));

    // The user closes it in time: the jump goes ahead.
    view.update(cx, |view, _| view.menu.page = None);
    turn(&view, start + Duration::from_secs(2), cx);
    let sent = asked(&mut local, &view, 0, cx);
    assert_eq!(focused(&sent), ["pane_2"]);

    // A menu left open is a jump that never showed its pane.
    let (mut local, _remote, view, cx) = client(cx);
    view.update(cx, |view, _| view.menu.page = Some(Page::About));
    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_2"), cx);
    turn(&view, start, cx);
    turn(&view, start + NAVIGATION_SETTLE_DEADLINE, cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::Timeout)
    );
}

#[gpui::test]
fn a_daemon_that_takes_no_answers_is_sent_none(cx: &mut gpui::TestAppContext) {
    // The connection would carry an answer; its welcome did not offer the method.
    let mut local = MockPeer::advertising(&[RESOLVE]);
    let remote = MockPeer::advertising(&[RESOLVE]);
    let (view, cx) = window(cx, |_| {});
    view.update(cx, |view, cx| attach(view, &local, &remote, &[], cx));

    arrive(&view, 0, jump("jump-1", "local", "boot-0", "pane_1"), cx);
    turn(&view, Instant::now(), cx);

    assert!(received(&mut local).is_empty());
}
