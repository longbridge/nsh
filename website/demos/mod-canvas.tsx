import { mount, signal, Text, View } from "nsh";
import { createAnimatedValue, Easing } from "nsh/animated";
import { Canvas, Path2D } from "nsh/canvas";
import { tint, ui } from "./palette";

const TAU = Math.PI * 2;
const heart = new Path2D("M12 21s-7-4.5-9.5-9A5.5 5.5 0 0 1 12 6a5.5 5.5 0 0 1 9.5 6c-2.5 4.5-9.5 9-9.5 9z");

function Chip(props: { label: string; onPress: () => void }) {
  return (
    <Text
      onPress={props.onPress}
      activeOpacity={0.6}
      style={{
        paddingHorizontal: 14,
        paddingVertical: 8,
        borderRadius: 10,
        overflow: "hidden",
        ...ui().raised,
        color: ui().text,
        fontWeight: "600",
      }}
    >
      {props.label}
    </Text>
  );
}

function App() {
  const data = signal([12, 30, 22, 48, 36, 60, 52, 75, 64, 88]);
  const progress = createAnimatedValue(0.3);
  const target = signal(0.3);
  const go = () => {
    const to = Math.round(Math.random() * 100) / 100;
    target.set(to);
    progress.timing(to, { duration: 900, easing: Easing.inOut(Easing.cubic) });
  };

  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 16 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>Canvas</Text>
      <Canvas
        style={{ height: 170, borderRadius: 12, ...ui().inset }}
        draw={(ctx, { width, height }) => {
          const values = data();
          const pad = 16;
          const w = width - pad * 2, h = height - pad * 2;
          const x = (i: number) => pad + (i / (values.length - 1)) * w;
          const y = (v: number) => pad + h - (v / 100) * h;
          const area = ctx.createLinearGradient(0, pad, 0, pad + h);
          area.addColorStop(0, tint(ui().accent, 0.28));
          area.addColorStop(1, tint(ui().accent, 0));
          ctx.beginPath();
          ctx.moveTo(x(0), pad + h);
          values.forEach((v, i) => ctx.lineTo(x(i), y(v)));
          ctx.lineTo(x(values.length - 1), pad + h);
          ctx.closePath();
          ctx.fillStyle = area;
          ctx.fill();
          ctx.beginPath();
          values.forEach((v, i) => (i ? ctx.lineTo(x(i), y(v)) : ctx.moveTo(x(i), y(v))));
          ctx.strokeStyle = ui().accent;
          ctx.lineWidth = 2.5;
          ctx.lineJoin = "round";
          ctx.stroke();
        }}
      />
      <View style={{ flexDirection: "row", alignItems: "center", gap: 16 }}>
        <Canvas
          style={{ width: 110, height: 110 }}
          draw={(ctx, { width, height }) => {
            const cx = width / 2, cy = height / 2, r = width / 2 - 10;
            ctx.lineWidth = 12;
            ctx.lineCap = "round";
            ctx.strokeStyle = ui().inset.backgroundColor;
            ctx.beginPath();
            ctx.arc(cx, cy, r, 0, TAU);
            ctx.stroke();
            const g = ctx.createConicGradient(-Math.PI / 2, cx, cy);
            g.addColorStop(0, ui().info);
            g.addColorStop(1, ui().accent);
            ctx.strokeStyle = g;
            ctx.beginPath();
            ctx.arc(cx, cy, r, -Math.PI / 2, progress.mul(TAU).sub(Math.PI / 2));
            ctx.stroke();
            ctx.fillStyle = ui().text;
            ctx.font = "600 18px system-ui";
            ctx.textAlign = "center";
            ctx.textBaseline = "middle";
            ctx.fillText(`${Math.round(target() * 100)}%`, cx, cy);
          }}
        />
        <Canvas
          style={{ width: 96, height: 96 }}
          draw={(ctx) => {
            ctx.save();
            ctx.scale(4, 4);
            ctx.fillStyle = ui().down;
            ctx.fill(heart);
            ctx.restore();
          }}
        />
      </View>
      <View style={{ flexDirection: "row", gap: 10 }}>
        <Chip label="New point" onPress={() => data.update((v) => [...v.slice(1), Math.round(Math.random() * 90) + 5])} />
        <Chip label="Animate ring" onPress={go} />
      </View>
    </View>
  );
}

mount(App);
