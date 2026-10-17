# Text

- **Text**: `<Text numberOfLines ellipsizeMode="tail|head|middle|clip">`; styles `fontFamily` (system or imported fonts:
  `import Inter from "./Inter.ttf"` evaluates to the family name and registers the font before the first frame), `fontWeight`,
  `fontStyle`, `letterSpacing`, `textDecorationLine` + `textDecorationColor`, `textTransform`, `lineHeight`, `textAlign`,
  `textAlignVertical` (the core moves the content box down). `adjustsFontSizeToFit` shrinks the fonts (down to
  `minimumFontScale`, at least 0.1) until the text fits its box and lines (`Paragraph::fit`, a binary search in the core).
  `onTextLayout` reports the final lines (`TextEngine::lines`: range, box, ascender, descender, cap / x height).
  `selectable` (or `select-text`): range selection, drawing only (`AttributedText::selectable` /
  `selection_color`, not part of layout). iOS (`text_selection.rs`): the CoreText `TextView` adopts `UITextInput`
  (non-editable: no keyboard, `inputView` is empty) and a `UITextInteraction` (`NonEditable`) attached only while the text
  is selectable gives UIKit's long press, loupe, handles, highlight (the view's tint, which `selectionColor` sets) and edit
  menu (Copy, Select All, and the system's Look Up / Translate / Share); positions are UTF-16 offsets on the CTLines.
  Resigning first responder (a tap elsewhere, another input focusing) ends the selection. Android (`TextBox`): a long press
  selects the word (`BreakIterator`) and the rest of the gesture extends it; handles are the theme's
  `textSelectHandleLeft/Right` in popup windows (so they escape clipping and take their own touches), the highlight is
  `Layout.getSelectionPath` in `selectionColor` (55% alpha; the theme's `textColorHighlight` by default) and a floating
  action mode offers Copy, Select all and Share. A touch elsewhere in the surface (`Mounter.touchDown`), new text or
  detaching ends it; one selection at a time per surface. Copy leaves attachments' placeholders out on both. Text that is
  not selectable has no interaction, listener or extra drawing. `selectable` is `Option<bool>` (unset, or `false` from
  `selectable={false}` / `select-none`) because the web's desktop idiom selects text that is not set off by default, as a
  web page does, except inside views that take presses or touches. Web: the browser's own selection, `::selection` in
  `selectionColor`; copying joins the absolutely placed runs with each line's break ([web.md](web.md)). Linux (`text.rs`, desktop conventions): a mouse drag selects characters (past a 3 pt slop, 10 pt over a
  press target, so clicking a link still presses it), a double click words, a triple click the paragraph (drags go on by
  them), Shift+click extends; the highlight is drawn behind the glyphs in `selectionColor` at 30% (the system blue by
  default); the view takes keyboard focus only while it has a selection, for Ctrl+C (and Ctrl+Insert), Ctrl+A and
  Escape, and a press anywhere else or losing focus ends it; a finished drag also sets the primary selection.
  Touchscreen drags scroll instead; there is no context menu.
- **Inline attachments**: any other element inside a `<Text>` (also inside its spans) is an attachment: laid out as its own
  layout root at its content size (`update_attachments`), it becomes an object replacement character (U+FFFC,
  `AttributedText::attachments`) that the engine sizes to it on the baseline (CTRunDelegate; Android a `ReplacementSpan`), so
  it wraps with the text. After the final layout the core places its native view, a child of the text view (Android's
  `TextBox` is a ViewGroup for this), on the placeholder (`TextEngine::attachments`); attachments cut off by the line limit or
  an ellipsis move out of sight. Their own children are normal layout children; a text inside them built after the block
  resizes them and builds the block again (`check_attachments`). They take presses of their own; copying leaves the
  placeholders out.

- **Rust components** nest texts the same way: `text("a ").child(text("b").underline().on_press(..)).child(view())` is
  one block; a plain string child is a raw text node (as JS's strings), a text's own content its first piece
  (`core/components.rs`, `Part::Raw`).

See also the text pitfalls in [pitfalls.md](pitfalls.md) and the engines in `crates/nsh-platform-ios/src/text.rs` /
`crates/nsh-platform-android/src/text.rs`.
