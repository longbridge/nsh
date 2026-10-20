---
title: Stepper
description: 带 − 和 + 按钮、限定范围的数字；按住按钮会连续步进；可通过 ref 读取和设置数值。
order: 10
demo: ui-stepper
---

# Stepper

`Stepper` 在 − 和 + 两个按钮之间显示一个数字。每按一次，数字按 `step` 变化，并保持在 `min` 与 `max` 之间。按住按钮会连续步进。连续步进的计时器在核心中运行，JS 只会收到携带每个新值的 `onChange`。

```tsx
import { Stepper, type StepperRef } from "nsh/ui/mobile";
```

## 用法

```tsx
import { mount, signal, Text, View } from "nsh";
import { Stepper } from "nsh/ui/mobile";

function App() {
  const guests = signal(2);
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 48, gap: 12 }}>
      <Text>{`${guests()} guests`}</Text>
      <Stepper value={guests()} min={1} max={12} onChange={(v) => guests.set(v)} />
    </View>
  );
}

mount(App);
```

## 受控与非受控

- **受控**：传入 `value` 并在 `onChange` 中更新它。Stepper 显示 `value`；按下按钮时通过 `onChange` 报告下一个值，等你设置了新的 `value` 后数字才会变化。
- **非受控**：不传 `value`。Stepper 自己保存数值，从 `defaultValue` 开始，并通过 `onChange` 报告每次变化。

```tsx
<Stepper defaultValue={5} min={0} max={50} onChange={(v) => console.log("now", v)} />
```

## 范围、步长与小数位

- 数值始终位于 `min` 与 `max` 之间。如果 `min` 大于 `max`，两者会互换；负的 `step` 按正值处理。
- 显示的小数位数与 `step` 的小数位数相同：`step={0.25}` 显示 `1.75`，`step={1}` 显示 `2`。
- 数值会四舍五入到这个小数位数，从而避免 `0.30000000000000004` 这类浮点误差。
- `min`、`max` 或 `step` 改变时，数值会被移回新的范围之内。

```tsx
<Stepper value={weight()} min={0} max={5} step={0.5} onChange={(v) => weight.set(v)} />
```

## 按住按钮

按下时立即步进一次。如果 400 ms 后按钮仍被按住，Stepper 会每 80 ms 再步进一次，直到松开按钮，或数值到达 `min` 或 `max`。到达边界时，对应的按钮会变暗并禁用。

<a id="ref-methods"></a>
## Ref 方法

传入 `ref` 即可获得一个 `StepperRef`，用来在代码中读取和设置数值：

```tsx
import { mount, View } from "nsh";
import { Button, Stepper, type StepperRef } from "nsh/ui/mobile";

function App() {
  let stepper!: StepperRef;
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 48, gap: 12 }}>
      <Stepper ref={stepper} defaultValue={3} max={10} />
      <Button
        title="Reset"
        onPress={async () => {
          console.log("was", await stepper.value());
          stepper.setValue(0);
        }}
      />
    </View>
  );
}

mount(App);
```

| 成员 | 说明 |
|---|---|
| `setValue(value: number): void` | 设置数值（四舍五入到步长的小数位数，并限制在范围内），并像按下按钮一样通过 `onChange` 报告。对受控的 Stepper，数字要等你更新 `value` 后才会变化。 |
| `value(): Promise<number>` | 当前显示的数值。返回 Promise，因为数值保存在核心中，而不在 JS 里。 |
| `node: YNode` | Stepper 的宿主节点。 |

## 属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `value` | `number \| null` | 无（非受控） | 由应用控制时的数值。 |
| `defaultValue` | `number` | `0` | 不由应用控制时的初始值。 |
| `min` | `number` | `0` | 最小值。 |
| `max` | `number` | `100` | 最大值。 |
| `step` | `number` | `1` | 每按一次数值的变化量。 |
| `disabled` | `boolean` | `false` | 不响应按下，两个按钮都变暗。 |
| `ref` | `StepperRef \| ((ref: StepperRef) => void)` | 无 | 接收 [ref](#ref-methods)。 |
| `class`、`classList`、`style`、`testID`、`onLayout` | | | 作用于 Stepper 的根视图（整行）。 |

## 事件

| 事件 | 参数 | 触发时机 |
|---|---|---|
| `onChange` | `number`：新值 | 按下、按住时的连续步进或 `setValue` 改变了数值。数值不会变化时（位于边界）不触发。 |

## 主题

| 部位 | 主题键 |
|---|---|
| 按钮 | `secondary.background`，按下时为 `secondary.active.background`，圆角为 `radius` |
| − 和 + | `secondary.foreground`，字号 `font.size` + 4 |
| 数字 | `foreground`，字号 `font.size` + 1 |

按钮尺寸为 40 × 40 pt。禁用的按钮或位于边界的按钮以 40% 不透明度显示。参见[主题](./index.md#theming)。

## 无障碍

两个按钮的角色都是 `button`，标签分别为 "Decrease" 和 "Increase"。位于边界的按钮，或整个 Stepper 被禁用时，按钮会报告为已禁用。数字是普通文本，屏幕阅读器按原样朗读。

## 相关

- [Group](./group.md)：在设置行中使用 Stepper。
- [Input](../elements/input.md)：配合 `keyboardType` 自由输入数字。
