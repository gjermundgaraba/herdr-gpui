#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The `ui.pick` shapes match the Herdr fork's serialisation. The fixtures
//! are verbatim copies of the fork's `tests/fixtures/` files of the same
//! name, and the compact strings below are what its serde derives emit for
//! `EndpointPickOpen`, `EndpointPickClose`, and `UiPickResolveParams`.

use herdr_protocol::{Error, pick::*};
use serde_json::{Value, json};

const OPEN: &str = include_str!("fixtures/endpoint-pick-open-v1.json");
const CLOSE: &str = include_str!("fixtures/endpoint-pick-close-v1.json");
const RESOLVE: &str = include_str!("fixtures/endpoint-pick-resolve-v1.json");

fn open() -> EndpointPickOpen {
    EndpointPickOpen {
        pick_id: "pick-7".into(),
        title: "Move pane to".into(),
        items: vec![
            UiPickItem {
                id: "t1".into(),
                label: "shell".into(),
                detail: None,
                badge: None,
            },
            UiPickItem {
                id: "t2".into(),
                label: "build".into(),
                detail: Some("3 panes".into()),
                badge: Some("2".into()),
            },
        ],
        selected: Some("t2".into()),
        create: Some(UiPickCreate {
            label: "New tab".into(),
        }),
    }
}

#[test]
fn kinds_are_the_daemons() {
    assert_eq!(PICK_OPEN_KIND, "endpoint.pick.open.v1");
    assert_eq!(PICK_CLOSE_KIND, "endpoint.pick.close.v1");
}

#[test]
fn pick_open_fixture_decodes_and_round_trips() {
    let decoded = decode_open(OPEN).unwrap();
    assert_eq!(decoded, open());
    assert_eq!(
        serde_json::to_value(&decoded).unwrap(),
        serde_json::from_str::<Value>(OPEN).unwrap()
    );
    // Field order and omitted optionals, as the daemon writes the control.
    assert_eq!(
        serde_json::to_string(&decoded).unwrap(),
        r#"{"pick_id":"pick-7","title":"Move pane to","items":[{"id":"t1","label":"shell"},{"id":"t2","label":"build","detail":"3 panes","badge":"2"}],"selected":"t2","create":{"label":"New tab"}}"#
    );
}

#[test]
fn pick_open_optionals_default_and_unknown_fields_are_tolerated() {
    let minimal = decode_open(r#"{"pick_id":"p","title":"","items":[]}"#).unwrap();
    assert_eq!(
        minimal,
        EndpointPickOpen {
            pick_id: "p".into(),
            title: String::new(),
            items: Vec::new(),
            selected: None,
            create: None,
        }
    );
    assert_eq!(
        serde_json::to_string(&minimal).unwrap(),
        r#"{"pick_id":"p","title":"","items":[]}"#
    );
    // Generation 1 grows by optional fields; a newer daemon's are ignored.
    let newer = decode_open(
        r#"{"pick_id":"p","title":"t","items":[{"id":"a","label":"A","icon":"x"}],"multi":true,"selected":null,"create":null}"#,
    )
    .unwrap();
    assert_eq!(newer.items[0].id, "a");
    assert_eq!((newer.selected, newer.create), (None, None));
}

#[test]
fn pick_close_fixture_decodes_and_round_trips() {
    let decoded = decode_close(CLOSE).unwrap();
    assert_eq!(
        decoded,
        EndpointPickClose {
            pick_id: "pick-7".into()
        }
    );
    assert_eq!(
        serde_json::to_string(&decoded).unwrap(),
        r#"{"pick_id":"pick-7"}"#
    );
}

#[test]
fn resolve_params_fixture_covers_every_outcome() {
    let decoded: Vec<UiPickResolveParams> = serde_json::from_str(RESOLVE).unwrap();
    let resolve = |outcome| UiPickResolveParams {
        pick_id: "pick-7".into(),
        outcome,
    };
    assert_eq!(
        decoded,
        [
            resolve(UiPickOutcome::Picked { id: "t2".into() }),
            resolve(UiPickOutcome::Created {
                text: "New tab name".into()
            }),
            resolve(UiPickOutcome::Cancelled),
        ]
    );
    assert_eq!(
        serde_json::to_value(&decoded).unwrap(),
        serde_json::from_str::<Value>(RESOLVE).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&decoded[0]).unwrap(),
        r#"{"pick_id":"pick-7","outcome":{"type":"picked","id":"t2"}}"#
    );
    assert_eq!(
        serde_json::to_value(&decoded[2].outcome).unwrap(),
        json!({"type": "cancelled"})
    );
}

#[test]
fn malformed_pick_payloads_are_typed_errors() {
    use std::error::Error as _;

    for data in [
        "",
        "not json",
        "[]",
        "null",
        r#"{"title":"t","items":[]}"#,
        r#"{"pick_id":"p","items":[]}"#,
        r#"{"pick_id":"p","title":"t"}"#,
        r#"{"pick_id":7,"title":"t","items":[]}"#,
        r#"{"pick_id":"p","title":"t","items":{}}"#,
        r#"{"pick_id":"p","title":"t","items":[{"id":"a"}]}"#,
        r#"{"pick_id":"p","title":"t","items":[{"label":"A"}]}"#,
        r#"{"pick_id":"p","title":"t","items":[],"selected":3}"#,
        r#"{"pick_id":"p","title":"t","items":[],"create":{}}"#,
        r#"{"pick_id":"p","title":"t","items":[]} trailing"#,
    ] {
        let error = decode_open(data).expect_err(data);
        assert!(matches!(error, Error::PickPayload(_)), "{data}");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            error
                .source()
                .is_some_and(|source| source.is::<serde_json::Error>()),
            "{data}"
        );
    }
    for data in ["", "{}", r#"{"pick_id":null}"#, r#"{"pick_id":["p"]}"#] {
        assert!(
            matches!(decode_close(data), Err(Error::PickPayload(_))),
            "{data}"
        );
    }
    for outcome in [
        json!({"type": "picked"}),
        json!({"type": "created", "id": "a"}),
        json!({"type": "dismissed"}),
        json!({"id": "a"}),
        json!("cancelled"),
    ] {
        assert!(
            serde_json::from_value::<UiPickOutcome>(outcome.clone()).is_err(),
            "{outcome}"
        );
    }
}
