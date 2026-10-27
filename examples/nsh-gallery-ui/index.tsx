import type { JSX, ViewStyle } from "nsh";
import { effect, Text, View } from "nsh";
import { reduceMotion } from "nsh";
import { createAnimatedValue } from "nsh";
import { platform } from "nsh";
import { keymap } from "nsh";
import { Button as BaseButton, Sheet as BaseSheet, Switch, Tooltip } from "nsh/base";
import { safeArea } from "nsh";
import { Icon, type IconName } from "./icons";
import { alpha, colors, finish, focusRing, hovered, isDark, mono, motion, radius, toneColor, toneTint, type Tone } from "./theme";

export * from "./theme";
export { Segmented } from "./segmented";
export * from "./data-table";
export * from "./slider";

/**
 * The thumb of a resize handle (`handleThumbClass`), as gpui-component
 * draws it: none at rest, so the hairlines stay quiet; a faint pill
 * grows in under the pointer, and a long solid one while pressed or
 * dragged.
 */
export const handleThumb =
  "h-11 w-[3px] rounded-full bg-muted-foreground opacity-0 scale-y-0 transition duration-150 ease-out group-hover:opacity-40 group-hover:scale-y-[0.45] group-active:opacity-90 group-active:scale-y-100";

/**
 * Instructions for the device's input: `touch` on phones and tablets,
 * `pointer` on desktops (a mouse, a touchpad, a keyboard).
 */
export const hint = (touch: string, pointer: string) => (platform.idiom === "desktop" ? pointer : touch);

/**
 * Desktops' way to refresh what phones pull down (the mouse does not drag
 * scroll views): Ctrl+R / Cmd+R calls `refresh` while the calling component
 * shows, next to the refresh button the demo shows. Browsers keep the key to
 * reload the page, and the development hosts to reload the app. Returns the
 * key's label for the hint, or null where it is not bound.
 */
export function bindRefresh(refresh: () => void): string | null {
  if (platform.idiom !== "desktop" || platform.os === "web") return null;
  keymap.bind("mod+r", refresh);
  return keymap.label("mod+r");
}

/**
 * A pressable that sinks a little under the finger and springs back (the
 * spring runs natively; only press in / out reach JS).
 */
export function Press(props: {
  onPress?: () => void;
  onLongPress?: () => void;
  disabled?: boolean;
  /** How far it shrinks (default 0.97). */
  scale?: number;
  style?: ViewStyle;
  role?: "button" | "tab" | "switch" | "link";
  accessibilityLabel?: string;
  accessibilityState?: { selected?: boolean; checked?: boolean; disabled?: boolean };
  hitSlop?: number;
  children: JSX.Element;
}) {
  const s = createAnimatedValue(1);
  return (
    <View
      onPress={props.onPress}
      onLongPress={props.onLongPress}
      disabled={props.disabled}
      onPressDown={() => s.spring(props.scale ?? 0.97, motion.press)}
      onPressUp={() => s.spring(1, motion.press)}
      role={props.role ?? "button"}
      accessibilityLabel={props.accessibilityLabel}
      accessibilityState={props.accessibilityState}
      hitSlop={props.hitSlop}
      style={[props.style ?? null, { transform: [{ scale: s }], opacity: props.disabled ? 0.4 : 1 }]}
    >
      {props.children}
    </View>
  );
}

/**
 * `Press`'s feedback for a `Tab` of `nsh/base`, which reports no press down
 * to spring with: it sinks a little while pressed and eases back, and takes
 * presses 4 pt around it without taking more room (a hit slop). Spread
 * `style` and `activeStyle` into the tab's.
 */
export const tabPress = {
  style: { margin: -4, padding: 4, transitionProperty: "transform", transitionDuration: 150, transitionTimingFunction: "ease-out" },
  activeStyle: { transform: [{ scale: 0.97 }] },
} satisfies { style: ViewStyle; activeStyle: ViewStyle };

/**
 * Fades and lifts its content in once mounted; `index` staggers siblings
 * (40 ms apart). `flex` lets it wrap a page that fills its parent.
 */
export function Appear(props: { index?: number; flex?: boolean; children: JSX.Element }) {
  const t = createAnimatedValue(0);
  t.timing(1, { ...motion.enter, delay: Math.min(8, props.index ?? 0) * 40 });
  return (
    <View style={{ flex: props.flex ? 1 : undefined, opacity: t, transform: [{ translateY: t.interpolate({ inputRange: [0, 1], outputRange: [14, 0] }) }] }}>
      {props.children}
    </View>
  );
}

/** A rounded icon tile. */
export function IconTile(props: { name: IconName; tint?: string; size?: number }) {
  const size = props.size ?? 40;
  return (
    <View
      style={{
        width: size,
        height: size,
        borderRadius: size * 0.3,
        borderCurve: "continuous",
        borderWidth: 1,
        borderColor: colors.border,
        backgroundColor: colors.raised,
        ...finish.raised,
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <Icon name={props.name} size={size * 0.5} tint={props.tint ?? colors.text} />
    </View>
  );
}

/**
 * A card: an optional icon, a title and a subtitle, something on the right
 * (a badge, an action), then the content on an inset surface (or bare with
 * `flush`).
 */
export function Card(props: {
  icon?: IconName;
  title?: string;
  subtitle?: JSX.Element;
  right?: JSX.Element;
  flush?: boolean;
  style?: ViewStyle;
  children: JSX.Element;
}) {
  return (
    <View
      style={[
        {
          backgroundColor: colors.surface,
          borderRadius: radius.card,
          borderCurve: "continuous",
          borderWidth: 1,
          borderColor: colors.border,
          padding: 5,
          ...finish.card,
        },
        props.style ?? null,
      ]}
    >
      <If when={props.title}>
        <View style={{ flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 11, paddingTop: 11, paddingBottom: 12 }}>
          <If when={props.icon}>
            <IconTile name={props.icon!} />
          </If>
          <View style={{ flex: 1, gap: 2 }}>
            <Text style={{ fontSize: 17, fontWeight: "600", color: colors.text }}>{props.title}</Text>
            <If when={props.subtitle}>
              <Text style={{ numberOfLines: 2, fontSize: 13, color: colors.muted }}>
                {props.subtitle}
              </Text>
            </If>
          </View>
          {props.right}
        </View>
      </If>
      <If when={!props.flush} else={props.children}>
        <Inset>{props.children}</Inset>
      </If>
    </View>
  );
}

/** The darker surface inside a card. */
export function Inset(props: { gap?: number; padding?: number; children: JSX.Element }) {
  return (
    <View
      style={{
        backgroundColor: colors.inset,
        borderRadius: radius.inset,
        borderCurve: "continuous",
        borderWidth: 1,
        borderColor: colors.line,
        ...finish.inset,
        padding: props.padding ?? 14,
        gap: props.gap ?? 12,
      }}
    >
      {props.children}
    </View>
  );
}

/** A caption row: a label on the left, a quieter note on the right. */
export function Caption(props: { label: JSX.Element; note?: JSX.Element }) {
  return (
    <View style={{ flexDirection: "row", alignItems: "baseline", gap: 8 }}>
      <Text style={{ flex: 1, fontSize: 14, color: colors.muted }}>{props.label}</Text>
      <If when={props.note}>
        <Text style={{ fontSize: 13, color: colors.faint, fontVariant: ["tabular-nums"] }}>{props.note}</Text>
      </If>
    </View>
  );
}

/** A large figure with a tinted note after it ("38 / 1k  would pass"). */
export function Figure(props: { value: string; unit?: string; note?: string; tone?: Tone }) {
  return (
    <Text style={{ fontSize: 32, fontWeight: "600", color: colors.text, fontVariant: ["tabular-nums"], letterSpacing: -0.5 }}>
      {props.value}
      <If when={props.unit}>
        <Text style={{ fontSize: 17, fontWeight: "400", color: colors.muted }}> {props.unit}</Text>
      </If>
      <If when={props.note}>
        <Text style={{ fontSize: 15, fontWeight: "500", color: toneColor(props.tone ?? "accent") }}> {props.note}</Text>
      </If>
    </Text>
  );
}

/** A small label in a tinted capsule. */
export function Badge(props: { label: string; tone?: Tone; outline?: boolean }) {
  const tone = () => props.tone ?? "neutral";
  return (
    <View
      style={{
        paddingHorizontal: 10,
        height: 26,
        borderRadius: 13,
        justifyContent: "center",
        backgroundColor: props.outline ? "transparent" : toneTint(tone()),
        boxShadow: props.outline ? undefined : finish.chip.boxShadow,
        borderWidth: 1,
        borderColor: props.outline ? toneColor(tone()) : alpha(tone() === "neutral" ? colors.text : toneColor(tone()), 0.1),
      }}
    >
      <Text style={{ fontSize: 12, fontWeight: "500", color: tone() === "neutral" ? colors.text : toneColor(tone()) }}>{props.label}</Text>
    </View>
  );
}

/** A label above a value, for grids of figures. */
export function Stat(props: { label: string; value: string; tone?: Tone; width?: `${number}%` }) {
  return (
    <View style={{ width: props.width, gap: 3, paddingVertical: 4 }}>
      <Text style={{ fontSize: 12, color: colors.muted }}>{props.label}</Text>
      <Text style={{ fontSize: 15, color: props.tone ? toneColor(props.tone) : colors.text, fontVariant: ["tabular-nums"] }}>
        {props.value}
      </Text>
    </View>
  );
}

/** A colored square and a label, for chart legends. */
export function Legend(props: { color: string; label: string; value?: string }) {
  return (
    <View style={{ flexDirection: "row", alignItems: "center", gap: 6 }}>
      <View style={{ width: 9, height: 9, borderRadius: 2.5, backgroundColor: props.color }} />
      <Text style={{ fontSize: 13, color: colors.muted }}>
        {props.label}
        <If when={props.value}>
          <Text style={{ color: colors.text, fontVariant: ["tabular-nums"] }}> {props.value}</Text>
        </If>
      </Text>
    </View>
  );
}

/** A row inside an inset, with a hairline above all but the first. */
export function Row(props: { first?: boolean; onPress?: () => void; children: JSX.Element }) {
  return (
    <View
      onPress={props.onPress}
      activeStyle={props.onPress ? { backgroundColor: colors.raised } : undefined}
      style={{
        minHeight: 48,
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        borderTopWidth: props.first ? 0 : 1,
        borderTopColor: colors.line,
      }}
    >
      {props.children}
    </View>
  );
}

export type ButtonVariant = "primary" | "secondary" | "accent" | "ghost";

/** A pointer and a keyboard: the compact sizes of desktops. */
const pointer = platform.idiom === "desktop";

/**
 * A button: 28 pt tall on a desktop, 40 on a touch screen (`small`: 36).
 * `primary` and `accent` are the citrus fill (the main action), `secondary`
 * a raised neutral, `ghost` bare until the pointer is over it. A `color`
 * fills it with that color instead. Under the pointer a raised button
 * lightens, pressed it sinks (an inner shade, and 3% smaller).
 */
export function Button(props: {
  label: string;
  variant?: ButtonVariant;
  color?: string;
  icon?: IconName;
  disabled?: boolean;
  /** Grows to share a row with its siblings. */
  flex?: number;
  size?: "small" | "regular";
  onPress: () => void;
}) {
  const small = () => props.size === "small";
  // `nsh/base`'s Button gives the behavior (presses, disabled, the button
  // role); the look below is the gallery's own.
  const scale = createAnimatedValue(1);
  const variant = () => props.variant ?? "primary";
  const filled = () => !props.color && (variant() === "primary" || variant() === "accent");
  const height = () => (pointer ? 28 : small() ? 36 : 40);
  const fg = () => (props.color ? "#ffffff" : filled() ? colors.onAccent : colors.text);
  const look = (): ViewStyle =>
    props.color
      ? { backgroundColor: props.color, borderColor: "transparent" }
      : variant() === "ghost"
        ? { backgroundColor: "transparent", borderColor: "transparent" }
        : filled()
          ? finish.accent
          : finish.raised;
  return (
    <BaseButton
      onPress={props.onPress}
      onPressDown={() => scale.spring(0.97, motion.press)}
      onPressUp={() => scale.spring(1, motion.press)}
      disabled={props.disabled}
      accessibilityLabel={props.label}
      hoverStyle={props.disabled ? undefined : props.color || filled() ? { opacity: 0.94 } : variant() === "ghost" ? hovered() : finish.hover}
      activeStyle={props.disabled ? undefined : props.color || filled() ? { opacity: 0.88 } : finish.pressed}
      focusStyle={focusRing()}
      style={{
        flex: props.flex,
        height: height(),
        paddingHorizontal: pointer ? 12 : 16,
        borderRadius: radius.control,
        borderCurve: "continuous",
        borderWidth: 1,
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: pointer ? 6 : 8,
        ...look(),
        opacity: props.disabled ? 0.4 : 1,
        transform: [{ scale }],
        transitionProperty: "background-color, border-color, box-shadow, opacity",
        transitionDuration: 120,
      }}
    >
      <If when={props.icon}>
        <Icon name={props.icon!} size={pointer ? 14 : 16} tint={fg()} />
      </If>
      <Text style={{ fontSize: pointer ? 13 : 15, fontWeight: "500", color: fg() }}>{props.label}</Text>
    </BaseButton>
  );
}

export function RoundButton(props: {
  icon: IconName;
  label: string;
  variant?: "primary" | "secondary";
  size?: number;
  disabled?: boolean;
  onPress: () => void;
}) {
  const size = props.size ?? (pointer ? 28 : 40);
  const primary = () => (props.variant ?? "primary") === "primary";
  return (
    <Press onPress={props.onPress} disabled={props.disabled} scale={0.92} accessibilityLabel={props.label}>
      <View
        style={{
          width: size,
          height: size,
          borderRadius: size / 2,
          alignItems: "center",
          justifyContent: "center",
          borderWidth: 1,
          ...(primary() ? finish.accent : finish.raised),
        }}
      >
        <Icon name={props.icon} size={size * 0.42} tint={primary() ? colors.onLight : colors.text} />
      </View>
    </Press>
  );
}

/** A titled card of controls (the demos' sections). */
export function Section(props: { title: string; note?: string; style?: ViewStyle; children: JSX.Element }) {
  return (
    <View
      style={[
        {
          backgroundColor: colors.surface,
          borderRadius: radius.card,
          borderCurve: "continuous",
          borderWidth: 1,
          borderColor: colors.border,
          padding: 5,
          ...finish.card,
        },
        props.style ?? null,
      ]}
    >
      <View style={{ flexDirection: "row", alignItems: "baseline", paddingHorizontal: 11, paddingTop: 10, paddingBottom: 10, gap: 8 }}>
        <Text style={{ flex: 1, fontSize: 15, fontWeight: "600", color: colors.text }}>{props.title}</Text>
        <If when={props.note}>
          <Text style={{ fontSize: 13, color: colors.faint, fontVariant: ["tabular-nums"] }}>{props.note}</Text>
        </If>
      </View>
      <Inset>{props.children}</Inset>
    </View>
  );
}

/**
 * An on / off switch (`nsh/base`'s `Switch`, in the gallery's look): the
 * knob springs across, the track changes color natively.
 */
export function Toggle(props: { value: boolean; onChange: (value: boolean) => void; label?: string }) {
  const look = switchLook(() => props.value);
  return (
    <Switch
      checked={props.value}
      onChange={props.onChange}
      accessibilityLabel={props.label}
      // 8 pt more to press on every side (a hit slop around the small
      // track), without taking more room.
      style={{ margin: -8, padding: 8 }}
      trackStyle={look.trackStyle}
      thumbStyle={look.thumbStyle}
    />
  );
}

/**
 * The gallery's switch, as the styles of `Switch`'s parts: a track sunk
 * into the surface that turns citrus, and a knob that springs across, both
 * natively. The knob keeps its size on and off; on, it is white with an
 * olive edge. `on` is the value the app owns: the knob follows it, from a
 * press or from elsewhere. 28 × 16 on a desktop; on a touch screen 42 × 24
 * (`small`: 34 × 20). Read the styles in JSX, so the appearance is followed.
 */
export function switchLook(on: () => boolean, size: "small" | "regular" = "regular"): { readonly trackStyle: ViewStyle; readonly thumbStyle: ViewStyle } {
  // The track's size and inset, and the knob: it travels what is left inside
  // the border and the padding.
  const [width, height, inset, knob] = pointer ? [28, 16, 1, 12] : size === "small" ? [34, 20, 1, 16] : [42, 24, 1, 20];
  const t = createAnimatedValue(on() ? 1 : 0);
  const x = t.interpolate({ inputRange: [0, 1], outputRange: [0, width - 2 - 2 * inset - knob] });
  let shown = on();
  effect(() => {
    const next = on();
    if (next === shown) return;
    shown = next;
    // Reduced motion: the knob and the track change in place.
    if (reduceMotion()) t.set(next ? 1 : 0);
    else t.spring(next ? 1 : 0, motion.settle);
  });
  return {
    get trackStyle(): ViewStyle {
      return {
        width,
        height,
        borderRadius: height / 2,
        padding: inset,
        backgroundColor: t.interpolate({ inputRange: [0, 1], outputRange: [colors.track, colors.accentFill] }),
        borderWidth: 1,
        borderColor: on() ? finish.fill.borderColor : finish.track.borderColor,
        boxShadow: on() ? finish.fill.boxShadow : finish.track.boxShadow,
        transitionProperty: "border-color, box-shadow",
        transitionDuration: 150,
      };
    },
    get thumbStyle(): ViewStyle {
      const edge = on() ? (isDark() ? "#8db536" : "#8fb536") : undefined;
      return {
        width: knob,
        height: knob,
        borderRadius: knob / 2,
        borderWidth: 1,
        ...finish.knob,
        ...(on()
          ? {
              backgroundColor: "#ffffff",
              backgroundImage: "linear-gradient(180deg, #ffffff 0%, #f7f9f2 100%)",
              borderColor: edge,
              boxShadow: "0 1px 0 rgba(60,90,0,0.18)",
            }
          : {}),
        transform: [{ translateX: x }],
      };
    },
  };
}

/** A horizontal meter with an optional marker (a limit). */
export function Meter(props: { value: number; tone?: Tone; marker?: number }) {
  return (
    <View style={{ height: 6, borderRadius: 3, backgroundColor: colors.track, borderWidth: 1, ...finish.track }}>
      <View
        style={{
          width: `${Math.max(0, Math.min(1, props.value)) * 100}%`,
          height: 4,
          borderRadius: 2,
          backgroundColor: props.tone && props.tone !== "accent" ? toneColor(props.tone) : colors.accentFill,
        }}
      />
      <If when={props.marker !== undefined}>
        <View style={{ position: "absolute", left: `${(props.marker ?? 0) * 100}%`, top: -4, width: 2, height: 12, borderRadius: 1, backgroundColor: colors.text }} />
      </If>
    </View>
  );
}

/**
 * A panel at the bottom over a dimmed backdrop while `open` (`nsh/base`'s
 * `Sheet`, in the gallery's look); a press on the backdrop, Escape or the
 * back button closes it (`onClose`: set `open` to false).
 */
export function Sheet(props: { open: boolean; onClose: () => void; children: JSX.Element }) {
  return (
    <BaseSheet
      open={props.open}
      onOpenChange={(open) => !open && props.onClose()}
      backdropStyle={{ backgroundColor: "rgba(0,0,0,0.4)" }}
      popupStyle={{
        gap: 12,
        paddingTop: 8,
        paddingHorizontal: 16,
        paddingBottom: safeArea().bottom + 16,
        borderTopLeftRadius: 16,
        borderTopRightRadius: 16,
        backgroundColor: colors.surface,
      }}
    >
      <View style={{ width: 36, height: 5, borderRadius: 3, alignSelf: "center", backgroundColor: colors.raised }} />
      {props.children}
    </BaseSheet>
  );
}

export { Icon, type IconName } from "./icons";

/** A short text as an element's `tooltip`: a `Tooltip` of `nsh/base` in a
 * small bubble of the theme's `popover` tokens. */
export function Tip(props: { label: string }) {
  return (
    <Tooltip class="rounded-md border border-border bg-popover px-2 py-1">
      <Text class="text-xs text-popover-foreground">{props.label}</Text>
    </Tooltip>
  );
}
