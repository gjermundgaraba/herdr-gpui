//! This machine's `[keys]` table gives the client's own actions their keys
//! even under a server profile, so a keymap built from an older table is
//! stale although upstream's comparison, which knows only the profile and
//! the GUI overrides, would keep it. Counterpart of the Herdr fork's
//! `src/config/keybinds/client_actions.rs` reload.

use super::HerdrWindow;
use gpui::Context;

impl HerdrWindow {
    /// Drops a server keymap built from local keys that have since changed,
    /// before upstream's sync compares and rebuilds. The stale keymap is
    /// unbound here, so a rebuild that fails leaves the local keymap bound
    /// rather than the stale one. A rebuild that succeeds then publishes
    /// again: two rebinds for a rare table change, kept so upstream's sync
    /// stays untouched.
    pub(super) fn drop_stale_server_keys(&mut self, cx: &mut Context<Self>) {
        let local = &self.config.daemon_keys;
        let Some(stale) = self.server_keys.take_if(|keys| keys.local != *local) else {
            return;
        };
        if stale.keymap.is_ok() {
            // A chord armed under the stale prefix must not complete.
            self.disarm_prefix();
            if self.active {
                self.publish_server_keymap(cx);
            }
        }
    }
}
