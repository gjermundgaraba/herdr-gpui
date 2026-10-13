//! `client.report`, the gjermundgaraba/herdr fork's Agents list report,
//! measured in the lane request envelope it is written in.

use super::ClientHandle;
use crate::{
    Error, Result,
    method::{ForkMethod, Method},
    protocol::clients::{ClientReport, MAX_ENDPOINT_COMMAND_BYTES},
};

/// One lane request as it is written: `{"id", "method", "params"}`.
#[derive(serde::Serialize)]
struct LaneRequest<'a, P> {
    id: &'a str,
    method: &'a str,
    params: &'a P,
}

fn encode_request(id: &str, method: Method, params: &impl serde::Serialize) -> Result<String> {
    Ok(serde_json::to_string(&LaneRequest {
        id,
        method: method.as_str(),
        params,
    })?)
}

impl ClientHandle {
    /// Queue `report` as `client.report`. The daemon closes a connection that
    /// sends a lane request longer than [`MAX_ENDPOINT_COMMAND_BYTES`], and a
    /// report within its row and text limits can still be, so the request is
    /// measured as it will be written. One that does not fit loses rows from
    /// the end of the list, in proportion to how far over it is, until it
    /// does; a report that does not fit without rows is not sent at all.
    pub fn report(&self, boot_id: &str, mut report: ClientReport) -> Result<String> {
        let method = Method::Fork(ForkMethod::ClientReport);
        let id = self.next_request_id();
        loop {
            let request = encode_request(&id, method, &report)?;
            if request.len() <= MAX_ENDPOINT_COMMAND_BYTES {
                return self.queue_request(boot_id, id, method, request);
            }
            let rows = report.agents.len();
            if rows == 0 {
                return Err(Error::ClientReportTooLarge);
            }
            let keep = (rows * MAX_ENDPOINT_COMMAND_BYTES / request.len()).min(rows - 1);
            tracing::warn!(
                category = "client_report",
                rows,
                keep,
                "report exceeds the lane command size; dropping trailing rows"
            );
            report.agents.truncate(keep);
        }
    }
}
