---
title: nshm
description: "nshm（NSH 模块）：包含组件、模块和类的 Rust crate，任何 Native Shell 应用都可以用 nsh add 安装，并在 Rust 和 TSX 中同样使用；以及如何编写和发布、版本与锁文件。"
order: 5.7
group: start
---

# nshm

**nshm**（NSH 模块）是为 Native Shell 应用提供组件、模块和类的 Rust crate，发布在 crates.io 或 Git 上。应用用 `nsh add` 安装后，可以在 Rust 和 TSX 中使用它：同一份实现，在每个平台上都是原生的。

例如 [nsh-omarchy](https://github.com/huacnlee/nsh-omarchy) 为应用带来 [Omarchy](https://omarchy.org) 的风格：按钮、输入框、菜单、对话框、表格和图表，并随系统的 Omarchy 主题实时切换。

```sh
nsh add nsh-omarchy                  # 来自 crates.io
nsh add github:huacnlee/nsh-omarchy  # 或来自 GitHub
```

然后用应用所写的语言使用它，Rust 或 TSX 均可。**Rust 应用**中，它的组件就是 crate 里的函数：

```rust
// src/lib.rs：应用的根组件
use nsh_core::prelude::*;
use nsh_omarchy::tokens;

#[component]
pub fn App() -> impl IntoElement {
    // 使用系统当前的 Omarchy 主题绘制，并跟随它的变化。
    nsh_omarchy::init();
    let notify = signal(true);
    let saved = signal(0_u32);
    view()
        .flex_1()
        .p(px(tokens::SPACE_6))
        .gap(px(tokens::SPACE_4))
        .bg(move || tokens::background())
        .child(
            nsh_omarchy::panel("Settings")
                .child(
                    nsh_omarchy::switch("Notifications")
                        .checked(move || notify.get())
                        .on_change(move |on: &bool| notify.set(*on)),
                )
                .child(nsh_omarchy::button("Save").primary().on_press(move || {
                    saved.update(|n| *n += 1);
                    nsh_omarchy::toast(nsh_omarchy::Toast::new("Settings saved"));
                })),
        )
        .child(nsh_omarchy::body(move || format!("Saved {} times", saved.get())))
        .child(nsh_omarchy::toaster())
}
```

**TSX 应用**中，只要 nshm 向 TSX 导出了组件、模块或类，`nsh add` 安装后就能直接使用：按它的标识导入即可，这里是 `nsh-omarchy`。不需要升级 NSH，也不需要自己写 Rust 代码来扩展，`nsh` 会用这个 crate 构建应用的宿主。

```tsx
// App.tsx
import { mount, signal, View } from "nsh";
import { Body, Button, Omarchy, Panel, Switch, Toaster, toast } from "nsh-omarchy";

function App() {
  // 使用系统当前的 Omarchy 主题绘制，并跟随它的变化。
  Omarchy.followSystem();
  const notify = signal(true);
  const saved = signal(0);
  const save = () => {
    saved.set(saved() + 1);
    toast({ title: "Settings saved" });
  };
  return (
    <View class="flex-1 gap-4 p-6 bg-background">
      <Panel title="Settings">
        <Switch label="Notifications" checked={notify()} onChange={({ checked }) => notify.set(checked)} />
        <Button label="Save" variant="primary" onPress={save} />
      </Panel>
      <Body>{`Saved ${saved()} times`}</Body>
      <Toaster />
    </View>
  );
}

mount(() => <App />);
```

nshm 让功能不必成为 NSH 的一部分，也能交到需要它的应用手中：它按自己的节奏发布，只有安装它的应用才承担它的体积、构建时间和依赖。

## 使用 nshm

`nsh add` 可以从 crates.io、GitHub、任意 Git 地址或本地目录安装 nshm：

```sh
nsh add nsh-omarchy                                             # crates.io：最新版本
nsh add github:huacnlee/nsh-omarchy                             # GitHub：默认分支，或 @<标签>
nsh add https://github.com/huacnlee/nsh-omarchy.git --rev main  # 任意 Git 地址
nsh add --path ../nsh-omarchy                                   # 本地目录，按当前内容使用
```

它把 nshm 记录到 `nsh.json` 的 `"modules"`，把解析到的版本或提交锁定在 `nsh.lock`（两者都随应用提交），应用有自己的 crate 时还会把它加入该 crate 的 `[dependencies]`。所有命令都用锁定的版本构建，再次运行 `nsh add` 即更新。纯 TSX 应用会像带 crate 的应用一样用它的 nshm 构建宿主，因此开发机和 CI 需要 Rust 工具链（[用 Rust 开发](./rust.md)）。

```json
{
  "modules": ["nsh-omarchy", { "path": "../my-gauge" }]
}
```

包含多个模块的 crate 把每个模块放在一个 Cargo feature 后面，应用只开启自己用到的，也只承担这些模块的体积、权限和商店声明。`--features` 用来开启它们，条目随之写成对象形式：

```sh
nsh add nsh-extra --features camera
```

```json
{
  "modules": [{ "crate": "nsh-extra", "features": ["camera"] }]
}
```

[CLI：nsh add](./cli.md#nsh-add) 介绍了每种来源写法、版本要求以及锁文件的维护方式。

## nsh-extra

`nsh-extra` 是 Native Shell 的官方模块集：只有部分应用需要的功能，每个都是这个 crate 的一个 Cargo feature，TSX 从 `nsh-extra` 导入，Rust 里调用 `nsh_extra::<模块>`。

| Feature | TSX | 作用 |
|---|---|---|
| `camera` | `Camera` | 在 iOS 和 Android 上用系统相机拍一张照片 |

```tsx
import { Camera } from "nsh-extra";

if (await Camera.available()) {
  // 照片和 `Picker.pickImages` 选出的图片一样：JPEG、方向已摆正、在原生侧缩放。
  const photo = await Camera.takePhoto({ maxDimension: 2048 });
  if (photo) upload(photo.file);
}
```

用户取消时 `takePhoto` 返回 `null`；用户不允许应用使用相机时，它以错误码 `EACCES` 拒绝。没有相机可用的地方（桌面、Web、模拟器）`available()` 为 `false`，`takePhoto` 以 `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` 拒绝。开启这个 feature 后，打包的应用会带上它需要的条目：iOS 上的相机用途说明（应用可以在 `ios.infoPlist` 里写自己的 `NSCameraUsageDescription` 文案），Android 上对相机应用的查询声明。

## 编写 nshm

nshm 是普通的 Cargo 库，根部有 `#[native_module("<标识>")]` 入口，与应用自己的 crate 相同：标识就是 TSX 导入它的名字，函数注册它导出的内容（[用 TSX 扩展 Rust 应用](./rust.md#用-tsx-扩展-rust-应用)）。一个小例子：

```toml
# Cargo.toml
[package]
name = "my-gauge"
version = "0.1.0"
edition = "2024"
keywords = ["nshm"]

[dependencies]
nsh-core = "0.6"
```

```rust
// src/lib.rs
use nsh_core::prelude::*;

#[native_module("my-gauge")]
fn native(registry: &mut Registry) {
    registry.register::<Gauge>();
}

#[derive(Clone, PartialEq, Props)]
pub struct GaugeProps {
    /// The value shown, from 0 to 1.
    pub value: Prop<f32>,
}

/// A horizontal gauge.
#[component]
pub fn Gauge(props: GaugeProps) -> impl IntoElement {
    view()
        .h(px(8.0))
        .rounded_full()
        .bg(rgb(0xe5e7eb))
        .child(
            view()
                .h_full()
                .rounded_full()
                .bg(rgb(0x2563eb))
                .w(move || relative(props.value.get())),
        )
}
```

nshm 兼容的 NSH 版本就是它自己对 `nsh-core` 的版本要求（`nsh-core = "0.6"`），`nsh add` 在编译前会与当前 `nsh` 比对。它的 TypeScript 声明由 Rust 类型生成，应用检查 TSX 时会像检查自己的 crate 一样检查对它的使用。

[nsh-omarchy 的 `src/lib.rs`](https://github.com/huacnlee/nsh-omarchy/blob/main/src/lib.rs) 是一个完整的例子：它的组件由 `#[native_module("nsh-omarchy")]` 导出给 TSX，另有 `Omarchy` 主题模块。

### 打包的应用需要什么

用到系统功能的 nshm 在 `Cargo.toml` 里声明打包的应用需要什么：写在 `[package.metadata.nsh]` 下；只有某个 feature 需要时，写在 `[package.metadata.nsh.features.<feature>]` 下。`nsh` 构建安装包时，会把应用链接的每个 crate 的条目（按 Cargo 实际开启的 feature）加进去；开发宿主拿到其中的 `Info.plist` 键和 Android 条目：

```toml
[package.metadata.nsh.features.scanner.ios.infoPlist]
NSCameraUsageDescription = "扫描相机对准的条码。"

[package.metadata.nsh.features.scanner.android]
permissions = ["CAMERA"]
queries = ['<intent><action android:name="android.media.action.IMAGE_CAPTURE"/></intent>']
application = ['<service android:name="com.example.scanner.Sync" android:exported="false"/>']
```

| 键 | 添加的内容 |
|---|---|
| `ios.infoPlist` | iOS 应用 `Info.plist` 的键，与 `nsh.json` 的 `ios.infoPlist` 相同 |
| `ios.entitlements` | iOS 应用的 entitlements |
| `android.permissions` | `<uses-permission>` 元素，写法与 `nsh.json` 相同（`CAMERA` 即 `android.permission.CAMERA`） |
| `android.queries` | manifest `<queries>` 里的 `<intent>`、`<package>`、`<provider>` 元素 |
| `android.application` | `<application>` 里的 `<service>`、`<receiver>`、`<provider>`、`<activity>`、`<meta-data>` 元素；`${applicationId}` 是应用的包名 |

应用自己的 `nsh.json` 优先：它设置过的键保留它的值，数组（例如 `UIBackgroundModes`）合并。两个 crate 给同一个键设置了不同的值时（例如两段不同的相机用途说明），由应用在 `ios.infoPlist` 或 `ios.entitlements` 里写这个键来决定；应用没写之前，打包 iOS 应用会报错并指出这两个 crate。NSH 自己设置的键（包标识、版本等）不能由 crate 设置。

在应用旁边开发时用 `nsh add --path ../my-gauge`，按目录当前内容使用。发布方式与普通 crate 相同（`cargo publish`），加上关键词 `nshm`，便于在 [crates.io](https://crates.io/keywords/nshm) 上找到；也可以推送到 Git，分享它的地址。

## 安全

nshm 的构建脚本和过程宏会在应用构建时执行，其代码以应用的权限运行。应用依赖图里的任何 crate 还能通过 `[package.metadata.nsh]` 给打包的应用加系统权限、清单组件和 entitlements。请像审查应用自己的代码一样审查它的源码和 `nsh.lock` 的变更。
