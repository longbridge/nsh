---
title: 表单与键盘
description: 用 input 元素输入文本：受控与非受控的值、键盘类型、回车键、焦点、选区、自动增高的多行输入框，以及让内容避开软键盘。
order: 42
group: ui
demo: guide-forms
---

# 表单与键盘

文本输入使用 `<Input>` 元素：iOS 上是 UITextField 或 UITextView，Android 上是 EditText。输入、光标、选区、自动更正和自动填充都由平台自己处理；应用通过事件拿到文本，也可以控制文本。NSH 还会自动让获得焦点的输入框保持在软键盘上方，并把键盘高度作为响应式的值提供给你，用于它自己处理不了的情况。

如果需要符合移动端 UI 套件外观、带标签和清除按钮的输入框，见 [`TextField`](../components/text-field.md)。本页介绍底层的元素。

## 受控与非受控输入框

传入 `value` 时输入框是**受控**的：它显示 signal 中的内容，由你在 `onChangeText` 中更新 signal。

```tsx
import { Input, mount, signal, Text, View } from "nsh";

function App() {
  const name = signal("");
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 60, gap: 8 }}>
      <Input
        value={name()}
        onChangeText={(text) => name.set(text)}
        placeholder="Your name"
        style={{ fontSize: 17, borderWidth: 1, borderColor: "#d0d7de", borderRadius: 10, padding: 12 }}
      />
      <Text>Hello, {name() || "stranger"}</Text>
    </View>
  );
}

mount(App);
```

既然值由你控制，就可以变换用户输入的内容：`onChangeText={(t) => code.set(t.replace(/[^0-9]/g, ""))}` 只保留数字，`onChangeText={(t) => tag.set(t.toUpperCase())}` 转为大写。

不传 `value` 时输入框是**非受控**的：它自己保存文本，初始值为 `defaultValue`，需要时从 `onChangeText` 或 `onSubmitEditing` 读取。只在提交时读取的字段（例如密码）适合这样用。

> [!NOTE]
> 受控输入框不会丢失按键。每次原生编辑都带有一个编辑计数，JS 传回的 `value` 只有对应最新一次编辑时才会被应用。如果 JS 较慢，而用户在这期间又输入了更多内容，过期的值会被丢弃，不会覆盖更新的文本。

## 属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `value` | `string` | | 由 JS 控制的文本。 |
| `defaultValue` | `string` | | 非受控输入框的初始文本。 |
| `placeholder` | `string` | | 输入框为空时显示。 |
| `placeholderTextColor` | `ColorValue` | | 占位文字颜色（也可用 `placeholder:text-*` 类）。 |
| `secureTextEntry` | `boolean` | `false` | 隐藏文本（密码）。 |
| `keyboardType` | `KeyboardType` | `"default"` | 键盘布局。 |
| `returnKeyType` | `ReturnKeyType` | `"default"` | 回车键上的文字。 |
| `autoCapitalize` | `"none" \| "sentences" \| "words" \| "characters"` | `"sentences"` | 自动大写。 |
| `autoCorrect` | `boolean` | `true` | 系统的自动更正。 |
| `autoComplete` | `AutoComplete` | | 输入框内容的类型，用于自动填充。 |
| `multiline` | `boolean` | `false` | 多行；回车插入换行（`submitBehavior` 另有设置时除外）。 |
| `maxLength` | `number` | | 用户最多能输入的字符数。 |
| `editable` | `boolean` | `true` | 用户可以修改文本。 |
| `autoFocus` | `boolean` | `false` | 出现时自动获得焦点。 |
| `submitBehavior` | `"submit" \| "blurAndSubmit" \| "newline"` | 单行 `"blurAndSubmit"`，多行 `"newline"` | 回车键做什么：提交并保持焦点、提交后失去焦点，或插入换行（仅多行）。优先于 `blurOnSubmit`。 |
| `blurOnSubmit` | `boolean` | `true` | 单行输入框按回车后失去焦点。 |
| `caretHidden` | `boolean` | `false` | 不绘制光标。 |
| `selectionColor` | `ColorValue` | | 光标和选区的颜色。 |
| `ios={{ keyboardAppearance }}` | `"default" \| "light" \| "dark"` | `"default"` | 键盘的外观（iOS）。 |
| `selection` | `{ start: number; end?: number }` | | 受控的选区，单位为 UTF-16 码元。 |
| `allowFontScaling` | `boolean` | `true` | 跟随系统文字大小。 |
| `maxFontSizeMultiplier` | `number` | | 系统缩放的上限（0：无上限）。 |
| `ios={{ inputAccessoryViewID }}` | `string` | | iOS：显示在键盘上方的工具栏（见下文）。 |
| `ios={{ textContentType }}` | `TextContentType` | | iOS 对 `autoComplete` 取值的叫法。 |

事件：

| 事件 | 参数 | 触发时机 |
|---|---|---|
| `onChangeText` | `text: string` | 文本改变。 |
| `onSubmitEditing` | `{ text }` | 在会提交的输入框中按下回车键（单行，或 `submitBehavior` 为 `"submit"` / `"blurAndSubmit"`）。 |
| `onFocus` / `onBlur` | | 获得 / 失去焦点。 |
| `onSelectionChange` | `{ start, end }` | 光标或选区移动（UTF-16 码元）。 |
| `onKeyPress` | `{ key }` | 文本改变之前：`key` 是输入的文本、`"Backspace"` 或 `"Enter"`。 |
| `onContentSizeChange` | `{ width, height }` | 文本尺寸改变（多行时按输入框的宽度计算）。 |

输入框的样式与文本相同：`fontSize`、`color`、`fontFamily`、`textAlign`、`letterSpacing` 以及其余[文本样式](./text.md)，再加上边框、内边距和背景。Tailwind 的 `focus:` 变体在输入框获得焦点时生效：

```tsx
<Input class="rounded-lg border border-zinc-300 px-3 py-2 text-base focus:border-blue-500" placeholder="Search" />
```

## 键盘与自动填充

`keyboardType` 选择键盘布局：

| 值 | 键盘 |
|---|---|
| `"default"` | 文本。 |
| `"email-address"` | 文本，方便输入 `@` 和 `.`。 |
| `"url"` | 文本，方便输入 `/` 和 `.com`。 |
| `"number-pad"` | 只有数字：没有正负号和小数点（验证码、PIN）。 |
| `"decimal-pad"` | 数字和小数点。 |
| `"numeric"` | 与 React Native 相同：iOS 上是小数键盘；Android 上是带正负号和小数点的数字键盘。 |
| `"numbers-and-punctuation"` | 数字、负号和标点（iOS 的数字与标点键盘；Android 上是带正负号和小数点的数字键盘）。在 iOS 上输入负数时使用它。 |
| `"phone-pad"` | 电话拨号盘。 |

在 iOS 上，`ios={{ keyboardAppearance: "dark" }}`（或 `"light"`、`"default"`）让键盘使用浅色或深色外观而不随系统外观变化，适合在浅色模式下运行的深色应用。Android 的键盘始终跟随系统主题。

`autoComplete` 告诉系统字段里装的是什么，系统就能提供已保存的姓名、地址、密码以及短信中的验证码。它接受以下取值：`"name"`、`"given-name"`、`"family-name"`、`"email"`、`"tel"`、`"username"`、`"current-password"`、`"new-password"`、`"one-time-code"`、`"street-address"`、`"postal-code"`、`"country"`、`"cc-number"`、`"cc-exp"`、`"cc-csc"`、`"url"`、`"birthdate-full"` 等，或 `"off"`。iOS 自己的名字（`"emailAddress"`、`"oneTimeCode"` 等）也可以用在 `autoComplete` 或 `ios={{ textContentType }}` 中。

```tsx
<Input keyboardType="email-address" autoCapitalize="none" autoCorrect={false} autoComplete="email" placeholder="E-mail" />
<Input secureTextEntry autoComplete="new-password" placeholder="Password" />
<Input keyboardType="number-pad" autoComplete="one-time-code" maxLength={6} placeholder="Code" />
```

## 回车键与在字段间切换

`returnKeyType` 设置回车键上的文字：`"default"`、`"done"`、`"go"`、`"next"`、`"search"` 或 `"send"`。按下它做什么由 `submitBehavior` 决定：

| `submitBehavior` | 回车 | 默认用于 |
|---|---|---|
| `"blurAndSubmit"` | 触发 `onSubmitEditing`，随后输入框失去焦点 | 单行输入框 |
| `"submit"` | 触发 `onSubmitEditing`，输入框保持焦点 | 设置了 `blurOnSubmit={false}` 的单行输入框 |
| `"newline"` | 插入换行（仅多行输入框） | 多行输入框 |

设置了 `"submit"` 或 `"blurAndSubmit"` 的多行输入框像聊天输入框一样按回车发送，文本仍会自动换行；使用硬件键盘时（桌面、Mac、平板的外接键盘），Shift+Return 或 Option / Alt+Return 插入换行。用回车确认输入法的候选词（例如中文或日文输入）只会上屏，不会提交。

要切换到下一个字段，保存输入框的 ref 并调用 `focus()`：

```tsx
import { Input, mount, signal, View, type YNode } from "nsh";

function SignIn() {
  const email = signal("");
  let password!: YNode;
  const submit = () => console.log("sign in", email());
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 60, gap: 12 }}>
      <Input
        value={email()}
        onChangeText={(t) => email.set(t)}
        keyboardType="email-address"
        autoCapitalize="none"
        returnKeyType="next"
        blurOnSubmit={false}
        onSubmitEditing={() => password.focus()}
        placeholder="E-mail"
      />
      <Input ref={password} secureTextEntry returnKeyType="go" onSubmitEditing={submit} placeholder="Password" />
    </View>
  );
}

mount(SignIn);
```

第一个字段上的 `blurOnSubmit={false}` 让键盘在焦点切换时保持弹出，不会先收起再弹起。按回车发送并保持焦点的聊天输入框使用 `submitBehavior="submit"`（见[下面](#滚动视图之外的内容keyboardavoidingview)的输入栏）。

`maxLength` 和 `submitBehavior` 都在原生端执行：用户输入时，平台会询问"能否再加这个字符？"和"回车是否结束编辑？"，NSH 根据属性直接回答，不等待 JS。

## 从代码控制焦点

输入框的 ref 提供：

| 方法 | 说明 |
|---|---|
| `focus()` | 让输入框获得焦点并弹出键盘。 |
| `blur()` | 让它失去焦点。 |
| `setSelection(start, end?)` | 选中 `start..end`（UTF-16 码元）；不传 `end` 时为光标。 |

`nsh/ui/mobile` 中的 `Keyboard.dismiss()` 让当前拥有焦点的输入框失去焦点：

```tsx
import { View } from "nsh";
import { Keyboard } from "nsh/ui/mobile";

<View onPress={() => Keyboard.dismiss()}>…</View>
```

## 选区

`onSelectionChange` 报告光标和选区，`selection` 设置它们。位置以 UTF-16 码元计，与 JavaScript 字符串索引一致，所以 `text.slice(start, end)` 就是选中的文本。

```tsx
const text = signal("Select part of this text");
const range = signal({ start: 0, end: 0 });
const selection = signal<{ start: number; end?: number } | undefined>(undefined);

<Input
  value={text()}
  onChangeText={(t) => text.set(t)}
  selection={selection()}
  onSelectionChange={(r) => range.set(r)}
/>
<Text>Selected: "{text().slice(range().start, range().end)}"</Text>
<View onPress={() => selection.set({ start: 0, end: text().length })}><Text>Select all</Text></View>
```

## 自动增高的多行输入框

`multiline` 输入框在自身高度内滚动文本。要让它随内容增高，把 `onContentSizeChange` 报告的高度加上上限后写回样式：

```tsx
const height = signal(0);

<Input
  multiline
  placeholder="Write a comment"
  onContentSizeChange={(e) => height.set(Math.min(e.height, 120))}
  style={{ fontSize: 16, padding: 10, borderWidth: 1, borderColor: "#d0d7de", borderRadius: 10, height: Math.max(height(), 20) + 20 }}
/>
```

报告的高度是文本本身的高度（按输入框宽度计算），所以要像上面那样自己加上上下内边距。

## 让输入框保持在键盘上方

### 滚动视图内：自动处理

当键盘弹出、而获得焦点的输入框位于 `<Scroll>` 或 `List` 中时，NSH 会给滚动视图加上与键盘等高的底部 inset，并把输入框滚动到可见位置。这是原生完成的，不需要任何代码。长表单就是一个装满输入框的滚动视图：

```tsx
<Scroll style={{ flex: 1 }} keyboardDismissMode="on-drag" keyboardShouldPersistTaps="handled">
  <View style={{ padding: 16, gap: 12 }}>
    {/* 输入框 */}
  </View>
</Scroll>
```

`keyboardDismissMode`（`"none"`、`"on-drag"`、`"interactive"`）让拖动表单时收起键盘；`keyboardShouldPersistTaps="handled"` 让键盘弹出时点击按钮能直接生效，而不只是收起键盘。见[滚动](./scrolling.md#滚动视图与键盘)。

### 滚动视图之外的内容：KeyboardAvoidingView

固定在页面底部的消息栏不在滚动视图中，没有东西会移动它。用 `nsh/ui/mobile` 中的 `KeyboardAvoidingView` 包住页面：它会在底部让出与键盘等高的空间，并与键盘同步动画。

```tsx
import { Input, Scroll, signal, View } from "nsh";
import { keyboard, KeyboardAvoidingView, useSafeAreaInsets } from "nsh/ui/mobile";

function ChatScreen() {
  const insets = useSafeAreaInsets();
  const draft = signal("");
  const send = () => {
    if (draft().trim()) console.log("send", draft());
    draft.set("");
  };
  return (
    <KeyboardAvoidingView style={{ flex: 1 }}>
      <Scroll style={{ flex: 1 }} keyboardDismissMode="interactive">
        {/* 消息 */}
      </Scroll>
      <View
        style={{
          flexDirection: "row",
          padding: 8,
          // 在 Home 指示条之上，除非键盘已经盖住了它。
          paddingBottom: 8 + (keyboard().height > 0 ? 0 : insets().bottom),
        }}
      >
        <Input
          value={draft()}
          onChangeText={(t) => draft.set(t)}
          multiline
          submitBehavior="submit"
          returnKeyType="send"
          onSubmitEditing={send}
          placeholder="Message"
          style={{ flex: 1, maxHeight: 120 }}
        />
      </View>
    </KeyboardAvoidingView>
  );
}
```

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `behavior` | `"padding" \| "height" \| "position"` | `"padding"` | `padding` 按键盘高度增加底部内边距；`height` 缩短视图（背景止于键盘处）；`position` 把内部的内容视图向上移。 |
| `enabled` | `boolean` | `true` | 是否避开键盘。 |
| `keyboardVerticalOffset` | `number` | `0` | 键盘显示时额外加到键盘高度上的值（比如视图下方还有一个标签栏）。 |
| `contentContainerStyle` | `StyleProp<ViewStyle>` | | `behavior="position"` 时内部视图的样式。 |

它假定自己延伸到 surface 的底部，页面内容通常如此。在会为键盘缩小窗口的 Android 宿主上，窗口本身已经变小，`KeyboardAvoidingView` 无事可做。见 [KeyboardAvoidingView](../components/keyboard-avoiding-view.md)。

### keyboard 访问器

`nsh/ui/mobile` 中的 `keyboard()` 是一个响应式的 `{ height, duration }`：键盘伸入 surface 的高度（隐藏时为 0，包含被它盖住的底部安全区域）以及它的动画时长（秒）。需要自定义行为时读取它，例如输入时隐藏浮动按钮，或调整面板尺寸。

```tsx
import { View } from "nsh";
import { keyboard } from "nsh/ui/mobile";

<If when={keyboard().height === 0}>
  <View style={{ position: "absolute", right: 16, bottom: 16 }}>…</View>
</If>
```

## 键盘上方的工具栏：InputAccessoryView

`nsh/ios` 中的 `InputAccessoryView` 会在引用它的输入框获得焦点时，在键盘顶部显示一个附着的工具栏，即 iOS 的 `inputAccessoryView`。它由原生跟随键盘移动，包括交互式收起键盘的过程。它在树中所处的位置不渲染任何内容；输入框通过 `nativeID` 引用它：

```tsx
import { Input, signal, Text, View, type YNode } from "nsh";
import { InputAccessoryView } from "nsh/ios";

function Note() {
  const text = signal("");
  let input!: YNode;
  return (
    <View style={{ padding: 16 }}>
      <Input
        ref={input}
        value={text()}
        onChangeText={(t) => text.set(t)}
        ios={{ inputAccessoryViewID: "note-bar" }}
        placeholder="A note"
      />
      <InputAccessoryView nativeID="note-bar" style={{ backgroundColor: "#f6f8fa" }}>
        <View style={{ flexDirection: "row", alignItems: "center", padding: 8 }}>
          <Text style={{ flex: 1, color: "#57606a" }}>{text().length} characters</Text>
          <Text onPress={() => input.blur()} style={{ color: "#0969da", fontWeight: "600" }}>Done</Text>
        </View>
      </InputAccessoryView>
    </View>
  );
}
```

工具栏与窗口等宽，高度由内容决定。Android 没有输入附件视图：在那里 `InputAccessoryView` 不渲染任何内容，`nsh/ios` 的 `isAvailable` 为 `false`，所以在 `Platform.OS === "android"` 时要为这些控件另找位置（例如放在输入框上方）。

## 相关内容

- [`Input`](../elements/input.md)：元素参考。
- [`TextField`](../components/text-field.md)：带标签和清除按钮的输入框。
- [`KeyboardAvoidingView`](../components/keyboard-avoiding-view.md) 与 [`Keyboard`](../components/keyboard.md)。
- [`nsh/ios`](../modules/nsh-ios.md)：`InputAccessoryView`。
- [滚动](./scrolling.md#滚动视图与键盘)：滚动视图中的键盘收起与点击。
