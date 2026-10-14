//! Glyph painting, in the text color or in the glyph font's own colors.
//!
//! GPUI marks a glyph as color only when Apple Color Emoji supplies it, so a
//! color font in the terminal's cascade, such as an `sbix` spinner font, would
//! paint as a mask in the text color. Config resolution finds the codepoints
//! the cascade takes from color fonts; their glyphs paint as GPUI paints emoji.

use super::TerminalPainter;
use crate::config::ColorGlyphs;
use gpui::*;

impl TerminalPainter {
    /// The codepoints that draw in their font's own colors. Shaped glyphs
    /// stay cached: the face that draws a codepoint does not depend on this.
    pub fn set_color_glyphs(&mut self, glyphs: &ColorGlyphs) {
        if self.color_glyphs != *glyphs {
            self.color_glyphs = glyphs.clone();
        }
    }
}

/// `ShapedLine::paint` without its per-call layer, whose BoundsTree insert
/// would otherwise run once per cell. Glyph placement matches GPUI's.
pub(super) fn paint_glyphs(
    line: &ShapedLine,
    origin: Point<Pixels>,
    line_height: Pixels,
    color: Rgba,
    color_glyphs: &ColorGlyphs,
    window: &mut Window,
) -> Result<()> {
    let baseline = origin
        + point(
            px(0.),
            (line_height - line.ascent - line.descent) / 2. + line.ascent,
        );
    for run in &line.runs {
        for glyph in &run.glyphs {
            let position = baseline + point(glyph.position.x, px(0.));
            if glyph.is_emoji || in_color(line, glyph.index, color_glyphs) {
                window.paint_emoji(position, run.font_id, glyph.id, line.font_size)?;
            } else {
                window.paint_glyph(
                    position,
                    run.font_id,
                    glyph.id,
                    line.font_size,
                    color.into(),
                )?;
            }
        }
    }
    Ok(())
}

/// Whether the character a glyph was shaped from, at UTF-8 offset `index`,
/// draws in color.
pub(super) fn in_color(line: &ShapedLine, index: usize, color_glyphs: &ColorGlyphs) -> bool {
    line.text
        .get(index..)
        .and_then(|text| text.chars().next())
        .is_some_and(|c| color_glyphs.contains(c))
}
