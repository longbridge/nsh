//! A line clamp cuts its last line at the box, inside a word if need be,
//! not after the last word that fit there: a `truncate`d name in a row
//! ended at a space with the box empty past it.

use nsh_core::{AttributedText, EllipsizeMode, Points, ResolvedTextStyle, TextEngine};

use crate::text::{WebLayout, WebTextEngine};

fn clamped(s: &str, max_lines: u32) -> AttributedText {
    let mut t = AttributedText::default();
    t.push(s, ResolvedTextStyle::default());
    t.max_lines = max_lines;
    t.ellipsize = EllipsizeMode::Tail;
    t
}

#[test]
fn clamped_last_line_is_cut_at_the_box() {
    let engine = WebTextEngine::default();
    // The stub measures every character equally wide.
    let w = engine
        .measure(&engine.prepare(&clamped("x", 0)), None)
        .width
        .0;
    let box_width = 150.0;
    for (long, max_lines) in [
        ("Christopher Montgomery Wellington", 1),
        ("Karl Anton Hohenzollern-Sigmaringen", 1),
        (
            "Christopher Montgomery-Wellington the Third, Senior Equity Strategist",
            2,
        ),
    ] {
        let t = clamped(long, max_lines);
        let p = engine.prepare(&t);
        let size = engine.measure(&p, Some(Points(box_width)));
        assert_eq!(size.width.0, box_width, "{long}");
        let layout = engine.layout(&p, size);
        let lines = engine.lines(&layout);
        assert_eq!(lines.len(), max_lines as usize, "{long}");
        let last = lines.last().unwrap();
        let shown: String = layout
            .downcast::<WebLayout>()
            .unwrap()
            .runs
            .iter()
            .filter(|r| r.y >= last.y.0 - 0.01)
            .map(|r| r.text.as_str())
            .collect();
        // As many characters as fit with the ellipsis, spaces before it
        // dropped.
        let start = last.start as usize;
        let keep = ((box_width + 0.01) / w).floor() as usize - 1;
        let expected = format!("{}…", long[start..start + keep].trim_end());
        assert_eq!(shown, expected, "{long}");
        assert!(
            last.x.0 + last.width.0 <= box_width + 0.01,
            "{long}: {last:?}"
        );
        // The ellipsis stands for the rest of the text.
        assert_eq!(last.end as usize, long.len(), "{long}");
    }
}
