//! The Text demo on a headless engine: its sentences are one paragraph
//! each (spans, links, inline views), links and the button inside a
//! sentence take presses, other languages and alignments have cards.

use nsh_core::{EventKind, TextDecoration};

use crate::common::{press_link, shows};

/// The sentence with links.
const SENTENCE: &str = "By continuing you agree to the Terms of service and the Privacy policy, which mention nested links too.";

#[test]
fn text_renders_its_samples() {
    let (_engine, _rx, screen) = crate::common::open("Text");
    for text in [
        SENTENCE,
        "Spans and links",
        "Pressed",
        "nothing yet",
        "LIVE",
        "♥ 3",
        "Custom fonts",
        "Pacifico-Regular.ttf",
        "Light Bold Italic Both Spaced",
        "Underline Strikethrough Both",
        "UPPERCASE Capitalized Words Here",
        "Glow",
        "Drop shadow",
        "System text size",
        "1.00×",
        "maxFontSizeMultiplier={1.2}",
        "Truncation",
        "One line, middle",
        "Centered, three lines",
        "Chinese and Japanese",
        "Japanese: kana, kanji and Latin",
        "Right to left",
        "Arabic paragraph",
        "writingDirection=\"rtl\"",
        "Hebrew",
        "Alignment",
        "Justify, Chinese",
        "Left, in Arabic",
    ] {
        assert!(shows(&screen, text), "{text}: {:?}", screen.texts);
    }
    // The image, the badge and the button wrap with the words.
    let inline = screen
        .block(
            "Made with \u{fffc} nsh. Status \u{fffc} and a button inside the sentence \u{fffc} that takes presses of its own.",
        )
        .expect("one paragraph");
    assert_eq!(inline.attachments.len(), 3);
    let links = &screen.block(SENTENCE).expect("one paragraph").links;
    assert_eq!(links.len(), 3, "{links:?}");
    // "Both" is underlined and struck through; the strikethrough has a
    // color of its own.
    let decorations = screen.block("Underline Strikethrough Both").unwrap();
    let both = decorations.spans.last().unwrap();
    assert_eq!(
        both.style.decoration,
        TextDecoration {
            underline: true,
            line_through: true
        }
    );
    let struck = &decorations.spans[2];
    assert!(struck.style.decoration_color.is_some(), "{struck:?}");
    let shadow = screen
        .block("Drop shadow")
        .unwrap()
        .shadow
        .expect("a shadow");
    assert_eq!((shadow.radius.0, shadow.offset_x.0), (3.0, 2.0));
}

#[test]
fn links_and_the_inline_button_take_presses() {
    let (engine, rx, mut screen) = crate::common::open("Text");
    press_link(
        &engine,
        &rx,
        &mut screen,
        "Privacy policy",
        EventKind::Press,
    );
    assert!(shows(&screen, "Privacy policy"), "{:?}", screen.texts);
    press_link(&engine, &rx, &mut screen, "links", EventKind::Press);
    assert!(
        shows(&screen, "a link inside bold text"),
        "{:?}",
        screen.texts
    );
    press_link(
        &engine,
        &rx,
        &mut screen,
        "Terms of service",
        EventKind::LongPress,
    );
    assert!(shows(&screen, "Terms (long press)"), "{:?}", screen.texts);
    // The sentence around the links.
    crate::common::press_text(&engine, &rx, &mut screen, SENTENCE);
    assert!(
        shows(&screen, "the sentence (not a link)"),
        "{:?}",
        screen.texts
    );
    crate::common::press(&engine, &rx, &mut screen, "♥ 3");
    assert!(shows(&screen, "♥ 4"), "{:?}", screen.texts);
}
