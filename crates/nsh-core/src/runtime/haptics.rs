//! `nsh/mobile`'s `Haptics`: what the device plays under the user's finger
//! (iOS's feedback generators, Android's haptic feedback). Elsewhere, and
//! where the device has no haptics or the user turned them off, nothing
//! happens.

use crate::{
    Haptic,
    element::{ModuleCx, PropEnum, module},
};

/// The weight of an impact (`Haptics.impact`'s argument).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub(crate) enum HapticImpact {
    /// A small, light element.
    Light,
    /// A moderate element.
    Medium,
    /// A large, heavy element.
    Heavy,
    /// A soft, flexible element.
    Soft,
    /// A hard, inflexible element.
    Rigid,
}

/// How a task ended (`Haptics.notification`'s argument).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub(crate) enum HapticNotification {
    /// It succeeded.
    Success,
    /// It produced a warning.
    Warning,
    /// It failed.
    Error,
}

/// Haptics: a selection changing, an impact, a task's outcome.
#[module]
pub(crate) struct Haptics;

#[module]
impl Haptics {
    /// Plays the haptic of a selection changing: a picker's value, a
    /// segment, a slider's step.
    pub(crate) fn selection(cx: &mut ModuleCx) {
        cx.haptic(Haptic::Selection);
    }

    /// Plays the haptic of an element colliding: `style` is its weight
    /// (`"medium"` by default).
    pub(crate) fn impact(style: Option<HapticImpact>, cx: &mut ModuleCx) {
        cx.haptic(match style.unwrap_or(HapticImpact::Medium) {
            HapticImpact::Light => Haptic::Light,
            HapticImpact::Medium => Haptic::Medium,
            HapticImpact::Heavy => Haptic::Heavy,
            HapticImpact::Soft => Haptic::Soft,
            HapticImpact::Rigid => Haptic::Rigid,
        });
    }

    /// Plays the haptic of a task that ended: `"success"` (the default),
    /// `"warning"` or `"error"`.
    pub(crate) fn notification(kind: Option<HapticNotification>, cx: &mut ModuleCx) {
        cx.haptic(match kind.unwrap_or(HapticNotification::Success) {
            HapticNotification::Success => Haptic::Success,
            HapticNotification::Warning => Haptic::Warning,
            HapticNotification::Error => Haptic::Error,
        });
    }
}
