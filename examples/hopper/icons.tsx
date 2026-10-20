import { Canvas, type CanvasRenderingContext2D } from "nsh/canvas";
import { color } from "./theme";

/** Glyphs drawn in a 20 × 20 box with the tint as stroke. */
export type IconName = "hops" | "mass" | "plan" | "throttle" | "tilt" | "rcs" | "chevron" | "back" | "check" | "caret";

const path = (ctx: CanvasRenderingContext2D, points: [number, number][], close = false) => {
  ctx.beginPath();
  points.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
  if (close) ctx.closePath();
  ctx.stroke();
};

const glyphs: Record<IconName, (ctx: CanvasRenderingContext2D) => void> = {
  // An arc from the ground and back: a hop.
  hops: (ctx) => {
    ctx.beginPath();
    ctx.moveTo(3, 16);
    ctx.quadraticCurveTo(10, -1, 17, 16);
    ctx.stroke();
    path(ctx, [[2, 17], [18, 17]]);
  },
  // The lander's outline.
  mass: (ctx) => {
    path(ctx, [[10, 2], [13, 6], [13, 14], [7, 14], [7, 6]], true);
    path(ctx, [[7, 12], [4, 17]]);
    path(ctx, [[13, 12], [16, 17]]);
  },
  // Waypoints joined by a route.
  plan: (ctx) => {
    path(ctx, [[4, 15], [9, 7], [16, 5]]);
    for (const [x, y] of [[4, 15], [9, 7], [16, 5]]) {
      ctx.beginPath();
      ctx.arc(x, y, 1.8, 0, Math.PI * 2);
      ctx.stroke();
    }
  },
  throttle: (ctx) => {
    ctx.beginPath();
    ctx.moveTo(3, 16);
    ctx.bezierCurveTo(8, 16, 7, 4, 17, 4);
    ctx.stroke();
    path(ctx, [[3, 3], [3, 17], [17, 17]]);
  },
  // An artificial horizon.
  tilt: (ctx) => {
    ctx.beginPath();
    ctx.arc(10, 10, 7.5, 0, Math.PI * 2);
    ctx.stroke();
    path(ctx, [[3.5, 12], [16.5, 8]]);
  },
  // Four thruster blocks around a hull.
  rcs: (ctx) => {
    ctx.beginPath();
    ctx.arc(10, 10, 4, 0, Math.PI * 2);
    ctx.stroke();
    for (const [x, y] of [[3, 3], [15, 3], [3, 15], [15, 15]]) ctx.strokeRect(x, y, 2.5, 2.5);
  },
  chevron: (ctx) => path(ctx, [[8, 5], [13, 10], [8, 15]]),
  back: (ctx) => path(ctx, [[12, 5], [7, 10], [12, 15]]),
  check: (ctx) => path(ctx, [[4, 10.5], [8, 14.5], [16, 5.5]]),
  caret: (ctx) => path(ctx, [[6, 8], [10, 12], [14, 8]]),
};

/** A glyph drawn natively; `flip` turns it upside down (an open caret). */
export function Icon(props: { name: IconName; size?: number; tint?: string; flip?: boolean }) {
  const size = props.size ?? 20;
  return (
    <Canvas
      style={{ width: size, height: size }}
      draw={(ctx) => {
        ctx.scale(size / 20, size / 20);
        if (props.flip) {
          ctx.translate(20, 20);
          ctx.rotate(Math.PI);
        }
        ctx.strokeStyle = props.tint ?? color.text;
        ctx.lineWidth = 1.6;
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        glyphs[props.name](ctx);
      }}
    />
  );
}
