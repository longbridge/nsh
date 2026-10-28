import type { JSX, ScrollProps, ViewStyle, YNode } from "nsh";
import { Scroll, View } from "nsh";
import { Scrollbar } from "nsh/base";

/**
 * The track of the gallery's scroll bar (`<Scrollbar class>`): over the
 * scroll view's right edge, 12 points wide, the thumb at its right; it fades
 * in and out.
 */
export const scrollTrack = "absolute inset-y-0 right-0 w-3 items-end px-0.5 py-1 transition-opacity duration-200 motion-reduce:transition-none";

/**
 * The thumb of the gallery's scroll bar (`<Scrollbar thumbClass>`): a quiet
 * 6-point pill that widens and darkens as the pointer comes near, solid
 * while dragged.
 */
export const scrollThumb =
  "w-1.5 rounded-full bg-muted-foreground opacity-40 transition-all duration-150 group-hover:w-2 group-hover:opacity-70 active:w-2 active:opacity-90 motion-reduce:transition-none";

/**
 * A vertical scroll view with the gallery's own scroll bar in place of the
 * platform's indicator: it shows while the content scrolls and while the
 * pointer is over it. `style` is the area's (it fills its parent by
 * default); the other props are the scroll view's.
 */
export function ScrollArea(props: {
  style?: ViewStyle;
  /** What screen readers say of the bar. */
  label?: string;
  keyboardDismissMode?: ScrollProps["keyboardDismissMode"];
  keyboardShouldPersistTaps?: ScrollProps["keyboardShouldPersistTaps"];
  children?: JSX.Element;
}) {
  let scroll: YNode | undefined;
  return (
    <View style={{ flex: 1, ...props.style }}>
      <Scroll
        ref={scroll}
        scrollbar={false}
        style={{ flex: 1 }}
        keyboardDismissMode={props.keyboardDismissMode}
        keyboardShouldPersistTaps={props.keyboardShouldPersistTaps}
      >
        {props.children}
      </Scroll>
      <Scrollbar scroll={scroll} mode="hover" accessibilityLabel={props.label} class={scrollTrack} thumbClass={scrollThumb} />
    </View>
  );
}
