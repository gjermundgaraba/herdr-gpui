//! `separator_before` on a `[ui.sidebar]` token table, as Herdr reads it
//! (`src/config/sidebar.rs` there). Before it was accepted, one token carrying
//! it made the whole sidebar layout fall back to the defaults.

use super::*;
use sidebar::{SidebarLayout, SpaceToken};

fn layout(text: &str) -> std::result::Result<SidebarLayout, toml::de::Error> {
    SidebarLayout::from_daemon_config(&text.parse::<toml::Table>()?)
}

#[test]
fn separator_overrides_load_and_drop_control_characters() -> anyhow::Result<()> {
    let layout = layout(
        r##"
[ui.sidebar.agents]
rows = [["workspace", { token = "$summary", separator_before = "", fg = "#abc" }]]
[ui.sidebar.agents.rows_by_agent]
pi = [[{ token = "agent", separator_before = " / " }]]
[ui.sidebar.spaces]
rows = [["branch", { token = "$git_dirty", separator_before = "\t |\u001b\n" }]]
"##,
    )?;
    // A bare name, and a table that leaves it out, keep the default.
    assert_eq!(layout.agents.rows[0][0].separator_before, None);
    assert_eq!(layout.spaces.rows[0][0].separator_before, None);
    // Empty is an answer of its own: join directly.
    assert_eq!(
        layout.agents.rows[0][1].separator_before.as_deref(),
        Some("")
    );
    assert_eq!(layout.agents.rows[0][1].style.fg, Some(0xaabbcc));
    assert_eq!(
        layout.agents.rows_by_agent["pi"][0][0]
            .separator_before
            .as_deref(),
        Some(" / ")
    );
    // Tab, escape, and newline are dropped; spaces and the bar stay, untrimmed.
    assert_eq!(
        layout.spaces.rows[0][1].separator_before.as_deref(),
        Some(" |")
    );
    // The built-in layouts ask for no override.
    let defaults = SidebarLayout::default();
    assert!(
        defaults
            .agents
            .rows
            .iter()
            .flatten()
            .all(|token| token.separator_before.is_none())
    );
    assert!(
        defaults
            .spaces
            .rows
            .iter()
            .flatten()
            .all(|token| token.separator_before.is_none())
    );
    Ok(())
}

#[test]
fn token_tables_still_reject_unknown_and_mistyped_keys() {
    for token in [
        r#"{ token = "agent", separator_after = " " }"#,
        r#"{ token = "agent", separator = " " }"#,
        r#"{ token = "agent", separator_before = " ", size = 2 }"#,
        r#"{ token = "agent", separator_before = 1 }"#,
        r#"{ token = "agent", separator_before = [" "] }"#,
        r#"{ separator_before = " " }"#,
    ] {
        assert!(
            layout(&format!("[ui.sidebar.agents]\nrows = [[{token}]]")).is_err(),
            "accepted: {token}"
        );
    }
    // The separator does not make a fixed token take rules.
    assert!(
        layout(
            r#"[ui.sidebar.spaces]
rows = [[{ token = "git_status", separator_before = " ", rules = [{ equals = "x" }] }]]"#
        )
        .is_err()
    );
}

/// The `[ui.sidebar.spaces]` rows of a real Herdr config, verbatim, with the
/// `pinned` table Herdr's fork reads beside them and this client ignores.
#[test]
fn a_real_config_with_separators_and_pinned_spaces_loads_its_rows() -> anyhow::Result<()> {
    let layout = layout(
        r##"
[ui.sidebar.spaces]
rows = [
  ["state_icon", { token = "$numbered_workspace", bold = true, dim = false }, { token = "$space_priority", fg = "#e5c07b", dim = false, separator_before = " " }],
  ["$branch_line", { token = "$git_dirty", fg = "#e5c07b", dim = false, separator_before = " " }, "git_status"],
]

[[ui.sidebar.spaces.pinned]]
title = "Priority"
token = "space_priority"
fg = "#e5c07b"
"##,
    )?;
    let custom = |name: &str| SpaceToken::Custom(name.to_owned());
    let rows = &layout.spaces.rows;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0]
            .iter()
            .map(|token| token.token.clone())
            .collect::<Vec<_>>(),
        [
            SpaceToken::StateIcon,
            custom("numbered_workspace"),
            custom("space_priority")
        ]
    );
    assert_eq!(
        rows[0]
            .iter()
            .map(|token| token.separator_before.as_deref())
            .collect::<Vec<_>>(),
        [None, None, Some(" ")]
    );
    assert_eq!(
        (rows[0][1].style.bold, rows[0][1].style.dim),
        (Some(true), Some(false))
    );
    assert_eq!(
        (rows[0][2].style.fg, rows[0][2].style.dim),
        (Some(0xe5c07b), Some(false))
    );
    assert_eq!(
        rows[1]
            .iter()
            .map(|token| token.token.clone())
            .collect::<Vec<_>>(),
        [
            custom("branch_line"),
            custom("git_dirty"),
            SpaceToken::GitStatus
        ]
    );
    assert_eq!(
        rows[1]
            .iter()
            .map(|token| token.separator_before.as_deref())
            .collect::<Vec<_>>(),
        [None, Some(" "), None]
    );
    assert_eq!(rows[1][1].style.fg, Some(0xe5c07b));
    // The agents layout was not configured and keeps its defaults.
    assert_eq!(layout.agents, SidebarLayout::default().agents);
    Ok(())
}
