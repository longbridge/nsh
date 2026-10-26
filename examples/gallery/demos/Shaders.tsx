import { Animated, cleanup, createAnimatedValue, Easing, Shader, signal, Text, View } from "nsh";
import { Button, Caption, Card, DemoPage, Segmented } from "../ui";

// Every demo below is a WGSL `shade(uv)` function: the core checks it,
// the platform renders it on the GPU under the view's children, clipped to
// its rounded box. `nsh.time` makes it animate; uniforms take numbers,
// colors and animated values.

/** Value noise and fractal noise, shared by several demos. */
const NOISE = /* wgsl */ `
// An integer hash of the cell p is in: sin-based hashes differ between
// two computations of one corner once the compiler reorders the arithmetic.
fn hash(p: vec2f) -> f32 {
  let q = bitcast<vec2u>(vec2i(floor(p)));
  var h = (q.x * 1597334677u) ^ (q.y * 3812015801u);
  h = h * 1664525u + 1013904223u;
  h = (h ^ (h >> 16u)) * 2246822519u;
  h = h ^ (h >> 13u);
  return f32(h >> 8u) / 16777216.0;
}
fn noise(p: vec2f) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2f(1.0, 0.0)), u.x),
             mix(hash(i + vec2f(0.0, 1.0)), hash(i + vec2f(1.0, 1.0)), u.x), u.y);
}
fn fbm(p0: vec2f) -> f32 {
  var p = p0;
  var v = 0.0;
  var a = 0.5;
  for (var i = 0; i < 5; i++) {
    v += a * noise(p);
    p = p * 2.03 + vec2f(1.7, 9.2);
    a *= 0.5;
  }
  return v;
}
`;

const AURORA = /* wgsl */ `${NOISE}
fn shade(uv: vec2f) -> vec4f {
  let aspect = nsh.size.x / max(nsh.size.y, 1.0);
  let p = vec2f(uv.x * aspect, uv.y);
  let t = nsh.time * 0.12;
  var color = mix(vec3f(0.01, 0.02, 0.07), vec3f(0.02, 0.05, 0.16), uv.y);
  for (var i = 0; i < 3; i++) {
    let k = f32(i);
    let wave = fbm(vec2f(p.x * 1.4 + t * (1.0 + k * 0.4), k * 3.1 + t)) - 0.5;
    let center = 0.32 + k * 0.13 + wave * 0.35;
    let band = exp(-pow((p.y - center) * (9.0 - k * 2.0), 2.0));
    let rays = 0.6 + 0.4 * fbm(vec2f(p.x * 18.0 + k * 5.0, t * 3.0));
    let tint = mix(vec3f(0.1, 1.0, 0.6), vec3f(0.55, 0.3, 1.0), k * 0.5);
    color += tint * band * rays * 0.75;
  }
  let star = step(0.997, hash(floor(uv * nsh.size * 0.5)));
  color += vec3f(star * (1.0 - uv.y) * 0.8);
  return vec4f(color, 1.0);
}`;

const PLASMA = /* wgsl */ `
fn shade(uv: vec2f) -> vec4f {
  let p = (uv - 0.5) * vec2f(nsh.size.x / max(nsh.size.y, 1.0), 1.0) * 6.0;
  let t = nsh.time * 0.8;
  var v = sin(p.x + t) + sin((p.y + t) * 0.7) + sin((p.x + p.y + t) * 0.6);
  v += sin(length(p + vec2f(sin(t * 0.5), cos(t * 0.3)) * 2.0) * 1.5 - t);
  let c = 0.5 + 0.5 * cos(vec3f(0.0, 2.1, 4.2) + v * 1.2 + t * 0.2);
  return vec4f(mix(c * c, c, 0.35), 1.0);
}`;

const MESH = /* wgsl */ `
// An integer hash of the cell p is in: sin-based hashes differ between
// two computations of one corner once the compiler reorders the arithmetic.
fn hash(p: vec2f) -> f32 {
  let q = bitcast<vec2u>(vec2i(floor(p)));
  var h = (q.x * 1597334677u) ^ (q.y * 3812015801u);
  h = h * 1664525u + 1013904223u;
  h = (h ^ (h >> 16u)) * 2246822519u;
  h = h ^ (h >> 13u);
  return f32(h >> 8u) / 16777216.0;
}
struct Uniforms { colors: array<vec4f, 4>, speed: f32 }
fn shade(uv: vec2f) -> vec4f {
  let t = nsh.time * uniforms.speed;
  let warp = vec2f(sin(uv.y * 5.0 + t * 1.3) + sin(uv.x * 3.0 - t), cos(uv.x * 4.0 - t * 1.1) + sin(uv.y * 2.5 + t * 0.7)) * 0.12;
  let q = uv + warp;
  let points = array<vec2f, 4>(
    vec2f(0.2 + 0.1 * sin(t), 0.2),
    vec2f(0.85, 0.25 + 0.1 * cos(t * 1.3)),
    vec2f(0.15, 0.85 + 0.05 * sin(t * 0.7)),
    vec2f(0.8 + 0.1 * cos(t * 0.9), 0.8),
  );
  var color = vec3f(0.0);
  var total = 0.0;
  for (var i = 0; i < 4; i++) {
    let w = 1.0 / pow(distance(q, points[i]) + 0.05, 2.5);
    color += uniforms.colors[i].rgb * w;
    total += w;
  }
  let grain = (hash(uv * nsh.size * nsh.scale) - 0.5) * 0.05;
  return vec4f(color / total + grain, 1.0);
}`;

const TUNNEL = /* wgsl */ `
fn shade(uv: vec2f) -> vec4f {
  let p = (uv - 0.5) * vec2f(nsh.size.x / max(nsh.size.y, 1.0), 1.0);
  let r = length(p);
  let a = atan2(p.y, p.x);
  let t = nsh.time;
  let depth = 0.3 / max(r, 0.001) + t * 0.9;
  let grid = abs(fract(vec2f(a / 6.2831853 * 12.0, depth * 2.0)) - 0.5);
  let line = smoothstep(0.06, 0.0, min(grid.x, grid.y));
  let hue = 0.5 + 0.5 * cos(vec3f(0.0, 2.0, 4.0) + depth * 0.6);
  let fog = smoothstep(0.0, 0.35, r);
  return vec4f(hue * line * fog * 1.4 + vec3f(0.02, 0.0, 0.05), 1.0);
}`;

const RIPPLE = /* wgsl */ `
struct Uniforms { center: vec2f, wave: f32, tint: vec4f }
fn shade(uv: vec2f) -> vec4f {
  let p = uv * nsh.size;
  let d = distance(p, uniforms.center * nsh.size) / max(nsh.size.x, nsh.size.y);
  let front = uniforms.wave * 0.75;
  let ring = exp(-pow((d - front) * 28.0, 2.0)) * (1.0 - uniforms.wave);
  let rings = 0.5 + 0.5 * sin((d - front) * 90.0);
  let glow = ring * (0.6 + 0.4 * rings);
  let base = mix(vec3f(0.03, 0.03, 0.06), uniforms.tint.rgb * 0.25, 1.0 - uv.y);
  return vec4f(base + uniforms.tint.rgb * glow * 1.6, 1.0);
}`;

const HOLO = /* wgsl */ `${NOISE}
struct Uniforms { tilt: f32 }
fn shade(uv: vec2f) -> vec4f {
  let t = nsh.time * 0.25;
  let s = uv.x * 0.8 + uv.y * 0.6 + uniforms.tilt * 0.6 + t;
  let foil = 0.5 + 0.5 * cos(6.2831853 * (vec3f(0.0, 0.33, 0.67) + s * 1.5 + noise(uv * 3.0 + t) * 0.35));
  let sheen = pow(max(0.0, 1.0 - abs(uv.x + uv.y - 1.0 - uniforms.tilt * 1.2) * 2.5), 6.0);
  let color = mix(vec3f(0.08, 0.08, 0.12), foil, 0.55) + vec3f(sheen * 0.6);
  return vec4f(color, 1.0);
}`;

/** A shader with a caption under it. */
function Figure(props: { source: string; height?: number; caption: string }) {
  return (
    <View style={{ gap: 10 }}>
      <Shader source={props.source} style={{ height: props.height ?? 200, borderRadius: 14 }} />
      <Caption label={props.caption} />
    </View>
  );
}

function Aurora() {
  return (
    <Card icon="spark" title="Aurora" subtitle="Fractal noise curtains over a starfield">
      <Figure source={AURORA} caption="Three bands of fbm noise, each drifting at its own speed; stars from a hash of the pixel." />
    </Card>
  );
}

function Plasma() {
  return (
    <Card icon="canvas" title="Plasma" subtitle="Sums of sines through a cosine palette">
      <Figure source={PLASMA} caption="The demoscene classic, a few lines of WGSL, rendered at the screen's scale every frame." />
    </Card>
  );
}

function Tunnel() {
  return (
    <Card icon="animation" title="Tunnel" subtitle="Polar coordinates, endless depth">
      <Figure source={TUNNEL} caption="Angle and inverse radius make a grid that flies towards the viewer." />
    </Card>
  );
}

const PALETTES: Record<string, string[]> = {
  Sunset: ["#ff6b6b", "#ffb86b", "#7b5cff", "#ff4fa3"],
  Lagoon: ["#00c2a8", "#2f80ed", "#a0f0ff", "#0b3d91"],
  Citrus: ["#f9f871", "#ffc75f", "#9be15d", "#ff9671"],
};

function Mesh() {
  const palette = signal("Sunset");
  return (
    <Card icon="styles" title="Mesh gradient" subtitle="Colors from uniforms, warped by noise">
      <View style={{ gap: 12 }}>
        <Shader source={MESH} uniforms={{ colors: PALETTES[palette()], speed: 0.5 }} style={{ height: 200, borderRadius: 14 }} />
        <Segmented options={Object.keys(PALETTES).map((name) => ({ value: name, label: name }))} value={palette()} onChange={(name) => palette.set(name)} />
      </View>
    </Card>
  );
}

function Ripple() {
  const wave = createAnimatedValue(0);
  const center = { x: createAnimatedValue(0.5), y: createAnimatedValue(0.5) };
  const tint = wave.interpolate({ inputRange: [0, 1], outputRange: ["#38bdf8", "#c084fc"] });
  const loop = Animated.loop(Animated.timing(wave, { toValue: 1, duration: 2200, easing: Easing.out(Easing.quad) }));
  loop.start();
  cleanup(() => loop.stop());
  const fire = (x: number, y: number) => {
    loop.stop();
    center.x.set(x);
    center.y.set(y);
    wave.set(0);
    loop.start();
  };
  let size = { width: 1, height: 1 };
  return (
    <Card icon="gestures" title="Ripple" subtitle="Animated values as uniforms" right={<Button label="Pulse" variant="secondary" onPress={() => fire(0.5, 0.5)} />}>
      <View style={{ gap: 10 }}>
        <Shader
          source={RIPPLE}
          uniforms={{ center: [center.x, center.y], wave, tint }}
          style={{ height: 200, borderRadius: 14 }}
          onLayout={(bounds) => (size = bounds.size)}
          onStartShouldSetResponder={() => true}
          onResponderGrant={(e) => fire(e.locationX / size.width, e.locationY / size.height)}
        />
        <Caption label="Tap the view to move the source: the wave runs on the native animation driver, with no JS per frame." />
      </View>
    </Card>
  );
}

function Holo() {
  const tilt = createAnimatedValue(0);
  const loop = Animated.loop(
    Animated.sequence([
      Animated.timing(tilt, { toValue: 1, duration: 2600, easing: Easing.inOut(Easing.sin) }),
      Animated.timing(tilt, { toValue: -1, duration: 2600, easing: Easing.inOut(Easing.sin) }),
    ]),
  );
  loop.start();
  cleanup(() => loop.stop());
  return (
    <Card icon="star" title="Holographic card" subtitle="Children drawn over the shader">
      <Shader source={HOLO} uniforms={{ tilt }} style={{ height: 200, borderRadius: 18, padding: 20, justifyContent: "space-between" }}>
        <Text style={{ color: "#ffffff", fontSize: 13, fontWeight: "600", letterSpacing: 2, opacity: 0.85 }}>NATIVE SHELL</Text>
        <View>
          <Text style={{ color: "#ffffff", fontSize: 26, fontWeight: "700" }}>4242 0000 1717</Text>
          <Text style={{ color: "#ffffff", fontSize: 13, opacity: 0.8 }}>Foil from a looping animated uniform</Text>
        </View>
      </Shader>
    </Card>
  );
}

export function Shaders() {
  const cards = [Aurora, Mesh, Ripple, Holo, Plasma, Tunnel];
  return <DemoPage cards={cards} columns={3} />;
}
