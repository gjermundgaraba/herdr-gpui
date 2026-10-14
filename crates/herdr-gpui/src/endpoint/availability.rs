//! What an endpoint's connection is doing, as a closed set. The devices list
//! words it for the user, and navigation asks whether a host can be reached
//! at all.

use super::Endpoint;
use herdr_client::VersionMismatch;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Availability {
    /// The user switched the device off.
    Disabled,
    /// The user detached this window from it; its daemon keeps running.
    Detached,
    /// The handshake was refused until Herdr is updated on the host.
    DaemonOutdated,
    /// The handshake was refused until this app is updated.
    ClientOutdated,
    Reconnecting,
    Online,
    Connecting,
}

impl Availability {
    /// The words the devices list shows.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Detached => "detached",
            // A refused handshake is an outage too, but says what fixes it.
            Self::DaemonOutdated => "Herdr update needed",
            Self::ClientOutdated => "app update needed",
            Self::Reconnecting => "reconnecting",
            Self::Online => "online",
            Self::Connecting => "connecting",
        }
    }
}

impl Endpoint {
    pub(crate) fn availability(&self) -> Availability {
        if !self.enabled {
            Availability::Disabled
        } else if self.detached {
            Availability::Detached
        } else if let Some(mismatch) = &self.version_mismatch {
            match mismatch {
                VersionMismatch::DaemonOutdated { .. } => Availability::DaemonOutdated,
                VersionMismatch::ClientOutdated { .. } => Availability::ClientOutdated,
            }
        } else if self.outage.is_some() {
            // Before `live`: a stopped handle can precede its disconnect state.
            Availability::Reconnecting
        } else if self.live.status.is_connected() {
            Availability::Online
        } else if self.live.error.is_some() {
            Availability::Reconnecting
        } else {
            Availability::Connecting
        }
    }

    pub fn status(&self) -> &'static str {
        self.availability().label()
    }

    /// Whether the user can act on this host now: what it shows is current,
    /// and a navigation to it can be shown. Stepping and the agent picker
    /// ask this one question, as the Herdr fork's terminal client asks its
    /// endpoint status. The panel's order asks upstream's: whether the host's
    /// connection is up.
    pub(crate) fn online(&self) -> bool {
        self.availability() == Availability::Online
    }
}
