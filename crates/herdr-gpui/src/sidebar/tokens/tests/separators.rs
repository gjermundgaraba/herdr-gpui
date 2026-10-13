//! `separator_before` on a configured token replaces the separator drawn
//! before it, as Herdr's TUI does (`tokens::separator` in its
//! `src/ui/sidebar/tokens.rs`).

use super::*;

fn fixed(kind: &TokenKind) -> usize {
    match kind {
        TokenKind::StateIcon => 1,
        TokenKind::GitStatus { ahead, behind } => {
            let count = |n: &usize| usize::from(*n > 0) * (n.to_string().len() + 1);
            count(ahead) + count(behind) + usize::from(*ahead > 0 && *behind > 0)
        }
        TokenKind::Text(..) => 0,
    }
}

/// A row as its line reads at `width` glyphs: each drawn token cut to its
/// budget, joined by the separators the line places between them. This
/// copies the join in `token_line` (`row/configured.rs`), so these tests
/// check `budgets` and `separator`; the layout test
/// `layout_tests/token_separators.rs` checks what `token_line` paints.
fn line(row: &[ResolvedToken], width: usize) -> String {
    let visible: Vec<_> = row
        .iter()
        .zip(budgets(row, fixed, width))
        .filter_map(|(token, budget)| Some((token, budget?)))
        .collect();
    visible
        .iter()
        .enumerate()
        .map(|(index, &(token, budget))| {
            let gap = match index {
                0 => "",
                _ => separator(visible[index - 1].0, token),
            };
            let token = match &token.kind {
                TokenKind::StateIcon => "*".to_owned(),
                TokenKind::GitStatus { ahead, behind } => [("↑", ahead), ("↓", behind)]
                    .into_iter()
                    .filter(|(_, count)| **count > 0)
                    .map(|(arrow, count)| format!("{arrow}{count}"))
                    .collect::<Vec<_>>()
                    .join(" "),
                TokenKind::Text(text, _) => text.chars().take(budget).collect(),
            };
            format!("{gap}{token}")
        })
        .collect()
}

fn space_row(rows: &str, tokens: &[(String, String)]) -> Vec<ResolvedToken> {
    let layout = space_layout(rows);
    space_rows(
        &layout,
        SpaceContext {
            label: "repo",
            branch: Some("main"),
            status: AgentStatus::Working,
            ahead_behind: Some((1, 0)),
            tokens,
            indented: false,
        },
    )
    .remove(0)
}

fn token(name: &str, value: &str) -> (String, String) {
    (name.to_owned(), value.to_owned())
}

#[test]
fn an_override_replaces_the_default_separator() {
    let tokens = [token("dirty", "\u{f040}")];
    let default = space_row(r#"rows = [["branch", "$dirty", "git_status"]]"#, &tokens);
    assert_eq!(line(&default, 30), "main \u{b7} \u{f040} ↑1");
    for (separator, expected) in [
        (" ", "main \u{f040} ↑1"),
        ("", "main\u{f040} ↑1"),
        (" → ", "main → \u{f040} ↑1"),
    ] {
        let row = space_row(
            &format!(
                r#"rows = [["branch", {{ token = "$dirty", separator_before = "{separator}" }}, "git_status"]]"#
            ),
            &tokens,
        );
        assert_eq!(line(&row, 30), expected);
    }
    // The defaults after an icon and before the git counters yield as well.
    let row = space_row(
        r#"rows = [["state_icon", { token = "branch", separator_before = "/" }, { token = "git_status", separator_before = ":" }]]"#,
        &tokens,
    );
    assert_eq!(line(&row, 30), "*/main:↑1");
}

#[test]
fn an_override_applies_to_agent_rows_and_their_per_agent_replacements() {
    let mut snapshot = layout_tests::snapshot(1);
    snapshot.agents[0].tokens = vec![token("summary", "fixing")];
    let layout = agent_layout(
        r#"rows = [["workspace", { token = "$summary", separator_before = "" }]]
[rows_by_agent]
claude = [["workspace", { token = "$summary", separator_before = " / " }]]"#,
    );
    let workspace = snapshot.workspaces[0].label.clone();
    snapshot.agents[0].agent = Some("codex".into());
    let rows = agent_rows(&layout, &snapshot.agents[0], &snapshot, None).unwrap();
    assert_eq!(line(&rows[0], 60), format!("{workspace}fixing"));
    snapshot.agents[0].agent = Some("claude".into());
    let rows = agent_rows(&layout, &snapshot.agents[0], &snapshot, None).unwrap();
    assert_eq!(line(&rows[0], 60), format!("{workspace} / fixing"));
}

#[test]
fn a_separator_never_leaks_from_a_missing_hidden_or_first_token() {
    let tokens = [token("hidden", "hide"), token("dirty", "+")];
    let row = space_row(
        r#"rows = [[{ token = "$missing", separator_before = "LEAK" }, { token = "$hidden", separator_before = "LEAK", rules = [{ equals = "hide", hide = true }] }, { token = "$dirty", separator_before = "LEADING" }, "git_status"]]"#,
        &tokens,
    );
    assert_eq!(line(&row, 30), "+ ↑1");
    // The override joins a token to whichever token is drawn before it,
    // across the ones between them that are not.
    let row = space_row(
        r#"rows = [["branch", { token = "$hidden", separator_before = "LEAK", rules = [{ equals = "hide", hide = true }] }, { token = "$dirty", separator_before = " " }]]"#,
        &tokens,
    );
    assert_eq!(line(&row, 30), "main +");
}

#[test]
fn a_token_dropped_for_width_takes_its_separator_with_it() {
    let tokens = [token("dirty", "+")];
    let row = space_row(
        r#"rows = [["branch", { token = "$dirty", separator_before = " | " }]]"#,
        &tokens,
    );
    // "main" + " | " + "+" needs 8 glyphs; the minimum is one glyph of each
    // text token and the separator between them.
    assert_eq!(line(&row, 8), "main | +");
    assert_eq!(line(&row, 5), "m | +");
    // Too narrow for both: the leftmost text token goes, and the separator
    // that would have preceded the survivor is not drawn before a first token.
    assert_eq!(line(&row, 4), "+");
    assert_eq!(line(&row, 0), "");
}

#[test]
fn separator_width_is_counted_in_cells_when_fitting_text() {
    let fixed = |_: &TokenKind| 0;
    let row = |separator: &str| {
        space_row(
            &format!(
                r#"rows = [["branch", {{ token = "$dirty", separator_before = "{separator}" }}]]"#
            ),
            &[token("dirty", "x")],
        )
    };
    // The branch gets what the separator and the one-glyph token leave.
    for (separator, branch) in [("", 4), (" ", 4), ("界", 3), ("界 ", 2)] {
        assert_eq!(
            budgets(&row(separator), fixed, 6),
            [Some(branch), Some(1)],
            "{separator:?}"
        );
    }
    // A wide separator is two cells, not one character: at seven glyphs the
    // branch keeps its four only because the separator is counted as two.
    assert_eq!(budgets(&row("界"), fixed, 7), [Some(4), Some(1)]);
    assert_eq!(budgets(&row("界界"), fixed, 7), [Some(2), Some(1)]);
    // A separator too long for the line drops the leftmost text token.
    assert_eq!(
        budgets(&row(" very long separator "), fixed, 6),
        [None, Some(1)]
    );
}
