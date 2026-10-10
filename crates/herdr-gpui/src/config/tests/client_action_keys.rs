//! The local daemon's `[keys]` are kept beside the keymap built from them: a
//! device's keybinding profile still takes this machine's bindings for the
//! actions the client performs itself.

use super::*;
use crate::{Command, keymap::DaemonKeys};

#[test]
fn the_local_keys_table_is_kept_for_a_devices_profile() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let gui = temp.0.join("config-gpui.toml");
    let daemon = temp.0.join("config.toml");
    fs::write(&gui, "")?;

    // No daemon file: Herdr's defaults, which leave the five unset.
    assert_eq!(
        Config::load_path(&gui, &daemon)?.daemon_keys,
        DaemonKeys::default()
    );
    assert_eq!(Config::parse("")?.daemon_keys, DaemonKeys::default());

    fs::write(
        &daemon,
        "[keys]\nhistory_back = \"alt+o\"\nagent_picker = \"prefix+a\"\n",
    )?;
    let config = Config::load_path(&gui, &daemon)?;
    assert_ne!(config.daemon_keys, DaemonKeys::default());

    // Under a host's profile the two keep the keys written here.
    let profile = "[keys]\nprefix = \"cmd+j\"\nhistory_back = \"alt+b\"\n";
    let keys = DaemonKeys::from_profile(Some(profile))?.with_client_actions(&config.daemon_keys);
    let keymap = Keymap::with_overrides(&config.keybinding_overrides, &config.pane_keys, &keys)?;
    let key = gpui::Keystroke::parse;
    assert!(keymap.triggers(Command::Back, &key("alt-o")?, false));
    assert!(!keymap.triggers(Command::Back, &key("alt-b")?, false));
    assert!(keymap.is_prefix(&key("cmd-j")?));
    assert_eq!(keymap.chord(&key("a")?), Some(Command::AgentPicker));
    Ok(())
}
