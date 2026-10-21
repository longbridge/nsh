//! What the demos share (as the TSX gallery's `ui.tsx` and `icons.tsx`):
//! the gallery's look ([`theme`]: colors for both appearances, tones,
//! radii, motion, finishes), its icons, and the building blocks every demo
//! is made of.
//!
//! - Stateful, native motion (components: `X::element(XProps { .. })`,
//!   their events through `.on_event(..)`, or the helper functions):
//!   [`Press`] / [`press`], [`Appear`] / [`appear`], [`Toggle`] /
//!   [`toggle`], [`Segmented`] / [`segmented`], [`Icon`] / [`icon`],
//!   [`Sheet`] (a bottom sheet).
//! - Builders (`Card::new().title(..).child(..)`, styled at their root):
//!   [`Card`], [`Section`], [`Inset`], [`Row`], [`Figure`], [`Badge`],
//!   [`Stat`], [`Meter`], [`IconTile`], [`Button`], [`RoundButton`]; and
//!   [`caption`], [`legend`], [`code`], [`header`].
//! - The device's input: [`hint`] (touch or pointer wording), [`bind_refresh`]
//!   (a desktop's refresh key).
//! - The desktop layout of wide windows: [`SIDEBAR`], [`desktop`],
//!   [`pane_width`], [`columns_for`]; [`DemoPage`], a demo's cards in
//!   columns.

mod animated;
mod controls;
mod device;
mod icons;
mod layout;
mod page;
mod parts;
mod shapes;
mod sheet;
mod theme;

pub(crate) use self::{
    animated::{Appear, AppearProps, Press, PressProps, PressRole, appear, press_with},
    controls::{
        Segment, Segmented, SegmentedEvent, SegmentedProps, SegmentedSize, ToggleEvent, segmented,
        toggle,
    },
    device::{bind_refresh, hint},
    icons::{IconName, Tint, icon},
    layout::{SIDEBAR, columns_for, desktop, pane_width},
    page::DemoPage,
    parts::{
        Badge, Button, ButtonVariant, Card, Figure, IconTile, Inset, Meter, PageHeader,
        RoundButton, Row, Section, Stat, caption, code, header, header_style, legend, titled,
        with_header,
    },
    shapes::{lengths, pt, rect},
    sheet::{Sheet, SheetEvent, SheetProps},
    theme::{Finish, Finished, MONO, Tone, colors, inset_shadow, motion, radius, rgb_a, tints},
};
