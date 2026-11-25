---
title: 向下兼容
description: Native Shell 如何让应用跨版本持续运行：兼容版本的承诺、nsh-compat 中约束每次改动的记录、发版流程，以及如何新增、重命名或修改 API。
order: 22
group: delivery
---

# 向下兼容

Native Shell 对每个应用承诺：**为某个兼容版本构建的应用，在之后的每个版本上都能原样运行。** NSH 升级后应用不能正常工作，就是 NSH 的 bug。

之所以要这样承诺，是因为发布出去的应用可能再也不会重新构建。宿主应用以[签名包](./signed-packages.md)的形式下载 NSH 应用，当作插件运行。应用的团队可能已经解散，源码可能封存在业务流程里，宿主应用也会按自己的节奏升级 NSH。

本页说明这个承诺的内容、NSH 怎样保证它，以及怎样修改 NSH 而不破坏它。

## 应用可以依赖什么

承诺覆盖构建好的应用接触到的一切：

| 部分 | 为什么重要 |
| --- | --- |
| 每个模块（`nsh`、`nsh/*`、`nsh/base`、标准模块）的导出及其成员 | bundle 按名字导入它们。少一个名字，应用就无法启动。 |
| `nsh:rt`，编译后的 JSX 调用的函数 | 每个 bundle 都带着构建它的编译器的输出。 |
| 全局对象（定时器、`fetch`、`console` 等）及其成员 | 应用按名字调用它们。 |
| 元素的属性、事件及其载荷、样式键及其取值 | 应用按名字传入。属性改名后旧名字会被忽略，应用就会运行出错。 |
| Tailwind 工具类与变体 | 类名在运行时从 bundle 中的字符串解析。 |
| 行为：默认值、单位、事件载荷、回调顺序 | 应用依赖它编写时所看到的行为。 |
| TypeScript 声明（`.nsh/types`） | 团队修 bug 时，旧应用的源码仍能通过类型检查。 |
| 包格式及其签名 | 为早期版本签名的包依然能通过验证。 |

NSH 靠以下规则兑现承诺：

1. **版本发布过的内容一直保留。** 模块、导出、成员、属性、事件、载荷键、样式键及取值、Tailwind 工具类、全局对象和 `nsh:rt` 函数都不会消失。
2. **重命名保留旧名字**，作为同一实现的别名，标注 `@deprecated` 并指向新名字，永久保留。把导出移到另一个模块时，旧模块也继续导出它。
3. **类型只会放宽。** 联合类型可以增加选项，函数可以增加重载或末尾的可选参数，参数可以变为可选。应用传入的内容（属性、样式、选项）只增加可选成员。应用读到的值也一样：`platform.os` 可能多出新平台，状态和原因可能多出新情况。已有的情况保持原值。应用应在 `switch` 的 `default` 里处理不认识的值：

   ```tsx
   switch (platform.os) {
     case "ios":
     case "android":
       return <MobileLayout />;
     default:
       // 桌面、Web，以及 NSH 以后新增的平台。
       return <WideLayout />;
   }
   ```
4. **行为保持不变。** 新行为需要主动开启：通过新的属性、选项或取值。
5. **`nsh:rt` 只增不减**，每个函数保持签名与含义。
6. **格式永远可读。** 之后的版本都能读取早期版本的包、证书、吊销列表和 `app.json`。包会声明自己需要的最低运行时（`nsh publish --min-runtime`）。
7. **安全是唯一的例外。** 某项能力无法保证安全时，NSH 会关闭它，列出原因，并写进发版说明。

承诺从第一个被记录为兼容的版本开始，永久有效，与版本号无关。两次发版之间，只存在于 `main` 上的 API 仍可以修改。

## 如何保证

各个版本冻结的内容保存在独立的仓库 [longbridge/nsh-compat](https://github.com/longbridge/nsh-compat) 中，与 NSH 的代码分开。NSH 的改动要通过它的检查，但不能修改它。所以一个重命名 API 的 PR，没法顺手改掉这个 API 的记录。

```
nsh                                       nsh-compat
├─ crates/          代码                  ├─ releases/v<version>/   每个兼容版本一份，从不修改
└─ tests/compat/    测试开发中 API 的     │  ├─ runtime.txt         每个导出、全局对象和 nsh:rt 函数
   ├─ cases/        应用                  │  ├─ types/              应用构建时使用的声明
   └─ examples/                           │  ├─ tailwind.txt        每个 Tailwind 类及其设置的样式
                                          │  └─ apps/               该版本构建出的测试应用
                                          ├─ closed.txt             因安全原因关闭的 API 及原因
                                          └─ src/                   检查程序
```

NSH 的每个 PR 都会用当前代码跑 `nsh-compat` 的检查（**Compat** workflow，本地用 `cargo xtask compat`）：

| 检查 | 通过条件 |
| --- | --- |
| `checkout_apps_run` | `tests/compat/` 中的应用由当前代码构建后，行为符合各自的脚本；脚本声明检查的每个 API 都确实被检查到。 |
| `nothing_released_is_removed` | 每个已记录版本提供的一切（导出、成员及其类型、声明、Tailwind 类及其样式）仍然提供，保持不变或更宽。 |
| `released_apps_run` | 每个已记录版本构建出的应用（由当时的编译器构建）能在当前运行时上启动，行为符合当时的脚本。 |

第一项检查测试开发中的 API，另外两项守住每个已记录的版本。重命名一个已发布的 API 会这样失败：

```
v0.2.0: runtime [nsh] Platform: removed
Keep it: a rename keeps the old name as a @deprecated alias of the new one, a type only widens.
```

`nsh-compat` 由 NSH 的维护者审核，它自己的 CI 也会用 NSH 的 `main` 来跑。

## 发版流程

1. **开发。** 新 API 合入 `main` 时，要在 `tests/compat/` 里带一个用例。用例是一个小测试应用，用脚本检查这个 API。在某个版本记录它之前，API 可以改名或调整，用例跟着改。
2. **记录。** 发版时，运行 `cargo xtask compat record <version>`，在 `nsh-compat` 的检出目录中写入 `releases/v<version>/`。记录包括该版本的 API 表面，以及用该版本编译器构建的 `tests/compat/` 应用和脚本。只有每个部分都有脚本检查的 API 才会被记录。
3. **冻结。** 记录通过 `nsh-compat` 的 PR 合入。此后 NSH 的每次改动都要通过它和之前的所有记录。

## 在承诺之内开发

### 新增 API

每个新的导出、全局对象、属性、事件或样式键都要带一个用例：

1. 编写模块 `tests/compat/cases/<domain>/<case>.tsx`，默认导出一个使用该 API 并显示其结果的组件。domain 有 `base`、`core`、`events`、`layout`、`paint`、`props`、`modules`、`web`，以及由某个平台或某个 `nsh.json` 设置决定的 `android`、`macos`、`browser`。
2. 把它加到该 domain 的 `App.tsx` 中，`App.tsx` 一次显示一个用例。
3. 在该 domain 的 `script.txt` 末尾追加步骤：`tap id:case-<case>`，然后是检查 API 行为的步骤，并为新增的每个部分写一个 `covers <item>`。
4. 运行 `cargo xtask compat`（`NSH_COMPAT_APP=<domain>` 只跑一个 domain）。

```txt
# props-media-video
tap id:case-props-media-video
prop id:clip src https://example.com/clip.mp4
prop id:clip paused true
tap id:play
prop id:clip paused false
covers [props] src
covers [props] paused
```

NSH 会验证每个 `covers` 步骤。应用必须用到该项。如果是属性、事件或样式键，从构建好的应用里去掉它后，脚本中必须有步骤失败。各项的写法是 `[<module>] <export>`、`[globals] <name>`、`[props] <name>`、`[events] <name>`、`[styles] <name>`，组件自己的属性写作 `[props] <Component>.<name>`，平台属性写作 `ios.<name>`、`android.<name>`、`desktop.<name>`、`web.<name>`。

### 重命名或移动 API

加上新名字，保留旧名字作为同一实现的别名，标注 `@deprecated` 并指向新名字。把用例和脚本改成新名字。尚未被任何版本记录的 API 直接改名即可。

### 修改类型

只能放宽：联合类型增加选项、增加重载、末尾增加可选参数、应用传入的内容增加可选成员。应用读到的联合类型也这样增加成员，每个新成员对应一种新情况。参数名可以随意修改；参数的数量、是否可选和类型保持不变。

### 修改行为

保留旧行为，把新行为放在新的属性、选项或取值后面。修 bug 时如果改变了正常应用看到的结果，也算行为变化。要么做成需要主动开启，要么确认没有应用依赖旧结果。再在用例里为这个行为写一个步骤，把它固定下来。

### 修改编译器

不需要额外操作：每个版本的应用保持当时编译器构建的样子，用例则由你的编译器重新构建。编译器写进 bundle 的函数（`nsh:rt`）只增不减。

### 因安全原因关闭 API

先给 `nsh-compat` 提一个 PR，把条目和原因写进 `closed.txt`，格式是 `<entry> -- <reason>`，条目照检查报告的写法填。这个 PR 合入后，删除该 API 的 NSH 改动才能通过。发版说明里写明原因。

## 脚本

`script.txt` 每行一个步骤。

操作类步骤：`tap <selector>`、`type <selector> <text>`、`key <selector> <key>`、`scroll <selector> <x> <y>`、`touch start|move|end|cancel`、`event <selector> <onEvent> [<key>=<value>...]`、`wait <ms>`、`keyboard <height> <duration>`、`pixelratio <ratio>`。

检查类步骤：`expect <text>`、`absent <text>`、`frame <selector> <x> <y> <width> <height>`、`origin`、`prop` / `noprop`、`style` / `nostyle`、`listens`、`state`、`statusbar`、`log <text>`。

开头几行可以设置屏幕：`viewport <w> <h>`、`platform <os> [desktop]`、`fontscale <scale>`。

选择器是 `id:<testID>`、`label:<label>`、`text:<text>` 或 `span:<text>`。属性和样式按应用的写法书写（`style id:card backgroundColor #3366ffff`）。编写脚本时，`NSH_COMPAT_DUMP=1` 会打印每一步在屏幕上增加和移除的内容。

## 命令

```sh
cargo xtask compat                      # 用当前代码跑检查
NSH_COMPAT_APP=props cargo xtask compat -v
cargo xtask compat record 0.2.0         # 发版时：在 nsh-compat 中记录该版本，再到那里提 PR
```

`cargo xtask compat` 在 `target/nsh-compat/` 中维护一份 `nsh-compat` 的检出，每次运行时更新它。如果这份检出有本地改动，命令会原样使用它。这样你可以在提 PR 之前，先拿 NSH 试跑对检查程序的修改。
