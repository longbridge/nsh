//! Design tokens: colors with a light and a dark value and numbers, by key
//! (`primary.background`, `radius`). The core gives no key a meaning and
//! has none of its own: a UI set brings them (`nsh-base`'s theme, through
//! [`super::Registry::tokens_from`]), an app sets its own, an embedding app
//! its own over those ([`crate::Core::set_host_tokens`]).
//!
//! Tailwind classes name every token (`bg-primary`,
//! `rounded-(--radius)`; `tailwind/theme.rs`), and Rust components read
//! them ([`super::ambient::tokens`]).

use rustc_hash::FxHashMap;

use super::style::AdaptiveColor;

/// Design tokens by key: colors with a value per appearance, and numbers.
/// The default has none.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tokens {
    colors: FxHashMap<Box<str>, AdaptiveColor>,
    numbers: FxHashMap<Box<str>, f32>,
}

/// Makes the tokens of a theme file's content (`Some`), or the defaults
/// (`None`): how a UI set reads the app's theme ([`super::Registry::tokens_from`]).
pub type TokenSource = fn(Option<&str>) -> Result<Tokens, String>;

impl Tokens {
    /// The color of key `key`.
    pub fn color(&self, key: &str) -> Option<AdaptiveColor> {
        self.colors.get(key).copied()
    }

    /// Sets the color of key `key`.
    pub fn set_color(&mut self, key: &str, color: AdaptiveColor) {
        self.colors.insert(key.into(), color);
    }

    /// The number of key `key`.
    pub fn number(&self, key: &str) -> Option<f32> {
        self.numbers.get(key).copied()
    }

    /// Sets the number of key `key`.
    pub fn set_number(&mut self, key: &str, value: f32) {
        self.numbers.insert(key.into(), value);
    }

    /// Whether it has no token.
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty() && self.numbers.is_empty()
    }

    /// Each color by key.
    pub fn colors(&self) -> impl Iterator<Item = (&str, AdaptiveColor)> {
        self.colors.iter().map(|(k, c)| (&**k, *c))
    }

    /// Each number by key.
    pub fn numbers(&self) -> impl Iterator<Item = (&str, f32)> {
        self.numbers.iter().map(|(k, n)| (&**k, *n))
    }

    /// `self` with the tokens of `base` it does not have.
    pub fn over(&self, base: &Tokens) -> Tokens {
        let mut out = base.clone();
        out.colors
            .extend(self.colors.iter().map(|(k, c)| (k.clone(), *c)));
        out.numbers
            .extend(self.numbers.iter().map(|(k, n)| (k.clone(), *n)));
        out
    }
}

/// Tokens as apps have them (`nsh-base`'s default theme's), for the core's
/// tests.
#[cfg(test)]
pub(crate) fn sample() -> Tokens {
    let mut tokens = Tokens::default();
    for (key, light, dark) in [
        ("background", 0xffffff, 0x0a0a0a),
        ("foreground", 0x0a0a0a, 0xfafafa),
        ("border", 0xe5e5e5, 0x262626),
        ("ring", 0xa3a3a3, 0x525252),
        ("muted.background", 0xf5f5f5, 0x262626),
        ("muted.foreground", 0x737373, 0xa3a3a3),
        ("accent.background", 0xf5f5f5, 0x262626),
        ("primary.background", 0x171717, 0xfafafa),
        ("primary.active.background", 0x0a0a0a, 0xe5e5e5),
        ("primary.foreground", 0xfafafa, 0x171717),
        ("popover.background", 0xffffff, 0x0a0a0a),
        ("popover.foreground", 0x0a0a0a, 0xfafafa),
    ] {
        tokens.set_color(key, AdaptiveColor::rgb(light, dark));
    }
    for (key, n) in [("radius", 12.0), ("radius.lg", 16.0), ("font.size", 16.0)] {
        tokens.set_number(key, n);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::rgb;

    #[test]
    fn tokens_over_others_win_and_keep_the_rest() {
        let mut base = Tokens::default();
        base.set_color("primary", rgb(0x111111).into());
        base.set_color("ring", rgb(0x222222).into());
        base.set_number("radius", 12.0);
        let mut top = Tokens::default();
        top.set_color("primary", rgb(0x333333).into());
        top.set_number("radius.card", 10.0);
        let tokens = top.over(&base);
        assert_eq!(tokens.color("primary"), Some(rgb(0x333333).into()));
        assert_eq!(tokens.color("ring"), Some(rgb(0x222222).into()));
        assert_eq!(tokens.number("radius"), Some(12.0));
        assert_eq!(tokens.number("radius.card"), Some(10.0));
        assert!(Tokens::default().is_empty());
    }
}
