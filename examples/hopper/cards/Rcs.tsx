import { signal, Text, View } from "nsh";
import { Canvas, type CanvasRenderingContext2D, Path2D } from "nsh/canvas";
import { nitrogenTank, quads } from "../store";
import { color, mono } from "../theme";
import { Gauge, Inset, Line, Panel, Pill, track } from "../ui";

const SIZE = 240;
const C = SIZE / 2;
const HULL = 46;
const ARM = 78;

const at = (angle: number, r: number) => {
  const a = (angle * Math.PI) / 180;
  return [C + Math.cos(a) * r, C + Math.sin(a) * r] as const;
};

/** The lander from above: hull, legs and the four RCS quads with their
 * nozzles. A degraded quad is rose; tap a quad to select it. */
function Plan(props: { selected: string | null; onPick: (id: string | null) => void }) {
  // The last drawing answers which quad a tap is on; each quad's outline
  // (its block and the fan of its nozzles) is kept with it.
  let drawn: CanvasRenderingContext2D | undefined;
  let outlines: { id: string; paths: Path2D[] }[] = [];
  return (
    <Canvas
      style={{ width: SIZE, height: SIZE, alignSelf: "center" }}
      {...track((x, y, first) => {
        if (!first || drawn === undefined) return;
        const ctx = drawn;
        props.onPick(outlines.find((o) => o.paths.some((p) => ctx.isPointInPath(p, x, y)))?.id ?? null);
      })}
      accessibilityLabel="Top view: four RCS quads around the hull, Q3 degraded"
      draw={(ctx, { width }) => {
        drawn = ctx;
        outlines = [];
        ctx.scale(width / SIZE, width / SIZE);
        // Range rings.
        ctx.strokeStyle = color.grid;
        ctx.lineWidth = 1;
        for (const r of [40, 80, 112]) {
          ctx.beginPath();
          ctx.arc(C, C, r, 0, Math.PI * 2);
          ctx.stroke();
        }
        // Legs between the quads.
        ctx.strokeStyle = color.lineStrong;
        ctx.lineWidth = 3;
        ctx.lineCap = "round";
        for (const a of [0, 90, 180, 270]) {
          const [x0, y0] = at(a, HULL);
          const [x1, y1] = at(a, 108);
          ctx.beginPath();
          ctx.moveTo(x0, y0);
          ctx.lineTo(x1, y1);
          ctx.stroke();
          ctx.beginPath();
          ctx.arc(x1, y1, 5, 0, Math.PI * 2);
          ctx.stroke();
        }
        // Hull and engine.
        ctx.strokeStyle = "#c8d1da";
        ctx.lineWidth = 1.6;
        ctx.beginPath();
        ctx.arc(C, C, HULL, 0, Math.PI * 2);
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(C, C, 16, 0, Math.PI * 2);
        ctx.stroke();
        ctx.fillStyle = color.faint;
        ctx.font = `10px ${mono}`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText("FWD", C, C - HULL - 10);

        for (const q of quads) {
          const [x, y] = at(q.angle, ARM);
          const weak = q.delta < 0;
          const on = props.selected === q.id;
          const tint = weak ? color.alert : on ? color.accent : color.muted;
          // Strut from the hull.
          const [sx, sy] = at(q.angle, HULL);
          ctx.strokeStyle = color.lineStrong;
          ctx.lineWidth = 1.5;
          ctx.beginPath();
          ctx.moveTo(sx, sy);
          ctx.lineTo(x, y);
          ctx.stroke();
          // Three nozzles fanning out, longer while selected.
          ctx.strokeStyle = tint;
          ctx.lineWidth = on ? 2.5 : 1.5;
          ctx.setLineDash(weak ? [3, 3] : []);
          const ends = [-40, 0, 40].map((spread) => {
            const [ex, ey] = at(q.angle + spread * 0.35, ARM + (on ? 30 : 20));
            return [ex + (x - C) * 0.08, ey + (y - C) * 0.08] as const;
          });
          for (const [ex, ey] of ends) {
            ctx.beginPath();
            ctx.moveTo(x, y);
            ctx.lineTo(ex, ey);
            ctx.stroke();
          }
          const box = new Path2D();
          box.roundRect(x - 13, y - 13, 26, 26, 6);
          const fan = new Path2D();
          fan.moveTo(x, y);
          fan.lineTo(ends[0][0], ends[0][1]);
          fan.lineTo(ends[2][0], ends[2][1]);
          fan.closePath();
          outlines.push({ id: q.id, paths: [box, fan] });
          ctx.setLineDash([]);
          ctx.fillStyle = on ? color.accentDim : color.card;
          ctx.lineWidth = 1.5;
          ctx.fill(box);
          ctx.stroke(box);
          ctx.fillStyle = tint;
          ctx.font = `10px ${mono}`;
          ctx.fillText(q.id, x, y);
        }
      }}
    />
  );
}

export function Rcs() {
  const selected = signal<string | null>(null);
  const percent = Math.round((nitrogenTank.pressure / nitrogenTank.capacity) * 100);
  return (
    <Panel icon="rcs" title="RCS" status="Cold nitrogen · 4 quads · hop 40">
      <Plan selected={selected()} onPick={(id) => selected.set(id)} />
      <View>
        <Each of={quads}>
          {(q, i) => (
            <View
              onPress={() => selected.set(selected() === q().id ? null : q().id)}
              role="button"
              accessibilityState={{ selected: selected() === q().id }}
            >
              <Line first={i() === 0} selected={selected() === q().id}>
                <Pill label={q().id} tone={q().delta < 0 ? "alert" : selected() === q().id ? "accent" : "neutral"} width={44} />
                <Text style={{ flex: 1, fontSize: 15, color: color.text }}>{q().name}</Text>
                <If when={q().delta < 0}>
                  <Text style={{ fontFamily: mono, fontSize: 13, color: color.alert }}>{q().delta}%</Text>
                </If>
                <Text style={{ fontFamily: mono, fontSize: 13, color: color.text }}>{q().used.toFixed(1)} N·s</Text>
              </Line>
            </View>
          )}
        </Each>
      </View>
      <Inset>
        <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "baseline" }}>
          <Text style={{ fontSize: 14, color: color.text }}>Nitrogen bottle</Text>
          <Text style={{ fontFamily: mono, fontSize: 13, color: color.muted }}>{percent}%</Text>
        </View>
        <Gauge value={nitrogenTank.pressure / nitrogenTank.capacity} />
        <Text style={{ fontSize: 13, color: color.muted }}>
          <Text style={{ fontFamily: mono, color: color.text }}>{nitrogenTank.pressure}</Text> of {nitrogenTank.capacity} bar · about{" "}
          {nitrogenTank.hopsLeft} hops left
        </Text>
      </Inset>
    </Panel>
  );
}
