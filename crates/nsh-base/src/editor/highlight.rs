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

use std::time::Duration;

#[cfg(not(feature = "highlight"))]
pub(crate) use self::plain::{Done, GUESS_CONTEXT, Guess, Guessed, Highlighter, Job};
#[cfg(feature = "highlight")]
pub(crate) use self::syntect_lines::{Done, GUESS_CONTEXT, Guess, Guessed, Highlighter, Job};

/// How long a [`Job`] parses before the view shows what it found: on a
/// blocking thread where there are threads, else within a frame.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const SLICE: Duration = Duration::from_millis(25);
/// How long a [`Job`] parses before the view shows what it found: on a
/// blocking thread where there are threads, else within a frame.
#[cfg(target_arch = "wasm32")]
pub(crate) const SLICE: Duration = Duration::from_millis(4);

/// The lines in view are guessed ([`Highlighter::guess`]) when they are
/// at least this far past the parse from the text's start: further than a
/// slice or two parses.
pub(crate) const GUESS_DISTANCE: usize = 500;

/// How far past the last line in view the parse goes on (off the engine
/// thread, so scrolling down finds the lines highlighted).
pub(crate) const LOOKAHEAD: usize = 1000;

/// Runs `f` on a blocking thread where there are threads (WebAssembly has
/// none: there it runs at once).
pub(crate) async fn off_thread<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match nsh_core::ui::prelude::blocking(f).await {
            Ok(r) => r,
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        f()
    }
}

#[cfg(feature = "highlight")]
mod syntect_lines {
    use std::{ops::Range, time::Duration};

    use nsh_core::time::Instant;
    use ropey::Rope;
    use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

    use crate::syntax::{Token, find_syntax, loaded, scope_token, syntaxes};

    /// The parser and the scopes open at a line's start.
    type State = (ParseState, ScopeStack);

    /// A line's tokens: byte ranges within the line, sorted, apart.
    pub(crate) type Tokens = Vec<(Range<usize>, Token)>;

    /// The parser keeps its state at the start of every this many lines
    /// (and of the lines it kept one for before an edit shifted them): a
    /// state per line would cost about a kilobyte a line, and starting a
    /// parse up to this many lines early costs next to nothing.
    const CHECKPOINT: usize = 16;

    /// How many lines before the first one in view a guess starts at
    /// (from a fresh state): enough for most constructs that span lines to
    /// open again before the lines shown.
    pub(crate) const GUESS_CONTEXT: usize = 40;

    /// What the parser knows of one line.
    #[derive(Default)]
    struct Line {
        /// The state at the line's start, kept for checkpoints: what
        /// parsing starts again from, and what tells whether a parse that
        /// reaches the line converged.
        start: Option<State>,
        /// Its tokens are current (parsed since the line last changed).
        done: bool,
        /// Its bytes with its line break, as last parsed.
        len: usize,
    }

    /// The parsing half of a [`Highlighter`]: the states it parses on
    /// from, which a [`Job`] takes off the engine thread and back.
    pub(crate) struct Parser {
        syntax: &'static syntect::parsing::SyntaxReference,
        /// The lines from the first one, as far as they have been parsed
        /// (one past the last line parsed, unless that was the last line
        /// of the text).
        lines: Vec<Line>,
        /// The lines before this one are current.
        parsed: usize,
        /// Where line `parsed` starts in the text.
        offset: usize,
        /// The state at line `parsed`'s start when it has no checkpoint
        /// (parsing goes on from the line before).
        state: Option<State>,
        /// The last line the last job was to parse up to.
        last_line: usize,
        /// The line being parsed, reused.
        line: String,
        /// How many lines were parsed, for the tests of what an edit costs.
        #[cfg(test)]
        parses: usize,
    }

    /// A line's tokens as the view gets them.
    struct Known {
        tokens: Tokens,
        /// Parsed from a fresh state near the view, not from the text's
        /// start ([`Highlighter::guess`]): shown until the parse from the
        /// start reaches the line.
        guessed: bool,
    }

    /// Highlights a text line by line, again from where it changed: the
    /// tokens (on the engine thread, for the view) and the [`Parser`]
    /// (which parses on, here or on a blocking thread).
    pub(crate) struct Highlighter {
        /// Tells its jobs' and guesses' work from another highlighter's
        /// (one made since for other texts or another language).
        id: u64,
        syntax: &'static syntect::parsing::SyntaxReference,
        tokens: Vec<Option<Known>>,
        /// `None` while a [`Job`] has it.
        parser: Option<Parser>,
        /// The edits made while a job had the parser, in order: given to
        /// it when it comes back.
        pending: Vec<(usize, usize, usize)>,
        /// How far the parser had come, as last known.
        frontier: usize,
    }

    /// Parsing on from where a [`Highlighter`] stopped, which may run on
    /// another thread ([`Job::run`]) while the highlighter keeps showing
    /// the tokens it has.
    pub(crate) struct Job {
        id: u64,
        parser: Parser,
        rope: Rope,
        last_line: usize,
        budget: Duration,
    }

    /// What a [`Job`] did, for [`Highlighter::finish`].
    pub(crate) struct Done {
        id: u64,
        parser: Parser,
        /// Each line parsed, in order, and its tokens.
        parsed: Vec<(usize, Tokens)>,
    }

    /// Parsing the lines in view from a fresh state, ahead of the parse
    /// from the text's start ([`Highlighter::guess`]).
    pub(crate) struct Guess {
        id: u64,
        syntax: &'static syntect::parsing::SyntaxReference,
        rope: Rope,
        first: usize,
        offset: usize,
        last: usize,
    }

    /// What a [`Guess`] found: the tokens of its lines from the first.
    pub(crate) struct Guessed {
        id: u64,
        first: usize,
        tokens: Vec<Tokens>,
    }

    impl Highlighter {
        /// None for plain text, a language syntect lacks. `hint`: the
        /// language's syntax hint (an extension or a name syntect knows).
        /// Loads the syntaxes when they are not loaded yet ([`Self::ready`]).
        pub(crate) fn new(hint: &str) -> Option<Highlighter> {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let syntax = find_syntax(hint)?;
            Some(Highlighter {
                id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                syntax,
                tokens: Vec::new(),
                parser: Some(Parser {
                    syntax,
                    lines: vec![Line {
                        start: Some((ParseState::new(syntax), ScopeStack::new())),
                        ..Line::default()
                    }],
                    parsed: 0,
                    offset: 0,
                    state: None,
                    last_line: 0,
                    line: String::new(),
                    #[cfg(test)]
                    parses: 0,
                }),
                pending: Vec::new(),
                frontier: 0,
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
        /// of lines after the span shift (and stay until parsed again, so
        /// the colors do not flicker).
        pub(crate) fn edited(&mut self, first: usize, old_count: usize, new_count: usize) {
            splice(&mut self.tokens, first, old_count, new_count, || None);
            if let Some(slot) = self.tokens.get_mut(first) {
                *slot = None;
            }
            self.frontier = self.frontier.min(first);
            match &mut self.parser {
                Some(parser) => parser.edited(first, old_count, new_count),
                None => self.pending.push((first, old_count, new_count)),
            }
        }

        /// Parses the lines not current in order up to `last_line`
        /// (inclusive), within `budget` (at least one line per call), here;
        /// stops early once a line ends in the state the next line started
        /// with before (the rest is unchanged). Returns the lines whose
        /// tokens changed and whether all lines up to `last_line` are
        /// current.
        #[cfg(test)]
        pub(crate) fn highlight(
            &mut self,
            rope: &Rope,
            last_line: usize,
            budget: Duration,
        ) -> (Range<usize>, bool) {
            match self.job(rope, last_line, budget) {
                Some(job) => self.finish(job.run()),
                None => (
                    self.frontier..self.frontier,
                    self.parser.as_ref().is_some_and(|p| p.current(last_line)),
                ),
            }
        }

        /// The parse up to `last_line` within `budget`, to run anywhere
        /// ([`Job::run`]) and hand back ([`Self::finish`]); None when the
        /// lines up to `last_line` are current, or a job is running.
        pub(crate) fn job(
            &mut self,
            rope: &Rope,
            last_line: usize,
            budget: Duration,
        ) -> Option<Job> {
            let parser = self.parser.as_ref()?;
            if parser.current(last_line) {
                return None;
            }
            Some(Job {
                id: self.id,
                parser: self.parser.take()?,
                rope: rope.clone(),
                last_line,
                budget,
            })
        }

        /// A job's work: its tokens shown, its parser back, the edits made
        /// meanwhile given to it. Returns the lines whose tokens changed
        /// and whether the lines up to the job's last one are current.
        pub(crate) fn finish(&mut self, done: Done) -> (Range<usize>, bool) {
            if done.id != self.id {
                return (0..0, false);
            }
            let Done {
                mut parser, parsed, ..
            } = done;
            // What the job parsed holds before the first line edited since.
            let limit = self.pending.iter().map(|e| e.0).min().unwrap_or(usize::MAX);
            let mut changed: Option<Range<usize>> = None;
            for (line, tokens) in parsed {
                if line >= limit {
                    // Parsed from the text before the edit: parsed again
                    // later (never skipped as current).
                    parser.lines[line].done = false;
                    continue;
                }
                if self.tokens.len() <= line {
                    self.tokens.resize_with(line + 1, || None);
                }
                let slot = &mut self.tokens[line];
                if slot
                    .as_ref()
                    .is_some_and(|k| !k.guessed && k.tokens == tokens)
                {
                    continue;
                }
                *slot = Some(Known {
                    tokens,
                    guessed: false,
                });
                changed = Some(match changed {
                    Some(range) => range.start..line + 1,
                    None => line..line + 1,
                });
            }
            for (first, old_count, new_count) in self.pending.drain(..) {
                parser.edited(first, old_count, new_count);
            }
            self.frontier = parser.parsed;
            let last = parser.last_line;
            let current = parser.current(last);
            self.parser = Some(parser);
            (changed.unwrap_or(self.frontier..self.frontier), current)
        }

        /// How far the parse from the text's start has come: the lines
        /// before this one are current.
        pub(crate) fn frontier(&self) -> usize {
            self.frontier
        }

        /// The lines `first..=last` parsed from a fresh state, starting at
        /// byte `offset` (line `first`'s start), for lines far past the
        /// parse from the start to show colors at once; None when every
        /// line from `first + GUESS_CONTEXT` to `last` has tokens.
        pub(crate) fn guess(
            &self,
            rope: &Rope,
            first: usize,
            offset: usize,
            last: usize,
        ) -> Option<Guess> {
            let shown = (first + GUESS_CONTEXT).min(last)..last + 1;
            if shown
                .clone()
                .all(|line| self.tokens.get(line).is_some_and(Option::is_some))
            {
                return None;
            }
            Some(Guess {
                id: self.id,
                syntax: self.syntax,
                rope: rope.clone(),
                first,
                offset,
                last,
            })
        }

        /// Shows a guess's tokens on the lines the parse from the start has
        /// not reached (a guess made before an edit is dropped by the
        /// caller). Returns the lines whose tokens changed.
        pub(crate) fn guessed(&mut self, guessed: Guessed) -> Range<usize> {
            if guessed.id != self.id {
                return 0..0;
            }
            let mut changed: Option<Range<usize>> = None;
            for (i, tokens) in guessed.tokens.into_iter().enumerate() {
                let line = guessed.first + i;
                if self.tokens.len() <= line {
                    self.tokens.resize_with(line + 1, || None);
                }
                let slot = &mut self.tokens[line];
                if slot.as_ref().is_some_and(|k| !k.guessed) {
                    continue;
                }
                *slot = Some(Known {
                    tokens,
                    guessed: true,
                });
                changed = Some(match changed {
                    Some(range) => range.start..line + 1,
                    None => line..line + 1,
                });
            }
            changed.unwrap_or(0..0)
        }

        /// The tokens of `line`: byte ranges within the line (without its
        /// '\n'), sorted, not overlapping. None while the line is not
        /// highlighted yet; a line an edit has not reached yet keeps the
        /// tokens it had.
        pub(crate) fn tokens(&self, line: usize) -> Option<&[(Range<usize>, Token)]> {
            self.tokens.get(line)?.as_ref().map(|k| k.tokens.as_slice())
        }

        #[cfg(test)]
        fn parses(&mut self) -> &mut usize {
            &mut self.parser.as_mut().unwrap().parses
        }
    }

    impl Parser {
        /// Whether the lines up to `last_line` are current.
        fn current(&self, last_line: usize) -> bool {
            self.parsed > last_line || self.parsed >= self.lines.len()
        }

        /// See [`Highlighter::edited`].
        fn edited(&mut self, first: usize, old_count: usize, new_count: usize) {
            if first >= self.lines.len() {
                // Past the lines parsed: nothing known changes.
                return;
            }
            // Parsing starts again at the last kept state before the edit
            // (what comes before the span is unchanged, so is the state at
            // its start).
            if self.parsed > first {
                let mut at = first;
                while self.lines[at].start.is_none() {
                    at -= 1;
                }
                // The shorter of the two sums.
                self.offset = if self.parsed - at < at {
                    self.offset
                        - self.lines[at..self.parsed]
                            .iter()
                            .map(|l| l.len)
                            .sum::<usize>()
                } else {
                    self.lines[..at].iter().map(|l| l.len).sum()
                };
                self.parsed = at;
                self.state = None;
            }
            let start = self.lines[first].start.take();
            splice(&mut self.lines, first, old_count, new_count, Line::default);
            if self.lines.len() == first {
                self.lines.push(Line::default());
            }
            let line = &mut self.lines[first];
            line.start = start;
            line.done = false;
        }

        /// Parses on (see [`Highlighter::highlight`]), collecting each
        /// line's tokens into `out`.
        fn parse_on(
            &mut self,
            rope: &Rope,
            last_line: usize,
            budget: Duration,
            out: &mut Vec<(usize, Tokens)>,
        ) {
            let started = Instant::now();
            let set = syntaxes();
            while self.parsed < self.lines.len() && self.parsed <= last_line {
                if self.offset > rope.len_bytes() {
                    // Edits the highlighter was not told of: the text ends
                    // before the lines known.
                    log::warn!("editor: the highlighter missed an edit");
                    self.lines.truncate(self.parsed.max(1));
                    self.parsed = self.parsed.min(self.lines.len() - 1);
                    break;
                }
                let index = self.parsed;
                let newline = read_line(rope, self.offset, &mut self.line);
                let len = self.line.len();
                let content = len - usize::from(newline);
                if !newline {
                    // The syntaxes expect every line with its line break.
                    self.line.push('\n');
                }
                let mut state = match self.state.take() {
                    Some(state) => state,
                    // A line parsing starts at always has a kept state;
                    // were it missing, parsing from scratch is the best
                    // guess.
                    None => self.lines[index]
                        .start
                        .clone()
                        .unwrap_or_else(|| (ParseState::new(self.syntax), ScopeStack::new())),
                };
                let tokens = parse(&mut state, &self.line, content, set);
                #[cfg(test)]
                {
                    self.parses += 1;
                }
                out.push((index, tokens));
                let line = &mut self.lines[index];
                line.done = true;
                line.len = len;
                self.parsed = index + 1;
                self.offset += len;
                if !newline {
                    // The last line of the text.
                    self.lines.truncate(index + 1);
                    break;
                }
                let next = index + 1;
                if next == self.lines.len() {
                    self.lines.push(Line::default());
                }
                let line = &mut self.lines[next];
                if line.done && line.start.as_ref() == Some(&state) {
                    // Converged: the lines parsed before from here on
                    // hold, up to the next one an edit left unparsed,
                    // whose parse starts at the kept state before it.
                    while self.lines.get(self.parsed).is_some_and(|l| l.done) {
                        self.offset += self.lines[self.parsed].len;
                        self.parsed += 1;
                    }
                    if self.parsed < self.lines.len() {
                        while self.lines[self.parsed].start.is_none() {
                            self.parsed -= 1;
                            self.offset -= self.lines[self.parsed].len;
                        }
                    }
                } else {
                    if line.start.is_some() || next.is_multiple_of(CHECKPOINT) {
                        line.start = Some(state.clone());
                    }
                    self.state = Some(state);
                }
                if started.elapsed() >= budget {
                    break;
                }
            }
        }
    }

    impl Job {
        /// Parses (blocking: on a blocking thread where there are some).
        pub(crate) fn run(mut self) -> Done {
            let mut parsed = Vec::new();
            self.parser.last_line = self.last_line;
            self.parser
                .parse_on(&self.rope, self.last_line, self.budget, &mut parsed);
            Done {
                id: self.id,
                parser: self.parser,
                parsed,
            }
        }
    }

    impl Guess {
        /// Parses its lines (blocking).
        pub(crate) fn run(self) -> Guessed {
            let set = syntaxes();
            let mut state = (ParseState::new(self.syntax), ScopeStack::new());
            let mut line = String::new();
            let mut offset = self.offset;
            let mut tokens = Vec::new();
            for _ in self.first..=self.last {
                if offset > self.rope.len_bytes() {
                    break;
                }
                let newline = read_line(&self.rope, offset, &mut line);
                let len = line.len();
                let content = len - usize::from(newline);
                if !newline {
                    line.push('\n');
                }
                tokens.push(parse(&mut state, &line, content, set));
                offset += len;
                if !newline {
                    break;
                }
            }
            Guessed {
                id: self.id,
                first: self.first,
                tokens,
            }
        }
    }

    /// Replaces `first..first + old_count` of `items` with `new_count` new
    /// ones; past the end, cuts `items` at `first`.
    fn splice<T>(
        items: &mut Vec<T>,
        first: usize,
        old_count: usize,
        new_count: usize,
        new: impl FnMut() -> T,
    ) {
        if first >= items.len() {
            return;
        }
        let end = first + old_count;
        if end <= items.len() {
            items.splice(first..end, std::iter::repeat_with(new).take(new_count));
        } else {
            items.truncate(first);
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
    fn parse(state: &mut State, line: &str, content: usize, set: &SyntaxSet) -> Tokens {
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
    fn push(tokens: &mut Tokens, range: Range<usize>, line: &str, scopes: &ScopeStack) {
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
            assert_eq!(*highlighter.parses(), 202);

            // Line 51 is `let x50 = 50;`: a longer number.
            let at = text.find("= 50;").unwrap() + 2;
            let text = edit(&mut highlighter, &text, at..at + 2, "5000");
            *highlighter.parses() = 0;
            assert_eq!(check_fresh(&mut highlighter, "rust", &text), 51..52);
            // From the kept state before it to the next one.
            assert!(*highlighter.parses() <= CHECKPOINT);

            // A line in, a line out: the lines after shift.
            let at = text.find("    let x10").unwrap();
            let text = edit(&mut highlighter, &text, at..at, "    let s = \"新\";\n");
            *highlighter.parses() = 0;
            check_fresh(&mut highlighter, "rust", &text);
            assert!(*highlighter.parses() <= 2 * CHECKPOINT);
            let start = text.find("    let x3 ").unwrap();
            let end = text.find("    let x7 ").unwrap();
            let text = edit(&mut highlighter, &text, start..end, "");
            *highlighter.parses() = 0;
            check_fresh(&mut highlighter, "rust", &text);
            assert!(*highlighter.parses() <= 2 * CHECKPOINT);

            // Nothing changed: nothing to parse.
            *highlighter.parses() = 0;
            let (changed, current) =
                highlighter.highlight(&Rope::from_str(&text), usize::MAX, Duration::MAX);
            assert!(current && changed.is_empty());
            assert_eq!(*highlighter.parses(), 0);
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

        #[test]
        fn edits_while_a_job_parses_are_given_to_it_after() {
            let text = rust_lines(300);
            let mut highlighter = highlighted("rust", &text);
            // A block comment opens at line 3: the job parses on from there
            // while, meanwhile, line 1 changes and the comment closes.
            let at = text.find("    let x2 ").unwrap();
            let text = edit(&mut highlighter, &text, at..at, "/* ");
            let job = highlighter
                .job(&Rope::from_str(&text), usize::MAX, Duration::MAX)
                .unwrap();
            assert!(
                highlighter
                    .job(&Rope::from_str(&text), usize::MAX, Duration::MAX)
                    .is_none()
            );
            let at = text.find("    let x0 ").unwrap();
            let text = edit(&mut highlighter, &text, at..at, "// ");
            let at = text.find("/* ").unwrap();
            let text = edit(&mut highlighter, &text, at..at + 3, "");
            let (_, current) = highlighter.finish(job.run());
            assert!(!current || highlighter.frontier() < 3);
            check_fresh(&mut highlighter, "rust", &text);
        }

        #[test]
        fn a_guess_shows_lines_far_ahead_until_the_parse_reaches_them() {
            let text = rust_lines(1000);
            let rope = Rope::from_str(&text);
            let mut highlighter = Highlighter::new("rust").unwrap();
            let offset = text.match_indices('\n').nth(899).unwrap().0 + 1;
            let guess = highlighter.guess(&rope, 900, offset, 950).unwrap();
            let changed = highlighter.guessed(guess.run());
            assert_eq!(changed, 900..951);
            assert!(highlighter.tokens(899).is_none());
            assert_eq!(
                token_of(&highlighter, &text, 920, "let"),
                Some(Token::Keyword)
            );
            assert!(highlighter.guess(&rope, 900, offset, 950).is_none());
            // The parse from the start takes the lines over.
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

        #[cfg(test)]
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

        pub(crate) fn job(
            &mut self,
            _rope: &Rope,
            _last_line: usize,
            _budget: Duration,
        ) -> Option<Job> {
            None
        }

        pub(crate) fn finish(&mut self, _done: Done) -> (Range<usize>, bool) {
            (0..0, true)
        }

        pub(crate) fn frontier(&self) -> usize {
            0
        }

        pub(crate) fn guess(
            &self,
            _rope: &Rope,
            _first: usize,
            _offset: usize,
            _last: usize,
        ) -> Option<Guess> {
            None
        }

        pub(crate) fn guessed(&mut self, _guessed: Guessed) -> Range<usize> {
            0..0
        }
    }

    /// Never made without the feature.
    pub(crate) struct Job(());

    /// Never made without the feature.
    pub(crate) struct Done(());

    /// Never made without the feature.
    pub(crate) struct Guess(());

    /// Never made without the feature.
    pub(crate) struct Guessed(());

    impl Job {
        pub(crate) fn run(self) -> Done {
            Done(())
        }
    }

    impl Guess {
        pub(crate) fn run(self) -> Guessed {
            Guessed(())
        }
    }

    /// See the feature's.
    pub(crate) const GUESS_CONTEXT: usize = 40;
}
