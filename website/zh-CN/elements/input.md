---
title: Input
description: 原生文本输入框：受控与非受控文本、键盘、自动填充、选区、多行输入和焦点。
order: 6
demo: el-input
---

# Input

`<Input>` 是原生文本输入框：iOS 和 Mac Catalyst 上是 UITextField（单行）或 UITextView（`multiline`），Android 上是 EditText。输入、光标、选区、自动更正和自动填充都由平台自己处理。

```tsx
import { Input, signal } from "nsh";

function NameField() {
  const name = signal("");
  return (
    <Input
      value={name()}
      onChangeText={(text) => name.set(text)}
      placeholder="你的名字"
      autoCapitalize="words"
      returnKeyType="done"
      style={{ fontSize: 16, padding: 12, borderWidth: 1, borderColor: "#d0d7de", borderRadius: 8 }}
    />
  );
}
```

## 受控与非受控

- **受控**：传入 `value`，并在 `onChangeText` 中更新它。JS 可以改写用户的输入（去掉空白、格式化）：输入框会立即显示用户的编辑，新的 `value` 到达后再被修正。每次原生编辑都带有一个计数，只有基于最新一次编辑的 `value` 才会被应用，因此即使某一轮 JS 执行较慢，也不会覆盖用户更新的输入。
- **非受控**：传入 `defaultValue`（或不传），从 `onChangeText` 或 `onSubmitEditing` 读取文本。

用户输入过程中必须遵守的规则以属性形式给出，由原生侧直接判断而不询问 JS：`maxLength` 限制长度，`submitBehavior`（或 `blurOnSubmit`）决定回车键的行为。参见[表单与键盘](../docs/forms-and-keyboard.md)。

## 属性

除[通用属性](./index.md)外（`style` 的类型为 `TextStyle`：字体、颜色、对齐方式和视图样式都适用）：

### 文本

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `value` | `string` | | 由 JS 控制的文本。 |
| `defaultValue` | `string` | | 初始文本。 |
| `placeholder` | `string` | | 输入框为空时显示的提示。 |
| `placeholderTextColor` | `ColorValue` | | 提示文字的颜色。 |
| `multiline` | `boolean` | `false` | 多行；回车插入换行（`submitBehavior` 另有设置时除外）。 |
| `maxLength` | `number` | | 用户最多能输入的字符数。 |
| `editable` | `boolean` | `true` | 用户可以修改文本。 |
| `secureTextEntry` | `boolean` | `false` | 隐藏文本（密码）。 |
| `allowFontScaling` | `boolean` | `true` | 跟随系统文字大小。 |
| `maxFontSizeMultiplier` | `number` | `0` | 系统缩放的上限（0 表示不限）。 |

### 键盘

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `keyboardType` | `"default" \| "number-pad" \| "decimal-pad" \| "numeric" \| "numbers-and-punctuation" \| "email-address" \| "phone-pad" \| "url"` | `"default"` | 键盘布局（[各取值](../docs/forms-and-keyboard.md#键盘与自动填充)；`numbers-and-punctuation` 在 iOS 上有负号）。 |
| `returnKeyType` | `"default" \| "done" \| "go" \| "next" \| "search" \| "send"` | `"default"` | 回车键的标签。 |
| `autoCapitalize` | `"none" \| "sentences" \| "words" \| "characters"` | `"sentences"` | 自动大写。 |
| `autoCorrect` | `boolean` | `true` | 系统自动更正。 |
| `autoComplete` | `AutoComplete \| TextContentType` | | 输入框内容的类型，用于自动填充：`"username"`、`"current-password"`、`"new-password"`、`"one-time-code"`、`"email"`、`"tel"`、`"postal-code"`、`"cc-number"`、`"off"` 等。 |
| `submitBehavior` | `"submit" \| "blurAndSubmit" \| "newline"` | `"blurAndSubmit"`（单行），`"newline"`（多行） | 回车键做什么：`"submit"` 触发 `onSubmitEditing` 并保持焦点，`"blurAndSubmit"` 提交后再失去焦点，`"newline"` 插入换行（仅多行；单行总是提交）。会提交的多行输入框在硬件键盘上仍可用 Shift+Return 或 Option / Alt+Return 换行。优先于 `blurOnSubmit`。 |
| `blurOnSubmit` | `boolean` | `true` | 回车让单行输入框失去焦点；设为 `false` 时键盘保持弹出（例如连续发送多条消息）。`submitBehavior` 取代了它。 |
| `autoFocus` | `boolean` | `false` | 出现时自动获得焦点。 |

### 光标与选区

| 属性 | 类型 | 说明 |
|---|---|---|
| `caretHidden` | `boolean` | 不绘制光标。 |
| `selectionColor` | `ColorValue` | 光标和选区的颜色。 |
| `selection` | `{ start: number; end?: number }` | 受控选区，以 UTF-16 为单位；`end` 默认等于 `start`（即光标）。 |

### iOS

| `ios` 键 | 类型 | 说明 |
|---|---|---|
| `textContentType` | `AutoComplete \| TextContentType` | `autoComplete` 在 iOS 上的名称（`"oneTimeCode"`、`"newPassword"` 等）。 |
| `inputAccessoryViewID` | `string` | 一个 `InputAccessoryView`（[`nsh/ios`](../modules/nsh-ios.md)）的 `nativeID`，输入框获得焦点时显示在键盘上方。 |
| `keyboardAppearance` | `"default" \| "light" \| "dark"` | 键盘的外观；`"default"` 跟随系统外观。Android 的键盘跟随系统主题。 |

```tsx
<Input autoComplete="one-time-code" keyboardType="number-pad" ios={{ textContentType: "oneTimeCode" }} />
```

## 事件

| 事件 | 载荷 | 触发时机 |
|---|---|---|
| `onChangeText` | `(text: string)` | 用户修改了文本。 |
| `onSubmitEditing` | `{ text }` | 在会提交的输入框中按下了回车键（单行，或 `submitBehavior` 为 `"submit"` / `"blurAndSubmit"`）。 |
| `onFocus` / `onBlur` | 无 | 输入框获得 / 失去焦点。 |
| `onSelectionChange` | `{ start, end }` | 光标或选区移动（UTF-16 单位）。 |
| `onKeyPress` | `{ key }` | 文本变化之前：`key` 是输入的文本、`"Backspace"` 或 `"Enter"`。 |
| `onContentSizeChange` | `{ width, height }` | 文本尺寸变化（多行时按输入框宽度计算），随输入报告；输入框为空时报告一行空行的尺寸，与占位文字无关。 |

随文本增高的多行输入框：

```tsx
const height = signal(40);
<Input
  multiline
  onContentSizeChange={(e) => height.set(Math.min(120, Math.max(40, e.height + 16)))}
  style={{ height: height(), paddingHorizontal: 12, paddingVertical: 8 }}
/>;
```

## ref 方法

| 方法 | 说明 |
|---|---|
| `focus()` | 让输入框获得焦点并弹出键盘。 |
| `blur()` | 让它失去焦点。 |
| `setSelection(start, end?)` | 选中 `start..end`（UTF-16 单位）；省略 `end` 时为光标。 |

按回车把焦点移到下一个输入框：

```tsx
let password!: YNode;
<>
  <Input returnKeyType="next" onSubmitEditing={() => password.focus()} />
  <Input ref={password} secureTextEntry autoComplete="current-password" />
</>;
```

无论哪个输入框持有键盘，[`nsh/ui/mobile`](../components/keyboard.md) 的 `Keyboard.dismiss()` 都能把它收起。

## 平台说明

- [`<Scroll>`](./scroll.md) 中的输入框会在原生侧自动避开键盘。其他位置请使用 `KeyboardAvoidingView`。
- `disabled` 除了应用 `disabled:` 类，还会禁用输入框。
- `inputAccessoryViewID` 在 Android 上无效，因为 Android 没有键盘附件栏。
