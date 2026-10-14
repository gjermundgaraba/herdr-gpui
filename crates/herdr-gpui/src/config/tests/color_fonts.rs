use super::*;
use crate::config::color_fonts::MAX_COLOR_RANGES;
use std::ops::RangeInclusive;

fn installed() -> Vec<String> {
    ["Menlo", "Icons", "Spinner Color", "Emoji Color", "After"]
        .map(str::to_owned)
        .into()
}

fn color(family: &str) -> bool {
    family.ends_with("Color")
}

/// Menlo covers ASCII and the spinner's first codepoints, Icons a run inside
/// the spinner's range, and each color family its own block.
fn coverage(family: &str) -> Vec<RangeInclusive<u32>> {
    match family {
        "Menlo" => vec![0x20..=0x7e, 0x10_0000..=0x10_0005],
        "Icons" => vec![0x10_0010..=0x10_0020],
        "Spinner Color" => vec![0x10_0000..=0x10_5c6a],
        "Emoji Color" => vec![0x41..=0x41, 0x1_f600..=0x1_f601],
        other => panic!("{other} is after the last color family"),
    }
}

fn configured(fallback: &str) -> anyhow::Result<Config> {
    let mut config = Config::parse(&format!(
        "[terminal]\nfamily = 'Menlo'\nfallback = {fallback}"
    ))?;
    config.resolve_fonts_with(installed, |_| false);
    Ok(config)
}

fn drawn_in_color(config: &Config, codepoints: &[u32]) -> Vec<bool> {
    codepoints
        .iter()
        .map(|&c| char::from_u32(c).is_some_and(|c| config.color_glyphs.contains(c)))
        .collect()
}

#[test]
fn a_color_family_wins_what_no_earlier_family_covers() -> anyhow::Result<()> {
    let mut config = configured("['Icons', 'Spinner Color', 'Emoji Color', 'After']")?;
    config.check_color_fonts(color, coverage);
    assert_eq!(
        drawn_in_color(
            &config,
            &[
                0x10_0005, 0x10_0006, 0x10_000f, 0x10_0010, 0x10_0020, 0x10_0021, 0x10_5c6a
            ]
        ),
        [false, true, true, false, false, true, true]
    );
    // Menlo has `A`; the emoji block is the second color family's own.
    assert_eq!(
        drawn_in_color(&config, &[0x41, 0x1_f600, 0x1_f601, 0x1_f602, 0x10_5c6b]),
        [false, true, true, false, false]
    );
    Ok(())
}

#[test]
fn cascade_order_decides_which_family_draws() -> anyhow::Result<()> {
    let mut config = configured("['Spinner Color', 'Icons']")?;
    config.check_color_fonts(color, coverage);
    // Ahead of Icons, the spinner font takes the codepoints both cover.
    assert_eq!(
        drawn_in_color(&config, &[0x10_0010, 0x10_0020]),
        [true, true]
    );
    assert_eq!(drawn_in_color(&config, &[0x10_0005]), [false]);
    Ok(())
}

#[test]
fn coverage_is_read_only_for_a_color_cascade() -> anyhow::Result<()> {
    let mut config = configured("['Icons', 'After']")?;
    config.check_color_fonts(color, |family| panic!("read {family}"));
    assert_eq!(config.color_glyphs, ColorGlyphs::default());

    // A missing family is reported as missing, not checked.
    let mut config = configured("['Missing Color', 'Spinner Color']")?;
    assert_eq!(config.missing_fonts, ["Missing Color"]);
    config.check_color_fonts(
        |family| {
            assert_ne!(family, "Missing Color");
            color(family)
        },
        coverage,
    );
    assert!(config.color_glyphs.contains('\u{100100}'));
    Ok(())
}

#[test]
fn only_the_terminal_fallbacks_draw_in_color() -> anyhow::Result<()> {
    // The terminal's own family draws text, and other faces' cascades are
    // not the terminal's.
    let mut config = Config::parse(
        "[terminal]\nfamily = 'Spinner Color'\nfallback = []\n\
         [sidebar]\nfallback = ['Spinner Color']",
    )?;
    config.resolve_fonts_with(installed, |_| false);
    config.check_color_fonts(|_| true, coverage);
    assert_eq!(config.color_glyphs, ColorGlyphs::default());
    Ok(())
}

#[test]
fn color_ranges_are_bounded() {
    let glyphs: ColorGlyphs = (0..MAX_COLOR_RANGES as u32 + 10)
        .map(|n| 0x10_0000 + 2 * n..=0x10_0000 + 2 * n)
        .collect();
    let last = 0x10_0000 + 2 * (MAX_COLOR_RANGES as u32 - 1);
    assert!(glyphs.contains('\u{100000}'));
    assert!(!glyphs.contains('\u{100001}'));
    assert!(char::from_u32(last).is_some_and(|c| glyphs.contains(c)));
    assert!(char::from_u32(last + 2).is_some_and(|c| !glyphs.contains(c)));
}

#[test]
fn touching_ranges_join() {
    let glyphs: ColorGlyphs = [0x12..=0x20, 0x10..=0x11, 0x15..=0x16]
        .into_iter()
        .collect();
    assert_eq!(glyphs, [0x10..=0x20].into_iter().collect());
    assert!(!glyphs.contains('\u{f}') && glyphs.contains('\u{10}') && !glyphs.contains('\u{21}'));
}

#[cfg(target_os = "macos")]
#[test]
fn core_text_reports_color_fonts_and_cmap_coverage() {
    use crate::config::color_fonts::{cmap_coverage, platform_coverage, platform_has_color};
    let missing = format!("Missing Font {}", uuid::Uuid::new_v4());
    assert!(platform_has_color("Apple Color Emoji"));
    assert!(!platform_has_color("Menlo"));
    assert!(!platform_has_color(&missing));

    let menlo: ColorGlyphs = platform_coverage("Menlo").into_iter().collect();
    assert!(menlo.contains('M'));
    assert!(!menlo.contains('\u{1f600}'));
    assert!(platform_coverage(&missing).is_empty());
    assert!(cmap_coverage(b"not a cmap", 10).is_empty());
}
