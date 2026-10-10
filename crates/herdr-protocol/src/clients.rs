//! Attached clients: the Agents list each one reports to its daemon, and the
//! jump a tool asks one of them to perform.
//!
//! Adapted from the `gjermundgaraba/herdr` fork's `src/api/schema/clients.rs`
//! (the report, the client record, and the jump parameters and outcome) and
//! `src/protocol/endpoint.rs` (the jump control); Apache-2.0, see NOTICE.md.
//! The JSON shapes, optional fields, tags, and message kind are unchanged.
//! The daemon's report validation and `schemars` derives are not retained.
//!
//! A client is one connection, keyed by the `client_id` the daemon assigned
//! it. It sends [`ClientReport`] with the `client.report` method, and each
//! report replaces the previous one. A tool reads the reports as
//! [`ClientInfo`] and asks one client to show a pane with
//! [`ClientJumpParams`]. The daemon forwards that as [`EndpointClientJump`]
//! to the client, which navigates and answers on the same connection with
//! the `client.jump.resolve` method ([`ClientJumpResolveParams`]).

use crate::{ClientShellAgent, Error, Result};
use serde::{Deserialize, Serialize};

/// Server to client: show a pane; the client answers with the
/// `client.jump.resolve` method on the same connection.
pub const CLIENT_JUMP_KIND: &str = "endpoint.client-jump.v1";

/// Endpoints one report may list; the daemon rejects a longer list.
pub const MAX_CLIENT_REPORT_ENDPOINTS: usize = 64;
/// Agent rows one report may list; the daemon rejects a longer list.
pub const MAX_CLIENT_REPORT_AGENTS: usize = 512;
/// Byte limit for the ids a report or jump carries; the daemon rejects longer ids.
pub const MAX_CLIENT_REPORT_ID_BYTES: usize = 128;
/// Byte limit for display text in a report; the daemon truncates longer text.
pub const MAX_CLIENT_REPORT_TEXT_BYTES: usize = 512;
/// State labels or tokens the daemon keeps per agent row; it drops the rest.
pub const MAX_CLIENT_REPORT_PAIRS: usize = 32;

/// What a client shows in its Agents list, across every endpoint it is
/// attached to. Each report replaces the previous one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientReport {
    /// Whether the client's OS window or outer terminal has focus; `null`
    /// until the client knows.
    pub focused: Option<bool>,
    /// The endpoint the client is presenting.
    pub active_endpoint_id: Option<String>,
    pub endpoints: Vec<ClientReportEndpoint>,
    /// The Agents list in the client's display order.
    pub agents: Vec<ClientReportAgent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientReportEndpoint {
    /// The client's own id for the endpoint, such as `local` or `ssh:<profile>`.
    pub endpoint_id: String,
    pub label: String,
    pub status: ClientEndpointStatus,
    /// Boot of the daemon behind the endpoint; `null` before its first snapshot.
    pub boot_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientEndpointStatus {
    Connecting,
    Online,
    Reconnecting,
    /// The endpoint needs the user, for example to sign in or update.
    Attention,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientReportAgent {
    pub endpoint_id: String,
    /// The agent the client has focused: the focused pane of the endpoint it
    /// presents. At most one row is focused; `agent.focused` is instead the
    /// focus inside the row's own endpoint.
    pub focused: bool,
    /// Label and number of the space the agent belongs to.
    pub workspace_label: String,
    pub workspace_number: usize,
    /// The pane's working directory, when its endpoint knows it.
    pub cwd: Option<String>,
    pub foreground_cwd: Option<String>,
    /// The row's record from its endpoint's snapshot, as the client shows it.
    pub agent: ClientShellAgent,
}

/// One attached client, as `client.list` and `client.changed` describe it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientInfo {
    pub client_id: u64,
    /// The client's latest report; absent until it sends one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<ClientReport>,
}

/// Asks one client to show a pane, switching endpoint when it has to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientJumpParams {
    pub client_id: u64,
    /// The endpoint, in the ids of that client's report.
    pub endpoint_id: String,
    /// The endpoint's boot the pane id was read from.
    pub boot_id: String,
    pub pane_id: String,
}

/// How a client's jump ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientJumpOutcome {
    /// The client presents the endpoint with the pane focused.
    Focused,
    /// The endpoint now runs another boot, so the pane id may name another pane.
    StaleBoot,
    /// The client knows no such endpoint, or the endpoint no such pane.
    NotFound,
    /// The endpoint is not online.
    Unavailable,
    /// The client did not see the pane focused in time; it may still happen.
    Timeout,
    Failed {
        message: String,
    },
}

/// Sent by the client that received the jump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientJumpResolveParams {
    pub jump_id: String,
    pub outcome: ClientJumpOutcome,
}

/// A `client.jump` forwarded to the client that should perform it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointClientJump {
    pub jump_id: String,
    /// The endpoint in the client's own ids, as its report lists them.
    pub endpoint_id: String,
    pub boot_id: String,
    pub pane_id: String,
}

/// Decodes the data of a [`CLIENT_JUMP_KIND`] control.
pub fn decode_jump(data: &str) -> Result<EndpointClientJump> {
    serde_json::from_str(data).map_err(Error::ClientJumpPayload)
}
