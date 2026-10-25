//! The editor's syntax highlighting: syntect, line by line, as the text
//! changes (feature `highlight`; without it nothing is highlighted).
//!
//! The parser's state at the start of every line parsed is kept, so an edit
//! costs only the lines it touched: parsing starts again at the first line
//! the edit changed, from the state kept there, and stops as soon as a
//! line ends in the state the next line started with before (its tokens
//! and every later line's are what they were). Only an edit that changes
//! what follows (opening a block comment or a string) parses on, and then
//! within a time budget per call: the view shows the lines parsed so far
//! and asks again on the next frame. Lines not parsed again yet keep the
//! tokens they had, so the colors do not flicker while that runs.
//!
//! The tokens and their colors are `crate::syntax`'s, shared with
//! `TextView`'s code blocks.

#[cfg(not(feature = "highlight"))]
pub(crate) use self::plain::Highlighter;
#[cfg(feature = "highlight")]
pub(crate) use self::syntect_lines::Highlighter;

#[cfg(feature = "highlight")]
mod syntect_lines {
    use std::{ops::Range, time::Duration};

    use nsh_core::time::Instant;
    use ropey::Rope;
    use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

    use crate::syntax::{Token, find_syntax, loaded, scope_token, syntaxes};

    /// The parser and the scopes open at a line's start.
    type State = (ParseState, ScopeStack);

    /// What is known of one line.
    #[derive(Default)]
    struct Line {
        /// The state at the line's start: the end of the line before as
        /// last parsed, `None` before that is parsed.
        start: Option<State>,
        /// Its tokens as parsed from `start`, `None` while it is not
        /// parsed (or was edited since). Whenever these are known, so is
        /// the next line's `start` (unless this is the last line).
        tokens: Option<Vec<(Range<usize>, Token)>>,
        /// Its bytes with its line break, as last parsed.
        len: usize,
    }

    /// Highlights a text line by line, again from where it changed.
    pub(crate) struct Highlighter {
        syntax: &'static syntect::parsing::SyntaxReference,
        /// The lines from the first one, as far as they have been parsed
        /// (one past the last line parsed, unless that was the last line
        /// of the text).
        lines: Vec<Line>,
        /// The lines before this one are current; its `start` is known.
        parsed: usize,
        /// The line being parsed, reused.
        line: String,
        /// How many lines were parsed, for the tests of what an edit costs.
        #[cfg(test)]
        parses: usize,
    }

    impl Highlighter {
        /// None for plain text, a language syntect lacks. `hint`: the
        /// language's syntax hint (an extension or a name syntect knows).
        /// Loads the syntaxes when they are not loaded yet ([`Self::ready`]).
        pub(crate) fn new(hint: &str) -> Option<Highlighter> {
            let syntax = find_syntax(hint)?;
            Some(Highlighter {
                syntax,
                lines: vec![Line {
                    start: Some((ParseState::new(syntax), ScopeStack::new())),
                    ..Line::default()
                }],
                parsed: 0,
                line: String::new(),
                #[cfg(test)]
                parses: 0,
            })
        }

        /// Whether the syntaxes are loaded (loading takes tens of ms: the
        /// view loads them on a blocking thread first).
        pub(crate) fn ready() -> bool {
            loaded()
        }

        /// Loads the syntaxes (blocking; call off the engine thread).
        pub(crate) fn load() {
            syntaxes();
        }

        /// Lines `first..first + old_count` became `first..first +
        /// new_count`: the states from `first` on are invalid; the tokens
        /// of lines after the span shift.
        pub(crate) fn edited(&mut self, first: usize, old_count: usize, new_count: usize) {
            self.parsed = self.parsed.min(first);
            let Some(line) = self.lines.get_mut(first) else {
                // Past the lines parsed: nothing known changes.
                return;
            };
            // What comes before the span is unchanged, so is the state at
            // its start.
            let start = line.start.take();
            let end = first + old_count;
            if end <= self.lines.len() {
                // The line after the span keeps its start and tokens: the
                // state its line before ends in tells whether they hold.
                self.lines.splice(
                    first..end,
                    std::iter::repeat_with(Line::default).take(new_count),
                );
            } else {
                // The span reaches past the lines parsed: what follows it
                // is not known.
                self.lines.truncate(first);
            }
            if self.lines.len() == first {
                self.lines.push(Line::default());
            }
            let line = &mut self.lines[first];
            line.start = start;
            line.tokens = None;
        }

        /// Parses dirty lines in order up to `last_line` (inclusive), within
        /// `budget` (at least one line per call); stops early once a line's
        /// end state equals what it was before (the rest is unchanged).
        /// Returns the lines whose tokens changed and whether all lines up
        /// to `last_line` are current.
        pub(crate) fn highlight(
            &mut self,
            rope: &Rope,
            last_line: usize,
            budget: Duration,
        ) -> (Range<usize>, bool) {
            let started = Instant::now();
            let set = syntaxes();
            let mut changed: Option<Range<usize>> = None;
            let mut offset: usize = self.lines[..self.parsed].iter().map(|line| line.len).sum();
            while self.parsed < self.lines.len() && self.parsed <= last_line {
                if offset > rope.len_bytes() {
                    // Edits the highlighter was not told of: the text ends
                    // before the lines known.
                    log::warn!("editor: the highlighter missed an edit");
                    self.lines.truncate(self.parsed.max(1));
                    break;
                }
                let index = self.parsed;
                let newline = read_line(rope, offset, &mut self.line);
                let len = self.line.len();
                let content = len - usize::from(newline);
                if !newline {
                    // The syntaxes expect every line with its line break.
                    self.line.push('\n');
                }
                // The first line not current always has its start; were it
                // missing, parsing from scratch is the best guess.
                let mut state = self.lines[index]
                    .start
                    .clone()
                    .unwrap_or_else(|| (ParseState::new(self.syntax), ScopeStack::new()));
                let tokens = parse(&mut state, &self.line, content, set);
                #[cfg(test)]
                {
                    self.parses += 1;
                }
                let line = &mut self.lines[index];
                if line.tokens.as_ref() != Some(&tokens) {
                    changed = Some(match changed {
                        Some(range) => range.start..index + 1,
                        None => index..index + 1,
                    });
                }
                line.tokens = Some(tokens);
                line.len = len;
                self.parsed = index + 1;
                offset += len;
                if !newline {
                    // The last line of the text.
                    self.lines.truncate(index + 1);
                    break;
                }
                match self.lines.get_mut(index + 1) {
                    None => self.lines.push(Line {
                        start: Some(state),
                        ..Line::default()
                    }),
                    Some(next) if next.start.as_ref() == Some(&state) => {
                        // Converged: the lines parsed before from here on
                        // hold, up to the next one an edit left unparsed.
                        while let Some(line) = self.lines.get(self.parsed) {
                            if line.tokens.is_none() {
                                break;
                            }
                            offset += line.len;
                            self.parsed += 1;
                        }
                    }
                    Some(next) => next.start = Some(state),
                }
                if started.elapsed() >= budget {
                    break;
                }
            }
            let current = self.parsed > last_line || self.parsed == self.lines.len();
            (changed.unwrap_or(self.parsed..self.parsed), current)
        }

        /// The tokens of `line`: byte ranges within the line (without its
        /// '\n'), sorted, not overlapping. None while the line is not
        /// highlighted yet; a line an edit has not reached yet keeps the
        /// tokens it had.
        pub(crate) fn tokens(&self, line: usize) -> Option<&[(Range<usize>, Token)]> {
            self.lines.get(line)?.tokens.as_deref()
        }
    }

    /// Reads the line starting at `offset` into `line`, with its line
    /// break; whether it has one (the text's last line has none).
    fn read_line(rope: &Rope, offset: usize, line: &mut String) -> bool {
        line.clear();
        let (chunks, chunk_start, _, _) = rope.chunks_at_byte(offset);
        let mut skip = offset - chunk_start;
        for chunk in chunks {
            let chunk = &chunk[skip..];
            skip = 0;
            if let Some(end) = chunk.find('\n') {
                line.push_str(&chunk[..=end]);
                return true;
            }
            line.push_str(chunk);
        }
        false
    }

    /// The tokens of `line` (with its line break; `content` is its length
    /// without), parsed on from `state`, which it leaves at the line's end.
    fn parse(
        state: &mut State,
        line: &str,
        content: usize,
        set: &SyntaxSet,
    ) -> Vec<(Range<usize>, Token)> {
        let mut tokens = Vec::new();
        let (parser, scopes) = state;
        // A line syntect cannot parse keeps the scopes it started with;
        // the next line starts over from them.
        let Ok(operations) = parser.parse_line(line, set) else {
            return tokens;
        };
        let mut cursor = 0;
        for (index, operation) in operations {
            push(&mut tokens, cursor..index.min(content), line, scopes);
            let _ = scopes.apply(&operation);
            cursor = index;
        }
        push(&mut tokens, cursor..content, line, scopes);
        tokens
    }

    /// Adds `range` with the token of its innermost scope that has one,
    /// merged with the token before when they touch and are alike.
    fn push(
        tokens: &mut Vec<(Range<usize>, Token)>,
        range: Range<usize>,
        line: &str,
        scopes: &ScopeStack,
    ) {
        if range.start >= range.end {
            return;
        }
        let Some(token) = scope_token(scopes, &line[range.clone()]) else {
            return;
        };
        match tokens.last_mut() {
            Some((last, last_token)) if last.end == range.start && *last_token == token => {
                last.end = range.end;
            }
            _ => tokens.push((range, token)),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn highlighted(hint: &str, text: &str) -> Highlighter {
            let mut highlighter = Highlighter::new(hint).unwrap();
            let (_, current) =
                highlighter.highlight(&Rope::from_str(text), usize::MAX, Duration::MAX);
            assert!(current);
            highlighter
        }

        /// Every line's tokens, checked to be in order and on the line's
        /// character boundaries.
        fn all_tokens(highlighter: &Highlighter, text: &str) -> Vec<Vec<(Range<usize>, Token)>> {
            let lines: Vec<&str> = text.split('\n').collect();
            assert!(
                highlighter.tokens(lines.len()).is_none(),
                "a line past the text"
            );
            lines
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    let tokens = highlighter.tokens(index).expect("highlighted").to_vec();
                    let mut at = 0;
                    for (range, _) in &tokens {
                        assert!(at <= range.start && range.start < range.end, "line {index}");
                        assert!(range.end <= line.len(), "line {index}");
                        assert!(line.is_char_boundary(range.start));
                        assert!(line.is_char_boundary(range.end));
                        at = range.end;
                    }
                    tokens
                })
                .collect()
        }

        /// The token covering `word` on `line`.
        fn token_of(
            highlighter: &Highlighter,
            text: &str,
            line: usize,
            word: &str,
        ) -> Option<Token> {
            let start = text.split('\n').nth(line).unwrap().find(word).unwrap();
            highlighter
                .tokens(line)
                .unwrap()
                .iter()
                .find(|(range, _)| range.start <= start && start + word.len() <= range.end)
                .map(|(_, token)| *token)
        }

        /// Replaces `range` of `text` with `with` and tells `highlighter`
        /// which lines changed, as the buffer would.
        fn edit(
            highlighter: &mut Highlighter,
            text: &str,
            range: Range<usize>,
            with: &str,
        ) -> String {
            let first = text[..range.start].matches('\n').count();
            let old_count = text[range.clone()].matches('\n').count() + 1;
            let new_count = with.matches('\n').count() + 1;
            highlighter.edited(first, old_count, new_count);
            format!("{}{with}{}", &text[..range.start], &text[range.end..])
        }

        /// Highlights `text` until it is current and checks the tokens are
        /// those a fresh parse gives.
        fn check_fresh(highlighter: &mut Highlighter, hint: &str, text: &str) -> Range<usize> {
            let (changed, current) =
                highlighter.highlight(&Rope::from_str(text), usize::MAX, Duration::MAX);
            assert!(current);
            assert_eq!(
                all_tokens(highlighter, text),
                all_tokens(&highlighted(hint, text), text)
            );
            changed
        }

        fn rust_lines(count: usize) -> String {
            let mut text = String::from("fn main() {\n");
            for i in 0..count {
                text.push_str(&format!("    let x{i} = {i}; // 注释 👍🏽\n"));
            }
            text.push('}');
            text
        }

        #[test]
        fn rust_and_typescript_tokens() {
            let ts = "// 注释 ✓ 👋🏽\nexport async function load(id: number): Promise<string> {\n\tconst s = \"héllo\";\n  return s;\n}\n";
            let highlighter = highlighted("ts", ts);
            all_tokens(&highlighter, ts);
            assert_eq!(
                token_of(&highlighter, ts, 0, "// 注释"),
                Some(Token::Comment)
            );
            assert_eq!(
                token_of(&highlighter, ts, 1, "function"),
                Some(Token::Keyword)
            );
            assert_eq!(token_of(&highlighter, ts, 1, "load"), Some(Token::Title));
            assert_eq!(
                token_of(&highlighter, ts, 2, "\"héllo\""),
                Some(Token::String)
            );
            // The empty line after the last line break.
            assert_eq!(highlighter.tokens(5), Some(&[][..]));

            let rs = "fn main() {\n    let x: u32 = 42; // ünïcode\n}";
            let highlighter = highlighted("rs", rs);
            all_tokens(&highlighter, rs);
            assert_eq!(token_of(&highlighter, rs, 0, "fn"), Some(Token::Keyword));
            assert_eq!(token_of(&highlighter, rs, 0, "main"), Some(Token::Title));
            assert_eq!(token_of(&highlighter, rs, 1, "u32"), Some(Token::Built));
            assert_eq!(token_of(&highlighter, rs, 1, "42"), Some(Token::Number));
            assert_eq!(token_of(&highlighter, rs, 1, "// ü"), Some(Token::Comment));
            assert!(highlighter.tokens(3).is_none());
        }

        #[test]
        fn empty_and_plain_texts() {
            let highlighter = highlighted("rust", "");
            assert_eq!(highlighter.tokens(0), Some(&[][..]));
            assert!(highlighter.tokens(1).is_none());
            for plain in ["", "txt", "text", "no-such-language"] {
                assert!(Highlighter::new(plain).is_none(), "{plain}");
            }
            for hint in [
                "rs", "tsx", "ts", "js", "sh", "py", "Rust", "Makefile", "toml",
            ] {
                assert!(Highlighter::new(hint).is_some(), "{hint}");
            }
        }

        #[test]
        fn an_edit_parses_until_the_state_converges() {
            let text = rust_lines(200);
            let mut highlighter = highlighted("rust", &text);
            assert_eq!(highlighter.parses, 202);

            // Line 51 is `let x50 = 50;`: a longer number.
            let at = text.find("= 50;").unwrap() + 2;
            let text = edit(&mut highlighter, &text, at..at + 2, "5000");
            highlighter.parses = 0;
            assert_eq!(check_fresh(&mut highlighter, "rust", &text), 51..52);
            assert_eq!(highlighter.parses, 1);

            // A line in, a line out: the lines after shift.
            let at = text.find("    let x10").unwrap();
            let text = edit(&mut highlighter, &text, at..at, "    let s = \"新\";\n");
            highlighter.parses = 0;
            check_fresh(&mut highlighter, "rust", &text);
            assert_eq!(highlighter.parses, 2);
            let start = text.find("    let x3 ").unwrap();
            let end = text.find("    let x7 ").unwrap();
            let text = edit(&mut highlighter, &text, start..end, "");
            highlighter.parses = 0;
            check_fresh(&mut highlighter, "rust", &text);
            assert_eq!(highlighter.parses, 1);

            // Nothing changed: nothing to parse.
            highlighter.parses = 0;
            let (changed, current) =
                highlighter.highlight(&Rope::from_str(&text), usize::MAX, Duration::MAX);
            assert!(current && changed.is_empty());
            assert_eq!(highlighter.parses, 0);
        }

        #[test]
        fn opening_a_block_comment_highlights_the_lines_after_it() {
            let original = rust_lines(20);
            let mut highlighter = highlighted("rust", &original);
            let lines = original.split('\n').count();
            assert_eq!(
                token_of(&highlighter, &original, 10, "let"),
                Some(Token::Keyword)
            );

            let at = original.find("    let x2 ").unwrap();
            let text = edit(&mut highlighter, &original, at..at, "/* ");
            let changed = check_fresh(&mut highlighter, "rust", &text);
            assert_eq!(changed, 3..lines);
            for line in 3..lines {
                assert!(
                    highlighter
                        .tokens(line)
                        .unwrap()
                        .iter()
                        .all(|(_, token)| *token == Token::Comment),
                    "line {line}"
                );
            }

            // Closing it again restores the lines after it.
            let text = edit(&mut highlighter, &text, at..at + 3, "");
            assert_eq!(text, original);
            assert_eq!(check_fresh(&mut highlighter, "rust", &text), 3..lines);
            assert_eq!(
                token_of(&highlighter, &text, 10, "let"),
                Some(Token::Keyword)
            );
        }

        #[test]
        fn a_budget_parses_on_over_several_calls() {
            let text = rust_lines(50);
            let rope = Rope::from_str(&text);
            let mut highlighter = Highlighter::new("rust").unwrap();
            let mut calls = 0;
            while !highlighter.highlight(&rope, usize::MAX, Duration::ZERO).1 {
                calls += 1;
                assert!(calls < 100);
            }
            assert!(calls > 1);
            assert_eq!(
                all_tokens(&highlighter, &text),
                all_tokens(&highlighted("rust", &text), &text)
            );

            // Only up to the last line asked for.
            let mut highlighter = Highlighter::new("rust").unwrap();
            let (changed, current) = highlighter.highlight(&rope, 9, Duration::MAX);
            assert!(current);
            assert_eq!(changed, 0..10);
            assert!(highlighter.tokens(9).is_some() && highlighter.tokens(10).is_none());

            // An edit while lines after it are still unparsed.
            let at = text.find("    let x3 ").unwrap();
            let text = edit(&mut highlighter, &text, at..at, "/*\n");
            check_fresh(&mut highlighter, "rust", &text);
            let at = text.find("    let x40 ").unwrap();
            let text = edit(&mut highlighter, &text, at..text.len(), "*/ }");
            check_fresh(&mut highlighter, "rust", &text);
        }
    }
}

#[cfg(not(feature = "highlight"))]
mod plain {
    use std::{ops::Range, time::Duration};

    use ropey::Rope;

    use crate::syntax::Token;

    /// Without the feature nothing is highlighted: [`Highlighter::new`]
    /// makes none.
    pub(crate) struct Highlighter(());

    impl Highlighter {
        /// None: there are no syntaxes.
        pub(crate) fn new(_hint: &str) -> Option<Highlighter> {
            None
        }

        /// Nothing to load.
        pub(crate) fn ready() -> bool {
            true
        }

        /// Nothing to load.
        pub(crate) fn load() {}

        pub(crate) fn edited(&mut self, _first: usize, _old_count: usize, _new_count: usize) {}

        pub(crate) fn highlight(
            &mut self,
            _rope: &Rope,
            _last_line: usize,
            _budget: Duration,
        ) -> (Range<usize>, bool) {
            (0..0, true)
        }

        pub(crate) fn tokens(&self, _line: usize) -> Option<&[(Range<usize>, Token)]> {
            None
        }
    }
}
