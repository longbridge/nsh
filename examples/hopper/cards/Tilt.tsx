import { computed, signal, Text, View } from "nsh";
import { Canvas } from "nsh/canvas";
import { gimbalCommands, HOP_LENGTH, tiltAt, tiltLimit } from "../store";
import { color, mono } from "../theme";
import { Caption, Panel, track } from "../ui";

const samples = Array.from({ length: HOP_LENGTH * 20 + 1 }, (_, i) => ({ t: i / 20, deg: tiltAt(i / 20) }));
const peak = samples.reduce((a, s) => (Math.abs(s.deg) > Math.abs(a.deg) ? s : a));
const RANGE = 7;

/**
 * Tilt over the hop inside the ±5° band; excursions past it are rose. Drag
 * across the chart to read the tilt at any moment.
 */
function Chart(props: { cursor: number; onScrub: (t: number) => void }) {
  const width = signal(0);
  const pad = { left: 34, right: 8, top: 12, bottom: 22 };
  return (
    <Canvas
      style={{ height: 210 }}
      onLayout={(e) => width.set(e.width)}
      {...track((x) => {
        const w = width() - pad.left - pad.right;
        if (w > 0) props.onScrub(Math.round(Math.min(HOP_LENGTH, Math.max(0, ((x - pad.left) / w) * HOP_LENGTH)) * 10) / 10);
      })}
      role="adjustable"
      accessibilityLabel="Tilt during the hop"
      accessibilityValue={{ text: `${tiltAt(props.cursor).toFixed(1)} degrees at ${props.cursor.toFixed(1)} seconds` }}
      accessibilityActions={[{ name: "increment" }, { name: "decrement" }]}
      onAccessibilityAction={(e) =>
        props.onScrub(Math.min(HOP_LENGTH, Math.max(0, props.cursor + (e.actionName === "increment" ? 0.5 : -0.5))))
      }
      draw={(ctx, { width, height }) => {
        const w = width - pad.left - pad.right, h = height - pad.top - pad.bottom;
        const X = (t: number) => pad.left + (t / HOP_LENGTH) * w;
        const Y = (d: number) => pad.top + ((RANGE - d) / (2 * RANGE)) * h;

        ctx.fillStyle = "rgba(61,214,198,0.06)";
        ctx.fillRect(pad.left, Y(tiltLimit), w, Y(-tiltLimit) - Y(tiltLimit));
        ctx.strokeStyle = color.lineStrong;
        ctx.lineWidth = 1;
        ctx.setLineDash([4, 4]);
        for (const d of [tiltLimit, -tiltLimit]) {
          ctx.beginPath();
          ctx.moveTo(pad.left, Y(d));
          ctx.lineTo(width - pad.right, Y(d));
          ctx.stroke();
        }
        ctx.setLineDash([]);
        ctx.strokeStyle = color.grid;
        ctx.beginPath();
        ctx.moveTo(pad.left, Y(0));
        ctx.lineTo(width - pad.right, Y(0));
        ctx.stroke();

        ctx.font = `11px ${mono}`;
        ctx.fillStyle = color.faint;
        ctx.textBaseline = "middle";
        ctx.textAlign = "right";
        for (const d of [tiltLimit, 0, -tiltLimit]) ctx.fillText(`${d > 0 ? "+" : ""}${d}°`, pad.left - 6, Y(d));
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        for (const t of [0, 6, 12, 18, 24]) ctx.fillText(`${t}s`, X(t), height - pad.bottom + 6);

        // The trace, rose where it leaves the band.
        ctx.lineWidth = 2;
        ctx.lineJoin = "round";
        for (let i = 1; i < samples.length; i++) {
          const a = samples[i - 1], b = samples[i];
          ctx.strokeStyle = Math.abs(b.deg) > tiltLimit ? color.alert : color.accent;
          ctx.beginPath();
          ctx.moveTo(X(a.t), Y(a.deg));
          ctx.lineTo(X(b.t), Y(b.deg));
          ctx.stroke();
        }

        // The cursor.
        const deg = tiltAt(props.cursor);
        ctx.strokeStyle = color.light;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(X(props.cursor), pad.top);
        ctx.lineTo(X(props.cursor), pad.top + h);
        ctx.stroke();
        ctx.fillStyle = color.ink;
        ctx.strokeStyle = Math.abs(deg) > tiltLimit ? color.alert : color.light;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(X(props.cursor), Y(deg), 5, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
      }}
    />
  );
}

/** Gimbal deflection, one bar per half second; slide across to read one. */
function Commands(props: { picked: number | null; onPick: (i: number | null) => void }) {
  const width = signal(0);
  return (
    <Canvas
      style={{ height: 40 }}
      onLayout={(e) => width.set(e.width)}
      {...track(
        (x) => width() > 0 && props.onPick(Math.min(gimbalCommands.length - 1, Math.max(0, Math.floor((x / width()) * gimbalCommands.length)))),
        () => props.onPick(null),
      )}
      accessibilityLabel={`${gimbalCommands.length} gimbal commands`}
      draw={(ctx, { width, height }) => {
        const step = width / gimbalCommands.length;
        gimbalCommands.forEach((v, i) => {
          const h = Math.max(3, v * height);
          ctx.fillStyle = props.picked === i ? color.light : v > 0.8 ? color.caution : color.bar;
          ctx.beginPath();
          ctx.roundRect(i * step + 1, (height - h) / 2, step - 2, h, 2);
          ctx.fill();
        });
      }}
    />
  );
}

export function Tilt() {
  const cursor = signal(peak.t);
  const picked = signal<number | null>(null);
  const excursions = computed(() => {
    let n = 0;
    for (let i = 1; i < samples.length; i++) if (Math.abs(samples[i].deg) > tiltLimit && Math.abs(samples[i - 1].deg) <= tiltLimit) n++;
    return n;
  });
  const commandText = () => {
    const i = picked();
    if (i === null) return "48 × 0.5 s";
    return `${(i / 2).toFixed(1)} s · ${Math.round(gimbalCommands[i] * 100)}%`;
  };
  return (
    <Panel icon="tilt" title="Tilt" status={`Hop 40 replay · 0–${HOP_LENGTH} s`}>
      <View style={{ flexDirection: "row", alignItems: "flex-end", gap: 16 }}>
        <View style={{ flex: 1, gap: 2 }}>
          <Text style={{ fontSize: 12, color: color.muted, textTransform: "uppercase", letterSpacing: 0.6 }}>at cursor</Text>
          <Text style={{ fontSize: 30, fontWeight: "600", color: Math.abs(tiltAt(cursor())) > tiltLimit ? color.alert : color.text }}>
            {tiltAt(cursor()) >= 0 ? "+" : ""}
            {tiltAt(cursor()).toFixed(1)}°
            <Text style={{ fontSize: 15, fontWeight: "400", color: color.muted }}> T+{cursor().toFixed(1)} s</Text>
          </Text>
        </View>
        <View style={{ alignItems: "flex-end", gap: 2 }}>
          <Text style={{ fontFamily: mono, fontSize: 13, color: color.alert }}>peak {peak.deg.toFixed(1)}°</Text>
          <Text style={{ fontSize: 12, color: color.muted }}>
            {excursions()} excursion{excursions() === 1 ? "" : "s"} past ±{tiltLimit}°
          </Text>
        </View>
      </View>
      <Chart cursor={cursor()} onScrub={(t) => cursor.set(t)} />
      <Caption label="Gimbal commands" value={commandText()} />
      <Commands picked={picked()} onPick={(i) => picked.set(i)} />
    </Panel>
  );
}
