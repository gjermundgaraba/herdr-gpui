//! The discovery probe: one block per installed Herdr candidate, each naming
//! the candidate's path and holding its client and server status, then a done
//! marker. POSIX hosts and WSL run [`probe_script`]; Windows hosts run the
//! PowerShell probe, which prints the same blocks. [`probe_candidates`] reads
//! either, so applications that reach a host their own way choose a binary by
//! the same output this crate classifies.
use super::quote;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::time::Duration;

pub(super) const PROBE_CANDIDATE: &str = "herdr-probe:candidate";
pub(super) const PROBE_DONE: &str = "herdr-probe:done";
/// Precedes a POSIX candidate's path, printed as is.
const POSIX_PATH_MARKER: &str = "herdr-probe:path:";
/// Precedes a Windows candidate's base64 UTF-8 path.
pub(super) const WINDOWS_PATH_MARKER: &str = "herdr-windows-path:";
/// How long the probe may run before it counts as failed.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// The most probe output worth reading; more is not a probe's.
pub const PROBE_OUTPUT_LIMIT: u64 = 64 * 1024;

/// What a remote host offers for a saved SSH device, learned without
/// installing, upgrading, or starting anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostProbe {
    /// SSH could not run a command without prompting: unknown host key,
    /// password or passphrase authentication, or an unreachable host.
    SshFailed(super::SshFailure),
    /// No Herdr executable was found in the known install locations.
    Missing,
    /// Herdr is installed, but no copy speaks this client's endpoint protocol.
    Outdated,
    /// A compatible Herdr is installed and its server for the session is down.
    Stopped,
    /// A compatible Herdr server for the session is running.
    Running,
}

/// One probe block: an installed Herdr executable and what it reported. A
/// field is `None` when the block lacked it or it did not parse; the caller
/// decides which fields it needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeCandidate {
    /// The executable's path on the host.
    pub path: Option<String>,
    /// Its `status client --json` object.
    pub client: Option<serde_json::Value>,
    /// Its `--session <session> status server --json` object.
    pub server: Option<serde_json::Value>,
}

impl ProbeCandidate {
    fn statuses(&self) -> impl Iterator<Item = &serde_json::Value> {
        self.client.iter().chain(&self.server)
    }
}

/// The probe as an SSH remote command: `/bin/sh -c` and the quoted script.
/// Run it on a POSIX host however the application reaches it, then read the
/// output with [`probe_candidates`], within [`PROBE_TIMEOUT`] and
/// [`PROBE_OUTPUT_LIMIT`].
///
/// ```
/// let command = herdr_client::probe_command("default");
/// assert!(command.starts_with("/bin/sh -c "));
/// let _bounds = (herdr_client::PROBE_TIMEOUT, herdr_client::PROBE_OUTPUT_LIMIT);
/// assert_eq!(herdr_client::probe_candidates(b"herdr-probe:done\n"), Some(Vec::new()));
/// ```
pub fn probe_command(session: &str) -> String {
    format!("/bin/sh -c {}", quote(&probe_script(session)))
}

/// The probe as a script for `/bin/sh -c`.
pub(crate) fn probe_script(session: &str) -> String {
    format!(
        r#"{candidates}
    if [ -n "$path" ] && [ -x "$path" ]; then
        status=$("$path" status client --json </dev/null) || continue
        server=$("$path" --session {session} status server --json </dev/null) || server=
        printf '%s\n%s%s\n%s\n%s\n' '{PROBE_CANDIDATE}' '{POSIX_PATH_MARKER}' "$path" "$status" "$server"
    fi
done
printf '%s\n' '{PROBE_DONE}'"#,
        candidates = super::CANDIDATES,
        session = quote(session)
    )
}

/// Every candidate block in probe output, in the order the host listed them,
/// or `None` when the probe never printed its done marker, so partial output
/// is not mistaken for a host without Herdr. Lines before the first block,
/// such as a login banner, are ignored.
///
/// After its marker and path, a block holds the client status line, then the
/// server status line. Each is read by position, so a client command that
/// printed nothing or garbage leaves `client` empty instead of taking the
/// server's status.
pub fn probe_candidates(output: &[u8]) -> Option<Vec<ProbeCandidate>> {
    let mut candidates: Vec<ProbeCandidate> = Vec::new();
    // How many status lines the current block has had.
    let mut statuses = 0;
    for line in lines(output) {
        if line == PROBE_DONE.as_bytes() {
            return Some(candidates);
        }
        if line == PROBE_CANDIDATE.as_bytes() {
            candidates.push(ProbeCandidate {
                path: None,
                client: None,
                server: None,
            });
            statuses = 0;
            continue;
        }
        let Some(candidate) = candidates.last_mut() else {
            continue;
        };
        if let Some(path) = line.strip_prefix(POSIX_PATH_MARKER.as_bytes()) {
            candidate.path = valid_path(String::from_utf8(path.to_vec()).ok());
            continue;
        }
        if let Some(encoded) = line.strip_prefix(WINDOWS_PATH_MARKER.as_bytes()) {
            let decoded = STANDARD.decode(encoded).ok();
            candidate.path = valid_path(decoded.and_then(|bytes| String::from_utf8(bytes).ok()));
            continue;
        }
        let slot = match statuses {
            0 => &mut candidate.client,
            1 => &mut candidate.server,
            _ => continue,
        };
        *slot = serde_json::from_slice(line).ok();
        statuses += 1;
    }
    None
}

fn valid_path(path: Option<String>) -> Option<String> {
    path.filter(|path| !path.is_empty() && !path.chars().any(char::is_control))
}

/// `None` when the script never finished, so partial output is not mistaken
/// for a host without Herdr.
pub(crate) fn classify_probe(output: &[u8]) -> Option<HostProbe> {
    classify_probe_with(output, super::compatible)
}

/// [`classify_probe`] with the host's rule for a compatible client status.
pub(super) fn classify_probe_with(
    output: &[u8],
    compatible: impl Fn(&serde_json::Value) -> Option<bool>,
) -> Option<HostProbe> {
    let candidates = probe_candidates(output)?;
    // The first compatible copy decides, as it does for the bridge.
    let Some(candidate) = candidates
        .iter()
        .find(|candidate| candidate.statuses().any(|s| compatible(s).is_some()))
    else {
        return Some(if candidates.is_empty() {
            HostProbe::Missing
        } else {
            HostProbe::Outdated
        });
    };
    let running = candidate
        .statuses()
        .any(|s| s["running"].as_bool() == Some(true));
    Some(if running {
        HostProbe::Running
    } else {
        HostProbe::Stopped
    })
}

/// `output`'s lines without their `\n` or `\r\n` endings, since a Windows
/// host's PowerShell ends its lines with both.
pub(super) fn lines(output: &[u8]) -> impl Iterator<Item = &[u8]> {
    output
        .split(|b| *b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
}
