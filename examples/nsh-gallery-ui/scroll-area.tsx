import type { JSX, ScrollProps, ViewStyle, YNode } from "nsh";
import { platform, Scroll, View } from "nsh";
import { Scrollbar, type ScrollbarMode, type ScrollbarMotion } from "nsh/base";

const touch = platform.idiom !== "desktop";

/**
 * The track of the gallery's scroll bar (`<Scrollbar class>`), as
 * gpui-component draws it: over the scroll view's right edge, 16 points
 * wide (the widest thumb and an inset of 4 on each side), the thumb at its
 * right; 2 points of inset on a touch screen.
 */
export const scrollTrack = touch ? "absolute inset-y-0 right-0 w-4 items-end p-0.5" : "absolute inset-y-0 right-0 w-4 items-end p-1";

/**
 * The thumb of the gallery's scroll bar (`<Scrollbar thumbClass>`), as
 * gpui-component draws it: 6 points at rest (3, the system's indicator, on a
 * touch screen), 8 and darker under the pointer or while dragged, its width
 * easing over 300 ms.
 */
export const scrollThumb = `${touch ? "w-[3px]" : "w-1.5"} rounded-full bg-muted-foreground opacity-50 transition-all duration-300 hover:w-2 hover:opacity-80 active:w-2 active:opacity-80 motion-reduce:transition-none`;

/**
 * The gallery's scroll bar motion, gpui-component's: it fades in over
 * 300 ms and out over 500 ms, 2 s after the last scroll, drag or hover; in
 * the `hover` mode, pointing at its hidden thumb slides it in from the edge.
 */
export const scrollMotion = (mode: ScrollbarMode = "scrolling"): ScrollbarMotion => ({
  idle: 2000,
  enter: 300,
  exit: 500,
  entrance: "fade",
  thumbHoverEntrance: mode === "hover" ? "slideAndFade" : "fade",
});

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
      <Scrollbar scroll={scroll} mode="hover" motion={scrollMotion("hover")} accessibilityLabel={props.label} class={scrollTrack} thumbClass={scrollThumb} />
    </View>
  );
}
