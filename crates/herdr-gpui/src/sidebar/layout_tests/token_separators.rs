//! `separator_before` as a configured spaces row paints it: the separator
//! before a token is that token's own, and only ever sits between two tokens
//! the row shows. `tokens/tests/separators.rs` covers the budgets and the
//! choice of separator; this is the line `token_line` actually paints.

use super::*;

/// The text painted inside `key`'s row, left to right.
fn painted_row(cx: &mut gpui::VisualTestContext, key: &'static str) -> String {
    cx.update(|window, cx| {
        cx.default_global::<TextProbes>().0.clear();
        full_draw(window, cx).clear(cx);
    });
    let row = cx.debug_bounds(key).unwrap();
    cx.update(|_, cx| {
        let mut parts: Vec<_> = cx
            .global::<TextProbes>()
            .0
            .iter()
            .filter(|(_, (bounds, ..))| {
                bounds.top() >= row.top()
                    && bounds.bottom() <= row.bottom()
                    && bounds.left() >= row.left()
                    && bounds.right() <= row.right()
            })
            .map(|(text, (bounds, ..))| (bounds.left(), text.clone()))
            .collect();
        parts.sort_by(|a, b| f32::from(a.0).total_cmp(&f32::from(b.0)));
        parts.into_iter().map(|(_, text)| text).collect()
    })
}

#[gpui::test]
fn a_hidden_token_neither_paints_nor_lends_its_separator(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_agents = false;
        view.live.snapshot = Some(Arc::new(snapshot(1)));
        view.sidebar_width = Some(320.);
        view
    });
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.run_until_parked();
    // `$missing` has no value for the space, so the row hides it.
    for (rows, expected) in [
        (
            r#"[["workspace", { token = "$missing", separator_before = " / " }, { token = "branch", separator_before = " | " }]]"#,
            "herdr | main",
        ),
        (
            r#"[["workspace", { token = "$missing", separator_before = " | " }, "branch"]]"#,
            "herdr \u{b7} main",
        ),
    ] {
        view.update(cx, |view, cx| {
            view.config.sidebar_layout =
                toml::from_str(&format!("[spaces]\nrows = {rows}")).unwrap();
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(painted_row(cx, "row-herdr"), expected, "{rows}");
    }
}
