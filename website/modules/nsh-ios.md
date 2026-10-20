---
title: nsh/ios
description: "iOS-only APIs: isAvailable and InputAccessoryView, a bar over the keyboard."
order: 11
---

# nsh/ios

`nsh/ios` holds APIs that only iOS has. The module can be imported on every platform, so one bundle runs everywhere: elsewhere its components render nothing, and `isAvailable` tells you whether you are on iOS.

```ts
import { InputAccessoryView, isAvailable } from "nsh/ios";
```

iOS-only *props* of elements and components are not here: they go in the `ios={{…}}` prop (for example `<Input ios={{ textContentType: "oneTimeCode" }} />`). See [Platform-specific code](../docs/platform-specific.md).

## isAvailable

```ts
const isAvailable: boolean;
```

`true` on iOS, the iOS simulator and Mac Catalyst; `false` elsewhere.

## InputAccessoryView

```ts
function InputAccessoryView(props: InputAccessoryViewProps): YNode | null;

interface InputAccessoryViewProps {
  nativeID: string;
  style?: StyleProp<ViewStyle>;
  class?: string;
  children?: JSX.Element;
}
```

A bar over the keyboard (UIKit's `inputAccessoryView`), shown while an input whose `ios={{ inputAccessoryViewID }}` matches its `nativeID` has focus. It is as wide as the window and as tall as its content, and moves with the keyboard natively.

| Prop | Type | Description |
|---|---|---|
| `nativeID` | `string` | Inputs with this `inputAccessoryViewID` show the bar. |
| `style` | `StyleProp<ViewStyle>` | The bar's style (its background, for example). |
| `class` | `string` | Tailwind classes. |
| `children` | `JSX.Element` | The bar's content. |

It renders nothing where it is placed in the tree, and nothing at all on Android, so you can place it next to the input it belongs to.

```tsx
import { Input, signal, Text, View, type YNode } from "nsh";
import { InputAccessoryView } from "nsh/ios";

function Note() {
  const text = signal("");
  let input: YNode | undefined;
  return (
    <View>
      <Input
        ref={(n) => (input = n)}
        value={text()}
        onChange={(v) => text.set(v)}
        placeholder="Write a note"
        ios={{ inputAccessoryViewID: "note-bar" }}
      />
      <InputAccessoryView nativeID="note-bar" style={{ backgroundColor: "#f1f5f9" }}>
        <View style={{ flexDirection: "row", alignItems: "center", padding: 10 }}>
          <Text style={{ flex: 1, color: "#64748b" }}>{text().length} characters</Text>
          <Text onPress={() => input?.blur()} style={{ fontWeight: "600" }}>Done</Text>
        </View>
      </InputAccessoryView>
    </View>
  );
}
```

See also [Forms and keyboard](../docs/forms-and-keyboard.md).
