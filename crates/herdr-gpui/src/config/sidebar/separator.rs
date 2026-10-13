//! `separator_before` on a sidebar token table replaces the row's separator
//! before that token. The Herdr fork reads the same option in its
//! `src/config/sidebar/separator.rs`, and draws it as
//! `crate::sidebar::tokens::separator` does here.

/// Display text from a config file: as Herdr does, keep what was written
/// except the control characters a line cannot show.
pub(super) fn sanitize(separator: Option<String>) -> Option<std::sync::Arc<str>> {
    separator.map(|separator| {
        separator
            .chars()
            .filter(|ch| !ch.is_control())
            .collect::<String>()
            .into()
    })
}
