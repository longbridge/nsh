//! Spoken and ranged accessibility values for ArkUI nodes and virtual providers.

use nsh_core::{Accessibility, Checked};

/// Complete numeric range (`(min, max, current)`) accepted by ArkUI's
/// `NODE_ACCESSIBILITY_VALUE` setter, which rejects partial ranges or
/// out-of-bounds `current` with `ARKUI_ERROR_CODE_PARAM_INVALID`.
pub(crate) fn range_value(accessibility: &Accessibility) -> Option<(i32, i32, i32)> {
    let value = accessibility.value.as_ref()?;
    let (min, max, now) = (value.min?, value.max?, value.now?);
    if !min.is_finite() || !max.is_finite() || !now.is_finite() || max < min {
        return None;
    }
    let min = min.floor() as i32;
    let max = (max.ceil() as i32).max(min);
    let current = (now.round() as i32).clamp(min, max);
    Some((min, max, current))
}

pub(crate) fn spoken_value(accessibility: &Accessibility) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(value) = &accessibility.value {
        if let Some(text) = &value.text {
            if !text.is_empty() {
                parts.push(text.to_string());
            }
        } else if let Some(now) = value.now {
            parts.push(match (value.min, value.max) {
                (Some(min), Some(max)) if max > min => {
                    format!("{}%", (((now - min) / (max - min)) * 100.0).round())
                }
                _ => now.to_string(),
            });
        }
    }
    match accessibility.state.checked {
        Some(Checked::True) => parts.push("checked".into()),
        Some(Checked::False) => parts.push("not checked".into()),
        Some(Checked::Mixed) => parts.push("mixed".into()),
        None => {}
    }
    match accessibility.state.expanded {
        Some(true) => parts.push("expanded".into()),
        Some(false) => parts.push("collapsed".into()),
        None => {}
    }
    if accessibility.state.busy {
        parts.push("busy".into());
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

#[cfg(test)]
mod tests {
    use nsh_core::{AccessibilityState, AccessibilityValue};

    use super::*;

    #[test]
    fn explicit_value_keeps_mixed_expanded_and_busy_states() {
        let accessibility = Accessibility {
            value: Some(AccessibilityValue {
                text: Some("2 items".into()),
                now: Some(50.0),
                ..Default::default()
            }),
            state: AccessibilityState {
                checked: Some(Checked::Mixed),
                expanded: Some(true),
                busy: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            spoken_value(&accessibility).as_deref(),
            Some("2 items, mixed, expanded, busy")
        );
    }

    #[test]
    fn ranges_follow_the_ios_percentage_convention() {
        let accessibility = Accessibility {
            value: Some(AccessibilityValue {
                min: Some(20.0),
                max: Some(40.0),
                now: Some(25.0),
                text: None,
            }),
            ..Default::default()
        };
        assert_eq!(spoken_value(&accessibility).as_deref(), Some("25%"));
    }

    #[test]
    fn empty_value_still_exposes_state_and_clearing_removes_it() {
        let accessibility = Accessibility {
            value: Some(AccessibilityValue {
                text: Some("".into()),
                ..Default::default()
            }),
            state: AccessibilityState {
                checked: Some(Checked::False),
                expanded: Some(false),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            spoken_value(&accessibility).as_deref(),
            Some("not checked, collapsed")
        );
        assert_eq!(spoken_value(&Accessibility::default()), None);
    }

    #[test]
    fn range_value_requires_complete_finite_bounds_and_clamps_current() {
        let indeterminate = Accessibility {
            value: Some(AccessibilityValue {
                min: Some(0.0),
                max: Some(100.0),
                now: None,
                text: None,
            }),
            ..Default::default()
        };
        assert_eq!(range_value(&indeterminate), None);

        let without_bounds = Accessibility {
            value: Some(AccessibilityValue {
                min: None,
                max: None,
                now: Some(50.0),
                text: None,
            }),
            ..Default::default()
        };
        assert_eq!(range_value(&without_bounds), None);

        let complete = Accessibility {
            value: Some(AccessibilityValue {
                min: Some(0.0),
                max: Some(100.0),
                now: Some(42.4),
                text: None,
            }),
            ..Default::default()
        };
        assert_eq!(range_value(&complete), Some((0, 100, 42)));

        let out_of_bounds = Accessibility {
            value: Some(AccessibilityValue {
                min: Some(10.0),
                max: Some(20.0),
                now: Some(25.0),
                text: None,
            }),
            ..Default::default()
        };
        assert_eq!(range_value(&out_of_bounds), Some((10, 20, 20)));
    }
}
