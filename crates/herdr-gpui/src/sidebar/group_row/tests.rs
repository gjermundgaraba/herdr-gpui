use super::*;
use core::prelude::v1::test;

/// A pinned section's color reads like a token's, as in the terminal client:
/// the daemon sends `#rrggbb`, and the short form means the same color.
#[test]
fn a_section_color_reads_like_a_token_color() {
    assert_eq!(parse_color("#ff8800"), Some(0xff8800));
    assert_eq!(parse_color("#E5C07B"), Some(0xe5c07b));
    assert_eq!(parse_color("#000000"), Some(0));
    assert_eq!(parse_color("#f80"), Some(0xff8800));
    // Anything else keeps the default color rather than a guessed one.
    for bad in [
        "",
        "#",
        "ff8800",
        "#ff880",
        "#ff88000",
        "#gg8800",
        "#+f8800",
        "red",
        "# ff880",
        "#ｆｆ8800",
    ] {
        assert_eq!(parse_color(bad), None, "{bad:?}");
    }
}
