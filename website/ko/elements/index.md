---
title: 요소
description: "Rust와 TSX에서 NSH 앱을 구성하는 기본 요소와 컴포넌트, 공통 속성, 이벤트, ref 메서드를 살펴봅니다."
order: 1
---

# 요소

Native Shell의 모든 화면은 `View`, `Text`, `Image` 같은 기본 요소와 `Button`, `Dialog`, `NavStack` 같은 기본 컴포넌트로 구성됩니다. 각 기본 요소는 NSH 런타임이 생성하고 배치하는 네이티브 뷰입니다. iOS와 Mac Catalyst에서는 UIView의 하위 클래스, Android에서는 `View`, 데스크톱에서는 AppKit, GTK 또는 Win32 뷰를 사용합니다. 앱에서 사용하는 버튼, 카드, 행, 화면은 이 요소들로 만든 컴포넌트이며, 각각의 스타일과 앱 전체의 디자인은 앱에서 결정합니다.

예제는 Rust와 TSX로 제공합니다. 탭에서 언어를 고르면 같은 페이지에 있는 다른 예제에도 적용됩니다.

```rust tabs
use nsh_core::prelude::*;

fn app() -> impl IntoElement {
    let count = signal(0_u32);
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_3()
        .child(text(move || format!("Tapped {} times", count.get())).text_2xl())
        .child(
            view()
                .rounded_lg()
                .bg(rgb(0x2563eb))
                .px_4()
                .py_2()
                .active(|s| s.opacity(0.6))
                .on_press(move || count.update(|n| *n += 1))
                .child(text("Tap").text_color(white())),
        )
}
```

```tsx tabs
import { mount, signal, Text, View } from "nsh";

function App() {
  const count = signal(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 12 }}>
      <Text style={{ fontSize: 24 }}>Tapped {count()} times</Text>
      <View onPress={() => count.update((n) => n + 1)} class="rounded-lg bg-blue-600 px-4 py-2 active:opacity-60">
        <Text class="text-white">Tap</Text>
      </View>
    </View>
  );
}

mount(App);
```

Rust의 기본 요소는 `nsh_core::prelude`의 함수(`view()`, `text(..)`, `image(..)`)이며, 기본 컴포넌트는 `nsh_base` 크레이트에서 가져옵니다. 스타일은 타입이 있는 메서드(`.flex_1()`, `.gap_3()`, `.px(px(20.0))`, `.bg(..)`)로 지정하고, 상태별 스타일에는 클로저(`.active(|s| s.opacity(0.6))`)를 전달합니다. 이벤트 리스너도 클로저(`.on_press(move || ..)`, `.on_change(move |text: &str| ..)`)이며, 자식은 `.child(..)`로 추가합니다. UI를 구성하는 일반 함수는 한 번 실행되어 `impl IntoElement`를 반환합니다. `text(move || ..)`, `.bg(move || ..)` 등의 클로저 안에서 읽은 시그널이 바뀌면 해당 바인딩이 갱신됩니다. [Rust로 앱 작성하기](../docs/rust.md)를 참고하세요.

TSX 태그는 PascalCase로 작성하며 현재 범위에 있는 이름을 참조합니다. 기본 요소는 `nsh`에서, 기본 컴포넌트는 `nsh/base`에서 가져옵니다. 전역인 `<If>`와 `<Each>`만 가져오기 없이 쓸 수 있습니다. `<view>`처럼 소문자로 작성하면 빌드 오류가 발생합니다. 소문자 태그를 허용하면 TypeScript가 태그 이름마다 하나의 전역 타입을 사용하게 되어 `List`와 `Each` 같은 제네릭 컴포넌트에서 항목 타입을 추론할 수 없기 때문입니다. 컴파일러는 가져온 요소를 식별하고 사용 위치에 네이티브 뷰를 생성하므로 가져오기에 따른 런타임 비용은 없습니다. [JSX와 컴포넌트](../../docs/jsx-and-components.md)에서 더 알아볼 수 있습니다.

```tsx
import { Canvas, Image, List, Overlay, Scroll, Shader, Text, Video, View, WebView } from "nsh";
import { Button, Dialog, Menu, NavPage, NavStack, Popover, Sheet, Tooltip, Input } from "nsh/base";
```

## 기본 요소

TSX에서는 `nsh`에서, Rust에서는 `nsh_core::prelude`에서 가져옵니다.

| 요소 | Rust | 네이티브 뷰(iOS / Android) | 문서 |
|---|---|---|---|
| `<View>` | `view()` | UIView / ViewGroup. `selectable`이면 내부 텍스트를 함께 선택할 수 있음 | [View](../../elements/view.md), [텍스트 선택](../../elements/text-selection.md) |
| `<Text>` | `text(content)` | CoreText 뷰 / StaticLayout 뷰 | [Text](../../elements/text.md) |
| `<Image>` | `image(src)` | 이미지 레이어가 있는 뷰 / 이미지 뷰 | [Image](../../elements/image.md) |
| `<Scroll>` | `scroll()` | UIScrollView / ScrollView | [Scroll](../../elements/scroll.md) |
| `<List>` | `virtual_list(count, \|at\| row)` | 셀을 재사용하는 UIScrollView / ScrollView | [List](../../elements/list.md) |
| `<Canvas>` | `canvas(painter)` | 2D 그리기 명령을 재생하는 뷰 | [Canvas](../../elements/canvas.md) |
| `<Shader>` | `shader(program)` | 자식 뷰 아래에 WGSL 프로그램으로 영역을 그리는 GPU 뷰 | [Shader](../../elements/shader.md) |
| `<WebView>` | `web_view(url)`, `web_view_html(html, base)` | WKWebView / android.webkit.WebView | [WebView](../../elements/web-view.md) |
| `<Video>` | `video(url)` | AVPlayerLayer / TextureView의 MediaPlayer | [Video](../../elements/video.md) |
| `<Overlay>` | `overlay()` | 창 또는 자신을 포함하는 대화상자 맨 위에 앵커를 기준으로 배치되는 뷰 | [Overlay](../../elements/overlay.md) |
| `<If>`, `<Each>` | `when(..)`, `each(..)`, `dynamic(..)` | 제어 흐름에 따라 선택되는 요소 | [제어 흐름](../../elements/control-flow.md) |
| `<InputAccessoryView>` (`nsh/mobile`) | `input_accessory_view(id)` | iOS 키보드 위에 표시되는 막대 | [`InputAccessoryView`](../../modules/nsh-mobile.md#inputaccessoryview) |

패딩과 마진에 CSS의 `env()`를 사용하면 시스템 UI가 콘텐츠를 가리지 않도록 여백을 확보할 수 있습니다. 가장자리의 [안전 영역](../../elements/safe-area.md)에는 `env(safe-area-inset-top)` 등 방향별 변수를, [키보드](../../elements/keyboard.md)에는 `env(keyboard-inset-height)`를 사용합니다.

## 기본 컴포넌트

TSX에서는 `nsh/base`에서, Rust에서는 `nsh_base` 크레이트에서 가져옵니다. 각 컴포넌트가 동작, 상태, 포커스, 접근성을 처리하고 앱에서 모양을 지정합니다.

| 컴포넌트 | Rust | 기능 | 문서 |
|---|---|---|---|
| `AlertDialog` | `nsh_base::alert_dialog()` | 시스템 알림 대화상자와 액션 시트 | [AlertDialog](../../elements/alert-dialog.md) |
| `<Accordion>` 및 하위 요소 | `nsh_base::accordion()`, `accordion_item()`, ... | 제목을 눌러 하나 또는 여러 섹션을 펼치고 접을 수 있는 목록 | [Accordion](../../elements/accordion.md) |
| `<Avatar>`, `<AvatarGroup>` | `nsh_base::avatar()`, `avatar_group()` | 프로필 사진과 로딩 중이거나 실패했을 때 표시할 대체 콘텐츠. 그룹은 앞에서부터 정해진 수만큼 표시 | [Avatar](../../elements/avatar.md) |
| `<Button>` | `nsh_base::button()` | 상태와 접근성 역할을 갖춘 누를 수 있는 컨트롤 | [Button](../../elements/button.md) |
| `<Carousel>` 및 하위 요소 | `nsh_base::carousel()`, `carousel_content()`, ... | 네이티브 스와이프, 이전·다음, 페이지 표시, 키보드, 끊김 없는 순환을 지원하며 슬라이드를 한 위치에 표시 | [Carousel](../../elements/carousel.md) |
| `<Checkbox>` | `nsh_base::checkbox()` | 선택, 해제, 부분 선택 상태를 표시하는 컨트롤 | [Checkbox](../../elements/checkbox.md) |
| `<Collapsible>` | `nsh_base::collapsible()` | 내용을 펼치거나 접을 수 있는 영역 | [Collapsible](../../elements/collapsible.md) |
| `<Command>` | `nsh_base::command()` | 검색 필드, 네이티브 필터링, 가상화된 목록과 Cmd+K 방식의 키 조작을 제공하는 명령 팔레트 | [Command](../../elements/command.md) |
| `<DataTable>` | `nsh_base::data_table()` | 행·열 수에 관계없이 선택과 정렬, 열 크기 조절과 이동을 지원하는 표 | [DataTable](../../elements/data-table.md) |
| `<Dialog>` | `nsh_base::dialog()` | 자식 콘텐츠를 다른 모든 콘텐츠 위에 표시 | [Dialog](../../elements/dialog.md) |
| `<DiffView>` | `nsh_base::diff_view()` | 두 텍스트의 차이나 패치를 분할 또는 통합 보기로 표시 | [DiffView](../../elements/diff-view.md) |
| `<DockArea>` | `nsh_base::dock_area()` | 사용자가 분할 영역, 탭 그룹, 가장자리 도크를 배치하는 작업 공간 | [Dock](../../elements/dock.md) |
| `<Editor>` | `nsh_base::editor()` | 구문 강조, 다중 커서, 코드 접기, 검색을 지원하는 편집기 | [Editor](../../elements/editor.md) |
| `<HoverCard>` | `nsh_base::hover_card()` | 포인터가 트리거 위에 머무는 동안 대상 콘텐츠를 미리 표시 | [HoverCard](../../elements/hover-card.md) |
| `<Input>` | `nsh_base::input()` | 네이티브 텍스트 입력 필드(UITextField 또는 UITextView / EditText) | [Input](../../elements/input.md) |
| `<Link>` | `nsh_base::link()` | 누르면 페이지, 웹 사이트 또는 앱 내부 경로를 여는 요소 | [Link](../../elements/link.md) |
| `<Menu>`, `<ContextMenu>`, `<MenuItem>`, `<MenuSeparator>`, `<Submenu>` | `nsh_base::menu()`, ... | 데스크톱에서는 하위 메뉴가 이어지는 팝업, 휴대폰에서는 시트로 표시하는 메뉴 | [Menu](../../elements/menu.md) |
| `<NavStack>`, `<NavPage>` | `nsh_base::nav_stack()`, `nsh_base::nav_page()` | 플랫폼 고유의 헤더와 뒤로 가기 제스처를 사용하는 페이지 스택 | [NavStack](../../elements/nav-stack.md) |
| `<NumberInput>` | `nsh_base::number_input()` | 입력란에 직접 입력하거나 버튼과 키로 증감하는 숫자 | [NumberInput](../../elements/number-input.md) |
| `<OtpInput>` | `nsh_base::otp_input()` | 고정 자릿수의 일회용 코드를 셀에 입력하거나 붙여넣고 자동 완성 | [OtpInput](../../elements/otp-input.md) |
| `<Pagination>` | `nsh_base::pagination()` | 이전·다음, 현재 페이지 주변 번호와 생략 기호로 구성된 페이지 탐색 | [Pagination](../../elements/pagination.md) |
| `<Tree>` | `nsh_base::tree()` | 하위 항목을 펼치고 접으며 항목 하나를 선택하는 가상화 트리. Tab 키로는 하나의 포커스 지점으로 이동 | [Tree](../../elements/tree.md) |
| `<Plot>`, `<Line>`, `<Area>`, `<Bar>`, `<Grid>`, `<Pie>`, `<Arc>` (`nsh/base/plot`) | `nsh_base::plot::{plot, line, area, bar, grid, pie, arc}` | 앱 데이터의 각 행을 바탕으로 네이티브에서 그리는 차트 영역과 도형 | [Plot](../../elements/plot.md) |
| `<Popover>` | `nsh_base::popover()` | 트리거 위치를 기준으로 다른 콘텐츠 위에 표시하는 팝업 | [Popover](../../elements/popover.md) |
| `<Progress>` | `nsh_base::progress()` | 작업 진행률 또는 작업이 진행 중임을 표시하는 막대 | [Progress](../../elements/progress.md) |
| `<Radio>`, `<RadioGroup>` | `nsh_base::radio()`, `radio_group()` | 여러 선택지 중 하나를 고르는 항목과 그룹 | [Radio](../../elements/radio.md) |
| `<ResizablePanelGroup>`, `<ResizablePanel>` | `nsh_base::h_resizable()`, `resizable_panel()` | 가로나 세로로 배치하며 핸들로 크기를 바꾸는 패널 | [Resizable](../../elements/resizable.md) |
| `<Scrollbar>` | `nsh_base::scrollbar()` | 스크롤 뷰나 목록에 표시하는 앱 디자인의 스크롤 막대 | [Scrollbar](../../elements/scrollbar.md) |
| `<Select>` | `nsh_base::select()` | 검색, 키보드 탐색과 섹션을 지원하는 목록에서 값 하나를 선택 | [Select](../../elements/select.md) |
| `<Sheet>` | `nsh_base::sheet()` | 배경을 덮는 레이어 위에 화면 가장자리를 따라 표시되는 패널 | [Sheet](../../elements/sheet.md) |
| `<Slider>` | `nsh_base::slider()` | 트랙 위에서 하나의 값이나 두 값으로 정한 범위를 선택 | [Slider](../../elements/slider.md) |
| `<Switch>` | `nsh_base::switch()` | 트랙과 손잡이로 켜짐과 꺼짐을 표시하는 컨트롤 | [Switch](../../elements/switch.md) |
| `<Table>` 및 하위 요소 | `nsh_base::table()`, `table_row()`, ... | 표에 필요한 역할을 갖추고 앱의 행과 셀로 구성되는 표 | [Table](../../elements/table.md) |
| `<Tab>`, `<Tabs>` | `nsh_base::tab()`, `tabs()` | 탭 목록과 목록 안의 개별 탭 | [Tabs](../../elements/tabs.md) |
| `<TextView>` | `nsh_base::text_view()` | 텍스트와 뷰로 Markdown 또는 HTML 문서를 표시 | [TextView](../../elements/text-view.md) |
| `<Toast>`, `<ToastStack>` | `nsh_base::toast()`, `toast_stack()` | 토스트 하나의 의미 구조 및 화면 가장자리에 여러 토스트를 겹쳐 배치하는 기능 | [Toast](../../elements/toast.md) |
| `<Toggle>`, `<ToggleGroup>` | `nsh_base::toggle()`, `toggle_group()` | 켜거나 끌 수 있는 버튼과 그러한 버튼의 도구 모음 | [Toggle](../../elements/toggle.md) |
| `<Toolbar>`, `<ToolbarGroup>`, `<ToolbarSeparator>` | `nsh_base::toolbar()`, `toolbar_group()`, `toolbar_separator()` | 버튼, 토글, 링크로 구성되며 키보드 포커스가 한 번 멈추는 도구 모음 | [Toolbar](../../elements/toolbar.md) |
| `<Tooltip>` | `nsh_base::tooltip()` | 포인터를 올리거나 길게 누르면 트리거 옆에 표시되는 설명 | [Tooltip](../../elements/tooltip.md) |

> [!NOTE]
> 문자열은 `Text` 안에서만 그려집니다. `View` 바로 아래에 문자열을 넣으면 네이티브 뷰가 생기지 않으며 레이아웃에서도 공간을 차지하지 않습니다.

## 공통 속성

모든 기본 요소는 다음 속성을 받습니다. TSX에서는 `CommonProps` 타입에, Rust에서는 `Styled`, `Interactive`, `Accessible` 트레이트에 정의되어 있습니다. 각 요소의 개별 문서에서는 그 요소만의 속성을 설명합니다. 표의 Rust 열에는 빌더 메서드를 표시했습니다.

### 스타일 지정

| 속성 | Rust | 타입 | 설명 |
|---|---|---|---|
| `style` | typed `Styled` methods; `.inline_style(style)` | `StyleProp<ViewStyle>` (`TextStyle` for `<Text>` and `<Input>`, `ImageStyle` for `<Image>`) | 인라인 스타일 또는 왼쪽부터 순서대로 병합할 스타일 배열. 거짓으로 평가되는 항목은 건너뜁니다. 배열을 중첩할 수 있어 `style={[base, props.style]}`에서 `props.style`도 배열일 수 있습니다. `class`보다 우선합니다. [스타일 레퍼런스](../../modules/style-reference.md)를 참고하세요. |
| `class` | typed `Styled` methods (`.flex_row().gap_2().p_4()`) | `string` | 네이티브에서 해석하는 Tailwind 클래스(`"flex-row gap-2 p-4 dark:bg-zinc-900"`). 뒤에 오는 클래스가 우선합니다. [Tailwind](../../docs/tailwind.md)를 참고하세요. |
| `classList` | `.style_when(when, \|s\| ..)` | `Record<string, boolean \| null \| undefined>` | 값이 참으로 평가되는 동안 적용할 클래스(`{ "opacity-50": busy() }`). |
| `disabled` | `.disabled(..)` | `boolean` | 누르기와 시각적 피드백을 무시하고 `disabled:` 스타일을 적용합니다. 입력 필드 등의 컨트롤을 비활성화하며 스크린 리더에도 비활성 상태(`accessibilityState.disabled`)를 알립니다. |

### 누르기와 피드백

누르기는 네이티브에서 처리합니다. 플랫폼이 누르기를 인식하고 앱이 지정한 스타일을 적용한 뒤 앱 코드에 이벤트를 전달합니다. 누를 수 있는 뷰가 중첩되어 있으면 가장 안쪽 뷰만 반응합니다. 스크롤 뷰 안에서는 스크롤이 먼저 시작될 수 있도록 누름 시작 처리를 약 130ms 늦춥니다.

| 속성 | Rust | 타입 | 기본값 | 설명 |
|---|---|---|---|---|
| `onPress` | `.on_press(..)` | `(e: PressEvent) => void` | | 뷰 안에서 탭을 마쳤을 때. |
| `onLongPress` | `.on_long_press(..)` | `(e: PressEvent) => void` | | 손가락을 `delayLongPress` 동안 누르고 있을 때. |
| `onPressDown` | `.on_press_down(..)` | `(e: PressEvent) => void` | | 손가락이 뷰에 닿았을 때. |
| `onPressUp` | `.on_press_up(..)` | `(e: PressEvent) => void` | | 손가락을 떼거나 뷰를 벗어났을 때. |
| `delayLongPress` | `.long_press_delay(ms)` | `number` | `500` | 길게 누르기로 인식하기까지의 시간(ms). |
| `hitSlop` | `.hit_slop(EdgeInsets::all(px(8.0)))` | `number \| Partial<Edges>` | | 뷰 경계 밖으로 터치 영역을 확장하는 거리(포인트). |

`PressEvent`는 `{ position, pagePosition, modifiers, clickCount }`로 구성됩니다. 차례대로 뷰와 루트 뷰 기준 포인터 위치(`Point`), 누르기 시작 시점에 눌린 보조 키, 연속 클릭 수(더블 클릭은 2)입니다. 플랫폼이 포인터 위치를 제공하지 않으면 뷰의 원점, 보조 키 없음, 클릭 1회로 전달됩니다.

스타일 속성 `pointerEvents`(Rust에서는 `.pointer_events(PointerEvents::BoxNone)`, 클래스는 `pointer-events-*`)는 뷰와 자식 뷰 중 어디에서 터치를 받을지 결정합니다. `"none"`이면 둘 다 받지 않지만 하위 뷰가 `"auto"`를 지정하면 그 뷰는 다시 받습니다. `"box-none"`은 자식만, `"box-only"`는 뷰 자신만 받습니다. [터치와 제스처](../../docs/touch-and-gestures.md#pointerevents)를 참고하세요.

눌렀을 때의 모양은 앱이 정합니다. 뷰를 누르고 있는 동안 플랫폼이 `activeStyle`(또는 `active:` 클래스, Rust의 `.active(..)`)을 직접 적용하므로 앱 코드를 거칠 필요가 없습니다.

### 상태별 스타일

뷰가 특정 상태일 때 네이티브에서 해당 스타일을 뷰 자체의 스타일 위에 적용합니다. 각 속성은 Tailwind 변형을 객체로 표현한 것입니다. selected, checked·indeterminate, hover, focus, active 순으로 뒤에 오는 상태가 우선합니다. 비활성화된 동안에는 `disabledStyle`이 가장 우선하며 다른 상태 스타일을 적용하지 않습니다. 속성값은 요소에 맞는 스타일 타입을 사용합니다. `<Text>`는 `TextStyle`, `<Input>`은 `InputStyle`, `<Image>`는 `ImageStyle`, 그 밖에는 `ViewStyle`입니다.

| 속성 | Rust | 클래스 변형 | 적용 조건 |
|---|---|---|---|
| `activeStyle` | `.active(\|s\| ..)` | `active:` | 누르고 있는 동안. |
| `hoverStyle` | `.hover(\|s\| ..)` | `hover:` | 포인터가 뷰 위에 있는 동안. |
| `focusStyle` | `.focus(\|s\| ..)` | `focus:` | 뷰에 포커스가 있는 동안. |
| `focusWithinStyle` | `.focus_within(\|s\| ..)` | `focus-within:` | 뷰나 그 자식에 포커스가 있는 동안. `Overlay`의 콘텐츠도 해당 오버레이를 포함하는 뷰 내부로 간주합니다. |
| `disabledStyle` | `.disabled_style(\|s\| ..)` | `disabled:` | `disabled`인 동안. |
| `checkedStyle` | `.checked(\|s\| ..)`, `.checked_style(style)` | `checked:` | `accessibilityState.checked`가 `true`일 때(체크박스, 스위치, 라디오). `"mixed"`는 제외합니다. |
| `indeterminateStyle` | `.indeterminate(\|s\| ..)`, `.indeterminate_style(style)` | `indeterminate:` | `accessibilityState.checked`가 `"mixed"`일 때(부분 선택된 체크박스). |
| `selectedStyle` | `.selected(\|s\| ..)` | `selected:` | `accessibilityState.selected`가 `true`일 때(선택된 `Button`이나 탭). |
| `expandedStyle` | `.expanded(\|s\| ..)`, `.expanded_style(style)` | `aria-expanded:` | `accessibilityState.expanded`가 `true`일 때(팝업이 열려 있는 `Popover`나 `Menu`의 트리거). |
| `pressedStyle` | `.aria_pressed(\|s\| ..)`, `.pressed_style(style)` | `aria-pressed:` | `accessibilityState.pressed`가 `true`일 때(켜진 토글 버튼). `"mixed"`는 제외합니다. 실제로 누르고 있는 동안의 모양은 `activeStyle`로 지정합니다. |

```rust tabs
view()
    .rounded_lg()
    .bg(rgb(0x2563eb))
    .px_4()
    .py_2()
    .active(|s| s.opacity(0.6))
    .hover(|s| s.bg(rgb(0x1d4ed8)))
    .on_press(save)
    .child(text("Save").text_color(white()))
```

```tsx tabs
<View onPress={save} activeStyle={{ opacity: 0.6 }} hoverStyle={{ backgroundColor: "#1d4ed8" }} class="rounded-lg bg-blue-600 px-4 py-2">
  <Text class="text-white">Save</Text>
</View>
```

### 호버

앱은 포인터가 뷰에 들어오고, 그 위를 움직이고, 벗어났을 때 이벤트를 받습니다. macOS, Windows, Linux, GPUI 백엔드, 데스크톱 브라우저뿐 아니라 iPad의 포인터나 Mac의 마우스를 사용하는 iOS 환경, 마우스나 스타일러스를 사용하는 Android 환경에서도 지원합니다. 같은 환경에서 `hover:`, `.hover(..)`로 포인터가 올라간 뷰의 스타일을 지정할 수 있습니다.

| 속성 | Rust | 타입 | 설명 |
|---|---|---|---|
| `onMouseEnter` | `.on_mouse_enter(..)` | `(e: MouseEvent) => void` | 마우스가 뷰 또는 그 안의 뷰로 들어왔습니다(DOM의 `mouseenter`). |
| `onMouseMove` | `.on_mouse_move(..)` | `(e: MouseEvent) => void` | 버튼을 누른 상태에 상관없이 마우스가 뷰 또는 그 안의 뷰 위에서 움직였습니다. 버튼을 누르지 않고 머무르는 동안 보조 키를 누르거나 떼도 발생합니다(DOM의 `mousemove`). |
| `onMouseLeave` | `.on_mouse_leave(..)` | `(e: MouseEvent) => void` | 마우스가 뷰와 그 안의 모든 뷰를 벗어났거나 창을 벗어났습니다. `e`에는 마지막 위치가 담깁니다(DOM의 `mouseleave`). |

CSS의 `:hover`나 DOM의 `mouseenter`와 마찬가지로 마우스가 뷰 자체 또는 그 안의 요소 위에 있으면 해당 뷰에 호버한 상태입니다. 따라서 포인터 바로 아래의 뷰뿐 아니라 이를 감싸는 뷰도 바깥쪽부터 안쪽 순으로 `onMouseEnter`를 받습니다. `onMouseLeave`는 반대 순서입니다. 두 이벤트는 상태가 바뀐 뒤에 전달되며 터치로 발생하지 않습니다. 마우스 아래 있던 뷰를 제거해도 `onMouseLeave`가 발생합니다. Linux와 Windows에서는 다음에 마우스가 움직일 때 전달됩니다.

`MouseEvent`는 GPUI의 `MouseMoveEvent`처럼 마우스 위치를 알려 줍니다. `position`은 뷰의 변환을 역으로 적용한 로컬 좌표(포인트 단위의 `Point`), `pagePosition`은 터치 이벤트와 같은 루트 뷰 기준 좌표입니다. `modifiers`에는 누르고 있는 보조 키(`control`, `alt`, `shift`, `platform`)가 담깁니다. `platform`은 Apple에서는 Command 키, 다른 플랫폼에서는 Windows 키입니다. `pressedButton`은 현재 누른 버튼(`"left"`, `"right"`, `"middle"`, `"back"`, `"forward"`, GPUI의 `MouseButton`) 또는 `null`입니다. Rust 필드명은 `position`, `page_position`, `modifiers`, `pressed_button`입니다. 데스크톱과 웹의 드래그는 터치(`onTouchMove`, 리스폰더)로도 처리되며 `pressedButton`이 설정된 `onMouseMove`도 발생합니다. 단순 호버 피드백을 그릴 때는 버튼을 누른 이동 이벤트를 건너뛰세요. iOS와 Android의 호버는 버튼을 누르지 않은 포인터를 보고합니다. 이동은 프레임당 최대 한 번, 마지막 위치만 전달하며 뒤따르는 `onMouseLeave`보다 먼저 전달하므로 이동할 때마다 핸들러에서 다시 그릴 수 있습니다.

```rust tabs
// A crosshair over a chart.
let at = signal(None::<Points>);

view()
    .h(px(200.0))
    .on_mouse_move(move |e: &MouseEvent| at.set(Some(e.position.x)))
    .on_mouse_leave(move |_: &MouseEvent| at.set(None))
    .child(when(move || at.get().is_some(), move || {
        view()
            .absolute()
            .left(move || at.get().unwrap_or_default())
            .top_0()
            .bottom_0()
            .w(px(1.0))
            .bg(rgb(0x888888))
    }))
```

```tsx tabs
// A crosshair over a chart.
const at = signal<number | null>(null);

<View onMouseMove={(e) => at.set(e.position.x)} onMouseLeave={() => at.set(null)} style={{ height: 200 }}>
  <If when={at() !== null}>
    <View style={{ position: "absolute", left: at()!, top: 0, bottom: 0, width: 1, backgroundColor: "#888" }} />
  </If>
</View>
```

### 툴팁

모든 요소에 `tooltip`으로 도움말을 지정할 수 있습니다. 데스크톱(macOS, Windows, Linux, GPUI 백엔드, 데스크톱 브라우저)에서는 포인터가 요소 위에 0.5초 머물면 위쪽에 나타나며, 포인터가 떠나거나 요소를 누르면 사라집니다. 모든 기기에서 키보드 포커스가 요소에 있을 때도 나타나며, 포커스를 옮기거나 Escape를 누르면 닫힙니다. 툴팁은 포인터나 누르기 이벤트를 가로채지 않으므로 요소를 그대로 조작할 수 있습니다.

| 속성 | Rust | 타입 | 설명 |
|---|---|---|---|
| `tooltip` | `.tooltip(..)` | `JSX.Element` | 앱에서 스타일을 지정한 도움말 요소입니다. `nsh/base`의 [`Tooltip`](../../elements/tooltip.md)을 쓰면 배치, 지연 시간, 이벤트가 지정된 툴팁 자체가 됩니다. 다른 요소는 작성한 그대로 표시됩니다. |

```rust tabs
view()
    .on_press(save)
    .tooltip(
        tooltip()
            .rounded_md()
            .bg(rgb(0x111827))
            .px_2()
            .py_1()
            .child(text("Save the document").text_color(white())),
    )
    .child(text("💾"))
```

```tsx tabs
<View
  onPress={save}
  tooltip={
    <Tooltip class="rounded-md bg-gray-900 px-2 py-1">
      <Text class="text-white">Save the document</Text>
    </Tooltip>
  }
>
  <Text>💾</Text>
</View>
```

GPUI의 `.tooltip(..)`에 툴팁을 전달하듯 `Tooltip` 자체에서 모양, 위치, 지연 시간, 이벤트를 지정합니다. 툴팁을 여럿 쓸 때는 앱에서 텍스트를 `Tooltip`으로 감싸는 작은 컴포넌트를 만들어 재사용하세요.

### 컨텍스트 메뉴

`onContextMenu`는 뷰 위에서 보조 버튼을 눌렀을 때 발생합니다. 오른쪽 클릭, macOS의 Control-클릭, 터치패드의 두 손가락 클릭, iPad 포인터의 보조 클릭, 브라우저에서 키보드의 메뉴 키를 누르는 경우가 해당합니다. 이 위치에 앱 자체 메뉴를 열 수 있고, TSX에서는 [`Menu.popup`](../../modules/nsh-desktop.md)으로 시스템 메뉴를 열 수 있습니다.

```rust tabs
// The app's own menu, placed where the click was.
let menu_at = signal(None::<Point<Points>>);

view()
    .on_context_menu(move |e: &ContextMenuEvent| menu_at.set(Some(e.page_position)))
    .child(text(row.name.clone()))
```

```tsx tabs
import { Menu } from "nsh/desktop";

<View onContextMenu={(e) => Menu.popup(rowMenu(row), e.pagePosition)}>…</View>
```

| 속성 | Rust | 타입 | 설명 |
|---|---|---|---|
| `onContextMenu` | `.on_context_menu(..)` | `(e: ContextMenuEvent) => void` | 뷰 또는 그 안의 뷰 위에서 보조 버튼을 눌렀습니다. |

`ContextMenuEvent`에는 뷰 안의 좌표 `position`, 루트 뷰 기준 좌표 `pagePosition`, 보조 키 `modifiers`가 있습니다. Rust에서는 `position`, `page_position`, `modifiers`입니다. `Menu.popup(items, e.pagePosition)`은 루트 좌표에 메뉴를 표시합니다. 포인터 아래의 뷰에서 바깥쪽으로 올라가며 이벤트를 수신하는 가장 가까운 뷰가 이를 받습니다. 그보다 안쪽에 텍스트 입력란이 있거나 편집 메뉴를 가진 선택 가능 텍스트(Windows, GPUI 백엔드, 브라우저)가 있으면 자체 메뉴를 표시합니다. 플랫폼이 버튼을 누르는 시점에 현재 마운트된 뷰를 보고 결정하므로 핸들러에서는 결과를 통지받기만 하며 기본 동작을 취소할 필요가 없습니다. `disabled` 상태인 뷰는 가장 가까운 활성 상위 리스너에 클릭을 넘깁니다. 터치스크린에서는 길게 눌러 메뉴를 열므로 같은 함수를 `onLongPress`에도 연결하세요.

### 키보드 포커스

macOS와 웹에서 Tab을 누르면 트리 순서에 따라 입력란, 누를 수 있는 뷰, `focusable` 뷰로 키보드 포커스가 이동합니다. 현재 뷰에는 `focus:` 또는 `.focus(..)` 스타일이 적용되고, 해당 뷰를 누를 수 있다면 Return이나 Space로 누릅니다. 마우스로 클릭하면 포커스를 옮기지 않고 누르기만 하므로 포커스 스타일은 키보드 포커스에만 나타납니다.

포커스와 누르기는 독립적으로 처리됩니다. 차트, 슬라이더, 방향키로 선택을 이동하는 목록처럼 포커스를 가졌을 때 키를 처리하는 뷰는 `focusable`을 설정하고 포커스를 유지하는 동안에만 키를 바인딩합니다(`onFocus`, `onBlur`와 [`keymap`](../../modules/nsh.md#keymap) 사용). 그 뷰를 누르면 페이지를 여는 카드처럼 주변에서 누르기를 처리하는 뷰에 이벤트가 전달됩니다.

ref의 `focus()`는 Tab처럼 포커스를 이동하고 포커스 스타일과 `onFocus`를 활성화합니다. 예를 들어 팝업을 닫았을 때 원래 팝업을 연 요소에 포커스를 돌려줄 수 있습니다. `blur()`는 포커스를 해제합니다.

`focus-within:`(`focusWithinStyle`, `.focus_within(..)`)은 뷰 자신이나 그 안의 뷰에 포커스가 있을 때 적용됩니다. 편집 중인 입력란의 테두리나 버튼에 포커스가 있는 도구 모음의 스타일을 지정할 때 쓸 수 있습니다. `Overlay`의 내용은 표시 위치에 상관없이 오버레이가 속한 뷰 안에 있는 것으로 처리됩니다. 따라서 팝업도 이를 여는 요소가 속한 뷰의 일부입니다. Rust 컴포넌트에서는 요소의 ref에서 `on_focus_in(..)`, `on_focus_out(..)`으로 포커스가 경계를 넘나드는 시점을 감지합니다. 내부 뷰 사이에서만 이동할 때는 둘 다 발생하지 않습니다.

| 속성 | Rust | 타입 | 기본값 | 설명 |
|---|---|---|---|---|
| `focusable` | `.focusable(..)` | `boolean` | 누르기를 처리하는 뷰는 `true` | Tab으로 포커스를 받을 수 있는지 지정합니다. `true`이면 뷰가 직접 누르기를 처리하든 상위 뷰에 넘기든 Tab으로 접근할 수 있습니다. `false`는 팝오버를 닫는 배경이나 방향키로 이동하는 행처럼 누를 수 있는 뷰를 Tab 순서에서 제외합니다. |
| `onFocus` | `.on_focus(..)` | `() => void` | | Tab 또는 `focus()`로 뷰가 키보드 포커스를 받았습니다. |
| `onBlur` | `.on_blur(..)` | `() => void` | | 키보드 포커스가 뷰를 떠났습니다. |

```rust tabs
view()
    .rounded_lg()
    .p_3()
    .focus(|s| s.bg(rgb(0xe0f2fe)))
    .on_press(open)
    .child(text("Open"))

// An overlay's backdrop: a click closes it, Tab goes past it to the content.
view().absolute().inset_0().focusable(false).on_press(close)

// A chart in a card: Tab stops at the chart, a click opens the card's page.
view().on_press(open_page).child(
    view()
        .h(px(160.0))
        .focusable(true)
        .focus(|s| s.bg(rgb(0xe0f2fe)))
        .on_focus(move || focused.set(true))
        .on_blur(move || focused.set(false)),
)
```

```tsx tabs
<View class="rounded-lg p-3 focus:bg-sky-100" onPress={open}>
  <Text>Open</Text>
</View>

{/* An overlay's backdrop: a click closes it, Tab goes past it to the content. */}
<View class="absolute inset-0" focusable={false} onPress={close} />

{/* A chart in a card: Tab stops at the chart, a click opens the card's page. */}
<View onPress={openPage}>
  <View
    class="h-40 focus:bg-sky-100"
    focusable
    onFocus={() => focused.set(true)}
    onBlur={() => focused.set(false)}
  />
</View>
```

### 레이아웃과 식별자

| 속성 | Rust | 타입 | 설명 |
|---|---|---|---|
| `onLayout` | `.on_layout(\|frame: &Bounds<Points>\| ..)` | `(e: { bounds: Bounds }) => void` | 레이아웃 변경 후 `bounds`에 부모 뷰 기준 프레임 `{ origin, size }`가 전달됩니다. |
| `ref` | `.element_ref(&handle)` | `ViewRef \| (node: ViewRef) => void` | 요소의 ref입니다([ref 메서드](#ref-메서드) 참고). `ViewRef` 또는 이를 확장한 요소별 타입을 사용합니다. `ref={el}`은 변수에 할당하고 `ref={fn}`은 함수를 호출합니다. |
| `nativeID` | `.native_id(id)` | `string` | 다른 뷰에서 참조할 이름입니다. 예를 들어 입력란이 연결할 `InputAccessoryView`를 이 이름으로 지정합니다. |
| `testID` | `.test_id(id)` | `string` | UI 테스트에서 사용할 식별자입니다. Android에서는 뷰의 리소스 ID입니다. |
| `key` | `.key(key)` | `string` | 형제 요소 사이에서 해당 요소를 식별하는 키입니다. |

### 제스처로 애니메이션 값 제어

다음 속성은 제스처를 UI 스레드에서 움직이는 [네이티브 애니메이션 값](../../modules/nsh.md#animation)에 연결합니다. 앱 코드가 매 프레임 값을 갱신할 필요가 없습니다. [터치와 제스처](../../docs/touch-and-gestures.md)를 참고하세요.

| 속성 | Rust | 타입 | 설명 |
|---|---|---|---|
| `pan` | `.pan_values(Some(&x), Some(&y))` | `{ x?: AnimatedValue; y?: AnimatedValue }` | 드래그가 포인트 단위의 값을 갱신합니다. |
| `onPanStart` | `.on_pan_start(..)` | `(e: PanEvent) => void` | 드래그가 시작되었습니다. |
| `onPanEnd` | `.on_pan_end(..)` | `(e: PanEvent) => void` | 드래그가 끝났습니다. 손을 뗄 때의 속도로 `decay` 등을 시작할 수 있습니다. |
| `pinch` | `.pinch_value(&scale)` | `{ scale?: AnimatedValue }` | 두 손가락의 핀치 동작으로 `scale`을 조절합니다. |
| `onPinchStart` / `onPinchEnd` | `.on_pinch_start(..)` / `.on_pinch_end(..)` | `(e: { scale: number; velocity: number }) => void` | 핀치가 시작되거나 끝났습니다. |
| `rotation` | `.rotation_value(&angle)` | `{ angle?: AnimatedValue }` | 두 손가락의 회전으로 `angle`을 도 단위로 조절합니다. |
| `onRotationStart` / `onRotationEnd` | `.on_rotation_start(..)` / `.on_rotation_end(..)` | `(e: { rotation: number; velocity: number }) => void` | 회전이 시작되거나 끝났습니다. |

`PanEvent`는 `{ cancelled, translation, velocity, value? }` 형태입니다. Rust에서는 `cancelled`, `translation`, `velocity_x`, `velocity_y`, `value_x`, `value_y`입니다. 시스템이 드래그를 취소했는지, 시작점 기준 변위와 초당 포인트 단위의 속도(`Point`), 그 순간 연결된 애니메이션 값(`pan`에 연결한 축별 `{ x?, y? }`)을 알려 줍니다. 애니메이션이 진행되는 중에도 드래그로 값을 이어받을 수 있습니다.

### 터치와 리스폰더 시스템

위 속성으로 처리하기 어려운 제스처에는 모든 요소가 지원하는 터치·리스폰더 속성을 사용하세요. `onTouchStart`, `onTouchMove`, `onTouchEnd`, `onTouchCancel` 및 이들의 `…Capture` 형식, `onStartShouldSetResponder`, `onMoveShouldSetResponder` 및 `…Capture` 형식, `onResponderGrant`, `onResponderReject`, `onResponderStart`, `onResponderMove`, `onResponderEnd`, `onResponderRelease`, `onResponderTerminate`, `onResponderTerminationRequest`가 있습니다. 각 핸들러는 `GestureResponderEvent`(`{ identifier, locationX, locationY, pageX, pageY, target, timestamp, touches, changedTouches, nativeEvent, stopPropagation() }`)를 받으며 `…ShouldSetResponder` 핸들러에는 `gestureState`(`dx`, `dy`, `vx`, `vy` 등)도 전달됩니다. `onResponderGrant`에서 `true`를 반환하면 네이티브 스크롤 뷰가 터치를 가져가지 않습니다. `panActivation`(`{ activeOffsetX, activeOffsetY, failOffsetX, failOffsetY }`)을 설정하면 플랫폼이 팬 제스처를 활성화한 뒤에야 뷰가 리스폰더가 됩니다. 그래서 스크롤 뷰 안에서 드래그할 때 다른 축의 움직임은 스크롤 뷰에 맡길 수 있습니다([스크롤 뷰 안의 팬](../../docs/touch-and-gestures.md#pans-inside-scroll-views-panactivation)). [`PanResponder`](../../modules/nsh.md#panresponder)는 이 시스템을 기반으로 합니다. Rust에서는 `.on_touch(..)`가 모든 터치(`TouchEvent`)를 받고 `.claim_touches(..)`가 뷰가 터치를 맡을 시점을 결정합니다. 이후에는 `.on_responder_grant(..)`, `.on_responder_move(..)`, `.on_responder_release(..)` 등이 리스폰더의 상태를 따라 호출됩니다.

### 접근성

| 속성 | Rust | 타입 | 설명 |
|---|---|---|---|
| `accessible` | `.accessible(..)` | `boolean` | 보조 기술에 이 뷰 전체를 하나의 요소로 노출합니다. 자식 요소는 따로 노출하지 않습니다. |
| `accessibilityLabel` | `.label(..)` | `string` | 화면 읽기 프로그램이 읽을 이름입니다. 레이블을 생략하면 뷰의 텍스트를 읽습니다. |
| `accessibilityHint` | `.hint(..)` | `string` | 요소를 활성화했을 때 어떤 동작이 일어나는지 설명합니다. |
| `role` | `.role(AccessibilityRole::Button)` | `AccessibilityRole` | 요소의 역할을 지정합니다(`"button"`, `"header"`, `"link"`, `"switch"` 등). |
| `accessibilityState` | `.accessibility_state(..)` | `{ disabled?, selected?, checked?, busy?, expanded?, pressed?, current? }` | 보조 기술이 알려 주는 상태입니다. `checked`와 `pressed`는 `"mixed"`일 수 있고, `current`는 `true` 또는 `"page"`, `"step"`, `"location"`, `"date"`, `"time"`입니다. |
| `accessibilityLevel` | `.accessibility_level(..)` | `number` | 제목 수준 또는 트리 항목의 깊이입니다. 최상위 값은 1입니다. |
| `accessibilityValue` | `.accessibility_value(..)` | `{ min?, max?, now?, text? }` | 범위 또는 텍스트 요소의 값입니다. |
| `accessibilityElementsHidden` | | `boolean` | 보조 기술에서 뷰와 그 하위 트리를 숨깁니다. |
| `importantForAccessibility` | | `"auto" \| "yes" \| "no" \| "no-hide-descendants"` | 뷰를 보조 기술에 노출할지 지정합니다. |
| `accessibilityActions` | | `{ name: string; label?: string }[]` | 보조 기술에 제공할 사용자 정의 동작입니다. |
| `onAccessibilityAction` | `.on_accessibility_action(..)` | `(e: { actionName: string }) => void` | 사용자가 `accessibilityActions` 중 하나를 실행했습니다. |

[접근성](../../docs/accessibility.md)을 참고하세요.

### 플랫폼 전용 속성

TSX에서는 한 플랫폼에만 있는 속성을 해당 플랫폼 이름의 객체에 넣습니다. 공통 속성은 모든 플랫폼에서 동일하고 현재 플랫폼의 객체만 전달됩니다. Rust 빌더에서는 각 플랫폼에서만 적용되는 메서드를 사용합니다.

| 속성 | 설명 |
|---|---|
| `ios={{…}}` | iOS 및 Mac Catalyst에 적용됩니다. `<Input>`, `Dialog`, `WebView`는 각각 전용 키도 제공합니다. |
| `android={{…}}` | Android에 적용됩니다. 모든 요소가 아래의 `ripple`, `elevation`, `accessibilityLiveRegion`을 지원합니다. |
| `desktop={{…}}` | 데스크톱 플랫폼과 데스크톱 브라우저에 적용되며 모바일에서는 무시합니다. 모든 요소가 아래의 `windowDrag`를 지원합니다. |
| `web={{…}}` | 브라우저에서만 적용됩니다. |

| `android` 키 | Rust | 타입 | 설명 |
|---|---|---|---|
| `ripple` | `.ripple(Ripple { .. })` | `{ color?, borderless?, radius?, foreground? }` | 눌렀을 때 나타나는 머티리얼 리플 효과입니다. |
| `elevation` | `.elevation(px(2.0))` | `number` | 뷰의 모양을 따르는 머티리얼 그림자의 높이(포인트)입니다. |
| `accessibilityLiveRegion` | | `"none" \| "polite" \| "assertive"` | 뷰의 변경 사항을 TalkBack이 어떻게 안내할지 정합니다. |

| `desktop` 키 | Rust | 타입 | 설명 |
|---|---|---|---|
| `windowDrag` | `.window_drag(..)` | `boolean` | 제목 표시줄처럼 뷰를 눌러 창을 끌 수 있습니다. 드래그가 가능한 뷰의 일부를 클릭 가능한 상태로 두려면 `false`로 설정하세요. [사용자 정의 제목 표시줄](../../modules/nsh-desktop.md#a-title-bar-of-your-own)을 참고하세요. |

```rust tabs
use nsh_core::Ripple;

view()
    .rounded_lg()
    .bg(white())
    .p_4()
    .elevation(px(2.0))
    .ripple(Ripple { color: Some(rgba(0x00000022).into()), ..Ripple::default() })
    .on_press(save)
    .child(text("Save"))
```

```tsx tabs
<View onPress={save} android={{ ripple: { color: "#00000022" }, elevation: 2 }} class="rounded-lg bg-white p-4">
  <Text>Save</Text>
</View>
```

[플랫폼별 코드](../../docs/platform-specific.md)를 참고하세요.

## ref 메서드

TSX에서 `ref`는 해당 요소 고유 타입의 참조를 받습니다. `View`, `Text`, `Image`, `Canvas`, `Overlay`, `Shader`는 `focus`, `blur`, `measure`를 갖는 `ViewRef`를 사용합니다. 그 밖의 요소는 `ViewRef`를 확장한 타입으로 전용 메서드를 제공합니다. `Input`은 `InputRef`, `Scroll`은 `ScrollRef`, `List`는 `ListRef`, `WebView`는 `WebViewRef`, `Video`는 `VideoRef`, 네이티브 컴포넌트는 `NativeComponentRef`입니다. Rust의 `element_ref()`는 `.element_ref(&handle)`로 연결하는 `ElementRef` 핸들을 만듭니다. ref 메서드로 보낸 명령은 같은 실행 차례에서 앞서 발생한 변경에 이어 순서대로 네이티브 뷰에 전달됩니다. 각 요소의 문서에서 지원 메서드를 확인하세요. ref 변수에는 요소 고유의 타입을 지정해야 합니다(`<Input>`이라면 `let field!: InputRef`). 고유 메서드가 있는 요소의 ref를 `ViewRef` 타입 변수에 연결할 수 없습니다.

| 메서드 | Rust | ref 타입 | 설명 |
|---|---|---|---|
| `focus()` / `blur()` | `focus()` / `blur()` | `ViewRef` | `<Input>`에 포커스를 주어 키보드를 표시하거나, 누를 수 있는 뷰에 키보드 포커스를 주거나 해제합니다. |
| `measure()` | `measure()` | `ViewRef` | [뷰의 위치를 측정합니다](../../docs/touch-and-gestures.md#where-a-view-is-measure). |
| `setSelection(start, end?)` | `set_selection(start, end)` | `InputRef` | UTF-16 단위로 `start..end` 구간을 선택합니다. `end`를 생략하면 커서만 놓습니다. |
| `scrollTo({ x?, y?, animated? })` | `scroll_to(point, animated)` | `ScrollRef`, `ListRef` | 지정한 오프셋으로 스크롤합니다. |
| `zoomTo({ scale, x?, y?, animated? })` | `zoom_to(scale, center, animated)` | `ScrollRef`(확대·축소 가능한 `<Scroll>`) | 콘텐츠의 한 점을 중심으로 배율을 `scale`로 조정합니다. |
| `scrollToOffset({ offset, animated? })` | `scroll_to_offset(offset, animated)` | `ListRef` | 목록의 스크롤 축을 따라 지정한 오프셋으로 이동합니다. |
| `scrollToIndex({ index, animated?, viewPosition?, viewOffset? })` | `scroll_to_index(index, position, offset, animated)` | `ListRef` | 항목이 보이도록 스크롤합니다. |
| `scrollToEnd({ animated? })` | `scroll_to_end(animated)` | `ScrollRef`, `ListRef` | 콘텐츠 끝으로 스크롤합니다. |
| `reload()`, `goBack()`, `goForward()`, `stopLoading()`, `injectJavaScript(script)`, `postMessage(data)` | `reload()`, `go_back()`, `go_forward()`, `stop_loading()`, `inject_java_script(..)`, `post_message(..)` | `WebViewRef` | 페이지를 제어합니다. |
| `seek(seconds)` | `seek(seconds)` | `VideoRef` | 지정한 재생 시점으로 이동합니다. |
| `command(name, args?)` | `command(name, args)` | `NativeComponentRef` | 등록한 네이티브 컴포넌트의 명령을 호출합니다([`nativeComponent`](../../modules/nsh.md#nativecomponent-and-nativemodule)). |

```rust tabs
let field = element_ref();

view()
    .on_press(move || field.focus())
    .child(input().element_ref(&field).placeholder("Tap the row to type"))
```

```tsx tabs
import { type InputRef, View } from "nsh";
import { Input } from "nsh/base";

let input!: InputRef;
<View onPress={() => input.focus()}>
  <Input ref={input} placeholder="Tap the row to type" />
</View>;
```

## 핸들러가 UI를 차단하지 않는 이유

메인 스레드는 앱 코드의 실행이 끝나기를 기다리지 않습니다. 시스템이 즉시 답을 요구하면(텍스트를 바꿀 수 있는지, 뒤로 가기 버튼으로 화면을 닫을 수 있는지, 시트를 닫아도 되는지 등) NSH가 미리 설정한 `maxLength`, `popOnBack`, `closeOnSwipe` 등의 속성을 기준으로 네이티브에서 판단하고 이후에 이벤트를 앱에 전달합니다. 이런 동작을 핸들러의 반환값 대신 속성으로 지정하는 이유입니다.
