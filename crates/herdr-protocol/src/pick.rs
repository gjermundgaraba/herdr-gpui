//! `ui.pick`: a picker a plugin asks the daemon to show on one client.
//!
//! Adapted from the Herdr fork's `src/protocol/endpoint.rs` (the two named
//! controls) and `src/api/schema/ui.rs` (items, the create row, the outcome
//! and the `ui.pick.resolve` parameters); Apache-2.0, see NOTICE.md. The JSON
//! shapes, optional fields and message kinds are unchanged. Only what a
//! client that shows the picker needs is retained.
//! [`MAX_ENDPOINT_COMMAND_BYTES`] is the daemon's limit from
//! `src/server/client_commands.rs`.
//!
//! The daemon sends [`EndpointPickOpen`] to the client that should show the
//! picker and holds the plugin's request open. The client answers with the
//! `ui.pick.resolve` method ([`UiPickResolveParams`]). [`EndpointPickClose`]
//! says the picker ended without this client's answer.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// Server to client: show a `ui.pick` picker; the client answers with the
/// `ui.pick.resolve` method.
pub const PICK_OPEN_KIND: &str = "endpoint.pick.open.v1";
/// Server to client: the picker closed without this client's answer.
pub const PICK_CLOSE_KIND: &str = "endpoint.pick.close.v1";

/// Bytes of JSON one client-lane request may be. The daemon closes a
/// connection that sends a larger one, so a client measures a request that
/// may exceed it, as a picker answer that carries created text can, before
/// sending.
pub const MAX_ENDPOINT_COMMAND_BYTES: usize = 1024 * 1024;

/// One row of a native picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPickItem {
    pub id: String,
    pub label: String,
    /// Secondary text shown after the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Short marker shown before the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
}

/// Offers a "create" row for query text that matches no item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPickCreate {
    /// Row label prefix, such as "New group".
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiPickOutcome {
    Picked { id: String },
    Created { text: String },
    Cancelled,
}

/// Sent by the client that shows a picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPickResolveParams {
    pub pick_id: String,
    pub outcome: UiPickOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointPickOpen {
    pub pick_id: String,
    pub title: String,
    pub items: Vec<UiPickItem>,
    /// Id of the item selected when the picker opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create: Option<UiPickCreate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointPickClose {
    pub pick_id: String,
}

/// Decodes the data of a [`PICK_OPEN_KIND`] control.
pub fn decode_open(data: &str) -> Result<EndpointPickOpen> {
    serde_json::from_str(data).map_err(Error::PickPayload)
}

/// Decodes the data of a [`PICK_CLOSE_KIND`] control.
pub fn decode_close(data: &str) -> Result<EndpointPickClose> {
    serde_json::from_str(data).map_err(Error::PickPayload)
}
