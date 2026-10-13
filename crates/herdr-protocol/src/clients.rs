//! Attached clients: the Agents list each one reports to its daemon, and the
//! jump a tool asks one of them to perform.
//!
//! Adapted from the `gjermundgaraba/herdr` fork's `src/api/schema/clients.rs`
//! (the report, and the jump answer's parameters and outcome) and
//! `src/protocol/endpoint.rs` (the jump control); Apache-2.0, see NOTICE.md.
//! The JSON shapes, optional fields, tags, and message kind are unchanged,
//! and [`ClientReport::normalize`] is the daemon's own check, so a client can
//! hold a report to the limits before sending it. The `schemars` derives are
//! not retained. [`MAX_ENDPOINT_COMMAND_BYTES`] is the daemon's limit from
//! `src/server/client_commands.rs`.
//!
//! A client is one connection, keyed by the `client_id` the daemon assigned
//! it. It sends [`ClientReport`] with the `client.report` method, and each
//! report replaces the previous one. Tools read the reports through the
//! daemon's socket API (`client.list`, `client.changed`) and ask one client to
//! show a pane with `client.jump`; those shapes are a tool's, not a client's,
//! so this crate does not declare them. The daemon forwards a jump as
//! [`EndpointClientJump`] to the client, which navigates and answers on the
//! same connection with the `client.jump.resolve` method
//! ([`ClientJumpResolveParams`]).

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
/// Bytes of JSON one client-lane request may be. The daemon closes a
/// connection that sends a larger one, so a client measures a request that
/// may exceed it, as a report of long rows can, before sending.
pub const MAX_ENDPOINT_COMMAND_BYTES: usize = 1024 * 1024;

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

/// Why the daemon rejects a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClientReportError {
    #[error("a report lists at most 64 endpoints")]
    TooManyEndpoints,
    #[error("a report lists at most 512 agents")]
    TooManyAgents,
    #[error("endpoint, boot, workspace, tab, and pane ids are at most 128 bytes")]
    IdTooLong,
    #[error("endpoint and pane ids must not be empty")]
    EmptyId,
    #[error("an endpoint id is listed more than once")]
    DuplicateEndpoint,
    #[error("active_endpoint_id names an endpoint the report omits")]
    UnknownActiveEndpoint,
    #[error("an agent names an endpoint the report omits")]
    UnknownAgentEndpoint,
    #[error("at most one agent is focused")]
    MultipleFocusedAgents,
}

impl ClientReport {
    /// Checks the report's structure and bounds its display text, exactly as
    /// the daemon does on receipt: a report this accepts is one the daemon
    /// keeps as sent.
    pub fn normalize(&mut self) -> std::result::Result<(), ClientReportError> {
        if self.endpoints.len() > MAX_CLIENT_REPORT_ENDPOINTS {
            return Err(ClientReportError::TooManyEndpoints);
        }
        if self.agents.len() > MAX_CLIENT_REPORT_AGENTS {
            return Err(ClientReportError::TooManyAgents);
        }
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            check_id(&endpoint.endpoint_id)?;
            if let Some(boot_id) = &endpoint.boot_id {
                check_id_length(boot_id)?;
            }
            if self.endpoints[..index]
                .iter()
                .any(|earlier| earlier.endpoint_id == endpoint.endpoint_id)
            {
                return Err(ClientReportError::DuplicateEndpoint);
            }
        }
        let listed = |endpoint_id: &str| {
            self.endpoints
                .iter()
                .any(|endpoint| endpoint.endpoint_id == endpoint_id)
        };
        if self
            .active_endpoint_id
            .as_deref()
            .is_some_and(|endpoint_id| !listed(endpoint_id))
        {
            return Err(ClientReportError::UnknownActiveEndpoint);
        }
        if self.agents.iter().any(|row| !listed(&row.endpoint_id)) {
            return Err(ClientReportError::UnknownAgentEndpoint);
        }
        if self.agents.iter().filter(|row| row.focused).count() > 1 {
            return Err(ClientReportError::MultipleFocusedAgents);
        }
        for row in &self.agents {
            check_id(&row.agent.pane_id)?;
            check_id_length(&row.agent.workspace_id)?;
            check_id_length(&row.agent.tab_id)?;
        }
        for endpoint in &mut self.endpoints {
            truncate(&mut endpoint.label);
        }
        for row in &mut self.agents {
            truncate(&mut row.workspace_label);
            for text in [&mut row.cwd, &mut row.foreground_cwd] {
                truncate_optional(text);
            }
            let agent = &mut row.agent;
            for text in [
                &mut agent.name,
                &mut agent.display_agent,
                &mut agent.agent,
                &mut agent.title,
                &mut agent.terminal_title,
                &mut agent.terminal_title_stripped,
                &mut agent.priority_mark,
            ] {
                truncate_optional(text);
            }
            truncate(&mut agent.priority_key);
            for pairs in [&mut agent.state_labels, &mut agent.tokens] {
                pairs.truncate(MAX_CLIENT_REPORT_PAIRS);
                for (key, value) in pairs {
                    truncate(key);
                    truncate(value);
                }
            }
        }
        Ok(())
    }
}

fn check_id(id: &str) -> std::result::Result<(), ClientReportError> {
    if id.is_empty() {
        return Err(ClientReportError::EmptyId);
    }
    check_id_length(id)
}

fn check_id_length(id: &str) -> std::result::Result<(), ClientReportError> {
    if id.len() > MAX_CLIENT_REPORT_ID_BYTES {
        return Err(ClientReportError::IdTooLong);
    }
    Ok(())
}

fn truncate_optional(text: &mut Option<String>) {
    if let Some(text) = text {
        truncate(text);
    }
}

/// Cuts `text` to the report's text limit on a character boundary.
fn truncate(text: &mut String) {
    let end = text.floor_char_boundary(MAX_CLIENT_REPORT_TEXT_BYTES);
    text.truncate(end);
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
    /// The endpoint is not online, or has sent the client no snapshot yet.
    Unavailable,
    /// The client did not see the pane focused in time; it may still happen.
    Timeout,
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
