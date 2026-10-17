---
title: 项目结构
description: NSH 应用包含的文件、nsh.json 的每个键、.nsh/types 中的 TypeScript 配置、资源，以及模块和包的导入方式。
order: 5
group: start
---

# 项目结构

一个 NSH 应用就是一个目录，包含一个入口模块和一个可选的 `nsh.json`。其他一切（更多模块、图片、字体、JSON 文件）都通过跟随入口的导入找到，因此不需要维护文件清单。

```text
my-app/
├── App.tsx            the entry module: calls mount()
├── nsh.json           name, id, version, icon, Tailwind theme, capabilities
├── tsconfig.json      TypeScript settings for editors
├── .gitignore         .nsh/ and out/
├── components/        your own modules (any layout you like)
│   └── Header.tsx
├── assets/
│   ├── logo@2x.png    imported images
│   └── Inter.ttf      imported fonts
├── .nsh/              generated: type declarations, development host builds
└── out/               generated: nsh build / nsh package output
```

每个命令都以入口作为参数（`nsh run my-app/App.tsx`）。包含入口的目录就是应用的根目录：`nsh.json` 从这里读取，资源路径相对于它，`.nsh/` 也写在这里。

## 入口模块

入口是一个普通模块，最后渲染根组件：

```tsx
import { mount } from "nsh";
import { Home } from "./components/Home";

function App() {
  return <Home />;
}

mount(App);
```

`mount(component, props?)` 把组件渲染到应用的界面上，并返回一个用于移除它的函数。应用只调用它一次。入口顶层的代码在 bundle 加载时、第一帧之前运行。

## nsh.json

`nsh.json` 位于入口旁边。所有键都是可选的；`nsh init` 会写入前四个。

```json
{
  "name": "Gallery",
  "id": "com.example.gallery",
  "version": "1.0.0",
  "build": 1,
  "icon": "icon.png",
  "tailwind": {
    "colors": { "brand": { "500": "#7c3aed", "600": "#6d28d9" } },
    "radius": { "card": 14 }
  },
  "capabilities": {
    "network": { "hosts": ["api.example.com", "*.cdn.example.com"] }
  }
}
```

| 键 | 类型 | 使用者 | 说明 |
|---|---|---|---|
| `name` | string | `package` | 应用图标下显示的名称。默认：入口所在目录的名称。 |
| `id` | string | `package`, `publish` | Bundle identifier / application id，例如 `com.example.app`。默认：`com.example.<name>`。 |
| `version` | string | `package`, `publish` | 用户可见的版本号，例如 `1.2.0`（`CFBundleShortVersionString`，Android 的 `versionName`）。 |
| `build` | number | `package`, `publish` | 构建号（`CFBundleVersion`，Android 的 `versionCode`）。签名包要求它在每次发布时递增。 |
| `icon` | string | `package` | 一张正方形 PNG，最好是 1024×1024，路径相对于 `nsh.json`。 |
| `tailwind` | object | 每次构建 | 扩展 `class` 字符串所依据的 Tailwind 主题（[Tailwind](./tailwind.md)）。 |
| `theme` | string 或 object | 每次构建 | `nsh/ui/mobile` 组件的主题：相对于 `nsh.json` 的主题文件路径，或直接内联的主题（[组件](../components/index.md)）。 |
| `capabilities` | object | 每次构建 | 应用可以访问 VM 之外的哪些资源：网络主机、网页视图主机、状态栏（[沙箱与权限](./sandbox-and-capabilities.md)）。 |

`nsh package` 和 `nsh publish` 的参数（`--name`、`--id`、`--app-version`、`--build`、`--icon`）会覆盖文件中的值（[CLI](./cli.md)）。

`tailwind`、`theme` 和 `capabilities` 在构建应用时会被校验。拼写错误会导致构建失败，而不是变成一个悄无声息不起作用的设置：

```text
error building App.tsx failed
nsh.json: tailwind: unknown theme key `colours`
```

> [!WARNING]
> `nsh package` 和 `nsh publish` 会严格读取 `nsh.json`，目前会拒绝它们不认识的键，包括 `theme`。如果你设置了组件主题，在此问题修复之前，请用去掉该键的应用副本进行打包。

### capabilities

没有 `capabilities` 键时，应用可以自由使用网络、网页视图和状态栏：独立应用归开发者自己所有。设置了该键后，未列出的一切都会被拒绝，无论是开发中还是打包后的应用：

```json
"capabilities": {
  "network": {
    "hosts": ["api.example.com", "*.cdn.example.com"],
    "http": [{ "host": "quotes.example.com", "methods": ["GET"], "pathPrefix": "/v1/" }]
  },
  "webview": { "hosts": ["*.example.com"] },
  "statusBar": true
}
```

`network` 涵盖 `fetch`、远程图片和视频源；本地资源始终允许访问。嵌入 NSH 的应用可以授予比应用声明更少的权限。详情见 [沙箱与权限](./sandbox-and-capabilities.md)。

## tsconfig.json 与 .nsh/types

NSH 不需要 `node_modules`。取而代之的是，每个构建应用的命令（`init`、`build`、`trace`、`dev`、`run`、`package`、`publish`）都会把 NSH 提供的所有内容的 TypeScript 声明写入 `.nsh/types`，并由 `tsconfig.json` 把 `nsh` 的导入指向那里：

```json
{
  "compilerOptions": {
    "target": "ES2023",
    "lib": ["ES2023"],
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "jsx": "preserve",
    "jsxImportSource": "nsh",
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "types": [],
    "paths": {
      "nsh": ["./.nsh/types/nsh/index.d.ts"],
      "nsh/*": ["./.nsh/types/nsh/*.d.ts"]
    }
  },
  "include": ["**/*.ts", "**/*.tsx", ".nsh/types/nsh/globals.d.ts", ".nsh/types/nsh/assets.d.ts"]
}
```

| `.nsh/types/nsh/` 中的文件 | 声明的内容 |
|---|---|
| `index.d.ts` | `nsh`：`signal`、`computed`、`effect`、`mount`、`View`、`Text` 等，并重新导出下列类型 |
| `types.d.ts` | 样式（`ViewStyle`、`TextStyle` 等）、每个元素的 props 和事件、`YNode`（`ref` 接收到的对象） |
| `jsx-runtime.d.ts` | TypeScript 通过 `jsxImportSource` 解析的 JSX 运行时（`JSX` 命名空间） |
| `globals.d.ts` | `If` 和 `Each`；`console`、定时器、`requestAnimationFrame`、`fetch` 及其他[全局 API](../modules/globals.md) |
| `assets.d.ts` | 导入图片或字体得到的值 |
| `list.d.ts`、`animated.d.ts`、`device.d.ts` 等 | 每个模块一个文件（`nsh/list`、`nsh/animated`、`nsh/device` 等） |
| `ui/mobile.d.ts` | `nsh/ui/mobile` |

`lib` 是不含 `DOM` 的 `ES2023`：没有 `document` 或 `window`。这些文件会被重新生成（只在内容变化时），所以不要编辑，也不要提交；`.gitignore` 已经排除了 `.nsh/`。如果克隆项目后编辑器提示找不到 `nsh`，运行任意一个构建命令即可，例如 `nsh trace App.tsx`。

`jsxImportSource: "nsh"` 和 `jsx: "preserve"` 只对编辑器有意义：无论 `tsconfig.json` 怎么写，NSH 都用自己的编译器编译 JSX。TypeScript 类型由编辑器检查（如果装了 `tsc` 也可以用它）；`nsh` 只剥离类型，不做检查。

## 模块

可以把应用拆成任意多个 `.ts` 和 `.tsx` 模块，用相对路径导入；扩展名可以省略：

```tsx
import { Header } from "./components/Header";
import { formatPrice } from "./lib/format";
import strings from "./strings.json";
```

它们都会被打包进同一个脚本。JSON 文件导入后就是解析后的值。

### NSH 的模块

对 `nsh` 和 `nsh/...` 的导入不会被打包：运行时以原生方式提供它们。

| 模块 | 内容 |
|---|---|
| `nsh` | 响应式（`signal`、`computed`、`effect` 等）、`mount`、`split`、`context`；`View`、`Text`、`Scroll`、`Input`（[参考](../modules/nsh.md)） |
| `nsh/ui/mobile` | 移动端 UI 组件集：组件、导航、弹窗、安全区域（[组件](../components/index.md)） |
| `nsh/list` | `List`：虚拟化、可复用的列表 |
| `nsh/animated` | 原生动画 |
| `nsh/canvas` | 2D 绘图 |
| `nsh/gesture` | `PanResponder` |
| `nsh/device` | `Platform`、`windowSize`、`colorScheme`、`fontScale`、`PixelRatio`、`I18nManager` |
| `nsh/accessibility` | 读屏与减少动态效果的状态、朗读通知 |
| `nsh/image` | `Image`（元素）、`Image.getSize`、`Image.prefetch` |
| `nsh/webview`, `nsh/video` | `WebView`、`Video` |
| `nsh/native` | 嵌入应用注册的组件 |
| `nsh/ios`, `nsh/android` | 平台专属 API |

[模块参考](../modules/index.md) 逐一介绍了它们。导入 NSH 没有的模块（`nsh/foo`）会导致构建错误。

### npm 包

NSH 本身不依赖 npm 上的任何东西，但如果你用包管理器安装了包，打包器会从 `node_modules` 目录解析裸导入。包会解析到它们的浏览器版本，因为运行时提供的是 Web API（`fetch`、streams、`URL`、`TextEncoder` 等），而不是 Node 的 API（`fs`、`Buffer`、`process`）。依赖 DOM 的包无法工作：这里没有 DOM。

`solid-js` 不能被导入：打包器会拒绝它，并提示改用 `nsh` 自己的 `signal`、`computed` 和 `effect`，它们取代了 Solid。

## 资源

图片和字体像模块一样导入。打包器会记录每个被导入的文件并随应用一起发布：`nsh build` 把它们复制到 `out/assets/`，`nsh package` 把它们放进应用，开发期间则由开发服务器提供。

```tsx
import { Text } from "nsh";
import { Image } from "nsh/image";
import logo from "./assets/logo@2x.png";   // { uri: "asset:/assets/logo@2x.png", width: 64, height: 64 }
import Inter from "./assets/Inter.ttf";     // "Inter"

<Image src={logo} />
<Text style={{ fontFamily: Inter }}>Hello</Text>
```

| 扩展名 | 导入得到 |
|---|---|
| `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.heic` | 一个 `ImageAsset`：`{ uri, width, height }`，单位为点。文件名带 `@2x` / `@3x` 后缀时，像素尺寸除以 2 / 3。 |
| `.ttf`, `.otf` | 字体的族名，一个字符串。字体会在第一帧之前注册。 |
| `.json` | 解析后的 JSON |

参见 [图片与资源](./images-and-assets.md) 和 [字体](./fonts.md)。

## 生成的目录

- **`.nsh/`**：类型声明（`.nsh/types`），以及 `nsh run` 安装到模拟器和设备上的内容（Catalyst 宿主应用、Android 开发 APK）。可以放心删除，会被重新生成。
- **`out/`**：`nsh build` 的默认输出（`bundle.js`、`bundle.js.map`、`bundle.nshc`、`capabilities.json`、`assets/`）以及 `nsh package` 的输出（`.app`、`.ipa`、`.apk`）。用 `-o` 修改。
