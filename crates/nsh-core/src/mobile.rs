//! Capabilities specific to iOS and Android (TS
//! `nsh/mobile`): the system's tab bar ([`TabBar`]), a native component the
//! two backends register themselves ([`TAB_BAR_COMPONENT`]), and the
//! system's segmented control ([`SegmentedControl`],
//! [`SEGMENTED_CONTROL_COMPONENT`], iOS's) and switch ([`Switch`],
//! [`SWITCH_COMPONENT`]). Elsewhere, and on a platform
//! built outside NSH without a tab bar
//! ([`crate::platform::Environment::tab_bar`]), a component takes no room
//! and shows nothing. Common controls belong to `nsh/base`; this module
//! exposes the system's own controls where an app needs the platform's
//! exact look and behavior, rather than a parallel set of styled controls.

use crate::{
    JsonValue,
    element::{NativeEvent, native},
    platform::{Os, SEGMENTED_CONTROL_COMPONENT, SWITCH_COMPONENT, TAB_BAR_COMPONENT},
    prelude::*,
};

/// One tab of a [`TabBar`].
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub struct TabBarItem {
    /// Its title.
    pub title: SharedString,
    /// Its icon, by the platform's name for it: an SF Symbol on iOS
    /// (`"house"`), a drawable of the app on Android (`res/drawable/home.xml`
    /// is `"home"`, `"android:ic_menu_search"` the system's); TS's
    /// `platform.select` picks one per platform.
    #[prop(skip_none)]
    pub icon: Option<SharedString>,
    /// Its badge: a text (`"3"`, `"New"`), or `""` for a badge with no
    /// text (Material 3's small dot on Android, UIKit's empty badge on iOS).
    /// `None` (the default) shows none. The badge is the system's
    /// (`badgeValue` on iOS, Material 3's on Android), and screen readers
    /// say its text with the title.
    #[prop(skip_none)]
    pub badge: Option<SharedString>,
}

impl TabBarItem {
    /// A tab titled `title`, with no icon.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    /// Its icon, by the platform's name for it ([`TabBarItem::icon`]).
    pub fn icon(mut self, name: impl Into<SharedString>) -> Self {
        self.icon = Some(name.into());
        self
    }

    /// Its badge ([`TabBarItem::badge`]): a text, or `""` for a badge with
    /// no text.
    pub fn badge(mut self, text: impl Into<SharedString>) -> Self {
        self.badge = Some(text.into());
        self
    }
}

/// The props of [`TabBar`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct TabBarProps {
    /// The tabs, in order.
    #[prop(default)]
    pub items: Prop<Vec<TabBarItem>>,
    /// The selected tab, by index (default 0; one past the tabs selects
    /// none). The app owns it: the user's choice is reported with
    /// `onChange`.
    #[prop(default)]
    pub selected_index: Prop<u32>,
    /// Out of sight, without a transition (default false): the app sets it when
    /// it switches tabs, whose pages are kept and shown again as they were.
    #[prop(default)]
    pub hidden: Prop<bool>,
}

/// The events of a [`TabBar`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum TabBarEvent {
    /// The user chose a tab: its index. The bar shows the choice at once,
    /// as the system's does; the app shows the tab's content and sets
    /// `selectedIndex`.
    #[args(index)]
    Change(u32),
}

/// The system's tab bar: `UITabBar` on iOS (Liquid Glass on iOS 26), a
/// Material 3 navigation bar on Android, each in the system's look. The app
/// gives the tabs (`items`), says which is selected (`selectedIndex`), hears
/// the user's choice (`onChange`), places the bar with its `class` and
/// `style` and shows the selected tab's content; the bar's height is the
/// system's where the style leaves it open. The selected tab takes the
/// theme's `primary` (the other tabs keep the system's). The desktops and
/// the web have no system tab bar: there it takes no room and shows
/// nothing, and the app shows its tabs with `nsh/base`'s `Tabs`.
#[component]
pub fn TabBar(props: TabBarProps, cx: Scope<TabBarEvent>) -> impl IntoElement {
    if !crate::element::has_tab_bar() {
        return view().hidden().into_element();
    }
    let items = props.items;
    native(TAB_BAR_COMPONENT)
        .prop("items", move || items.with(|items| tabs_json(items)))
        .prop("selectedIndex", move || {
            JsonValue::from(props.selected_index.get())
        })
        .prop("hidden", move || JsonValue::Bool(props.hidden.get()))
        // One color, for the appearance shown: sent again when either changes.
        .prop("tint", || {
            tokens()
                .color("primary")
                .map_or(JsonValue::Null, |primary| color(primary).to_hex().into())
        })
        .on_native_event(move |event: &NativeEvent| {
            if &*event.name == "change"
                && let Some(index) = event.data.as_u64()
            {
                cx.emit(TabBarEvent::Change(index as u32));
            }
        })
        .into_element()
}

/// The tabs as the platforms read them: `[{ title, icon?, badge? }]`.
fn tabs_json(items: &[TabBarItem]) -> JsonValue {
    items
        .iter()
        .map(|item| {
            let mut tab = serde_json::Map::new();
            tab.insert("title".into(), item.title.as_ref().into());
            if let Some(icon) = &item.icon {
                tab.insert("icon".into(), icon.as_ref().into());
            }
            if let Some(badge) = &item.badge {
                tab.insert("badge".into(), badge.as_ref().into());
            }
            JsonValue::Object(tab)
        })
        .collect()
}

/// One segment of a [`SegmentedControl`].
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub struct SegmentedControlItem {
    /// Its title: what the segment shows and what VoiceOver says (with an
    /// icon, only what VoiceOver says).
    pub title: SharedString,
    /// Its icon, an SF Symbol's name (`"sun.max"`), shown in place of the
    /// title.
    #[prop(skip_none)]
    pub icon: Option<SharedString>,
    /// The user cannot choose it (default false): the system dims it.
    #[prop(default)]
    pub disabled: bool,
}

impl SegmentedControlItem {
    /// A segment titled `title`, with no icon.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    /// Its icon, an SF Symbol's name ([`SegmentedControlItem::icon`]).
    pub fn icon(mut self, name: impl Into<SharedString>) -> Self {
        self.icon = Some(name.into());
        self
    }

    /// Whether the user cannot choose it
    /// ([`SegmentedControlItem::disabled`]).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// The props of [`SegmentedControl`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SegmentedControlProps {
    /// The segments, in order.
    #[prop(default)]
    pub items: Prop<Vec<SegmentedControlItem>>,
    /// The selected segment, by index (default 0; one past the segments
    /// selects none). The app owns it: the user's choice is reported with
    /// `onChange`.
    #[prop(default)]
    pub selected_index: Prop<u32>,
    /// No segment can be chosen (default false): the system dims the
    /// control and VoiceOver says it is dimmed.
    #[prop(default)]
    pub disabled: Prop<bool>,
}

/// The events of a [`SegmentedControl`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum SegmentedControlEvent {
    /// The user chose another segment: its index. The control shows the
    /// choice at once, as the system's does; the app sets
    /// `selectedIndex`.
    #[args(index)]
    Change(u32),
}

/// The system's segmented control: a `UISegmentedControl` on iOS (Liquid
/// Glass on iOS 26 and later, the system's earlier look before), in the
/// system's look, behavior, VoiceOver and keyboard focus. The app gives
/// the segments (`items`), says which is selected (`selectedIndex`), hears
/// the user's choice (`onChange`) and places the control with its `class`
/// and `style`; its size is the system's where the style leaves it open (a
/// width the style gives is shared by the segments). New `items` change
/// the segments in place, and the selection follows `selectedIndex`. Other
/// platforms have no such control: there it takes no room and shows
/// nothing, and the app shows its choice with `nsh/base`'s `ToggleGroup` or
/// `RadioGroup`.
#[component]
pub fn SegmentedControl(
    props: SegmentedControlProps,
    cx: Scope<SegmentedControlEvent>,
) -> impl IntoElement {
    if os() != Os::Ios {
        return view().hidden().into_element();
    }
    let items = props.items;
    native(SEGMENTED_CONTROL_COMPONENT)
        .prop("items", move || items.with(|items| segments_json(items)))
        .prop("selectedIndex", move || {
            JsonValue::from(props.selected_index.get())
        })
        .prop("disabled", move || JsonValue::Bool(props.disabled.get()))
        .on_native_event(move |event: &NativeEvent| {
            if &*event.name == "change"
                && let Some(index) = event.data.as_u64()
            {
                cx.emit(SegmentedControlEvent::Change(index as u32));
            }
        })
        .into_element()
}

/// The segments as the platform reads them: `[{ title, icon?, disabled? }]`.
fn segments_json(items: &[SegmentedControlItem]) -> JsonValue {
    items
        .iter()
        .map(|item| {
            let mut segment = serde_json::Map::new();
            segment.insert("title".into(), item.title.as_ref().into());
            if let Some(icon) = &item.icon {
                segment.insert("icon".into(), icon.as_ref().into());
            }
            if item.disabled {
                segment.insert("disabled".into(), true.into());
            }
            JsonValue::Object(segment)
        })
        .collect()
}

/// The props of [`Switch`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SwitchProps {
    /// On or off (default off). The app owns the value: the user's flip is
    /// reported with `onChange`, and the switch shows the value the app
    /// passes back.
    #[prop(default)]
    pub checked: Prop<bool>,
    /// It cannot be flipped (default false): the system dims it and screen
    /// readers say it is dimmed.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// What screen readers say with the switch's state: the setting it
    /// turns on (`"Notifications"`).
    pub accessibility_label: Prop<Option<SharedString>>,
}

/// The events of a [`Switch`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum SwitchEvent {
    /// The user flipped the switch: the value it asks for. The switch moves
    /// at once, as the system's does; the app sets `checked`, and a value
    /// the app keeps moves it back.
    #[args(checked)]
    Change(bool),
}

/// The system's switch, in the system's look, motion, haptics and
/// accessibility: a `UISwitch` on iOS (Liquid Glass on iOS 26 and later),
/// Material 3's switch on Android. The app says whether it is on
/// (`checked`), hears the user's flip (`onChange`), names it for screen
/// readers (`accessibilityLabel`) and places it with its `class` and
/// `style`; its size is the system's where the style leaves it open. It
/// is on in the theme's `primary` (the system's color without one). The
/// desktops and the web have no such control: there it takes no room and
/// shows nothing, and the app shows the setting with `nsh/base`'s
/// `Switch`.
#[component]
pub fn Switch(props: SwitchProps, cx: Scope<SwitchEvent>) -> impl IntoElement {
    if !matches!(os(), Os::Ios | Os::Android) {
        return view().hidden().into_element();
    }
    // The control moves itself under the finger. Each flip bumps
    // `revision` after the app has heard it, and the switch then shows
    // `checked` again: a value the app keeps (it turned the flip down)
    // moves it back, as an unchanged `checked` is not sent again.
    let flips = signal(0u32);
    native(SWITCH_COMPONENT)
        .prop("checked", move || JsonValue::Bool(props.checked.get()))
        .prop("revision", move || JsonValue::from(flips.get()))
        .prop("disabled", move || JsonValue::Bool(props.disabled.get()))
        .prop("accessibilityLabel", move || {
            props
                .accessibility_label
                .get()
                .map_or(JsonValue::Null, |label| label.as_ref().into())
        })
        // One color, for the appearance shown: sent again when either changes.
        .prop("tint", || {
            tokens()
                .color("primary")
                .map_or(JsonValue::Null, |primary| color(primary).to_hex().into())
        })
        .on_native_event(move |event: &NativeEvent| {
            if &*event.name == "change"
                && let Some(checked) = event.data.as_bool()
            {
                cx.emit(SwitchEvent::Change(checked));
                flips.update(|n| *n = n.wrapping_add(1));
            }
        })
        .into_element()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{SegmentedControlItem, TabBarItem, segments_json, tabs_json};

    #[test]
    fn segments_reach_the_platform_as_titles_icons_and_disabled() {
        let segments = [
            SegmentedControlItem::new("Light").icon("sun.max"),
            SegmentedControlItem::new("Dark").disabled(true),
        ];
        assert_eq!(
            segments_json(&segments),
            json!([
                { "title": "Light", "icon": "sun.max" },
                { "title": "Dark", "disabled": true },
            ])
        );
    }

    #[test]
    fn tabs_reach_the_platform_as_titles_and_icons() {
        let tabs = [
            TabBarItem::new("Records").icon("list.bullet"),
            TabBarItem::new("Stats"),
        ];
        assert_eq!(
            tabs_json(&tabs),
            json!([{ "title": "Records", "icon": "list.bullet" }, { "title": "Stats" }])
        );
    }

    #[test]
    fn badges_reach_the_platform_as_texts_and_empty_ones() {
        let tabs = [
            TabBarItem::new("Inbox").badge("3"),
            TabBarItem::new("Brain").badge(""),
            TabBarItem::new("Stats"),
        ];
        assert_eq!(
            tabs_json(&tabs),
            json!([
                { "title": "Inbox", "badge": "3" },
                { "title": "Brain", "badge": "" },
                { "title": "Stats" },
            ])
        );
    }
}
