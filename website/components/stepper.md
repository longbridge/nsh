---
title: Stepper
description: A number with − and + buttons within a range; holding a button keeps stepping; a ref reads and sets it.
order: 10
demo: ui-stepper
---

# Stepper

`Stepper` shows a number between a − and a + button. Each press changes the number by `step`, within `min` and `max`.
Holding a button keeps stepping. The timer for the repeat runs in the core, so JS only receives `onChange` with each
new value.

```tsx
import { Stepper, type StepperRef } from "nsh/ui/mobile";
```

## Usage

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

## Controlled and uncontrolled

- **Controlled**: pass `value` and update it in `onChange`. The stepper shows `value`. A press reports the next value
  with `onChange`, and the number changes when you set the new `value`.
- **Uncontrolled**: leave out `value`. The stepper keeps its own value, starting at `defaultValue`, and reports each
  change with `onChange`.

```tsx
<Stepper defaultValue={5} min={0} max={50} onChange={(v) => console.log("now", v)} />
```

## Range, step and decimals

- The value always stays within `min` and `max`. If `min` is larger than `max`, the two are swapped. A negative `step`
  is used as positive.
- The number is shown with as many decimals as `step` has: `step={0.25}` shows `1.75`, `step={1}` shows `2`.
- Values are rounded to that number of decimals, which avoids floating-point drift such as `0.30000000000000004`.
- When `min`, `max` or `step` change, the value is moved back into the new range.

```tsx
<Stepper value={weight()} min={0} max={5} step={0.5} onChange={(v) => weight.set(v)} />
```

## Holding a button

A press steps once immediately. If the button is still held after 400 ms, the stepper steps again every 80 ms until the
button is released or the value reaches `min` or `max`. At a bound, that button is dimmed and disabled.

## Ref methods

Pass a `ref` to get a `StepperRef`, which reads and sets the value from your code:

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

| Member | Description |
|---|---|
| `setValue(value: number): void` | Sets the value, rounded to the step's decimals and kept within the range, and reports it with `onChange` as a press would. On a controlled stepper, the number changes when you update `value`. |
| `value(): Promise<number>` | The value shown. It is a promise because the value lives in the core, not in JS. |
| `node: YNode` | The stepper's host node. |

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `value` | `number \| null` | none (uncontrolled) | The value, when the app controls it. |
| `defaultValue` | `number` | `0` | The first value, when the app does not control it. |
| `min` | `number` | `0` | The smallest value. |
| `max` | `number` | `100` | The largest value. |
| `step` | `number` | `1` | How much a press changes the value. |
| `disabled` | `boolean` | `false` | No presses; both buttons dimmed. |
| `ref` | `StepperRef \| ((ref: StepperRef) => void)` | none | Receives the [ref](#ref-methods). |
| `class`, `classList`, `style`, `testID`, `onLayout` | | | Applied to the stepper's root view (the row). |

## Events

| Event | Payload | When |
|---|---|---|
| `onChange` | `number`: the new value | A press, a held button's repeat or `setValue` changed the value. Not sent when the value would not change (at a bound). |

## Theming

| Part | Theme key |
|---|---|
| Buttons | `secondary.background`, `secondary.active.background` while pressed, corner radius `radius` |
| − and + | `secondary.foreground`, `font.size` + 4 |
| Number | `foreground`, `font.size` + 1 |

The buttons are 40 × 40 pt. A disabled button, or one at a bound, is shown at 40% opacity. See
[Theming](./index.md#theming).

## Accessibility

The buttons have the `button` role and the labels "Decrease" and "Increase". A button at a bound, or on a disabled
stepper, is reported as disabled. The number is a text that screen readers read as it is.

## Related

- [Group](./group.md): steppers in settings rows.
- [Input](../elements/input.md): free numeric input with `keyboardType`.
