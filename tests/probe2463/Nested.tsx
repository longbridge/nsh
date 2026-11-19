import { createAnimatedValue, mount, ShaderEffect, signal, Text, View } from 'nsh';
import { Button } from 'nsh/base';

const source = `
struct Uniforms { gain: f32 }
fn shade(uv: vec2f) -> vec4f {
  return sample_content(uv + vec2f(0.0, 180.0 / nsh.size.y)) * uniforms.gain;
}`;

function App() {
  const clipHeight = signal(100);
  const open = signal(true);
  const revision = signal(0);
  const gain = createAnimatedValue(1);

  return (
    <View style={{ flex: 1, padding: 16, gap: 16, backgroundColor: '#222222' }}>
      <View style={{ flexDirection: 'row', gap: 12 }}>
        <Button testID="reveal" onPress={() => clipHeight.set(300)}>
          <Text style={{ color: '#ffffff' }}>1. Reveal source</Text>
        </Button>
        <Button testID="uniform" onPress={() => gain.set(0.99)}>
          <Text style={{ color: '#ffffff' }}>2. Update uniform</Text>
        </Button>
        <Button testID="recapture" onPress={() => revision.set(revision() + 1)}>
          <Text style={{ color: '#ffffff' }}>3. Recapture input</Text>
        </Button>
      </View>
      <Button testID="toggle" onPress={() => open.set(!open())}><Text>Toggle effect</Text></Button>
      <Text style={{ color: '#ffffff' }}>Clip height: {clipHeight()}</Text>
      <View style={{ width: 560, height: clipHeight(), overflow: 'hidden',
        borderWidth: 1, borderColor: '#888888' }}>
        {open() ? <ShaderEffect source="fn shade(uv: vec2f) -> vec4f { return sample_content(uv); }" style={{ width: 540, height: 260 }}><ShaderEffect source={source + '\n// revision ' + revision()}
          uniforms={{ gain }} style={{ width: 540, height: 260 }}>
          <Text style={{ position: 'absolute', left: 8, top: 180,
            fontSize: 30, color: '#ffffff' }}>
            Deferred text ABC 123
          </Text>
        </ShaderEffect></ShaderEffect> : null}
      </View>
    </View>
  );
}

mount(App);
