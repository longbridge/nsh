import { Text, View } from "nsh";
import { Canvas, type CanvasRenderingContext2D, Path2D } from "nsh/canvas";
import { cgHeight, load, vehicle, wetMass } from "../store";
import { color, mono } from "../theme";
import { Caption, Gauge, Inset, Line, Panel, track } from "../ui";

/** Drawn in a 120 × 240 space: 1 m is 40 units, the feet at y = 224. */
const DW = 120;
const DH = 240;
const FEET = 224;
const M = 40;
const yAt = (meters: number) => FEET - meters * M;
const [TANK_BOTTOM, TANK_TOP] = vehicle.tank;

/** The hull's walls. */
const BODY = { left: 44, right: 84 };
/** The propellant tank, between its bulkheads. */
const TANK = new Path2D();
TANK.rect(BODY.left, yAt(TANK_TOP), BODY.right - BODY.left, yAt(TANK_BOTTOM) - yAt(TANK_TOP));

/** Thrust of the engine at full throttle (N). */
const THRUST = 11_000;

/**
 * The lander in section: drag up and down from inside the tank to set the
 * propellant level; the CG marker and every figure next to it follow.
 * Touches elsewhere scroll the page.
 */
function Section() {
  // The last drawing answers whether a finger went down in the tank.
  let drawn: CanvasRenderingContext2D | undefined;
  return (
    <Canvas
      style={{ width: 132, height: 264 }}
      {...track(
        (_x, y) => {
          const h = drawn?.canvas.height ?? 0;
          if (h <= 0) return;
          const meters = (FEET - (y * DH) / h) / M;
          load.set(Math.round(Math.min(1, Math.max(0, (meters - TANK_BOTTOM) / (TANK_TOP - TANK_BOTTOM))) * 100) / 100);
        },
        undefined,
        (x, y) => drawn?.isPointInPath(TANK, x, y) ?? false,
      )}
      role="adjustable"
      accessibilityLabel="Propellant load"
      accessibilityValue={{ text: `${Math.round(load() * 100)} percent` }}
      accessibilityActions={[{ name: "increment" }, { name: "decrement" }]}
      onAccessibilityAction={(e) => load.set(Math.min(1, Math.max(0, Math.round((load() + (e.actionName === "increment" ? 0.05 : -0.05)) * 100) / 100)))}
      draw={(ctx, { width, height }) => {
        drawn = ctx;
        ctx.scale(width / DW, height / DH);
        // A ruler, one tick per meter.
        ctx.strokeStyle = color.lineStrong;
        ctx.fillStyle = color.faint;
        ctx.lineWidth = 1;
        ctx.font = `9px ${mono}`;
        ctx.textAlign = "left";
        ctx.textBaseline = "middle";
        for (let m = 1; m <= 5; m++) {
          ctx.beginPath();
          ctx.moveTo(2, yAt(m));
          ctx.lineTo(8, yAt(m));
          ctx.stroke();
          ctx.fillText(`${m}`, 10, yAt(m));
        }
        // Propellant, filling the tank from the bottom.
        const level = TANK_BOTTOM + (TANK_TOP - TANK_BOTTOM) * load();
        const fill = ctx.createLinearGradient(0, yAt(level), 0, yAt(TANK_BOTTOM));
        fill.addColorStop(0, "rgba(61,214,198,0.55)");
        fill.addColorStop(1, "rgba(61,214,198,0.15)");
        ctx.fillStyle = fill;
        ctx.fillRect(BODY.left + 2, yAt(level), BODY.right - BODY.left - 4, yAt(TANK_BOTTOM) - yAt(level));
        ctx.strokeStyle = color.accent;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.moveTo(BODY.left + 2, yAt(level));
        ctx.lineTo(BODY.right - 2, yAt(level));
        ctx.stroke();

        // Hull, nose, tank bulkheads, engine bell and legs.
        ctx.strokeStyle = "#c8d1da";
        ctx.lineWidth = 1.6;
        ctx.lineJoin = "round";
        ctx.beginPath();
        ctx.moveTo(BODY.left, yAt(1.1));
        ctx.lineTo(BODY.left, yAt(4.3));
        ctx.quadraticCurveTo(BODY.left + 4, yAt(5.1), 64, yAt(vehicle.height));
        ctx.quadraticCurveTo(BODY.right - 4, yAt(5.1), BODY.right, yAt(4.3));
        ctx.lineTo(BODY.right, yAt(1.1));
        ctx.closePath();
        ctx.stroke();
        ctx.setLineDash([3, 3]);
        ctx.strokeStyle = color.faint;
        for (const m of [TANK_BOTTOM, TANK_TOP]) {
          ctx.beginPath();
          ctx.moveTo(BODY.left, yAt(m));
          ctx.lineTo(BODY.right, yAt(m));
          ctx.stroke();
        }
        ctx.setLineDash([]);
        ctx.strokeStyle = "#c8d1da";
        ctx.beginPath();
        ctx.moveTo(56, yAt(1.1));
        ctx.lineTo(50, yAt(0.45));
        ctx.lineTo(78, yAt(0.45));
        ctx.lineTo(72, yAt(1.1));
        ctx.stroke();
        for (const [top, foot] of [[BODY.left, 22], [BODY.right, 106]] as const) {
          ctx.beginPath();
          ctx.moveTo(top, yAt(1.6));
          ctx.lineTo(foot, FEET);
          ctx.moveTo(foot - 6, FEET);
          ctx.lineTo(foot + 6, FEET);
          ctx.stroke();
        }
        ctx.strokeStyle = color.lineStrong;
        ctx.beginPath();
        ctx.moveTo(0, FEET + 1);
        ctx.lineTo(DW, FEET + 1);
        ctx.stroke();

        // The CG: a quartered disc beside the hull.
        const y = yAt(cgHeight());
        ctx.strokeStyle = color.caution;
        ctx.lineWidth = 1.2;
        ctx.setLineDash([2, 3]);
        ctx.beginPath();
        ctx.moveTo(BODY.left, y);
        ctx.lineTo(102, y);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.fillStyle = color.ink;
        ctx.beginPath();
        ctx.arc(108, y, 6, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = color.caution;
        for (const a of [0, Math.PI]) {
          ctx.beginPath();
          ctx.moveTo(108, y);
          ctx.arc(108, y, 6, a - Math.PI / 2, a);
          ctx.closePath();
          ctx.fill();
        }
      }}
    />
  );
}

export function Mass() {
  const propellant = () => Math.round(vehicle.propellantMax * load());
  const tw = () => THRUST / (wetMass() * 9.81);
  const margin = () => wetMass() / vehicle.maxTakeoff;
  const rows = () => [
    { label: "Dry structure", value: `${vehicle.dry} kg` },
    { label: `Propellant · ${Math.round(load() * 100)}%`, value: `${propellant()} kg` },
    { label: "Nitrogen", value: `${vehicle.nitrogen} kg` },
    { label: "CG height", value: `${cgHeight().toFixed(2)} m`, tint: color.caution },
  ];
  return (
    <Panel icon="mass" title="Mass" status="Lander L-3 · aluminium tank · rev C">
      <View style={{ flexDirection: "row", gap: 16 }}>
        <Section />
        <View style={{ flex: 1, gap: 12 }}>
          <View style={{ gap: 2 }}>
            <Text style={{ fontSize: 30, fontWeight: "600", color: color.text, fontVariant: ["tabular-nums"] }}>
              {wetMass()}
              <Text style={{ fontSize: 16, color: color.muted, fontWeight: "400" }}> kg wet</Text>
            </Text>
            <Text style={{ fontSize: 13, color: color.muted }}>Drag the propellant level</Text>
          </View>
          <View>
            <Each of={rows()}>
              {(row, i) => (
                <Line first={i() === 0}>
                  <Text style={{ flex: 1, fontSize: 13, color: color.muted }}>{row().label}</Text>
                  <Text style={{ fontFamily: mono, fontSize: 13, color: row().tint ?? color.text }}>{row().value}</Text>
                </Line>
              )}
            </Each>
          </View>
        </View>
      </View>
      <Inset>
        <Caption
          label={
            <Text>
              <Text style={{ color: color.text }}>Takeoff mass</Text> · {Math.round(margin() * 100)}%
            </Text>
          }
          value={`${vehicle.maxTakeoff - wetMass()} kg margin`}
          tint={margin() > 0.92 ? color.caution : color.muted}
        />
        <Gauge value={margin()} warnAbove={0.92} />
        <Text style={{ fontSize: 13, color: color.muted }}>
          Thrust to weight{" "}
          <Text style={{ fontFamily: mono, color: tw() < 1.3 ? color.caution : color.text }}>{tw().toFixed(2)}</Text> at full
          throttle (<Text style={{ fontFamily: mono, color: color.text }}>{THRUST.toLocaleString("en-US")} N</Text>)
        </Text>
      </Inset>
    </Panel>
  );
}
