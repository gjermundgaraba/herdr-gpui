//! The codepoints the terminal's cascade draws from a color font.
//!
//! GPUI draws a glyph in color only when Apple Color Emoji supplies it; every
//! other face, color bitmaps included, becomes a mask in the text color. Nor
//! can the painter ask which face a shaped run came from: GPUI names runs by
//! font ID, and loading a family by name skips faces without an `m` glyph,
//! which icon and spinner fonts lack, so no lookup yields the ID their runs
//! carry. Core Text takes each codepoint from the first family in the cascade
//! that has it, so the codepoints a color family wins are known in advance,
//! and the painter draws those in color.
use super::Config;
use std::ops::RangeInclusive;

/// Bounds the ranges kept, so an unusual cmap cannot grow the lookup without
/// limit. Ranges past it draw in the text color.
pub(super) const MAX_COLOR_RANGES: usize = 4096;

/// Codepoints the terminal draws in their font's own colors, as sorted,
/// disjoint, non-adjacent ranges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColorGlyphs(Vec<RangeInclusive<u32>>);

impl ColorGlyphs {
    pub fn contains(&self, c: char) -> bool {
        let c = u32::from(c);
        let next = self.0.partition_point(|range| *range.end() < c);
        self.0.get(next).is_some_and(|range| range.contains(&c))
    }

    /// The codepoints each color family wins: those no family before it in
    /// `cascade` covers. Each family is a color flag and its sorted coverage.
    fn from_cascade(cascade: impl IntoIterator<Item = (bool, Vec<RangeInclusive<u32>>)>) -> Self {
        let mut covered: Vec<RangeInclusive<u32>> = Vec::new();
        let mut color = Vec::new();
        for (is_color, coverage) in cascade {
            if is_color {
                color.extend(coverage.iter().flat_map(|range| subtract(range, &covered)));
            }
            covered = merge(covered.into_iter().chain(coverage));
        }
        color.into_iter().collect()
    }
}

impl FromIterator<RangeInclusive<u32>> for ColorGlyphs {
    fn from_iter<I: IntoIterator<Item = RangeInclusive<u32>>>(ranges: I) -> Self {
        let mut ranges = merge(ranges);
        ranges.truncate(MAX_COLOR_RANGES);
        Self(ranges)
    }
}

/// Sorts `ranges` and joins those that overlap or touch.
fn merge(ranges: impl IntoIterator<Item = RangeInclusive<u32>>) -> Vec<RangeInclusive<u32>> {
    let mut ranges: Vec<_> = ranges.into_iter().collect();
    ranges.sort_unstable_by_key(|range| *range.start());
    let mut merged: Vec<RangeInclusive<u32>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if *range.start() <= last.end().saturating_add(1) => {
                *last = *last.start()..=(*last.end()).max(*range.end());
            }
            _ => merged.push(range),
        }
    }
    merged
}

/// The parts of `range` outside `covered`, which is sorted and disjoint.
fn subtract(
    range: &RangeInclusive<u32>,
    covered: &[RangeInclusive<u32>],
) -> Vec<RangeInclusive<u32>> {
    let mut pieces = Vec::new();
    let mut start = *range.start();
    let first = covered.partition_point(|hole| hole.end() < range.start());
    for hole in &covered[first..] {
        if hole.start() > range.end() {
            break;
        }
        if *hole.start() > start {
            pieces.push(start..=hole.start() - 1);
        }
        match hole.end().checked_add(1) {
            Some(next) => start = start.max(next),
            None => return pieces,
        }
    }
    if start <= *range.end() {
        pieces.push(start..=*range.end());
    }
    pieces
}

impl Config {
    /// Finds the codepoints the terminal's cascade draws from a color font.
    /// `has_color` says whether a family carries color glyphs; `coverage`
    /// lists a family's codepoints and is asked only when some fallback is a
    /// color font, for the terminal's family and each fallback up to the
    /// last color one. Runs after [`Self::resolve_fonts_with`], so it checks
    /// the cascade that will draw, and skips families reported missing.
    pub(super) fn check_color_fonts(
        &mut self,
        has_color: impl Fn(&str) -> bool,
        coverage: impl Fn(&str) -> Vec<RangeInclusive<u32>>,
    ) {
        let fallbacks: Vec<(&str, bool)> = self
            .terminal
            .fallbacks
            .iter()
            .flatten()
            .filter(|family| !self.missing_fonts.contains(family))
            .map(|family| (family.as_str(), has_color(family)))
            .collect();
        let Some(last) = fallbacks.iter().rposition(|(_, color)| *color) else {
            self.color_glyphs = ColorGlyphs::default();
            return;
        };
        // The terminal's own family draws text; it is never painted in color.
        let cascade = std::iter::once((self.terminal.family.as_str(), false))
            .chain(fallbacks[..=last].iter().copied())
            .map(|(family, color)| (color, coverage(family)));
        self.color_glyphs = ColorGlyphs::from_cascade(cascade);
    }
}

#[cfg(all(target_os = "macos", test))]
pub(super) use platform::cmap_coverage;
#[cfg(target_os = "macos")]
pub(super) use platform::{platform_coverage, platform_has_color};

#[cfg(target_os = "macos")]
mod platform {
    use skrifa::raw::{
        FontData, FontRead,
        tables::cmap::{Cmap, CmapIterLimits},
    };
    use std::ops::RangeInclusive;

    /// Core Text's `kCTFontColorGlyphsTrait`: the face has `sbix`, `COLR` or
    /// another color glyph table.
    const COLOR_GLYPHS_TRAIT: u32 = 1 << 13;
    const CMAP: u32 = u32::from_be_bytes(*b"cmap");
    /// Bounds the ranges read from one face. A face mapping more scattered
    /// codepoints is treated as covering only the first of them.
    const MAX_FACE_RANGES: usize = 1 << 16;

    /// Every face of `family`. Core Text loads them, so callers stay off the
    /// UI thread.
    fn faces(family: &str) -> Vec<font_kit::font::Font> {
        let Ok(handle) = font_kit::source::SystemSource::new().select_family_by_name(family) else {
            return Vec::new();
        };
        handle
            .fonts()
            .iter()
            .filter_map(|face| face.load().ok())
            .collect()
    }

    /// Whether any face of `family` carries color glyphs.
    pub(in crate::config) fn platform_has_color(family: &str) -> bool {
        faces(family)
            .iter()
            .any(|face| face.native_font().symbolic_traits() & COLOR_GLYPHS_TRAIT != 0)
    }

    /// Every codepoint some face of `family` maps, read from each face's
    /// `cmap` table. Only that table is copied, never the font file.
    pub(in crate::config) fn platform_coverage(family: &str) -> Vec<RangeInclusive<u32>> {
        let faces = faces(family);
        let ranges = faces.iter().flat_map(|face| {
            face.native_font()
                .get_font_table(CMAP)
                .map(|table| cmap_coverage(table.bytes(), face.glyph_count()))
                .unwrap_or_default()
        });
        super::merge(ranges)
    }

    /// The codepoints a `cmap` table maps to one of `glyph_count` glyphs,
    /// from its most complete subtable.
    pub(in crate::config) fn cmap_coverage(
        table: &[u8],
        glyph_count: u32,
    ) -> Vec<RangeInclusive<u32>> {
        let Some((_, _, subtable)) = Cmap::read(FontData::new(table))
            .ok()
            .and_then(|cmap| cmap.best_subtable())
        else {
            return Vec::new();
        };
        let limits = CmapIterLimits {
            max_char: u32::from(char::MAX),
            glyph_count,
        };
        let mut ranges: Vec<RangeInclusive<u32>> = Vec::new();
        for (c, glyph) in subtable.iter_with_limits(limits) {
            if glyph.to_u32() == 0 {
                continue;
            }
            if let Some(last) = ranges.last_mut()
                && last.end().checked_add(1) == Some(c)
            {
                *last = *last.start()..=c;
            } else if ranges.len() == MAX_FACE_RANGES {
                break;
            } else {
                ranges.push(c..=c);
            }
        }
        ranges
    }
}

/// Only the macOS rasterizer is known to draw another face's color glyphs
/// through GPUI's emoji path, so other platforms keep every fallback in the
/// text color rather than risk painting outlines black.
#[cfg(not(target_os = "macos"))]
pub(super) fn platform_has_color(_family: &str) -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub(super) fn platform_coverage(_family: &str) -> Vec<RangeInclusive<u32>> {
    Vec::new()
}
