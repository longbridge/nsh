//! TeX formulas typeset with primitives: what a `TextView` shows for a
//! formula without the app's `renderMath`.
//!
//! [`parse`] reads the TeX most documents write (KaTeX's common subset:
//! symbols and Greek letters, scripts, fractions, roots, big operators with
//! limits, `\left` / `\right`, accents, fonts, `\text`, matrices, `cases`
//! and `aligned`) into a small tree; [`render`] lays it out as views and
//! texts. Every row centers its atoms on one line (the math axis), so a
//! fraction is a grid of two equal halves around its bar and scripts are
//! raised or lowered with margins; stretchy delimiters and radicals are
//! glyphs sized from an estimate of what they enclose ([`Node::extent`]).
//! TeX this does not know fails the parse, and the view shows the TeX
//! itself, as before.

use nsh_core::prelude::*;

use crate::theme::theme;

/// A formula: atoms and the structures they make.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Node {
    Row(Vec<Node>),
    /// A symbol or a run of digits.
    Glyph {
        text: String,
        class: Class,
    },
    /// Upright text (`\text`, `\operatorname`).
    Text(String),
    Frac {
        num: Box<Node>,
        den: Box<Node>,
        bar: bool,
    },
    Sqrt {
        index: Option<Box<Node>>,
        body: Box<Node>,
    },
    Scripts {
        base: Box<Node>,
        sup: Option<Box<Node>>,
        sub: Option<Box<Node>>,
    },
    /// A big operator (`\sum`, `\int`, `\lim`).
    BigOp {
        symbol: String,
        limits: bool,
        sup: Option<Box<Node>>,
        sub: Option<Box<Node>>,
    },
    Delimited {
        open: String,
        close: String,
        body: Box<Node>,
    },
    /// A mark over or under its body.
    Accent {
        body: Box<Node>,
        mark: Mark,
    },
    /// `\overset` / `\underset`: a small row over or under the body.
    Stack {
        body: Box<Node>,
        over: Option<Box<Node>>,
        under: Option<Box<Node>>,
    },
    /// Rows of cells (`matrix`, `cases`, `aligned`).
    Array {
        rows: Vec<Vec<Node>>,
        align: ArrayAlign,
    },
    Space(f32),
    Boxed(Box<Node>),
}

/// How an atom is spaced from its neighbors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Class {
    /// A variable: italic.
    Var,
    /// Digits, upright symbols.
    Ord,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
    /// A function's name (`\sin`).
    Func,
    /// Bold, blackboard, script letters (already their own glyphs).
    Upright,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mark {
    Over(&'static str),
    Overline,
    Underline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArrayAlign {
    Center,
    Left,
    /// Right, left, right, ... (`aligned`).
    Alternate,
}

/// Why a formula could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Error(pub String);

/// Reads `tex`.
pub(super) fn parse(tex: &str) -> Result<Node, Error> {
    let mut p = Parser {
        chars: tex.chars().collect(),
        pos: 0,
        depth: 0,
    };
    let row = p.row(Stop::End)?;
    Ok(Node::Row(row))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stop {
    End,
    Brace,
    Bracket,
    Right,
    /// `\end{…}` of an environment, its cells split at `&` and `\\`.
    Env,
}

/// What ended a row inside an environment.
enum Ended {
    Cell,
    Line,
    End,
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    depth: u32,
}

const MAX_DEPTH: u32 = 64;

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn err<T>(&self, what: impl Into<String>) -> Result<T, Error> {
        Err(Error(what.into()))
    }

    /// A row up to `stop` (consumed).
    fn row(&mut self, stop: Stop) -> Result<Vec<Node>, Error> {
        let (row, _) = self.row_until(stop)?;
        Ok(row)
    }

    fn row_until(&mut self, stop: Stop) -> Result<(Vec<Node>, Ended), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.err("too deep");
        }
        let mut out: Vec<Node> = Vec::new();
        let ended = loop {
            self.skip_ws();
            let Some(c) = self.peek() else {
                if stop == Stop::End {
                    break Ended::End;
                }
                return self.err("unclosed group");
            };
            match c {
                '}' => {
                    self.pos += 1;
                    if stop == Stop::Brace {
                        break Ended::End;
                    }
                    return self.err("unexpected }");
                }
                ']' if stop == Stop::Bracket => {
                    self.pos += 1;
                    break Ended::End;
                }
                '&' if stop == Stop::Env => {
                    self.pos += 1;
                    break Ended::Cell;
                }
                '^' | '_' => {
                    self.pos += 1;
                    let arg = self.arg()?;
                    self.attach(&mut out, c == '^', arg)?;
                }
                '\'' => {
                    self.pos += 1;
                    let mut primes = String::from("′");
                    while self.peek() == Some('\'') {
                        self.pos += 1;
                        primes.push('′');
                    }
                    let prime = glyph(&primes, Class::Ord);
                    self.attach(&mut out, true, prime)?;
                }
                '\\' => {
                    let name = self.command();
                    match name.as_str() {
                        "\\" | "cr" if stop == Stop::Env => break Ended::Line,
                        "end" if stop == Stop::Env => {
                            self.braced_name()?;
                            break Ended::End;
                        }
                        "right" if stop == Stop::Right => break Ended::End,
                        "limits" | "nolimits" => {
                            let limits = name == "limits";
                            match out.last_mut() {
                                Some(Node::BigOp { limits: l, .. }) => *l = limits,
                                _ => return self.err("\\limits without an operator"),
                            }
                        }
                        _ => {
                            if let Some(node) = self.command_node(&name)? {
                                out.push(node);
                            }
                        }
                    }
                }
                _ => {
                    self.pos += 1;
                    out.push(self.char_node(c)?);
                }
            }
        };
        self.depth -= 1;
        Ok((out, ended))
    }

    /// Puts a script on the last atom of `out`.
    fn attach(&mut self, out: &mut Vec<Node>, up: bool, arg: Node) -> Result<(), Error> {
        let arg = Box::new(arg);
        match out.last_mut() {
            Some(Node::BigOp { sup, sub, .. }) => {
                let slot = if up { sup } else { sub };
                if slot.is_some() {
                    return self.err("double script");
                }
                *slot = Some(arg);
            }
            Some(Node::Scripts { sup, sub, .. })
                if (if up { sup.is_none() } else { sub.is_none() }) =>
            {
                *(if up { sup } else { sub }) = Some(arg);
            }
            _ => {
                let base = out.pop().unwrap_or(Node::Row(Vec::new()));
                let (sup, sub) = if up {
                    (Some(arg), None)
                } else {
                    (None, Some(arg))
                };
                out.push(Node::Scripts {
                    base: Box::new(base),
                    sup,
                    sub,
                });
            }
        }
        Ok(())
    }

    /// A command's name after its backslash: letters, or one other character.
    fn command(&mut self) -> String {
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        if self.pos == start
            && let Some(c) = self.peek()
        {
            self.pos += 1;
            return c.to_string();
        }
        self.chars[start..self.pos].iter().collect()
    }

    /// One argument: a group, a command or a character.
    fn arg(&mut self) -> Result<Node, Error> {
        self.skip_ws();
        match self.peek() {
            None => self.err("missing argument"),
            Some('{') => {
                self.pos += 1;
                Ok(Node::Row(self.row(Stop::Brace)?))
            }
            Some('\\') => {
                let name = self.command();
                match self.command_node(&name)? {
                    Some(node) => Ok(node),
                    None => Ok(Node::Row(Vec::new())),
                }
            }
            // One character, as TeX takes it: `\frac12` is ½.
            Some(c) if c.is_ascii_digit() => {
                self.pos += 1;
                Ok(glyph(&c.to_string(), Class::Ord))
            }
            Some(c) => {
                self.pos += 1;
                self.char_node(c)
            }
        }
    }

    /// `[…]` if one follows.
    fn optional(&mut self) -> Result<Option<Node>, Error> {
        self.skip_ws();
        if self.peek() != Some('[') {
            return Ok(None);
        }
        self.pos += 1;
        Ok(Some(Node::Row(self.row(Stop::Bracket)?)))
    }

    /// A `{…}` taken as raw text.
    fn braced_text(&mut self) -> Result<String, Error> {
        self.skip_ws();
        if self.peek() != Some('{') {
            return match self.peek() {
                Some(c) => {
                    self.pos += 1;
                    Ok(c.to_string())
                }
                None => self.err("missing text"),
            };
        }
        self.pos += 1;
        let mut depth = 1;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                }
                '\\' => {
                    // Escapes in text: `\{`, `\$`, `\ `.
                    if let Some(n) = self.peek() {
                        self.pos += 1;
                        out.push(n);
                    }
                    continue;
                }
                _ => {}
            }
            out.push(c);
        }
        self.err("unclosed text")
    }

    fn braced_name(&mut self) -> Result<String, Error> {
        let name = self.braced_text()?;
        Ok(name.trim().to_owned())
    }

    /// A delimiter after `\left`, `\right` or `\big`: `.` is none.
    fn delimiter(&mut self) -> Result<String, Error> {
        self.skip_ws();
        match self.peek() {
            None => self.err("missing delimiter"),
            Some('\\') => {
                let name = self.command();
                match delimiter_symbol(&name) {
                    Some(s) => Ok(s.to_owned()),
                    None => self.err(format!("unknown delimiter \\{name}")),
                }
            }
            Some('.') => {
                self.pos += 1;
                Ok(String::new())
            }
            Some(c) => {
                self.pos += 1;
                Ok(c.to_string())
            }
        }
    }

    fn char_node(&mut self, c: char) -> Result<Node, Error> {
        Ok(match c {
            '0'..='9' | '.' => {
                let mut s = c.to_string();
                while let Some(n) = self.peek().filter(|n| n.is_ascii_digit() || *n == '.') {
                    self.pos += 1;
                    s.push(n);
                }
                glyph(&s, Class::Ord)
            }
            'a'..='z' | 'A'..='Z' => glyph(&c.to_string(), Class::Var),
            '+' => glyph("+", Class::Bin),
            '-' => glyph("−", Class::Bin),
            '*' => glyph("∗", Class::Bin),
            '/' => glyph("/", Class::Ord),
            '=' | '<' | '>' | ':' => glyph(&c.to_string(), Class::Rel),
            ',' | ';' => glyph(&c.to_string(), Class::Punct),
            '(' | '[' => glyph(&c.to_string(), Class::Open),
            ')' | ']' => glyph(&c.to_string(), Class::Close),
            '!' | '?' => glyph(&c.to_string(), Class::Close),
            '~' => Node::Space(0.33),
            '{' => {
                return Ok(Node::Row(self.row(Stop::Brace)?));
            }
            '$' | '#' | '%' => return self.err(format!("unexpected {c}")),
            c if c.is_alphabetic() && !c.is_ascii() => {
                // Greek and other letters typed as themselves.
                glyph(&c.to_string(), Class::Var)
            }
            c => glyph(&c.to_string(), Class::Ord),
        })
    }

    /// The node of `\name`; `None` for commands that only change state.
    fn command_node(&mut self, name: &str) -> Result<Option<Node>, Error> {
        if let Some((s, class)) = symbol(name) {
            return Ok(Some(glyph(s, class)));
        }
        if let Some((s, limits)) = big_operator(name) {
            return Ok(Some(Node::BigOp {
                symbol: s.to_owned(),
                limits,
                sup: None,
                sub: None,
            }));
        }
        if FUNCTIONS.contains(&name) {
            return Ok(Some(glyph(name, Class::Func)));
        }
        if LIMIT_FUNCTIONS.contains(&name) {
            let symbol = match name {
                "liminf" => "lim inf",
                "limsup" => "lim sup",
                n => n,
            };
            return Ok(Some(Node::BigOp {
                symbol: symbol.to_owned(),
                limits: true,
                sup: None,
                sub: None,
            }));
        }
        if let Some(c) = name.chars().next().filter(|_| name.chars().count() == 1) {
            if let Some(space) = match c {
                ',' => Some(3.0 / 18.0),
                ':' | '>' => Some(4.0 / 18.0),
                ';' => Some(5.0 / 18.0),
                ' ' => Some(0.33),
                '!' => Some(0.0),
                _ => None,
            } {
                return Ok(Some(Node::Space(space)));
            }
            if let Some(s) = match c {
                '{' => Some(("{", Class::Open)),
                '}' => Some(("}", Class::Close)),
                '|' => Some(("‖", Class::Ord)),
                '%' | '$' | '#' | '&' | '_' => None,
                _ => return self.err(format!("unknown command \\{name}")),
            } {
                return Ok(Some(glyph(s.0, s.1)));
            }
            return Ok(Some(glyph(name, Class::Ord)));
        }
        let node = match name {
            "quad" => Node::Space(1.0),
            "qquad" => Node::Space(2.0),
            "enspace" => Node::Space(0.5),
            "thinspace" => Node::Space(3.0 / 18.0),
            "medspace" => Node::Space(4.0 / 18.0),
            "thickspace" => Node::Space(5.0 / 18.0),
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let num = self.arg()?;
                let den = self.arg()?;
                Node::Frac {
                    num: Box::new(num),
                    den: Box::new(den),
                    bar: true,
                }
            }
            "binom" | "dbinom" | "tbinom" => {
                let num = self.arg()?;
                let den = self.arg()?;
                Node::Delimited {
                    open: "(".into(),
                    close: ")".into(),
                    body: Box::new(Node::Frac {
                        num: Box::new(num),
                        den: Box::new(den),
                        bar: false,
                    }),
                }
            }
            "sqrt" => {
                let index = self.optional()?.map(Box::new);
                let body = Box::new(self.arg()?);
                Node::Sqrt { index, body }
            }
            "left" => {
                let open = self.delimiter()?;
                let body = Node::Row(self.row(Stop::Right)?);
                let close = self.delimiter()?;
                Node::Delimited {
                    open,
                    close,
                    body: Box::new(body),
                }
            }
            "middle" => glyph(&self.delimiter()?, Class::Rel),
            "big" | "Big" | "bigg" | "Bigg" | "bigl" | "Bigl" | "biggl" | "Biggl" | "bigr"
            | "Bigr" | "biggr" | "Biggr" | "bigm" | "Bigm" => {
                let d = self.delimiter()?;
                let class = if name.ends_with('l') {
                    Class::Open
                } else if name.ends_with('r') {
                    Class::Close
                } else {
                    Class::Ord
                };
                let scale = match name.trim_end_matches(['l', 'r', 'm']) {
                    "big" => 1.2,
                    "Big" => 1.8,
                    "bigg" => 2.4,
                    _ => 3.0,
                };
                // An empty body of the given height.
                Node::Delimited {
                    open: if class == Class::Close {
                        String::new()
                    } else {
                        d.clone()
                    },
                    close: if class == Class::Close {
                        d
                    } else {
                        String::new()
                    },
                    body: Box::new(Node::Space(-scale)),
                }
            }
            "text" | "textrm" | "textnormal" | "mbox" | "textup" | "textsf" | "texttt" => {
                Node::Text(self.braced_text()?)
            }
            "textbf" | "textit" | "emph" => Node::Text(self.braced_text()?),
            "operatorname" => {
                let star = self.peek() == Some('*');
                if star {
                    self.pos += 1;
                }
                let name = self.braced_text()?;
                if star {
                    Node::BigOp {
                        symbol: name,
                        limits: true,
                        sup: None,
                        sub: None,
                    }
                } else {
                    glyph(&name, Class::Func)
                }
            }
            "mathrm" | "mathup" | "rm" => restyle(self.arg()?, |c| Some(c.to_string()), Class::Ord),
            "mathit" | "it" => self.arg()?,
            "mathbf" | "bf" | "boldsymbol" | "bm" => restyle(self.arg()?, bold, Class::Upright),
            "mathbb" => restyle(self.arg()?, double_struck, Class::Upright),
            "mathcal" | "mathscr" => restyle(self.arg()?, script, Class::Upright),
            "mathfrak" => restyle(self.arg()?, fraktur, Class::Upright),
            "mathsf" | "mathtt" => restyle(self.arg()?, |c| Some(c.to_string()), Class::Ord),
            "displaystyle" | "textstyle" | "scriptstyle" | "scriptscriptstyle" | "nonumber"
            | "notag" => return Ok(None),
            "color" => {
                self.braced_text()?;
                return Ok(None);
            }
            "textcolor" | "colorbox" => {
                self.braced_text()?;
                self.arg()?
            }
            "boxed" | "fbox" => Node::Boxed(Box::new(self.arg()?)),
            "not" => {
                let next = self.arg()?;
                match next {
                    Node::Glyph { text, class } => glyph(&negate(&text), class),
                    other => other,
                }
            }
            "hat" | "widehat" => accent(self.arg()?, "\u{302}", "ˆ"),
            "tilde" | "widetilde" => accent(self.arg()?, "\u{303}", "˜"),
            "bar" => accent(self.arg()?, "\u{304}", ""),
            "vec" => accent(self.arg()?, "\u{20D7}", "→"),
            "overrightarrow" => accent(self.arg()?, "", "→"),
            "overleftarrow" => accent(self.arg()?, "", "←"),
            "dot" => accent(self.arg()?, "\u{307}", "˙"),
            "ddot" => accent(self.arg()?, "\u{308}", "¨"),
            "acute" => accent(self.arg()?, "\u{301}", "´"),
            "grave" => accent(self.arg()?, "\u{300}", "`"),
            "breve" => accent(self.arg()?, "\u{306}", "˘"),
            "check" => accent(self.arg()?, "\u{30C}", "ˇ"),
            "overline" => Node::Accent {
                body: Box::new(self.arg()?),
                mark: Mark::Overline,
            },
            "underline" => Node::Accent {
                body: Box::new(self.arg()?),
                mark: Mark::Underline,
            },
            "overbrace" => Node::Accent {
                body: Box::new(self.arg()?),
                mark: Mark::Over("⏞"),
            },
            "underbrace" => Node::Stack {
                body: Box::new(self.arg()?),
                over: None,
                under: Some(Box::new(glyph("⏟", Class::Ord))),
            },
            "overset" | "stackrel" => {
                let over = self.arg()?;
                let body = self.arg()?;
                Node::Stack {
                    body: Box::new(body),
                    over: Some(Box::new(over)),
                    under: None,
                }
            }
            "underset" => {
                let under = self.arg()?;
                let body = self.arg()?;
                Node::Stack {
                    body: Box::new(body),
                    over: None,
                    under: Some(Box::new(under)),
                }
            }
            "begin" => {
                let env = self.braced_name()?;
                self.environment(&env)?
            }
            _ => return self.err(format!("unknown command \\{name}")),
        };
        Ok(Some(node))
    }

    fn environment(&mut self, env: &str) -> Result<Node, Error> {
        let (open, close, align) = match env {
            "matrix" | "smallmatrix" => ("", "", ArrayAlign::Center),
            "pmatrix" => ("(", ")", ArrayAlign::Center),
            "bmatrix" => ("[", "]", ArrayAlign::Center),
            "Bmatrix" => ("{", "}", ArrayAlign::Center),
            "vmatrix" => ("|", "|", ArrayAlign::Center),
            "Vmatrix" => ("‖", "‖", ArrayAlign::Center),
            "cases" | "dcases" => ("{", "", ArrayAlign::Left),
            "rcases" => ("", "}", ArrayAlign::Left),
            "aligned" | "align" | "align*" | "alignat" | "alignat*" | "split" | "eqnarray"
            | "eqnarray*" => ("", "", ArrayAlign::Alternate),
            "gathered" | "gather" | "gather*" | "equation" | "equation*" => {
                ("", "", ArrayAlign::Center)
            }
            "array" => {
                // The column spec: alignment by its first letter.
                let spec = self.braced_text()?;
                let align = match spec.trim().chars().find(|c| matches!(c, 'l' | 'c' | 'r')) {
                    Some('l') => ArrayAlign::Left,
                    _ => ArrayAlign::Center,
                };
                ("", "", align)
            }
            _ => return self.err(format!("unknown environment {env}")),
        };
        if env.starts_with("alignat") {
            self.braced_text()?;
        }
        let mut rows = Vec::new();
        let mut cells = Vec::new();
        loop {
            let (cell, ended) = self.row_until(Stop::Env)?;
            cells.push(Node::Row(cell));
            match ended {
                Ended::Cell => {}
                Ended::Line => rows.push(std::mem::take(&mut cells)),
                Ended::End => {
                    // A trailing `\\` leaves one empty cell.
                    if !(cells.len() == 1 && matches!(&cells[0], Node::Row(r) if r.is_empty()))
                        || rows.is_empty()
                    {
                        rows.push(cells);
                    }
                    break;
                }
            }
        }
        let array = Node::Array { rows, align };
        Ok(if open.is_empty() && close.is_empty() {
            array
        } else {
            Node::Delimited {
                open: open.into(),
                close: close.into(),
                body: Box::new(array),
            }
        })
    }
}

fn glyph(text: &str, class: Class) -> Node {
    Node::Glyph {
        text: text.to_owned(),
        class,
    }
}

/// `node`'s letters in another alphabet (`map`), upright.
fn restyle(node: Node, map: impl Fn(char) -> Option<String> + Copy, class: Class) -> Node {
    match node {
        Node::Glyph {
            text,
            class: Class::Var | Class::Ord,
        } => {
            let text = text
                .chars()
                .map(|ch| map(ch).unwrap_or_else(|| ch.to_string()))
                .collect::<String>();
            glyph(&text, class)
        }
        Node::Row(nodes) => Node::Row(nodes.into_iter().map(|n| restyle(n, map, class)).collect()),
        other => other,
    }
}

/// A letter or digit in the Unicode math alphabet starting at `upper` (A),
/// `lower` (a) and `digit` (0), with `exceptions` for the letters the
/// Letterlike Symbols block had first.
fn alphabet(
    c: char,
    upper: u32,
    lower: Option<u32>,
    digit: Option<u32>,
    exceptions: &[(char, char)],
) -> Option<String> {
    if let Some((_, e)) = exceptions.iter().find(|(k, _)| *k == c) {
        return Some(e.to_string());
    }
    let code = match c {
        'A'..='Z' => upper + (c as u32 - 'A' as u32),
        'a'..='z' => lower? + (c as u32 - 'a' as u32),
        '0'..='9' => digit? + (c as u32 - '0' as u32),
        _ => return None,
    };
    char::from_u32(code).map(String::from)
}

fn bold(c: char) -> Option<String> {
    alphabet(c, 0x1D400, Some(0x1D41A), Some(0x1D7CE), &[])
}

fn double_struck(c: char) -> Option<String> {
    alphabet(
        c,
        0x1D538,
        Some(0x1D552),
        Some(0x1D7D8),
        &[
            ('C', 'ℂ'),
            ('H', 'ℍ'),
            ('N', 'ℕ'),
            ('P', 'ℙ'),
            ('Q', 'ℚ'),
            ('R', 'ℝ'),
            ('Z', 'ℤ'),
        ],
    )
}

fn script(c: char) -> Option<String> {
    alphabet(
        c,
        0x1D49C,
        Some(0x1D4B6),
        None,
        &[
            ('B', 'ℬ'),
            ('E', 'ℰ'),
            ('F', 'ℱ'),
            ('H', 'ℋ'),
            ('I', 'ℐ'),
            ('L', 'ℒ'),
            ('M', 'ℳ'),
            ('R', 'ℛ'),
            ('e', 'ℯ'),
            ('g', 'ℊ'),
            ('o', 'ℴ'),
        ],
    )
}

fn fraktur(c: char) -> Option<String> {
    alphabet(
        c,
        0x1D504,
        Some(0x1D51E),
        None,
        &[('C', 'ℭ'), ('H', 'ℌ'), ('I', 'ℑ'), ('R', 'ℜ'), ('Z', 'ℨ')],
    )
}

/// `\not`: the negated relation, else a combining slash.
fn negate(text: &str) -> String {
    match text {
        "=" => "≠".into(),
        "∈" => "∉".into(),
        "⊂" => "⊄".into(),
        "⊆" => "⊈".into(),
        "≡" => "≢".into(),
        "<" => "≮".into(),
        ">" => "≯".into(),
        "≤" => "≰".into(),
        "≥" => "≱".into(),
        "∼" => "≁".into(),
        "≈" => "≉".into(),
        t => format!("{t}\u{338}"),
    }
}

/// An accent: combined with a single letter, else a mark over the body.
fn accent(body: Node, combining: &str, wide: &'static str) -> Node {
    let single = match &body {
        Node::Glyph { text, class } if text.chars().count() == 1 => Some((text.clone(), *class)),
        Node::Row(nodes) if nodes.len() == 1 => match &nodes[0] {
            Node::Glyph { text, class } if text.chars().count() == 1 => {
                Some((text.clone(), *class))
            }
            _ => None,
        },
        _ => None,
    };
    match single {
        Some((text, class)) if !combining.is_empty() => glyph(&format!("{text}{combining}"), class),
        _ if wide.is_empty() => Node::Accent {
            body: Box::new(body),
            mark: Mark::Overline,
        },
        _ => Node::Accent {
            body: Box::new(body),
            mark: Mark::Over(wide),
        },
    }
}

const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
    "coth", "log", "ln", "lg", "exp", "deg", "dim", "ker", "arg", "hom", "Pr", "mod", "bmod",
];

const LIMIT_FUNCTIONS: &[&str] = &[
    "lim", "liminf", "limsup", "max", "min", "sup", "inf", "det", "gcd", "argmax", "argmin",
];

/// A big operator's glyph, and whether its scripts are limits in display.
fn big_operator(name: &str) -> Option<(&'static str, bool)> {
    Some(match name {
        "sum" => ("∑", true),
        "prod" => ("∏", true),
        "coprod" => ("∐", true),
        "bigcup" => ("⋃", true),
        "bigcap" => ("⋂", true),
        "bigvee" => ("⋁", true),
        "bigwedge" => ("⋀", true),
        "bigoplus" => ("⨁", true),
        "bigotimes" => ("⨂", true),
        "bigodot" => ("⨀", true),
        "biguplus" => ("⨄", true),
        "bigsqcup" => ("⨆", true),
        "int" => ("∫", false),
        "iint" => ("∬", false),
        "iiint" => ("∭", false),
        "oint" => ("∮", false),
        "oiint" => ("∯", false),
        _ => return None,
    })
}

fn delimiter_symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "{" | "lbrace" => "{",
        "}" | "rbrace" => "}",
        "|" | "Vert" | "lVert" | "rVert" => "‖",
        "vert" | "lvert" | "rvert" | "mid" => "|",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "lbrack" => "[",
        "rbrack" => "]",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "Uparrow" => "⇑",
        "Downarrow" => "⇓",
        "backslash" => "\\",
        "/" => "/",
        _ => return None,
    })
}

/// A symbol command's glyph and class.
fn symbol(name: &str) -> Option<(&'static str, Class)> {
    use Class::*;
    Some(match name {
        // Greek: lower case italic, upper case upright.
        "alpha" => ("α", Var),
        "beta" => ("β", Var),
        "gamma" => ("γ", Var),
        "delta" => ("δ", Var),
        "epsilon" => ("ϵ", Var),
        "varepsilon" => ("ε", Var),
        "zeta" => ("ζ", Var),
        "eta" => ("η", Var),
        "theta" => ("θ", Var),
        "vartheta" => ("ϑ", Var),
        "iota" => ("ι", Var),
        "kappa" => ("κ", Var),
        "lambda" => ("λ", Var),
        "mu" => ("μ", Var),
        "nu" => ("ν", Var),
        "xi" => ("ξ", Var),
        "omicron" => ("ο", Var),
        "pi" => ("π", Var),
        "varpi" => ("ϖ", Var),
        "rho" => ("ρ", Var),
        "varrho" => ("ϱ", Var),
        "sigma" => ("σ", Var),
        "varsigma" => ("ς", Var),
        "tau" => ("τ", Var),
        "upsilon" => ("υ", Var),
        "phi" => ("ϕ", Var),
        "varphi" => ("φ", Var),
        "chi" => ("χ", Var),
        "psi" => ("ψ", Var),
        "omega" => ("ω", Var),
        "Gamma" => ("Γ", Ord),
        "Delta" => ("Δ", Ord),
        "Theta" => ("Θ", Ord),
        "Lambda" => ("Λ", Ord),
        "Xi" => ("Ξ", Ord),
        "Pi" => ("Π", Ord),
        "Sigma" => ("Σ", Ord),
        "Upsilon" => ("Υ", Ord),
        "Phi" => ("Φ", Ord),
        "Psi" => ("Ψ", Ord),
        "Omega" => ("Ω", Ord),
        // Binary operators.
        "pm" => ("±", Bin),
        "mp" => ("∓", Bin),
        "times" => ("×", Bin),
        "div" => ("÷", Bin),
        "cdot" => ("⋅", Bin),
        "ast" => ("∗", Bin),
        "star" => ("⋆", Bin),
        "circ" => ("∘", Bin),
        "bullet" => ("∙", Bin),
        "oplus" => ("⊕", Bin),
        "ominus" => ("⊖", Bin),
        "otimes" => ("⊗", Bin),
        "oslash" => ("⊘", Bin),
        "odot" => ("⊙", Bin),
        "cup" => ("∪", Bin),
        "cap" => ("∩", Bin),
        "setminus" => ("∖", Bin),
        "wedge" | "land" => ("∧", Bin),
        "vee" | "lor" => ("∨", Bin),
        "sqcup" => ("⊔", Bin),
        "sqcap" => ("⊓", Bin),
        "uplus" => ("⊎", Bin),
        "dagger" => ("†", Bin),
        "ddagger" => ("‡", Bin),
        "wr" => ("≀", Bin),
        "amalg" => ("⨿", Bin),
        // Relations.
        "leq" | "le" => ("≤", Rel),
        "geq" | "ge" => ("≥", Rel),
        "leqslant" => ("⩽", Rel),
        "geqslant" => ("⩾", Rel),
        "neq" | "ne" => ("≠", Rel),
        "equiv" => ("≡", Rel),
        "approx" => ("≈", Rel),
        "cong" => ("≅", Rel),
        "sim" => ("∼", Rel),
        "simeq" => ("≃", Rel),
        "propto" => ("∝", Rel),
        "ll" => ("≪", Rel),
        "gg" => ("≫", Rel),
        "prec" => ("≺", Rel),
        "succ" => ("≻", Rel),
        "preceq" => ("⪯", Rel),
        "succeq" => ("⪰", Rel),
        "in" => ("∈", Rel),
        "notin" => ("∉", Rel),
        "ni" | "owns" => ("∋", Rel),
        "subset" => ("⊂", Rel),
        "supset" => ("⊃", Rel),
        "subseteq" => ("⊆", Rel),
        "supseteq" => ("⊇", Rel),
        "subsetneq" => ("⊊", Rel),
        "supsetneq" => ("⊋", Rel),
        "sqsubseteq" => ("⊑", Rel),
        "sqsupseteq" => ("⊒", Rel),
        "perp" => ("⊥", Rel),
        "parallel" => ("∥", Rel),
        "mid" => ("∣", Rel),
        "nmid" => ("∤", Rel),
        "models" => ("⊨", Rel),
        "vdash" => ("⊢", Rel),
        "dashv" => ("⊣", Rel),
        "asymp" => ("≍", Rel),
        "doteq" => ("≐", Rel),
        "coloneqq" => ("≔", Rel),
        "triangleq" => ("≜", Rel),
        // Arrows.
        "to" | "rightarrow" => ("→", Rel),
        "gets" | "leftarrow" => ("←", Rel),
        "leftrightarrow" => ("↔", Rel),
        "Rightarrow" | "implies" => ("⇒", Rel),
        "Leftarrow" | "impliedby" => ("⇐", Rel),
        "Leftrightarrow" | "iff" => ("⇔", Rel),
        "longrightarrow" => ("⟶", Rel),
        "longleftarrow" => ("⟵", Rel),
        "longleftrightarrow" => ("⟷", Rel),
        "Longrightarrow" => ("⟹", Rel),
        "Longleftarrow" => ("⟸", Rel),
        "Longleftrightarrow" => ("⟺", Rel),
        "mapsto" => ("↦", Rel),
        "longmapsto" => ("⟼", Rel),
        "hookrightarrow" => ("↪", Rel),
        "hookleftarrow" => ("↩", Rel),
        "uparrow" => ("↑", Rel),
        "downarrow" => ("↓", Rel),
        "updownarrow" => ("↕", Rel),
        "Uparrow" => ("⇑", Rel),
        "Downarrow" => ("⇓", Rel),
        "nearrow" => ("↗", Rel),
        "searrow" => ("↘", Rel),
        "nwarrow" => ("↖", Rel),
        "swarrow" => ("↙", Rel),
        "rightleftharpoons" => ("⇌", Rel),
        "leftharpoonup" => ("↼", Rel),
        "rightharpoonup" => ("⇀", Rel),
        // Delimiters as atoms.
        "langle" => ("⟨", Open),
        "rangle" => ("⟩", Close),
        "lceil" => ("⌈", Open),
        "rceil" => ("⌉", Close),
        "lfloor" => ("⌊", Open),
        "rfloor" => ("⌋", Close),
        "lbrace" => ("{", Open),
        "rbrace" => ("}", Close),
        "lvert" => ("|", Open),
        "rvert" => ("|", Close),
        "lVert" => ("‖", Open),
        "rVert" => ("‖", Close),
        "vert" => ("|", Ord),
        "Vert" => ("‖", Ord),
        "backslash" => ("\\", Ord),
        // Other symbols.
        "infty" => ("∞", Ord),
        "partial" => ("∂", Ord),
        "nabla" => ("∇", Ord),
        "forall" => ("∀", Ord),
        "exists" => ("∃", Ord),
        "nexists" => ("∄", Ord),
        "emptyset" | "varnothing" => ("∅", Ord),
        "neg" | "lnot" => ("¬", Ord),
        "hbar" => ("ℏ", Ord),
        "ell" => ("ℓ", Ord),
        "Re" => ("ℜ", Ord),
        "Im" => ("ℑ", Ord),
        "wp" => ("℘", Ord),
        "aleph" => ("ℵ", Ord),
        "imath" => ("ı", Var),
        "jmath" => ("ȷ", Var),
        "angle" => ("∠", Ord),
        "triangle" => ("△", Ord),
        "square" | "Box" => ("□", Ord),
        "diamond" => ("⋄", Bin),
        "top" => ("⊤", Ord),
        "bot" => ("⊥", Ord),
        "prime" => ("′", Ord),
        "degree" => ("°", Ord),
        "checkmark" => ("✓", Ord),
        "therefore" => ("∴", Rel),
        "because" => ("∵", Rel),
        "cdots" => ("⋯", Ord),
        "ldots" | "dots" | "dotsc" | "dotsb" | "dotso" => ("…", Ord),
        "vdots" => ("⋮", Ord),
        "ddots" => ("⋱", Ord),
        "colon" => (":", Punct),
        "S" => ("§", Ord),
        "P" => ("¶", Ord),
        "clubsuit" => ("♣", Ord),
        "diamondsuit" => ("♢", Ord),
        "heartsuit" => ("♡", Ord),
        "spadesuit" => ("♠", Ord),
        "flat" => ("♭", Ord),
        "natural" => ("♮", Ord),
        "sharp" => ("♯", Ord),
        _ => return None,
    })
}

// --- Layout ---------------------------------------------------------------------

/// The fonts' size ratio of a script, and of a script's script.
const SCRIPT: f32 = 0.7;
const SCRIPT_SCRIPT: f32 = 0.5;
/// A glyph's line, in ems: the box every atom is centered in.
const LINE: f32 = 1.2;

/// Where a formula is typeset.
#[derive(Clone, Copy)]
struct Style {
    /// Font size in points.
    size: f32,
    /// Display style: limits over and under operators, full-size fractions.
    display: bool,
    /// Script depth: 0 the formula's text, 1 its scripts, 2 deeper.
    level: u8,
}

impl Style {
    /// The style of scripts.
    fn script(self, base: f32) -> Self {
        let level = (self.level + 1).min(2);
        Style {
            size: base * if level == 1 { SCRIPT } else { SCRIPT_SCRIPT },
            display: false,
            level,
        }
    }

    /// The style of a fraction's parts.
    fn fraction(self, base: f32) -> Self {
        if self.display {
            Style {
                display: false,
                ..self
            }
        } else {
            self.script(base)
        }
    }
}

impl Node {
    /// About how far the node reaches above and below the axis, in points.
    fn extent(&self, st: Style, base: f32) -> (f32, f32) {
        let half = st.size * LINE / 2.0;
        match self {
            Node::Row(nodes) => nodes.iter().fold((half * 0.0, 0.0), |(a, d), n| {
                let (na, nd) = n.extent(st, base);
                (a.max(na), d.max(nd))
            }),
            Node::Glyph { .. } | Node::Text(_) => (half, half),
            Node::Space(em) if *em < 0.0 => (half * -em, half * -em),
            Node::Space(_) => (0.0, 0.0),
            Node::Frac { num, den, .. } => {
                let fs = st.fraction(base);
                let (na, nd) = num.extent(fs, base);
                let (da, dd) = den.extent(fs, base);
                let h = (na + nd).max(da + dd) + st.size * 0.15;
                (h, h)
            }
            Node::Sqrt { body, .. } => {
                let (a, d) = body.extent(st, base);
                (a + st.size * 0.2, d)
            }
            Node::Scripts { base: b, sup, sub } => {
                let (a, d) = b.extent(st, base);
                let ss = st.script(base);
                let up = sup.as_ref().map_or(0.0, |s| {
                    let (sa, _) = s.extent(ss, base);
                    sup_raise(a, st) + sa
                });
                let down = sub.as_ref().map_or(0.0, |s| {
                    let (_, sd) = s.extent(ss, base);
                    sub_drop(d, st) + sd
                });
                (a.max(up), d.max(down))
            }
            Node::BigOp {
                sup,
                sub,
                limits,
                symbol,
            } => {
                let big = symbol.chars().count() == 1;
                let scale = if big && st.display { 1.6 } else { 1.0 };
                let h = half * scale;
                if *limits && st.display {
                    let ss = st.script(base);
                    let up = sup.as_ref().map_or(0.0, |s| {
                        let (a, d) = s.extent(ss, base);
                        a + d
                    });
                    let down = sub.as_ref().map_or(0.0, |s| {
                        let (a, d) = s.extent(ss, base);
                        a + d
                    });
                    let m = up.max(down);
                    (h + m, h + m)
                } else {
                    (h, h)
                }
            }
            Node::Delimited { body, .. } => {
                let (a, d) = body.extent(st, base);
                let m = a.max(d).max(half);
                (m, m)
            }
            Node::Accent { body, .. } => {
                let (a, d) = body.extent(st, base);
                (a + st.size * 0.3, d)
            }
            Node::Stack { body, over, under } => {
                let (a, d) = body.extent(st, base);
                let ss = st.script(base);
                let up = over.as_ref().map_or(0.0, |o| {
                    let (oa, od) = o.extent(ss, base);
                    oa + od
                });
                let down = under.as_ref().map_or(0.0, |u| {
                    let (ua, ud) = u.extent(ss, base);
                    ua + ud
                });
                (a + up, d + down)
            }
            Node::Array { rows, .. } => {
                let total: f32 = rows
                    .iter()
                    .map(|r| {
                        let (a, d) = r.iter().fold((half, half), |(a, d), c| {
                            let (ca, cd) = c.extent(st, base);
                            (a.max(ca), d.max(cd))
                        });
                        a + d + st.size * ROW_GAP
                    })
                    .sum();
                (total / 2.0, total / 2.0)
            }
            Node::Boxed(body) => {
                let (a, d) = body.extent(st, base);
                (a + st.size * 0.25, d + st.size * 0.25)
            }
        }
    }
}

const ROW_GAP: f32 = 0.3;

/// How far a superscript's center is above the axis.
fn sup_raise(base_ascent: f32, st: Style) -> f32 {
    (st.size * 0.42).max(base_ascent - st.size * 0.2)
}

/// How far a subscript's center is below the axis.
fn sub_drop(base_descent: f32, st: Style) -> f32 {
    (st.size * 0.3).max(base_descent - st.size * 0.25)
}

/// The formula `node`, typeset at `size` points; `display` for a block of
/// its own.
pub(super) fn render(node: &Node, size: f32, display: bool) -> AnyElement {
    let st = Style {
        size,
        display,
        level: 0,
    };
    Layout { base: size }.node(node, st)
}

struct Layout {
    /// The formula's own font size.
    base: f32,
}

fn row_view() -> View {
    view().flex_row().items_center().flex_none()
}

fn math_text(s: &str, size: f32, italic: bool) -> Text {
    text(s.to_owned())
        .font_family("serif")
        .text_size(px(size))
        .line_height(px(size * LINE))
        .when(italic, |t| t.italic())
}

fn rule_color() -> AdaptiveColor {
    theme().foreground
}

impl Layout {
    fn node(&self, node: &Node, st: Style) -> AnyElement {
        match node {
            Node::Row(nodes) => self.row(nodes, st).into_element(),
            Node::Glyph { text: s, class } => self.glyph(s, *class, st, None).into_element(),
            Node::Text(s) => math_text(s, st.size, false).into_element(),
            Node::Space(em) => view().w(px(st.size * em.max(0.0))).into_element(),
            Node::Frac { num, den, bar } => self.frac(num, den, *bar, st),
            Node::Sqrt { index, body } => self.sqrt(index.as_deref(), body, st),
            Node::Scripts { base, sup, sub } => {
                let (a, d) = base.extent(st, self.base);
                let base_el = self.node(base, st);
                self.scripts(base_el, (a, d), sup.as_deref(), sub.as_deref(), st)
            }
            Node::BigOp {
                symbol,
                limits,
                sup,
                sub,
            } => self.big_op(symbol, *limits, sup.as_deref(), sub.as_deref(), st),
            Node::Delimited { open, close, body } => self.delimited(open, close, body, st),
            Node::Accent { body, mark } => self.accent(body, *mark, st),
            Node::Stack { body, over, under } => {
                let ss = st.script(self.base);
                let col = view().flex_col().items_center().flex_none();
                let col = match over {
                    Some(o) => col.child(self.node(o, ss)),
                    None => col,
                };
                let col = col.child(self.node(body, st));
                match under {
                    Some(u) => col.child(self.node(u, ss)),
                    None => col,
                }
                .into_element()
            }
            Node::Array { rows, align } => self.array(rows, *align, st),
            Node::Boxed(body) => row_view()
                .px(px(st.size * 0.25))
                .py(px(st.size * 0.15))
                .border_1()
                .border_color(rule_color)
                .child(self.node(body, st))
                .into_element(),
        }
    }

    /// A row with TeX's spacing between its atoms (none in scripts).
    fn row(&self, nodes: &[Node], st: Style) -> View {
        let mut el = row_view();
        let mut prev: Option<Class> = None;
        for (i, n) in nodes.iter().enumerate() {
            let class = match n {
                Node::Glyph { class, .. } => Some(*class),
                Node::Scripts { base, .. } => match base.as_ref() {
                    Node::Glyph { class, .. } => Some(*class),
                    _ => Some(Class::Ord),
                },
                Node::Space(_) => None,
                _ => Some(Class::Ord),
            };
            // A binary operator with nothing on its left is a sign.
            let class = match class {
                Some(Class::Bin)
                    if matches!(
                        prev,
                        None | Some(Class::Bin | Class::Rel | Class::Open | Class::Punct)
                    ) =>
                {
                    Some(Class::Ord)
                }
                c => c,
            };
            let next_is_atom = nodes.get(i + 1).is_some();
            let space = match class {
                _ if st.level > 0 => 0.0,
                Some(Class::Bin) if next_is_atom => 4.0 / 18.0,
                Some(Class::Rel) => 5.0 / 18.0,
                _ => 0.0,
            };
            let before = match (prev, class) {
                (Some(Class::Func), Some(Class::Var | Class::Ord | Class::Upright)) => 3.0 / 18.0,
                _ => 0.0,
            };
            let left = if prev.is_some() { space + before } else { 0.0 };
            let right = match class {
                Some(Class::Punct) if st.level == 0 => 3.0 / 18.0,
                _ if next_is_atom => space,
                _ => 0.0,
            };
            let child = row_view()
                .when(left > 0.0, |v| v.ml(px(st.size * left)))
                .when(right > 0.0, |v| v.mr(px(st.size * right)))
                .child(self.node(n, st));
            el = el.child(child);
            if class.is_some() {
                prev = class;
            }
        }
        el
    }

    fn glyph(&self, s: &str, class: Class, st: Style, size: Option<f32>) -> Text {
        let italic = class == Class::Var && s.chars().all(|c| c.is_alphabetic());
        let t = math_text(s, size.unwrap_or(st.size), italic);
        // An italic letter leans past its box: a little room after it.
        if italic { t.pr(px(st.size * 0.05)) } else { t }
    }

    fn frac(&self, num: &Node, den: &Node, bar: bool, st: Style) -> AnyElement {
        let fs = st.fraction(self.base);
        let thickness = (st.size * 0.06).max(1.0);
        let cell = |n: &Node| {
            view()
                .flex_row()
                .justify_center()
                .items_center()
                .child(self.node(n, fs))
        };
        // Two equal halves around the bar: the bar is on the axis.
        view()
            .grid()
            .grid_template_rows("1fr auto 1fr")
            .mx(px(st.size * 0.12))
            .flex_none()
            .child(cell(num).justify_center().items_end().pb(px(st.size * 0.1)))
            .child(
                view()
                    .h(px(thickness))
                    .min_w(px(st.size * 0.6))
                    .when(bar, |v| v.bg(rule_color)),
            )
            .child(cell(den).items_start().pt(px(st.size * 0.1)))
            .into_element()
    }

    fn sqrt(&self, index: Option<&Node>, body: &Node, st: Style) -> AnyElement {
        let (a, d) = body.extent(st, self.base);
        let height = a + d + st.size * 0.2;
        let scale = (height / (st.size * LINE)).max(1.0);
        let radical = math_text("√", st.size * scale, false)
            .line_height(px(height))
            .flex_none();
        let thickness = (st.size * 0.06).max(1.0);
        let content = row_view()
            .border_t(px(thickness))
            .border_color(rule_color)
            .pt(px(st.size * 0.1))
            .pl(px(st.size * 0.05))
            .child(self.node(body, st));
        let el = view().flex_row().items_end().flex_none();
        let el = match index {
            Some(i) => {
                let ss = st.script(self.base).script(self.base);
                el.child(
                    view()
                        .mb(px(height * 0.45))
                        .mr(px(-st.size * 0.3))
                        .child(self.node(i, ss)),
                )
            }
            None => el,
        };
        el.child(radical).child(content).into_element()
    }

    /// `base_el` with its scripts, centered on the axis by margins.
    fn scripts(
        &self,
        base_el: AnyElement,
        (a, d): (f32, f32),
        sup: Option<&Node>,
        sub: Option<&Node>,
        st: Style,
    ) -> AnyElement {
        let ss = st.script(self.base);
        let raise = sup_raise(a, st);
        let drop = sub_drop(d, st);
        let column = match (sup, sub) {
            (Some(sup), None) => view().mb(px(raise * 2.0)).child(self.node(sup, ss)),
            (None, Some(sub)) => view().mt(px(drop * 2.0)).child(self.node(sub, ss)),
            (Some(sup), Some(sub)) => {
                // The column's middle on the axis: equal halves.
                let (_, sd) = sup.extent(ss, self.base);
                let (sba, _) = sub.extent(ss, self.base);
                let up = (raise - sd).max(st.size * 0.05);
                let down = (drop - sba).max(st.size * 0.05);
                let half = up.max(down);
                view()
                    .grid()
                    .grid_template_rows("1fr auto 1fr")
                    .child(view().flex_col().justify_end().child(self.node(sup, ss)))
                    .child(view().h(px(half * 2.0)))
                    .child(view().flex_col().child(self.node(sub, ss)))
            }
            (None, None) => return base_el,
        };
        row_view()
            .child(base_el)
            .child(column.flex_none().items_start())
            .into_element()
    }

    fn big_op(
        &self,
        symbol: &str,
        limits: bool,
        sup: Option<&Node>,
        sub: Option<&Node>,
        st: Style,
    ) -> AnyElement {
        let big = symbol.chars().count() == 1;
        let integral = big && !limits;
        let scale = match (big, st.display) {
            (true, true) => {
                if integral {
                    2.0
                } else {
                    1.6
                }
            }
            (true, false) => 1.15,
            _ => 1.0,
        };
        let op = math_text(symbol, st.size * scale, integral)
            .line_height(px(st.size * LINE * scale.min(1.6)));
        if limits && st.display {
            let ss = st.script(self.base);
            let empty = || view();
            let over = sup.map_or_else(|| empty().into_element(), |s| self.node(s, ss));
            let under = sub.map_or_else(|| empty().into_element(), |s| self.node(s, ss));
            return view()
                .grid()
                .grid_template_rows("1fr auto 1fr")
                .flex_none()
                .mx(px(st.size * 0.1))
                .child(view().flex_col().items_center().justify_end().child(over))
                .child(view().flex_row().justify_center().child(op))
                .child(view().flex_col().items_center().child(under))
                .into_element();
        }
        let (a, d) = (st.size * LINE * scale / 2.0, st.size * LINE * scale / 2.0);
        let el = row_view().mr(px(st.size * 0.1)).child(op).into_element();
        self.scripts(el, (a, d), sup, sub, st)
    }

    fn delimited(&self, open: &str, close: &str, body: &Node, st: Style) -> AnyElement {
        let (a, d) = body.extent(st, self.base);
        let height = 2.0 * a.max(d);
        let scale = (height / (st.size * LINE)).max(1.0);
        let delim = |s: &str| {
            math_text(s, st.size * scale.min(4.0), false)
                .line_height(px(height.max(st.size * LINE)))
                .flex_none()
        };
        let mut el = row_view();
        if !open.is_empty() {
            el = el.child(delim(open).mr(px(st.size * 0.05)));
        }
        if !matches!(body, Node::Space(em) if *em < 0.0) {
            el = el.child(self.node(body, st));
        }
        if !close.is_empty() {
            el = el.child(delim(close).ml(px(st.size * 0.05)));
        }
        el.into_element()
    }

    fn accent(&self, body: &Node, mark: Mark, st: Style) -> AnyElement {
        let thickness = (st.size * 0.06).max(1.0);
        match mark {
            Mark::Overline => row_view()
                .border_t(px(thickness))
                .border_color(rule_color)
                .pt(px(st.size * 0.08))
                .child(self.node(body, st))
                .into_element(),
            Mark::Underline => row_view()
                .border_b(px(thickness))
                .border_color(rule_color)
                .pb(px(st.size * 0.08))
                .child(self.node(body, st))
                .into_element(),
            Mark::Over(s) => view()
                .flex_col()
                .items_center()
                .flex_none()
                .child(
                    math_text(s, st.size * 0.8, false)
                        .line_height(px(st.size * 0.5))
                        .mb(px(-st.size * 0.05)),
                )
                .child(self.node(body, st))
                .into_element(),
        }
    }

    fn array(&self, rows: &[Vec<Node>], align: ArrayAlign, st: Style) -> AnyElement {
        let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let gap = match align {
            ArrayAlign::Alternate => 0.0,
            ArrayAlign::Left => 1.0,
            ArrayAlign::Center => 0.8,
        };
        let mut grid = view()
            .grid()
            .grid_template_columns(SharedString::from(format!("repeat({columns}, auto)")))
            .gap_x(px(st.size * gap))
            .gap_y(px(st.size * ROW_GAP))
            .items_center()
            .flex_none()
            .mx(px(st.size * 0.1));
        for row in rows {
            for c in 0..columns {
                let cell = row_view();
                let cell = match (align, c % 2) {
                    (ArrayAlign::Center, _) => cell.justify_center(),
                    (ArrayAlign::Left, _) | (ArrayAlign::Alternate, 1) => cell.justify_start(),
                    (ArrayAlign::Alternate, _) => cell.justify_end(),
                };
                let cell = match row.get(c) {
                    // A cell of `aligned` after an `&` starts with its
                    // relation: TeX's spacing before it.
                    Some(Node::Row(nodes)) if align == ArrayAlign::Alternate && c % 2 == 1 => {
                        let mut padded = vec![Node::Glyph {
                            text: String::new(),
                            class: Class::Ord,
                        }];
                        padded.extend(nodes.iter().cloned());
                        cell.child(self.row(&padded, st))
                    }
                    Some(n) => cell.child(self.node(n, st)),
                    None => cell,
                };
                grid = grid.child(cell);
            }
        }
        grid.into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(s: &str, class: Class) -> Node {
        glyph(s, class)
    }

    #[test]
    fn symbols_and_scripts() {
        assert_eq!(
            parse("x^2 + \\alpha_i").unwrap(),
            Node::Row(vec![
                Node::Scripts {
                    base: Box::new(g("x", Class::Var)),
                    sup: Some(Box::new(g("2", Class::Ord))),
                    sub: None,
                },
                g("+", Class::Bin),
                Node::Scripts {
                    base: Box::new(g("α", Class::Var)),
                    sup: None,
                    sub: Some(Box::new(g("i", Class::Var))),
                },
            ])
        );
        // Both scripts, either order.
        let both = parse("a_1^2").unwrap();
        assert!(
            matches!(&both, Node::Row(r) if matches!(&r[0], Node::Scripts { sup: Some(_), sub: Some(_), .. }))
        );
    }

    #[test]
    fn structures() {
        assert!(matches!(
            parse("\\frac{a}{b}").unwrap(),
            Node::Row(r) if matches!(r[0], Node::Frac { bar: true, .. })
        ));
        assert!(matches!(
            parse("\\sqrt[3]{x}").unwrap(),
            Node::Row(r) if matches!(&r[0], Node::Sqrt { index: Some(_), .. })
        ));
        assert!(matches!(
            parse("\\sum_{i=1}^n i").unwrap(),
            Node::Row(r) if matches!(&r[0], Node::BigOp { limits: true, sup: Some(_), sub: Some(_), .. })
        ));
        assert!(matches!(
            parse("\\left( \\frac12 \\right]").unwrap(),
            Node::Row(r) if matches!(&r[0], Node::Delimited { open, close, .. } if open == "(" && close == "]")
        ));
        assert!(matches!(
            parse("\\int\\limits_0^1").unwrap(),
            Node::Row(r) if matches!(&r[0], Node::BigOp { limits: true, .. })
        ));
    }

    #[test]
    fn environments() {
        let Node::Row(r) = parse("\\begin{pmatrix} 1 & 2 \\\\ 3 & 4 \\end{pmatrix}").unwrap()
        else {
            unreachable!()
        };
        let Node::Delimited { body, .. } = &r[0] else {
            panic!("{r:?}")
        };
        let Node::Array { rows, .. } = body.as_ref() else {
            panic!()
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].len(), 2);
        // A trailing line break adds no row.
        let Node::Row(r) = parse("\\begin{aligned} a &= b \\\\ \\end{aligned}").unwrap() else {
            unreachable!()
        };
        assert!(
            matches!(&r[0], Node::Array { rows, align: ArrayAlign::Alternate } if rows.len() == 1)
        );
        assert!(parse("\\begin{cases} 1 & x > 0 \\\\ 0 & \\text{otherwise} \\end{cases}").is_ok());
    }

    #[test]
    fn fonts_and_accents() {
        assert_eq!(
            parse("\\mathbb{R}").unwrap(),
            Node::Row(vec![Node::Row(vec![g("ℝ", Class::Upright)])])
        );
        assert_eq!(
            parse("\\hat x").unwrap(),
            Node::Row(vec![g("x\u{302}", Class::Var)])
        );
        assert_eq!(
            parse("\\not=").unwrap(),
            Node::Row(vec![g("≠", Class::Rel)])
        );
        assert_eq!(
            parse("\\text{if } x").unwrap(),
            Node::Row(vec![Node::Text("if ".into()), g("x", Class::Var)])
        );
    }

    #[test]
    fn unknown_tex_fails() {
        assert!(parse("\\unknowncommand").is_err());
        assert!(parse("{a").is_err());
        assert!(parse("a}").is_err());
        assert!(parse("\\begin{tikzpicture}\\end{tikzpicture}").is_err());
        // Deep nesting is refused, not a stack overflow.
        assert!(parse(&"{".repeat(10_000)).is_err());
    }

    #[test]
    fn common_formulas_parse() {
        for tex in [
            "E = mc^2",
            "e^{i\\pi} + 1 = 0",
            "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}",
            "\\int_0^\\infty e^{-x^2} \\, dx = \\frac{\\sqrt{\\pi}}{2}",
            "\\lim_{n \\to \\infty} \\left(1 + \\frac{1}{n}\\right)^n = e",
            "f'(x) = \\lim_{h\\to 0}\\frac{f(x+h)-f(x)}{h}",
            "\\nabla \\times \\vec{B} = \\mu_0 \\vec{J}",
            "\\binom{n}{k} = \\frac{n!}{k!(n-k)!}",
            "\\mathcal{L}\\{f\\}(s) = \\int_0^\\infty f(t)e^{-st}\\,dt",
            "\\det\\begin{vmatrix} a & b \\\\ c & d \\end{vmatrix} = ad - bc",
            "\\overline{AB} \\perp \\overrightarrow{CD}",
            "\\operatorname{softmax}(x)_i = \\frac{e^{x_i}}{\\sum_j e^{x_j}}",
        ] {
            assert!(parse(tex).is_ok(), "{tex}: {:?}", parse(tex));
        }
    }
}
