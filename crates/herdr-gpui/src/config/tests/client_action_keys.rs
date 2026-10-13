//! The local daemon's `[keys]` are kept beside the keymap built from them: a
//! device's keybinding profile still takes this machine's bindings for the
//! actions the client performs itself.

use super::*;
use crate::keymap::DaemonKeys;

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

    let table = "[keys]\nhistory_back = \"alt+o\"\nagent_picker = \"prefix+a\"\n";
    fs::write(&daemon, table)?;
    assert_eq!(
        Config::load_path(&gui, &daemon)?.daemon_keys,
        DaemonKeys::from_profile(Some(table))?
    );
    Ok(())
}
