import type { TextStyle, ViewStyle } from "nsh";
import { platform } from "nsh";
import { alpha, colors, finish } from "./theme";

/** Options of `sliderLook`. */
export interface SliderLookOptions {
  /** A vertical slider: the rail 8 wide, the thumb 8 × 16. */
  vertical?: boolean;
  /** The value column after the track (`children`: a 42-point label). */
  value?: boolean;
}

/** The value column's label (`<Text class={sliderValueClass}
 * style={sliderValueStyle()}>`): 42 points, right aligned, monospaced, its
 * line as tall as its type so it centers on the rail. */
export const sliderValueClass = "w-[42px] text-right font-mono text-[11px] leading-none";

/** The value column label's color: the soft text color (60% of the
 * text's). */
export const sliderValueStyle = (): TextStyle => ({ color: alpha(colors.text, 0.6) });

/**
 * A `Slider`'s parts (spread them: `<Slider {...sliderLook()}>`): a 4-point
 * rail sunk into the surface and a round thumb on it, 12 points with a
 * pointer (a 16-point band around the rail takes presses), 18 on a touch
 * screen (a 32-point band). Under the pointer the thumb lightens, the
 * keyboard's focus rings the rail in the theme's `ring`, a disabled slider
 * fades to 35% (transitions still with reduced motion).
 *
 * Only what never changes is here, so the spread reads no signal: a spread
 * is evaluated where the slider is made, and a signal read there would make
 * the slider again when it changes (in the middle of a drag). The colors are
 * getters, read by the slider's props as the appearance changes; the fill
 * (`rangeStyle={sliderFill(..)}`) and `disabled` are props of their own.
 */
export function sliderLook(o: SliderLookOptions = {}) {
  const vertical = !!o.vertical;
  const pointer = platform.idiom === "desktop";
  const thumb = pointer ? 12 : 18;
  // The band keeps half a thumb free at each end, so the thumbs stay inside.
  const band = vertical
    ? `${pointer ? "w-4" : "w-8"} ${pointer ? "h-32" : "h-36"} items-center ${pointer ? "py-1.5" : "py-[9px]"}`
    : `${pointer ? "h-4" : "h-8"} justify-center ${pointer ? "px-1.5" : "px-[9px]"}`;
  const focus = "focus-within:ring-2 focus-within:ring-ring/50 focus-within:ring-offset-2 focus-within:ring-offset-card";
  const fade = "transition duration-150 motion-reduce:transition-none";
  return {
    class: o.value ? "flex-row items-center gap-2" : undefined,
    trackClass: `${band} ${o.value ? "flex-1" : ""} cursor-pointer ${fade} disabled:opacity-35 disabled:cursor-not-allowed`,
    indicatorClass: `${vertical ? "w-1 h-full" : "h-1"} rounded-full ${focus}`,
    get indicatorStyle(): ViewStyle {
      return {
        backgroundColor: colors.track,
        boxShadow: `inset 0 0 0 1px ${finish.track.borderColor}, ${finish.track.boxShadow}`,
      };
    },
    rangeClass: "rounded-full",
    // `focus:` has the platform report the thumb's keyboard focus, which the
    // rail's `focus-within:` ring follows, and drops its default outline.
    thumbClass: `rounded-full ${fade} hover:brightness-105 active:brightness-95 focus:outline-none disabled:brightness-100`,
    get thumbStyle(): ViewStyle {
      return { width: thumb, height: thumb, borderWidth: 1, ...finish.knob };
    },
  };
}

/**
 * A `Slider`'s fill (`rangeStyle`): the accent's run along the rail, a
 * little lighter at the top, edged a shade deeper. `from` and `to` are where
 * the fill starts and ends along the track, from 0 to 1 (unused: the run is
 * one color along its length, kept for the callers).
 */
export function sliderFill(_from: number, _to: number, vertical = false): ViewStyle {
  return {
    backgroundImage: vertical ? finish.fill.backgroundImage?.replace("180deg", "90deg") : finish.fill.backgroundImage,
    borderWidth: 1,
    borderColor: finish.fill.borderColor,
  };
}
