use nsh_core::{AttributedText, Points, TextEngine};
use nsh_platform_tui::CellTextEngine;

#[test]
fn wide_and_combining_graphemes_use_display_cells_at_any_font_size() {
    let engine = CellTextEngine::default();
    let mut text = AttributedText::default();
    text.push("A界e\u{301}👩‍💻", Default::default());
    let prepared = engine.prepare(&text);
    let measured = engine.measure(&prepared, None);
    assert_eq!(measured.width, Points(48.0));
    assert_eq!(measured.height, Points(16.0));
    text.spans[0].style.font_size = Points(64.0);
    assert_eq!(engine.measure(&engine.prepare(&text), None), measured);
}

#[test]
fn wrapping_prefers_spaces_and_keeps_utf16_grapheme_ranges() {
    let engine = CellTextEngine::default();
    let mut text = AttributedText::default();
    text.push("ab 界👩‍💻z", Default::default());
    let prepared = engine.prepare(&text);
    let measured = engine.measure(&prepared, Some(Points(32.0)));
    assert_eq!(measured, nsh_core::Size::new(Points(32.0), Points(48.0)));
    let lines = engine.lines(&engine.layout(&prepared, measured));
    assert_eq!(
        lines.iter().map(|l| (l.start, l.end)).collect::<Vec<_>>(),
        [(0, 2), (3, 9), (9, 10)]
    );
    assert_eq!(lines[1].y, Points(16.0));
}

#[test]
fn line_limit_and_alignment_use_cell_metrics() {
    let engine = CellTextEngine::default();
    let mut text = AttributedText::default();
    text.push("ab cd ef", Default::default());
    text.max_lines = 1;
    text.align = nsh_core::TextAlign::Right;
    let prepared = engine.prepare(&text);
    assert_eq!(
        engine.measure(&prepared, Some(Points(32.0))).height,
        Points(16.0)
    );
    let lines =
        engine.lines(&engine.layout(&prepared, nsh_core::Size::new(Points(32.0), Points(16.0))));
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].width, Points(24.0));
    assert_eq!((lines[0].start, lines[0].end), (0, 2));
    assert_eq!(lines[0].x, Points(8.0));
}

#[test]
fn explicit_lines_empty_text_and_narrow_boxes_are_finite() {
    let engine = CellTextEngine::default();
    for (value, height) in [("", 16.0), ("a\n", 32.0), ("\n\n", 48.0), ("界", 16.0)] {
        let mut text = AttributedText::default();
        text.push(value, Default::default());
        assert_eq!(
            engine
                .measure(&engine.prepare(&text), Some(Points::ZERO))
                .height,
            Points(height)
        );
    }
    assert!(nsh_platform_tui::CellSize::new(Points::ZERO, Points(16.0)).is_err());
    assert!(nsh_platform_tui::CellSize::new(Points(f32::NAN), Points(16.0)).is_err());
}

#[test]
fn a_space_at_the_wrap_boundary_does_not_start_an_extra_row() {
    let engine = CellTextEngine::default();
    let mut text = AttributedText::default();
    text.push("ab cd", Default::default());
    let prepared = engine.prepare(&text);
    let size = engine.measure(&prepared, Some(Points(16.0)));
    assert_eq!(size, nsh_core::Size::new(Points(16.0), Points(32.0)));
    let lines = engine.lines(&engine.layout(&prepared, size));
    assert_eq!((lines[1].start, lines[1].end), (3, 5));
}

#[test]
fn empty_lines_after_crlf_keep_their_utf16_offsets() {
    let engine = CellTextEngine::default();
    let mut text = AttributedText::default();
    text.push("a\r\n\r\nb", Default::default());
    let prepared = engine.prepare(&text);
    let size = engine.measure(&prepared, None);
    let lines = engine.lines(&engine.layout(&prepared, size));
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.start, line.end))
            .collect::<Vec<_>>(),
        [(0, 1), (3, 3), (5, 6)]
    );
}
