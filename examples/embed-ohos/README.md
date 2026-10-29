# Embedding NSH in an OHOS app

This is a minimal OHOS `UIAbility` app that embeds `examples/embedded/App.tsx` using `@nsh/ohos` and `NshSurface`, below its own native toolbar.

1. Build the SDK (requires DevEco Studio or `OHOS_NDK_HOME` with the API 20+ OpenHarmony NDK, and the `aarch64-unknown-linux-ohos` Rust target):

   ```sh
   cargo xtask ohos --release
   ```

2. Bundle the embedded app and build the HAP:

   ```sh
   cargo build -p nsh-cli
   NSH_PROFILE=release examples/embed-ohos/build.sh
   ```

3. Open `examples/embed-ohos` in DevEco Studio, configure signing for `com.example.nsh`, and run on an `arm64-v8a` device or emulator:

   ```sh
   hdc -t <target> install -r examples/embed-ohos/entry/build/default/outputs/default/entry-default-signed.hap
   hdc -t <target> shell aa start -a EntryAbility -b com.example.nsh
   ```

The example also registers a native component (`embed.badge`) used by `examples/embedded/App.tsx`:

```ts
surface.registerComponent('embed.badge', (id: number) => createHostBadge(this.getUIContext(), surface, id));
```

And talks to the app over the [`nsh/host`](../../website/modules/nsh-host.md) channel without a view:

```ts
surface.setParams({ host: 'OHOS (ArkTS)', initialCount: 10 });
surface.handle('host.OrderTicket.submit', (data, reply) => {
  reply.resolve({ orderId: 'ORD-1', status: 'filled' });
});
surface.send('host.OrderTicket.symbol', { symbol: 'NVDA' });
```

And switches the app's theme at runtime with `setThemeOverrides`:

```ts
surface.setThemeOverrides(JSON.stringify({
  cssVars: {
    theme: { radius: '0.75rem' },
    light: { primary: '#e11d48', 'primary-foreground': '#ffffff' },
    dark: { primary: '#fb7185', 'primary-foreground': '#1c1917' },
  },
}));
```

## Signed packages

To verify and run a signed `.nsh` package downloaded at runtime:

```ts
import { NshTrust } from 'libnsh_platform_ohos.so';

const trust = await NshTrust.open(`${context.filesDir}/nsh-trust`, ['<base64 root.pub>']);
const pkg = await trust.openPackage(downloadedFilePath);
surface.runPackage(pkg);
```

## Connecting to `nsh dev`

Build the `--dev` SDK (`cargo xtask ohos --dev`) and call:

```ts
surface.connectDevServer('ws://127.0.0.1:8790/ws'); // with `hdc rport tcp:8790 tcp:8790`
```
