//! Tailwind CSS classes (`class="flex-row gap-4 p-4 bg-white dark:bg-zinc-900"`),
//! resolved in Rust: each distinct class string is parsed once into rules
//! (cached), and applied to a node's style whenever its variants may have
//! changed (appearance, window width, press / hover / focus, group
//! states). JS only passes the string.
//!
//! Utilities later in the string win over earlier ones, so components can
//! take a `class` prop and append it (no `tailwind-merge`). Variant
//! utilities apply after plain ones, ordered by Tailwind's variant order
//! (`hover:` before `active:`, breakpoints ascending, `dark:` last).
//!
//! Each class string is parsed once into rules, cached in an LRU of up to
//! 4096 strings, with what the rules depend on (appearance, window width and
//! height, direction, the node's own states, a group's states, sibling
//! position, reduce motion): the core (`core/classes.rs`) resolves a node again only when
//! one of those changes. Interaction states are reported by the platforms
//! only for the bits a node's classes read (`MountItem::TrackStates`), and
//! `focus-within` propagates to ancestors in the core. A node's style is the
//! default, then its class rules, then its inline `style` replayed on top
//! (`Op::SetStyle` carries a `StylePatch` besides the merged `Style`). The
//! theme is Tailwind 4's (the palette converted from OKLCH to sRGB) extended
//! by `nsh.json` `"tailwind"`, compiled into the bundle as
//! `nsh:tailwind-theme` and pushed as a root prop at startup.

pub(crate) mod palette;
mod theme;
mod utilities;

use std::sync::Arc;

use rustc_hash::FxHashMap;

pub use self::theme::{Theme, ThemeError};
pub(crate) use self::utilities::{Decl, Len, Part};
use self::utilities::{Parts, touched};
use crate::{
    ops::ResizeMode,
    props::PointerEvents,
    style::{Color, EllipsizeMode, Style, TextShadow},
};

/// Interaction states a class can depend on (`active:`, `hover:`, ...).
pub mod state {
    /// The view is pressed (`active:`).
    pub const PRESSED: u8 = 1;
    /// A pointer is over the view (`hover:`).
    pub const HOVERED: u8 = 2;
    /// The input has focus (`focus:`).
    pub const FOCUSED: u8 = 4;
    /// The view is disabled (`disabled:`).
    pub const DISABLED: u8 = 8;
    /// The view or a descendant has focus (`focus-within:`).
    pub const FOCUS_WITHIN: u8 = 16;
}

/// The view's place among its siblings (`first:`, `odd:`, ...).
pub mod position {
    /// The first child.
    pub const FIRST: u8 = 1;
    /// The last child.
    pub const LAST: u8 = 2;
    /// An odd child (1st, 3rd, ...).
    pub const ODD: u8 = 4;
    /// An even child.
    pub const EVEN: u8 = 8;
    /// The only child.
    pub const ONLY: u8 = 16;
}

/// What resolved classes depend on, so the core knows when to resolve
/// them again.
pub mod deps {
    /// Light or dark appearance.
    pub const DARK: u16 = 1;
    /// The window width (breakpoints, `w-screen`).
    pub const WIDTH: u16 = 2;
    /// The window's orientation or height (`landscape:`, `h-screen`).
    pub const HEIGHT: u16 = 4;
    /// The layout direction.
    pub const RTL: u16 = 8;
    /// The view's own interaction states.
    pub const STATE: u16 = 16;
    /// The states of an ancestor `group`.
    pub const GROUP: u16 = 32;
    /// The position among siblings.
    pub const POSITION: u16 = 64;
    /// The reduce-motion setting.
    pub const MOTION: u16 = 128;
}

/// The platform classes run on (`ios:`, `android:`, `web:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Platform {
    /// iOS and Mac Catalyst.
    #[default]
    Ios,
    /// Android.
    Android,
    /// A web browser.
    Web,
}

/// The environment variants depend on, set by the engine.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClassEnvironment {
    /// Dark appearance.
    pub dark: bool,
    /// The platform.
    pub platform: Platform,
    /// The user asked for less motion.
    pub reduce_motion: bool,
}

/// What a node's classes are matched against.
pub(crate) struct Context<'a> {
    pub(crate) env: ClassEnvironment,
    /// The window (root view) size.
    pub(crate) window: (f32, f32),
    pub(crate) rtl: bool,
    /// [`state`] bits of the node.
    pub(crate) states: u8,
    /// [`position`] bits of the node.
    pub(crate) position: u8,
    /// [`state`] bits of the nearest ancestor group with this name (`""`
    /// for unnamed groups).
    pub(crate) group: &'a dyn Fn(&str) -> u8,
}

/// Conditions of one rule (all must hold).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Condition {
    dark: Option<bool>,
    min_width: Option<f32>,
    max_width: Option<f32>,
    landscape: Option<bool>,
    platform: Option<Platform>,
    rtl: Option<bool>,
    reduce_motion: Option<bool>,
    /// Required [`state`] bits.
    states: u8,
    /// [`state`] bits that must be off (`enabled:`).
    not_states: u8,
    /// A group name and its required states.
    group: Option<(Box<str>, u8)>,
    /// Allowed [`position`] bits (any of them).
    position: u8,
    /// Applies to an input's placeholder.
    placeholder: bool,
}

impl Condition {
    fn matches(&self, cx: &Context<'_>) -> bool {
        if self.dark.is_some_and(|d| d != cx.env.dark)
            || self.platform.is_some_and(|p| p != cx.env.platform)
            || self.rtl.is_some_and(|r| r != cx.rtl)
            || self
                .reduce_motion
                .is_some_and(|r| r != cx.env.reduce_motion)
            || self.min_width.is_some_and(|w| cx.window.0 < w)
            || self.max_width.is_some_and(|w| cx.window.0 >= w)
            || self
                .landscape
                .is_some_and(|l| l != (cx.window.0 > cx.window.1))
            || cx.states & self.states != self.states
            || cx.states & self.not_states != 0
        {
            return false;
        }
        if self.position != 0 && cx.position & self.position == 0 {
            return false;
        }
        if let Some((name, states)) = &self.group
            && (cx.group)(name) & states != *states
        {
            return false;
        }
        true
    }

    /// Applies only while the view (or its group) is pressed, hovered or
    /// focused: press feedback.
    fn interactive(&self) -> bool {
        const INTERACTION: u8 =
            state::PRESSED | state::HOVERED | state::FOCUSED | state::FOCUS_WITHIN;
        self.states & INTERACTION != 0
            || self
                .group
                .as_ref()
                .is_some_and(|(_, states)| states & INTERACTION != 0)
    }

    fn deps(&self) -> u16 {
        let mut d = 0;
        if self.dark.is_some() {
            d |= deps::DARK;
        }
        if self.min_width.is_some() || self.max_width.is_some() {
            d |= deps::WIDTH;
        }
        if self.landscape.is_some() {
            d |= deps::WIDTH | deps::HEIGHT;
        }
        if self.rtl.is_some() {
            d |= deps::RTL;
        }
        if self.reduce_motion.is_some() {
            d |= deps::MOTION;
        }
        if self.states != 0 || self.not_states != 0 {
            d |= deps::STATE;
        }
        if self.group.is_some() {
            d |= deps::GROUP;
        }
        if self.position != 0 {
            d |= deps::POSITION;
        }
        d
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Rule {
    cond: Condition,
    decls: Vec<Decl>,
    /// Variant order: plain utilities 0, then by Tailwind's order.
    rank: u16,
    /// `!`: applied after everything else.
    important: bool,
}

/// Ranks of the variants Rust components use (Tailwind's order, as
/// [`variant`] gives them).
pub(crate) mod rank {
    pub(crate) const GROUP: u16 = 20;
    pub(crate) const HOVER: u16 = 31;
    pub(crate) const FOCUS: u16 = 32;
    pub(crate) const ACTIVE: u16 = 33;
    pub(crate) const DISABLED: u16 = 35;
    pub(crate) const DARK: u16 = 80;
}

impl Rule {
    /// A rule applying `decls` while the node is in all of [`state`]
    /// `states`, ordered by `rank`.
    pub(crate) fn new(states: u8, rank: u16, decls: Vec<Decl>) -> Self {
        Self {
            cond: Condition {
                states,
                ..Condition::default()
            },
            decls,
            rank,
            important: false,
        }
    }

    /// [`Rule::new`], also requiring the nearest ancestor group `group`
    /// (`""`: unnamed) in its states, and dark appearance when `dark`.
    pub(crate) fn with(
        states: u8,
        group: Option<(&str, u8)>,
        dark: bool,
        rank: u16,
        decls: Vec<Decl>,
    ) -> Self {
        let mut rule = Self::new(states, rank, decls);
        rule.cond.group = group.map(|(name, states)| (name.into(), states));
        rule.cond.dark = dark.then_some(true);
        rule
    }
}

/// Node properties classes can set (an explicit prop wins over them).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClassProps {
    /// `line-clamp-*`, `truncate`; `Some(0)` removes a clamp.
    pub number_of_lines: Option<u32>,
    /// `text-ellipsis`, `text-clip`, `truncate`.
    pub ellipsize: Option<EllipsizeMode>,
    /// `whitespace-nowrap`, `text-nowrap`, `truncate`: lines never wrap.
    pub no_wrap: Option<bool>,
    /// `pointer-events-none` / `-auto`.
    pub pointer_events: Option<PointerEvents>,
    /// `object-cover` and friends (images).
    pub resize_mode: Option<ResizeMode>,
    /// `select-none` / `select-text`.
    pub selectable: Option<bool>,
    /// `placeholder:text-*` (inputs).
    pub placeholder_color: Option<Color>,
    /// `caret-*` (inputs).
    pub caret_color: Option<Color>,
    /// `text-shadow-*` layers beyond the first (drawn as one shadow).
    pub text_shadows: Option<Vec<TextShadow>>,
}

/// A parsed class string.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClassList {
    rules: Vec<Rule>,
    /// [`deps`] bits.
    deps: u16,
    /// [`state`] bits the platform must report for this view.
    tracked: u8,
    /// The view is a `group` (`""`) or named groups (`group/item`).
    groups: Vec<Box<str>>,
    /// Classes that are not known utilities.
    unknown: Vec<Box<str>>,
}

impl ClassList {
    /// [`deps`] bits.
    #[inline]
    pub fn deps(&self) -> u16 {
        self.deps
    }

    /// [`state`] bits whose changes the platform must report.
    #[inline]
    pub fn tracked_states(&self) -> u8 {
        self.tracked
    }

    /// Group names this view defines (`""`: an unnamed `group`).
    pub fn groups(&self) -> &[Box<str>] {
        &self.groups
    }

    /// Classes that are not known utilities or variants.
    pub fn unknown(&self) -> &[Box<str>] {
        &self.unknown
    }

    /// Whether nothing applies.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Applies the matching rules to `style` and `props`.
    pub(crate) fn apply(
        &self,
        cx: &Context<'_>,
        theme: &Theme,
        style: &mut Style,
        props: &mut ClassProps,
        inline: impl FnOnce(&mut Style),
    ) {
        let mut parts = Parts::default();
        let mut important = false;
        for rule in &self.rules {
            if rule.cond.placeholder || !rule.cond.matches(cx) {
                continue;
            }
            for decl in &rule.decls {
                if let Decl::Default(key, value) = decl {
                    let _ = style.set_key(*key, value.as_value());
                }
            }
        }
        // A background color of an interaction state (`active:bg-*`) hides
        // the gradients unless that state sets one too: drawn over the
        // color, they would hide the feedback (CSS would keep them).
        let mut state_background = false;
        let mut state_image = false;
        for rule in &self.rules {
            if rule.cond.placeholder || !rule.cond.matches(cx) {
                continue;
            }
            important |= rule.important;
            for decl in &rule.decls {
                decl.apply(cx.env.dark, style, props, &mut parts);
            }
            let background = rule.decls.iter().any(Decl::sets_background_color);
            if rule.cond.interactive() {
                if background {
                    state_background = true;
                    state_image = false;
                }
                state_image |= rule.decls.iter().any(Decl::sets_background_image);
            } else if background {
                // A later rule's color (`dark:bg-*`) is not feedback.
                state_background = false;
            }
        }
        let state_color = style.view.background_color;
        // `placeholder:` rules only carry a color.
        for rule in self.rules.iter().filter(|r| r.cond.placeholder) {
            if !rule.cond.matches(cx) {
                continue;
            }
            for decl in &rule.decls {
                if let Some(c) = decl.text_color() {
                    props.placeholder_color = Some(c);
                }
            }
        }
        parts.finish(style, props, theme, cx.window, touched::ALL);
        inline(style);
        if important {
            // `!` utilities win over the inline style too, as in CSS. The
            // parts keep every rule's values (a `!scale-*` still composes
            // with a plain `translate-*`); only the groups the important
            // rules touch are written again.
            parts.touched = 0;
            parts.screen.clear();
            for rule in self
                .rules
                .iter()
                .filter(|r| r.important && !r.cond.placeholder)
            {
                if rule.cond.matches(cx) {
                    for decl in &rule.decls {
                        decl.apply(cx.env.dark, style, props, &mut parts);
                    }
                }
            }
            parts.finish(style, props, theme, cx.window, parts.touched);
        }
        // Unless the inline style replaced the state's color.
        if state_background && !state_image && style.view.background_color == state_color {
            style.view.clear_background_images();
        }
    }
}

impl ClassList {
    /// The rules of a Rust component's styles (already in order), on a
    /// view that is the groups `groups` (`""`: unnamed).
    pub(crate) fn from_rules(rules: Vec<Rule>, groups: Vec<Box<str>>) -> Self {
        let mut list = ClassList::default();
        for rule in rules {
            list.add(rule);
        }
        if !groups.is_empty() {
            list.tracked |= state::PRESSED | state::HOVERED | state::FOCUS_WITHIN;
            list.groups = groups;
        }
        list.rules.sort_by_key(|r| (r.important, r.rank));
        list
    }

    fn add(&mut self, rule: Rule) {
        self.deps |= rule.cond.deps();
        self.tracked |= rule.cond.states & !state::DISABLED;
        if rule.cond.states & state::FOCUS_WITHIN != 0 {
            self.tracked |= state::FOCUS_WITHIN;
        }
        for d in &rule.decls {
            self.deps |= d.deps();
        }
        self.rules.push(rule);
    }

    /// `self`, then `over`: at equal rank `over`'s rules win (the app's
    /// classes on a Rust component).
    pub(crate) fn merged(&self, over: &ClassList) -> ClassList {
        let mut list = self.clone();
        for rule in &over.rules {
            list.add(rule.clone());
        }
        list.groups.extend(over.groups.iter().cloned());
        list.unknown.extend(over.unknown.iter().cloned());
        list.tracked |= over.tracked;
        list.rules.sort_by_key(|r| (r.important, r.rank));
        list
    }
}

/// The class cache and theme of one engine.
#[derive(Debug, Default)]
pub struct Tailwind {
    theme: Theme,
    cache: FxHashMap<Box<str>, Arc<ClassList>>,
}

impl Tailwind {
    /// Classes with the default theme.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the theme (cached classes are parsed again).
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.cache.clear();
    }

    /// The theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// The parsed form of `classes` (cached by string).
    pub fn classes(&mut self, classes: &str) -> Arc<ClassList> {
        self.get(classes).0
    }

    /// Like [`Tailwind::classes`], and whether `classes` was parsed just
    /// now (to report its unknown classes once).
    pub fn get(&mut self, classes: &str) -> (Arc<ClassList>, bool) {
        if let Some(c) = self.cache.get(classes) {
            return (c.clone(), false);
        }
        let parsed = Arc::new(parse(&self.theme, classes));
        // Class strings are static in practice; a bound keeps generated
        // ones (`w-[${n}px]`) from growing the cache forever.
        if self.cache.len() >= 4096 {
            self.cache.clear();
        }
        self.cache.insert(classes.into(), parsed.clone());
        (parsed, true)
    }
}

/// Parses a class string.
pub fn parse(theme: &Theme, classes: &str) -> ClassList {
    let mut list = ClassList::default();
    for token in classes.split_ascii_whitespace() {
        match parse_token(theme, token) {
            Some(Token::Rule(rule)) => list.add(rule),
            Some(Token::Group(name)) => {
                list.tracked |= state::PRESSED | state::HOVERED | state::FOCUS_WITHIN;
                list.groups.push(name);
            }
            Some(Token::Ignored) => {}
            None => list.unknown.push(token.into()),
        }
    }
    // Stable: equal ranks keep the order of the string.
    list.rules.sort_by_key(|r| (r.important, r.rank));
    list
}

enum Token {
    Rule(Rule),
    Group(Box<str>),
    /// Accepted, but has no effect natively (`antialiased`).
    Ignored,
}

/// Splits `token` at `:` outside brackets: variants, then the utility.
fn split_variants(token: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in token.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ':' if depth == 0 => {
                parts.push(&token[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&token[start..]);
    parts
}

fn parse_token(theme: &Theme, token: &str) -> Option<Token> {
    let parts = split_variants(token);
    let (utility, variants) = parts.split_last()?;
    let mut utility = *utility;
    // `!p-4` (v3) or `p-4!` (v4): important.
    let mut important = false;
    if let Some(u) = utility.strip_prefix('!') {
        utility = u;
        important = true;
    } else if let Some(u) = utility.strip_suffix('!') {
        utility = u;
        important = true;
    }
    if variants.is_empty() {
        if utility == "group" {
            return Some(Token::Group("".into()));
        }
        if let Some(name) = utility.strip_prefix("group/") {
            return Some(Token::Group(name.into()));
        }
        // Web-only utilities that need nothing natively.
        if matches!(
            utility,
            "antialiased"
                | "subpixel-antialiased"
                | "isolate"
                | "isolation-auto"
                | "box-border"
                | "box-content"
        ) || [
            "will-change-",
            "touch-",
            "appearance-",
            "scroll-",
            "snap-",
            "overscroll-",
        ]
        .iter()
        .any(|p| utility.starts_with(p))
        {
            return Some(Token::Ignored);
        }
    }
    let mut cond = Condition::default();
    let mut rank = 0u16;
    for v in variants {
        rank = rank.max(variant(theme, v, &mut cond)?);
    }
    let decls = utilities::parse(theme, utility)?;
    Some(Token::Rule(Rule {
        cond,
        decls,
        rank,
        important,
    }))
}

/// Applies variant `v` to `cond`; returns its rank (Tailwind's order).
fn variant(theme: &Theme, v: &str, cond: &mut Condition) -> Option<u16> {
    use state::*;
    let states = |cond: &mut Condition, s: u8| {
        cond.states |= s;
    };
    Some(match v {
        "first" => {
            cond.position |= position::FIRST;
            10
        }
        "last" => {
            cond.position |= position::LAST;
            11
        }
        "only" => {
            cond.position |= position::ONLY;
            12
        }
        "odd" => {
            cond.position |= position::ODD;
            13
        }
        "even" => {
            cond.position |= position::EVEN;
            14
        }
        "placeholder" => {
            cond.placeholder = true;
            15
        }
        "focus-within" => {
            states(cond, FOCUS_WITHIN);
            30
        }
        "hover" => {
            states(cond, HOVERED);
            31
        }
        "focus" | "focus-visible" => {
            states(cond, FOCUSED);
            32
        }
        "active" => {
            states(cond, PRESSED);
            33
        }
        "enabled" => {
            cond.not_states |= DISABLED;
            34
        }
        "disabled" => {
            states(cond, DISABLED);
            35
        }
        "motion-safe" => {
            cond.reduce_motion = Some(false);
            40
        }
        "motion-reduce" => {
            cond.reduce_motion = Some(true);
            41
        }
        "portrait" => {
            cond.landscape = Some(false);
            70
        }
        "landscape" => {
            cond.landscape = Some(true);
            71
        }
        "ltr" => {
            cond.rtl = Some(false);
            72
        }
        "rtl" => {
            cond.rtl = Some(true);
            73
        }
        "dark" => {
            cond.dark = Some(true);
            80
        }
        "ios" => {
            cond.platform = Some(Platform::Ios);
            81
        }
        "android" => {
            cond.platform = Some(Platform::Android);
            82
        }
        "web" => {
            cond.platform = Some(Platform::Web);
            83
        }
        v => {
            if let Some(rest) = v.strip_prefix("group-") {
                // `group-hover`, `group-hover/item`.
                let (s, name) = match rest.split_once('/') {
                    Some((s, n)) => (s, n),
                    None => (rest, ""),
                };
                let bit = match s {
                    "hover" => HOVERED,
                    "active" => PRESSED,
                    "focus" | "focus-visible" => FOCUSED,
                    "focus-within" => FOCUS_WITHIN,
                    "disabled" => DISABLED,
                    _ => return None,
                };
                let entry = cond.group.get_or_insert_with(|| (name.into(), 0));
                if *entry.0 != *name {
                    return None;
                }
                entry.1 |= bit;
                return Some(20);
            }
            // Breakpoints: `md:`, `max-md:`, `min-[600px]:`, `max-[600px]:`.
            let (max, name) = match v.strip_prefix("max-") {
                Some(n) => (true, n),
                None => (false, v.strip_prefix("min-").unwrap_or(v)),
            };
            let width = match name.strip_prefix('[').and_then(|n| n.strip_suffix(']')) {
                Some(arbitrary) => crate::css::length(arbitrary)?,
                None => theme
                    .breakpoints
                    .iter()
                    .position(|(n, _)| **n == *name)
                    .map(|i| theme.breakpoints[i].1)?,
            };
            let index = theme
                .breakpoints
                .iter()
                .filter(|(_, w)| *w <= width)
                .count() as u16;
            if max {
                cond.max_width = Some(cond.max_width.map_or(width, |w| w.min(width)));
                50 + index
            } else {
                cond.min_width = Some(cond.min_width.map_or(width, |w| w.max(width)));
                60 + index
            }
        }
    })
}

#[cfg(test)]
mod tests;
