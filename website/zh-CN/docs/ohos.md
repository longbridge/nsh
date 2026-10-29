---
title: OHOS
description: 通过 @nsh/ohos 和 NshSurface 将 NSH 应用嵌入现有 OHOS 应用。
order: 13
group: platforms
---

# OHOS

NSH 应用在 OHOS 上用 ArkUI 自定义节点和原生文本排版运行，要求 API 20 及以上，支持 `arm64-v8a`。现有应用可以在 `NshSurface` 中嵌入模块，加载 bundle 或签名包，并按自己设定的限制和能力运行它们。在 OHOS 上，`os()` 是 `Os::Ohos`（TSX 中 `platform.os` 是 `"ohos"`），TSX 的 `ohos={{…}}` 属性生效。

## 环境要求

- **DevEco Studio** 与 HarmonyOS / OpenHarmony SDK（API 20 或更高版本）、OHPM、Hvigor，以及 `arm64-v8a` 真机或模拟器。
- 在 macOS、Windows 或 Linux 上从源码构建 SDK 时需要 **Rust**（`rustup target add aarch64-unknown-linux-ohos`）。见[安装](./installation.md#构建-ohos-嵌入-sdk)。

## 嵌入 OHOS 应用

现有的 OHOS 应用可以在自己的界面中显示 NSH 应用。运行时以原生动态库（`libnsh_platform_ohos.so`）与配套 ArkTS 包（`@nsh/ohos`）的形式提供：创建一个 `NshSurface`，把 `surface.content` 放入标准的 `ContentSlot` 中，再往里加载一个 TSX bundle 或签名包即可。NSH 应用用原生 ArkUI 节点渲染，与你自己的组件并列。

每个 `NshSurface` 都是一个独立的引擎，拥有自己的 JS VM 和 `NodeContent`；没有全局单例，所以可以同时运行多个。

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/host-app-light.svg" alt="一个宿主应用里有两个 nsh 视图：每个都是独立的引擎，有自己的线程、虚拟机、限制和能力；宿主授予能力、设置限制，并注册地图这样的原生组件。">
<img class="diagram diagram-dark" src="/diagrams/host-app-dark.svg" alt="一个宿主应用里有两个 nsh 视图：每个都是独立的引擎，有自己的线程、虚拟机、限制和能力；宿主授予能力、设置限制，并注册地图这样的原生组件。">
</div>

## 获取库

在 NSH 仓库根目录构建 SDK（`xtask` 会自动从 DevEco Studio 默认目录或 `OHOS_NDK_HOME` 查找 OpenHarmony NDK）：

```sh
cargo xtask ohos --release          # release：bundle 和签名包
cargo xtask ohos --release --dev    # 同时支持连接 `nsh dev` 开发服务器
```

这会在 `target/ohos/release/`（或 `target/ohos/release-dev/`）下生成 `arm64-v8a/libnsh_platform_ohos.so` 和自动生成的类型声明 `index.d.ts`，并在 `target/ohos/arkts-deps/nsh/` 下暂存配套的 `@nsh/ohos` 包。面向 API 23 或 API 26+ 时，可按需传入 `--features api-23` 或 `--features api-26` 开启更高版本无障碍 provider 与减弱动态效果监听。

将 `libnsh_platform_ohos.so` 与 `$OHOS_NDK_HOME/native/llvm/lib/aarch64-linux-ohos/libc++_shared.so` 放入 `entry/libs/arm64-v8a/`，将 `index.d.ts` 复制到 `entry/src/main/cpp/types/libnsh_platform_ohos/Index.d.ts`，并在 `entry/oh-package.json5` 中引入：

```json5
{
  "dependencies": {
    "@nsh/ohos": "file:../ohos_nsh",
    "libnsh_platform_ohos.so": "file:./src/main/cpp/types/libnsh_platform_ohos"
  }
}
```

## 随应用附带 bundle

把 TSX 应用构建到模块的 `rawfile` 资源目录中：

```sh
nsh build App.tsx --release --out entry/src/main/resources/rawfile
```

这会写出 `bundle.nshc`（字节码）、`bundle.js`（源码）、`bundle.js.map`、`capabilities.json`（当 `nsh.json` 声明了[能力](./sandbox-and-capabilities.md)时）以及 `assets/`（应用导入的图片、字体和媒体资源）。

## 显示应用

```ts
import { NshSurface } from '@nsh/ohos';

@Entry
@Component
struct Index {
  private surface: NshSurface = new NshSurface(this.getUIContext());

  aboutToAppear(): void {
    this.surface.setErrorHandler((message: string) => {
      console.error(`nsh: ${message}`);
    });
    this.surface.loadResource('bundle.js');
  }

  aboutToDisappear(): void {
    this.surface.dispose();
  }

  onBackPress(): boolean {
    return this.surface.onBackPress();
  }

  build() {
    Stack() {
      ContentSlot(this.surface.content)
    }
    .width('100%')
    .height('100%')
  }
}
```

`NshSurface` 也可以放在任意 ArkUI 布局中，例如放在你自己的工具栏下方或并排放入 `Row`：

```ts
Column() {
  Toolbar()
  Stack() {
    ContentSlot(this.surface.content)
  }
  .width('100%')
  .layoutWeight(1)
}
.width('100%')
.height('100%')
```

插槽的尺寸就是应用的窗口：NSH 按插槽的尺寸布局应用，尺寸变化时重新布局。再次加载会替换正在运行的应用。完整示例见 `examples/embed-ohos/entry/src/main/ets/pages/Index.ets`。

## `NshSurface`

| 成员 | 说明 |
|---|---|
| `new NshSurface(uiContext: UIContext, options?: NshOptions)` | 创建一个绑定到 `uiContext` 的空视图，可配置 VM 限制、授予的能力、资源目录和初始环境。 |
| `readonly content: NodeContent` | 挂载到标准 `ContentSlot` 中的平台 `NodeContent` 对象。 |
| `setErrorHandler(handler: (message: string) => void)` | 在 UI 线程上接收应用的致命错误（加载失败、未捕获的 JS 异常、VM 因触及限制而停止）。错误总会写入日志。 |
| `setThemeOverrides(json?: string \| null)` | 把宿主的品牌加在应用的[主题](./tailwind.md#主题-token)之上：格式与 `nsh.json` 的 `"theme"` 相同的主题文件。立即生效，并作用于之后加载的应用；`null` 恢复应用自己的主题。无法解析时抛出异常。 |
| `setEnvironment(changes: NshEnvironment)` | 为该视图覆盖语言地区、字体缩放、配色方案（`"light"` / `"dark"`）、安全区域或键盘高度。未传字段保持原值。 |
| `loadResource(name: string, sourceMap?: string): Promise<void>` | 运行 `resources/rawfile/` 中的 `bundle.js` 或 `bundle.nshc`；应用资源取自 `rawfile/assets/`。 |
| `loadSource(source: string, sourceMap?: string)` | 直接运行可信 JS 源码并替换正在运行的应用。 |
| `runPackage(pkg: NshPackage)` | 运行一个已验证的签名包，替换正在运行的应用。每次 `openPackage` 打开的包只能运行一次。 |
| `registerComponent(name: string, factory: (id: number) => object)` | 为该视图注册一个[原生组件](./native-components.md)。 |
| `emitComponentEvent(handle: number, name: string, data: object \| string \| number \| boolean \| null)` | 从已挂载的原生组件向 JS 发送事件。 |
| `setComponentSize(handle: number, width: number, height: number)` | 以 `vp` 为单位上报已挂载原生组件的固有尺寸。 |
| `setParams(params?: object \| null)` | 之后加载的应用的启动参数：应用在启动时以 `host.params` 读取（[`nsh/host`](../modules/nsh-host.md)）。 |
| `send(name: string, data?: object \| string \| number \| boolean \| null)` | 向应用发送一个事件（`host.on(name, …)`）；应用开始监听前会一直保留，没有应用运行时留给下一个应用。 |
| `handle(name: string, handler?: ((data: Object \| null, reply: NshHostReply) => void) \| null)` | 在 UI 线程处理应用对 `name` 的调用（`host.call`）和投递（`host.post`）；传 `null`（或调用 `removeHandler(name)`）移除处理函数。通过 `reply.resolve(value)` 或 `reply.reject(message)` 回答一次。 |
| `setOpenUrlHandler(handler?: ((url: string) => boolean) \| null)` | 拦截应用打开的 URL；宿主自行处理时返回 `true`，交由系统打开时返回 `false`。 |
| `connectDevServer(url: string)` | 开发用：从 `nsh dev` 服务器加载，每次修改后重新加载（需使用 `--dev` SDK）。 |
| `disconnectDevServer()` | 断开开发服务器连接，保持当前应用继续运行。 |
| `activate()` | 在多个视图之间切换时，将该视图标记为系统返回手势的当前活动目标。 |
| `onBackPress(): boolean` | 将页面的 `onBackPress()` 事件路由到该视图所属的窗口会话；应用消费返回动作时返回 `true`。 |
| `stop()` | 停止正在运行的应用并取消未完成的加载，保留视图、配置和已注册的原生组件供后续加载使用。 |
| `dispose()` | 永久停止应用并释放该视图的原生资源。 |

## 与应用通信

视图与它运行的应用通过一条专属通道通信（[`nsh/host`](../modules/nsh-host.md)）：应用在启动时读取的启动参数、宿主应用发来的事件，以及应用发出的调用和投递，数据都是 JSON 值。应用里不需要任何视图，也不需要原生组件。

```ts
// 加载之前：应用首先打开的内容（TSX 中的 `host.params`）。
surface.setParams({ conversation: 'conv-42' });
surface.loadResource('bundle.js');

// 应用的 `await host.call("user.profile", { id })`，在 UI 线程处理。
surface.handle('user.profile', (data, reply) => {
  const req = data as Record<string, Object>;
  api.fetchProfile(req['id'] as string)
    .then((profile) => reply.resolve({ id: req['id'], name: profile.name }))
    .catch((err: Error) => reply.reject(err.message));
});

// 应用的 `host.post("analytics", …)`：没有回答。
surface.handle('analytics', (data, _reply) => {
  analytics.log(data);
});

// 之后：应用通过 `host.on("conversation.open", …)` 收到它。
surface.send('conversation.open', { id: tappedConversation });
```

处理函数在 UI 线程运行；调用在应用中等待回答，不会阻塞任何一方。应用尚未监听的名称的事件会保留到它开始监听，没有应用运行时发送的事件交给下一个应用。

## 主题覆盖

嵌入在多个品牌宿主应用中的同一个应用会采用各个宿主的品牌：宿主给出其品牌要改的 token，格式与 `nsh.json` 的 `"theme"` 相同的主题文件（shadcn/ui 的 `cssVars`，值按 CSS 的写法）。它们覆盖应用的同名 token，应用的其他 token 保持不变。在 `dark` 下给出 token 的深色模式值（只在 `light` 或 `theme` 下设置的 token 在两种模式下都是这个颜色）：

```ts
surface.setThemeOverrides(JSON.stringify({
  cssVars: {
    theme: { radius: '0.75rem' },
    light: { primary: '#e11d48', 'primary-foreground': '#ffffff' },
    dark: { primary: '#fb7185', 'primary-foreground': '#1c1917' },
  },
}));
surface.loadResource('bundle.js');
```

在加载之前或之后调用都可以：`bg-primary`、`rounded-(--radius)` 等类名立即更新样式，之后加载的每个应用在第一帧就带有这些覆盖。`setThemeOverrides(null)` 恢复应用自己的主题。

## 应用打开的链接

应用打开的 URL（[`openUrl`](../modules/nsh-device.md#openurl)，以及 `links="open"` 的 `TextView` 中的链接）先交给视图的 `setOpenUrlHandler` 回调，在 UI 线程调用：宿主应用自己处理了这个 URL（例如交给自己的路由）时返回 `true`，返回 `false` 则由系统打开。没有设置处理函数时，所有 URL 都由系统打开。到达这里的 URL 都已通过应用的 `openUrl` 能力检查。

```ts
surface.setOpenUrlHandler((url: string): boolean => {
  if (url.startsWith('lb://') || url.startsWith('https://longbridge.com/')) {
    router.open(url);
    return true;
  }
  return false;
});
```

## 限制

创建视图时通过 `NshOptions` 传入 VM 与节点限制：

```ts
const surface = new NshSurface(this.getUIContext(), {
  memoryBytes: 128 * 1024 * 1024, // 128 MiB 堆
  stackBytes: 4 * 1024 * 1024,    // 4 MiB 栈
  loadBudgetMs: 10_000,           // 加载 10 s
  callBudgetMs: 1_000,            // 每次调用 1 s
  maxNodes: 50_000,               // 50,000 个节点
});
```

默认值为：256 MiB 堆、4 MiB 栈、加载 10 s、每次调用 2 s、200,000 个节点。各项限制触发时会发生什么，见[沙箱与能力](./sandbox-and-capabilities.md)。

## 能力

通过 `NshOptions.capabilities` 传入宿主的授权（JSON 格式与 `nsh.json` 的 `capabilities` 相同）：

```ts
const surface = new NshSurface(this.getUIContext(), {
  capabilities: JSON.stringify({
    network: { hosts: ['api.example.com'] },
    webview: { hosts: ['help.example.com'] },
    host: true,
  }),
});
surface.loadResource('bundle.js');
```

一个请求必须既被应用在 `nsh.json` 中的声明允许，也被这里的授权允许。参见[沙箱与能力](./sandbox-and-capabilities.md)。

## 签名包

使用 `NshTrust` 在后台工作线程验证下载的包，再交给视图运行：

```ts
import { NshTrust } from 'libnsh_platform_ohos.so';

const trust = await NshTrust.open(`${context.filesDir}/nsh-trust`, [rootPublicKey]);
const pkg = await trust.openPackage(downloadedFilePath);
surface.runPackage(pkg);
```

| API | 说明 |
|---|---|
| `NshTrust.open(stateDirectory: string, rootPublicKeys?: string[]): Promise<NshTrust>` | 打开或创建信任库。它的状态（最新的吊销列表、每个应用已接受的最高构建号）持久化在该目录中，包的资源也解压到其下。 |
| `addRoot(publicKeyBase64: string): void` | 信任一个根公钥（base64，即 `nsh keygen` 生成的 `.pub` 文件内容）。 |
| `updateRevocations(revocationsJson: string): Promise<string>` | 应用 `nsh revoke` 生成的吊销列表；该列表须由受信任的根签名，且比信任库现有的更新。 |
| `openPackage(packagePath: string): Promise<NshPackage>` | 在工作线程验证包、解压资源并接受该包（此后该应用更旧的构建都会被拒绝）。 |
| `openResource(name: string): Promise<NshPackage>` | 验证并打开打包在 `resources/rawfile/` 中的签名 `.nsh` 包。 |
| `NshPackage` | `info`（`id`、`name`、`version`、`build`）、`isConsumed`。 |

密钥、证书和吊销机制的工作方式见[签名包](./signed-packages.md)。

## 开发

使用 `--dev` SDK（`cargo xtask ohos --dev`）时：

```ts
surface.connectDevServer('ws://127.0.0.1:8790/ws');
```

在电脑上运行 `nsh dev App.tsx`。对于模拟器或 USB 连接的设备，使用 `hdc` 转发端口，让设备自己的 `127.0.0.1` 能访问到服务器：

```sh
hdc rport tcp:8790 tcp:8790
```

否则使用 `nsh dev` 打印的局域网地址。

## Ability 配置

- **标准 `UIAbility`。** 创建 `NshSurface` 时，`@nsh/ohos` 会自动关联当前的 `UIContext`、`UIAbilityContext` 和窗口生命周期，宿主的 `EntryAbility` 保持标准的 `UIAbility`（`extends UIAbility`），无需自定义基类。如果应用使用 `<WebView>` 或原生组件，请在页面根节点挂载 `@nsh/ohos` 的 `nativeShellPlugins()`。
- **返回键。** 当应用需要处理返回键时（有打开的对话框、有 `BackHandler` 监听器、有被推入的导航页面），活动的 `NshSurface` 会优先接收返回手势，否则交给 Ability。同时展示多个视图（如多标签页）时，可在切换时调用 `surface.activate()`。
- **键盘与安全区域。** `@nsh/ohos` 自动观测系统栏、挖孔区域和软键盘避让区域，并只将与各个 `NshSurface` 重叠的部分作为 insets 上报。`Input` 使用系统 `TextInput` / `TextArea`，并自动滚动让获得焦点的输入框不被键盘遮挡。
- **生命周期。** 移除 `ContentSlot` 时，只要保留 `NshSurface` 实例，运行中的 VM 和状态就会保持；重新挂载会恢复原生节点树。调用 `surface.stop()` 可清空当前运行的应用并保留视图，页面永久销毁时在 `aboutToDisappear` 中调用 `surface.dispose()`。

## 另请参阅

- [用 Rust 开发](./rust.md)
- [嵌入 iOS 应用](./ios.md)
- [嵌入 Android 应用](./android.md)
- [原生组件](./native-components.md)
- [签名包](./signed-packages.md)
- [沙箱与能力](./sandbox-and-capabilities.md)
