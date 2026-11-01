import type { JSX, ViewStyle } from "nsh";
import { effect, Text, View } from "nsh";
import { reduceMotion } from "nsh";
import { createAnimatedValue } from "nsh";
import { platform } from "nsh";
import { keymap } from "nsh";
import { Button as BaseButton, Sheet as BaseSheet, Switch, Tooltip } from "nsh/base";
import { safeArea } from "nsh";
import { Icon, type IconName } from "./icons";
import { alpha, colors, desktopIdiom, finish, focusRing, isDark, mono, motion, radius, TERMINAL_COLUMN, TERMINAL_ROW, terminal, toneColor, toneTint, type Tone, wholeRows } from "./theme";

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
export const hint = (touch: string, pointer: string) => (desktopIdiom ? pointer : touch);

/**
 * Desktops' way to refresh what phones pull down (the mouse does not drag
 * scroll views): Ctrl+R / Cmd+R calls `refresh` while the calling component
 * shows, next to the refresh button the demo shows. Browsers keep the key to
 * reload the page, and the development hosts to reload the app. Returns the
 * key's label for the hint, or null where it is not bound.
 */
export function bindRefresh(refresh: () => void): string | null {
  if (!desktopIdiom || platform.os === "web") return null;
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
      focusStyle={focusRing()}
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

/** A rounded icon tile (decoration: none in the terminal). */
export function IconTile(props: { name: IconName; tint?: string; size?: number }) {
  if (terminal) return null;
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
 * `flush`). The terminal draws one box, a blank row under the title and the
 * content right in the card.
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
    <View style={[cardStyle(), props.style ?? null]}>
      <If when={props.title}>
        <View style={terminal ? { flexDirection: "row", gap: 2 * TERMINAL_COLUMN, paddingBottom: TERMINAL_ROW } : { flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 11, paddingTop: 11, paddingBottom: 12 }}>
          <If when={props.icon}>
            <IconTile name={props.icon!} />
          </If>
          <View style={{ flex: 1, gap: terminal ? 0 : 2 }}>
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

/**
 * A card's box: in the terminal a border whose row and column on each side
 * the padding fills out, so the content starts one Cell inside it.
 */
const cardStyle = (): ViewStyle =>
  terminal
    ? { borderWidth: 1, borderColor: colors.border, paddingVertical: TERMINAL_ROW - 1, paddingHorizontal: 2 * TERMINAL_COLUMN - 1 }
    : {
        backgroundColor: colors.surface,
        borderRadius: radius.card,
        borderCurve: "continuous",
        borderWidth: 1,
        borderColor: colors.border,
        padding: 5,
        ...finish.card,
      };

/** The darker surface inside a card (only its gap, in whole rows, in the terminal). */
export function Inset(props: { gap?: number; padding?: number; children: JSX.Element }) {
  if (terminal) return <View style={{ gap: wholeRows(props.gap ?? 12) }}>{props.children}</View>;
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

/** A small label in a tinted capsule (a word on its row in the terminal). */
export function Badge(props: { label: string; tone?: Tone; outline?: boolean }) {
  const tone = () => props.tone ?? "neutral";
  return (
    <View
      style={{
        paddingHorizontal: terminal ? 0 : 10,
        height: terminal ? TERMINAL_ROW : 26,
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

/** A label above a value, for grids of figures; on one row in the terminal,
 * the value right-aligned. */
export function Stat(props: { label: string; value: string; tone?: Tone; width?: `${number}%` }) {
  return (
    <View
      style={
        terminal
          ? { width: props.width, height: TERMINAL_ROW, flexDirection: "row", justifyContent: "space-between", gap: TERMINAL_COLUMN, paddingRight: 2 * TERMINAL_COLUMN }
          : { width: props.width, gap: 3, paddingVertical: 4 }
      }
    >
      <Text style={{ fontSize: 12, color: colors.muted }}>{props.label}</Text>
      <Text style={{ fontSize: 15, color: props.tone ? toneColor(props.tone) : colors.text, fontVariant: ["tabular-nums"] }}>
        {props.value}
      </Text>
    </View>
  );
}

/** A colored square and a label, for chart legends (the label alone in the
 * terminal, which draws no charts). */
export function Legend(props: { color: string; label: string; value?: string }) {
  return (
    <View style={{ flexDirection: "row", alignItems: "center", gap: 6 }}>
      <If when={!terminal}>
        <View style={{ width: 9, height: 9, borderRadius: 2.5, backgroundColor: props.color }} />
      </If>
      <Text style={{ fontSize: 13, color: colors.muted }}>
        {props.label}
        <If when={props.value}>
          <Text style={{ color: colors.text, fontVariant: ["tabular-nums"] }}> {props.value}</Text>
        </If>
      </Text>
    </View>
  );
}

/** A row inside an inset, with a hairline above all but the first (one row
 * of the terminal, which keeps no hairlines between rows). */
export function Row(props: { first?: boolean; onPress?: () => void; children: JSX.Element }) {
  return (
    <View
      onPress={props.onPress}
      activeStyle={props.onPress ? { backgroundColor: colors.raised } : undefined}
      style={{
        minHeight: terminal ? TERMINAL_ROW : 48,
        flexDirection: "row",
        alignItems: "center",
        gap: terminal ? 2 * TERMINAL_COLUMN : 12,
        borderTopWidth: props.first || terminal ? 0 : 1,
        borderTopColor: colors.line,
      }}
    >
      {props.children}
    </View>
  );
}

export type ButtonVariant = "primary" | "secondary" | "accent" | "ghost";

/**
 * A button, 44 pt tall (32 pt `small`, for pointer-sized surfaces; one row,
 * its label and no icon in the terminal, which brackets it):
 * `primary` is light (the main action), `secondary` dark, `accent` citrus
 * (the theme's `brand-fill`). A `color` fills it with that color instead.
 */
export function Button(props: {
  label: string;
  /** What screen readers say, when the label is shorter (default `label`). */
  accessibilityLabel?: string;
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
  const bg = () =>
    props.color ?? (variant() === "primary" ? colors.light : variant() === "accent" ? colors.accentFill : variant() === "ghost" ? "transparent" : colors.raised);
  const fg = () => (props.color ? "#ffffff" : variant() === "primary" ? colors.onLight : variant() === "accent" ? colors.onAccent : colors.text);
  return (
    <BaseButton
      onPress={props.onPress}
      onPressDown={() => scale.spring(0.97, motion.press)}
      onPressUp={() => scale.spring(1, motion.press)}
      disabled={props.disabled}
      accessibilityLabel={props.accessibilityLabel ?? props.label}
      hoverStyle={props.disabled ? undefined : { opacity: 0.88 }}
      focusStyle={focusRing(2)}
      style={{ flex: props.flex, opacity: props.disabled ? 0.4 : 1, transform: [{ scale }], borderRadius: small() ? 8 : radius.control }}
    >
      <View
        style={{
          height: terminal ? TERMINAL_ROW : small() ? 32 : 44,
          paddingHorizontal: terminal ? TERMINAL_COLUMN : small() ? 12 : 18,
          borderRadius: small() ? 8 : radius.control,
          borderCurve: "continuous",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "center",
          gap: small() ? 6 : 8,
          backgroundColor: bg(),
          ...(props.color || variant() === "ghost"
            ? {}
            : variant() === "primary"
              ? finish.light
              : variant() === "accent"
                ? finish.accent
                : finish.raised),
          borderWidth: variant() === "secondary" || variant() === "ghost" ? 1 : 0,
          borderColor: colors.border,
        }}
      >
        <If when={props.icon && !terminal}>
          <Icon name={props.icon!} size={small() ? 14 : 16} tint={fg()} />
        </If>
        <Text style={{ fontSize: small() ? 13 : 15, fontWeight: "600", color: fg() }}>{props.label}</Text>
      </View>
    </BaseButton>
  );
}

/**
 * A round icon button; in the terminal a text button reading `title` (a
 * word or two), else `label`.
 */
export function RoundButton(props: {
  icon: IconName;
  label: string;
  /** The terminal's text in place of the icon (default `label`). */
  title?: string;
  variant?: "primary" | "secondary";
  size?: number;
  disabled?: boolean;
  onPress: () => void;
}) {
  if (terminal) {
    return (
      <Button
        label={props.title ?? props.label}
        accessibilityLabel={props.label}
        variant={props.variant ?? "primary"}
        disabled={props.disabled}
        onPress={props.onPress}
      />
    );
  }
  const size = props.size ?? 44;
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
          backgroundColor: primary() ? colors.light : colors.raised,
          ...(primary() ? finish.light : finish.pill),
          borderWidth: primary() ? 0 : 1,
          borderColor: primary() ? colors.border : finish.pill.borderColor,
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
    <View style={[cardStyle(), props.style ?? null]}>
      <View style={terminal ? { flexDirection: "row", gap: 2 * TERMINAL_COLUMN, paddingBottom: TERMINAL_ROW } : { flexDirection: "row", alignItems: "baseline", paddingHorizontal: 11, paddingTop: 10, paddingBottom: 10, gap: 8 }}>
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
      // 8 pt more to press on every side (the hit slop of a 30 pt track),
      // without taking more room.
      style={{ margin: -8, padding: 8 }}
      trackStyle={look.trackStyle}
      thumbStyle={look.thumbStyle}
    />
  );
}

/**
 * The gallery's switch, as the styles of `Switch`'s parts: a 50 × 30
 * track that turns citrus and a knob that springs across, both natively.
 * `on` is the value the app owns: the knob follows it, from a press or from
 * elsewhere. `small` is 38 × 22, for pointer-sized rows; the terminal's is
 * one row, as wide as its glyph (`[off]`). Read the styles in
 * JSX, so the appearance is followed.
 */
export function switchLook(on: () => boolean, size: "small" | "regular" = "regular"): { readonly trackStyle: ViewStyle; readonly thumbStyle: ViewStyle } {
  // The track's size and inset, and the knob: it travels what is left inside
  // the border and the padding.
  const [width, height, inset, knob] = terminal ? [5 * TERMINAL_COLUMN, TERMINAL_ROW, 0, TERMINAL_ROW] : size === "small" ? [38, 22, 2, 16] : [50, 30, 3, 22];
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
        borderColor: colors.border,
        ...finish.track,
      };
    },
    get thumbStyle(): ViewStyle {
      return {
        width: knob,
        height: knob,
        borderRadius: knob / 2,
        backgroundColor: on() ? colors.onAccent : colors.knob,
        // A drop of glass: a specular top over the knob's color.
        backgroundImage: "linear-gradient(180deg, rgba(255,255,255,0.202) 0%, rgba(255,255,255,0.098) 60%)",
        boxShadow: isDark() ? "inset 0 1px 0 rgba(255,255,255,0.2), 0 1px 3px rgba(0,0,0,0.4)" : finish.knob.boxShadow,
        transform: [{ translateX: x }],
      };
    },
  };
}

/** A horizontal meter with an optional marker (a limit); half a row in the
 * terminal, so its fill is a solid bar rather than a rule. */
export function Meter(props: { value: number; tone?: Tone; marker?: number }) {
  const height = terminal ? TERMINAL_ROW / 2 : 6;
  return (
    <View style={{ height, borderRadius: 3, backgroundColor: colors.raised }}>
      <View
        style={{
          width: `${Math.max(0, Math.min(1, props.value)) * 100}%`,
          height,
          borderRadius: 3,
          backgroundColor: toneColor(props.tone ?? "accent"),
        }}
      />
      <If when={props.marker !== undefined}>
        <View style={{ position: "absolute", left: `${(props.marker ?? 0) * 100}%`, top: -3, width: 2, height: 12, borderRadius: 1, backgroundColor: colors.text }} />
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
