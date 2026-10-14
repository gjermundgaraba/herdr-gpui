use super::*;
use crate::config::ColorGlyphs;

#[gpui::test]
fn glyphs_paint_in_color_by_the_character_they_were_shaped_from(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    let colors: ColorGlyphs = [0x10_0000..=0x10_5c6a].into_iter().collect();
    cx.update(|window, _| {
        let text = "a\u{100100}\u{e9}\u{105c6a}";
        let line = TerminalPainter::default().shape(&font("Menlo"), 0, text, window);
        let glyphs: Vec<_> = line.runs.iter().flat_map(|run| &run.glyphs).collect();
        let painted: Vec<bool> = glyphs
            .iter()
            .map(|glyph| color_glyphs::in_color(&line, glyph.index, &colors))
            .collect();
        assert_eq!(painted, [false, true, false, true]);
        // An index past the text, or inside a character, is never color.
        assert!(!color_glyphs::in_color(&line, text.len(), &colors));
        assert!(!color_glyphs::in_color(&line, 2, &colors));
    });
}
