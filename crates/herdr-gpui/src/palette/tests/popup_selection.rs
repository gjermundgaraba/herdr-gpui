//! A plugin key whose entrypoint opens a popup: the Herdr fork's daemon
//! advertises it as `Popup`, so the client holds input until the popup shows,
//! and reads the selection for it as for any plugin action. The terminal
//! client sends the selection for `PluginAction | Popup`, and so does this
//! one; Herdr ignores a selection on a popup command that is not a plugin's.

use super::*;
use herdr_client::scrollback::TextPoint;

fn popup_snapshot() -> ClientShellSnapshot {
    let mut snapshot = snapshot();
    snapshot.commands = vec![ClientShellCommand {
        command_id: "plugin-popup".into(),
        action: ClientShellCommandAction::Popup,
        description: None,
        binding_label: String::new(),
        binding_labels: Vec::new(),
    }];
    snapshot
}

fn selection(pane_id: &str) -> SelectionReadParams {
    SelectionReadParams {
        pane_id: pane_id.into(),
        anchor: TextPoint { row: 2, col: 0 },
        cursor: TextPoint { row: 4, col: 7 },
        content_revision: Some(3),
    }
}

fn invoke(snapshot: &ClientShellSnapshot, target: &Target) -> Value {
    target
        .invocation(snapshot, "plugin-popup", ClientShellCommandAction::Popup)
        .unwrap()
}

#[test]
fn a_popup_command_is_sent_the_targeted_panes_selection() {
    let snapshot = popup_snapshot();
    let pane = snapshot.focused_pane_id.clone().unwrap();
    let target = Target::capture(&snapshot, Some(selection(&pane)));
    assert_eq!(
        invoke(&snapshot, &target)["selection"],
        json!({
            "pane_id": pane,
            "anchor": {"row": 2, "col": 0},
            "cursor": {"row": 4, "col": 7},
            "content_revision": 3,
        })
    );
}

#[test]
fn a_popup_command_is_not_sent_another_panes_selection() {
    let snapshot = popup_snapshot();
    let target = Target::capture(&snapshot, Some(selection("elsewhere")));
    assert_eq!(invoke(&snapshot, &target).get("selection"), None);
}
