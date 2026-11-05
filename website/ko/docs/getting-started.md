---
title: 시작하기
description: nsh init으로 Rust 또는 TSX 앱을 만들고 iOS, Android, macOS, Windows, Linux, 웹에서 실행합니다. TSX 모듈을 기존 앱에 임베딩해 서명된 패키지로 배포하거나 독립 실행형 앱을 패키징하는 방법도 살펴봅니다.
order: 4
group: start
demo: guide-getting-started
---

# 시작하기

이 가이드에서는 Native Shell 앱을 만들고 원하는 플랫폼에서 실행한 뒤 배포합니다. Rust와 TSX는 같은 엔진을 사용합니다. Rust 앱은 JavaScript VM 없이 실행되는 네이티브 코드로 컴파일됩니다. TSX는 QuickJS에서 실행되며 저장한 변경 사항을 즉시 반영합니다. 네이티브 앱이 실행 중에 TSX 모듈을 불러올 수도 있습니다. 우선 Rust로 앱을 만들고 같은 앱을 TSX로 작성해 보겠습니다. 먼저 [설치](./installation.md)를 따라 `nsh`를 `PATH`에 추가하세요. 소스 체크아웃에서는 `./target/debug/nsh`를 사용하면 됩니다.

> [!TIP]
> 기존 앱 프로젝트 없이도 처음 두 절을 따라 할 수 있습니다. `nsh run`으로 컴퓨터의 창, iOS 시뮬레이터, Android 기기, 브라우저에서 앱을 실행해 보세요. 여기까지 실습하거나 이어서 [독립 실행형 앱으로 패키징](#독립-실행형-앱으로-배포)할 수 있습니다.

## 앱 만들기

터미널에서 `nsh init`을 실행하면 앱을 작성할 언어를 묻습니다. `rust`, `ts`, `mixed`(자체 Rust 크레이트를 포함하는 TSX) 중에서 고르세요. `--template`으로 미리 지정할 수도 있습니다.

### Rust로 만들기

```sh
nsh init my-app --template rust
```

`nsh init`은 디렉터리를 만들고 다음 파일로 Cargo 프로젝트를 구성합니다.

| 파일 | 용도 |
|---|---|
| `src/lib.rs` | 앱의 루트 컴포넌트, 상태, 핸들러 |
| `src/main.rs` | `cargo run`으로 macOS, Windows 또는 Linux에서 같은 루트를 마운트하는 진입점 |
| `Cargo.toml` | CLI와 버전이 같은 `nsh-core`, `nsh-base`에 의존하는 크레이트 설정 |
| `nsh.json` | 앱 이름, ID, 버전, 빌드 번호, 크레이트(`"rust": "."`), 루트 컴포넌트(`"root": "App"`) 설정. [프로젝트 구조](./project-structure.md#nshjson) 참고 |
| `App.test.tsx` | TypeScript로 작성한 루트 컴포넌트 테스트(`nsh test`). 편집기용 `tsconfig.json`과 `.nsh/types`도 함께 생성 |
| `README.md` | 앱 실행, 테스트, 패키징, TSX 진입점 추가 방법 |
| `.gitignore` | Git에서 `.nsh/`와 `target/` 제외 |

Rust로 개발하려면 [rustup](https://rustup.rs)으로 Rust 툴체인을 설치하세요. 빌드에는 [huacnlee/cargo](https://github.com/huacnlee/cargo)를 권장합니다. `nsh` 명령은 CLI와 버전이 같은 crates.io의 NSH 크레이트를 사용해 앱 실행기를 빌드합니다.

### TSX로 만들기

```sh
nsh init my-app --template ts
```

TSX 앱은 하나 이상의 `.tsx` 파일로 구성됩니다. 진입점은 `nsh run`으로 실행할 파일이자 호스트 앱이 모듈로 불러올 파일입니다.

| 파일 | 용도 |
|---|---|
| `App.tsx` | `mount`로 앱을 렌더링하는 진입 모듈 |
| `App.test.tsx` | 앱 테스트(`nsh test`) |
| `nsh.json` | 앱 이름, ID, 버전, 빌드 번호. [프로젝트 구조](./project-structure.md#nshjson) 참고 |
| `tsconfig.json` | NSH의 TypeScript 설정인 `.nsh/tsconfig.json`을 확장해 `nsh`를 `.nsh/types`의 타입 선언에 연결 |
| `.gitignore` | Git에서 `.nsh/`와 `target/` 제외 |
| `.nsh/types/` | 편집기를 위해 생성한 모든 NSH 모듈의 타입 선언 |

TSX 템플릿은 기존 파일을 보존하므로 위 파일 중 일부가 이미 있는 디렉터리에서도 실행할 수 있습니다. Rust 앱은 새 디렉터리에 생성합니다.

두 템플릿 모두 기본 표시 이름은 디렉터리 이름입니다. 다른 이름을 쓰려면 `--name "My App"`을 전달하세요. ID는 `com.example.<name>`으로 생성되므로 배포하기 전에 `nsh.json`에서 변경하세요. `nsh init my-app --template mixed`는 Rust 크레이트를 포함하는 TSX 앱을 만듭니다. `App.tsx` 옆에 `Cargo.toml`과 `src/lib.rs`가 생기고, `App.tsx`는 크레이트의 컴포넌트, 모듈, 클래스를 가져옵니다. [Rust로 개발하기](./rust.md)를 참고하세요.

### 루트 컴포넌트

두 템플릿은 같은 카운터 앱을 만듭니다.

```rust tabs
// src/lib.rs
use nsh_base::theme;
use nsh_core::prelude::*;

/// The crate's UI set: what a TSX entry may import from `my-app/native`.
#[native_module("my-app/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// The application's root component.
#[component]
pub fn App() -> impl IntoElement {
    let count = signal(0_u32);
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(
            view()
                .flex_1()
                .p_safe_area()
                .child(
                    view()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_4()
                        .child(text(move || format!("Count: {}", count.get())).text_size(px(28.0)))
                        .child(
                            view()
                                .px_5()
                                .py_3()
                                .rounded(px(10.0))
                                .bg(move || theme().primary)
                                .active(|s| s.opacity(0.6))
                                .on_press(move || count.update(|value| *value += 1))
                                .child(
                                    text("Tap")
                                        .text_size(px(17.0))
                                        .text_color(move || theme().primary_foreground),
                                ),
                        ),
                ),
        )
}
```

```tsx tabs
// App.tsx
import { mount, signal, Text, View } from "nsh";

function App() {
  const count = signal(0);
  return (
    <View class="flex-1 items-center justify-center gap-4 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]">
      <Text class="text-3xl font-semibold text-foreground">{count()}</Text>
      <View
        onPress={() => count.update((c) => c + 1)}
        role="button"
        class="rounded-(--radius) bg-primary px-5 py-3 active:bg-primary/90"
      >
        <Text class="text-lg text-primary-foreground">Tap</Text>
      </View>
    </View>
  );
}

mount(App);
```

Rust 예제를 살펴봅시다.

- **`use nsh_core::prelude::*`**는 기본 요소(`view()`, `text()`, `scroll()`, `input()`, `image()` 등), [시그널](../../docs/reactivity.md), 타입이 지정된 스타일 메서드를 가져옵니다. 예를 들어 `.p_safe_area()`는 [안전 영역](../../elements/safe-area.md)만큼 뷰에 안쪽 여백을 줍니다. **`nsh_base`**에는 `button()`, 키보드, 대화상자 등 기본 스타일이 없는 컴포넌트와 앱이 그릴 색상과 모서리 반경을 제공하는 `theme()`이 있습니다([테마](./rust.md#테마)).
- **`#[component] pub fn App()`**은 **한 번** 실행되어 요소를 반환하는 컴포넌트 함수입니다. `nsh.json`의 `"root": "App"`이 이 함수를 지정하고, `nsh run`이 마운트합니다. `signal(0_u32)`는 시간에 따라 바뀔 수 있는 값을 만듭니다.
- **`text(move || format!(…, count.get()))`**는 텍스트와 시그널을 연결합니다. `count`가 바뀌면 NSH는 이 클로저를 다시 실행해서 해당 텍스트만 갱신합니다. 스타일에도 클로저를 쓸 수 있습니다. 예를 들어 `.bg(move || theme().primary)`는 다크 모드로 전환하면 테마를 따라 바뀝니다.
- **`.on_press(..)`**는 뷰를 눌렀을 때 호출할 동작을 지정합니다. `.active(|s| s.opacity(0.6))`로 지정한 스타일은 누르는 동안 네이티브에서 적용됩니다. NSH가 제공하는 기본 요소와 기능을 조합해 앱 자체의 버튼을 그릴 수 있습니다.
- **`.px_5()`, `.rounded(px(10.0))`, `.gap_4()`**는 Tailwind 유틸리티의 명명법을 따르는 타입 지정 스타일 메서드입니다. 단위는 포인트(`px`)입니다. 레이아웃은 flexbox이며 자식은 기본적으로 세로로 쌓입니다([레이아웃](../../docs/layout.md)).
- **`#[native_module("my-app/native")]`**는 나중에 TSX 화면을 추가할 때 진입 모듈이 이 크레이트에서 가져올 모듈의 이름을 지정합니다([Rust 앱에 TSX 추가](./rust.md#tsx로-rust-앱-확장하기)).

TSX 예제에서는 다음과 같습니다.

- **`import { mount, signal, Text, View } from "nsh"`**는 코어 모듈 `nsh`에서 API를 가져옵니다. 이 모듈에는 시그널, `mount`, 그 밖의 반응형 기본 기능과 호스트 요소가 있습니다. 네이티브 런타임이 직접 제공하므로 앱 번들에 포함되지 않습니다. **`nsh/base`**는 Rust의 `nsh_base`처럼 기본 컴포넌트를 제공합니다.
- **`function App()`**은 **한 번** 실행됩니다. JSX 안의 **`{count()}`**는 시그널 값을 읽습니다. 컴파일러는 이 호출을 인식하고 시그널이 바뀔 때 해당 텍스트를 갱신합니다.
- **`<View onPress>`**는 누르기에 반응하는 네이티브 뷰입니다. `active:` 클래스는 누르고 있는 동안의 스타일을 지정합니다. `View`, `Text`, `Image`, `Scroll` 등의 [호스트 요소](../elements/index.md)는 `nsh`에서, `Input`은 `nsh/base`에서 가져옵니다. `If`와 `Each`만 import 없이 사용할 수 있습니다.
- **`class`**에는 [Tailwind](../../docs/tailwind.md) 클래스를, `style={{ … }}`에는 camelCase 속성 이름으로 구성된 스타일 객체를 전달합니다. 숫자의 단위는 포인트입니다. `bg-background`, `bg-primary`, `text-primary-foreground`, `rounded-(--radius)`는 앱의 [테마 토큰](../../docs/tailwind.md#theme-tokens)을 가리킵니다. 다크 모드를 자동으로 따르며 `nsh.json`의 `"theme"`에서 디자인을 바꿀 수 있습니다.
- **`mount(App)`**은 컴포넌트를 앱의 화면에 렌더링합니다.

> [!IMPORTANT]
> 텍스트는 Rust에서 `text(..)`, TSX에서 `<Text>`로 표시하세요. TSX에서 `<View>Hello</View>`처럼 `<View>`에 텍스트를 직접 쓰면 해당 위치에서 빌드 오류가 발생합니다. `<View>{name}</View>`처럼 표현식이 문자열을 반환하면 로그에 표시됩니다.

## 앱 개발

### 실행

`nsh run`은 앱을 빌드해 실행하고, 코드를 수정하면 다시 빌드합니다. 앱 로그는 터미널에 표시됩니다. 앱 디렉터리를 전달하면 Rust 앱의 루트 또는 TSX 앱의 `App.tsx`를 실행합니다. `nsh run my-app/App.tsx`처럼 진입 파일을 직접 지정할 수도 있습니다.

Rust 앱에서는 `nsh run`이 현재 타깃용 실행기를 빌드해 시작합니다. 실행기에는 앱 크레이트와 마운트된 루트가 포함되며 QuickJS는 사용하지 않습니다. Rust 코드를 바꾸면 다시 빌드하고 앱을 재시작합니다. 앱 디렉터리에서 `cargo run`을 실행하면 같은 루트를 데스크톱 백엔드에서 바로 시작할 수 있습니다. TSX 앱에서는 `nsh run`이 개발 서버와 그곳에서 앱을 불러오는 개발 호스트를 시작하고, 저장할 때마다 실행 중인 앱에 변경 사항을 반영합니다.

`--target`을 생략하면 현재 머신에서 macOS는 AppKit 창, Windows는 Win32 창, Linux는 GTK 창으로 실행합니다. 다른 플랫폼을 지정하려면 `--target`을 사용하세요.

| 타깃 | 명령 | 실행 환경 |
|---|---|---|
| iOS 시뮬레이터 | `nsh run my-app --target ios` | Xcode가 설치된 macOS |
| Android | `nsh run my-app --target android` | macOS, Windows, Linux |
| macOS (AppKit) | `nsh run my-app` | macOS(기본값) |
| Windows | `nsh run my-app` | Windows(기본값) |
| Linux (GTK 4) | `nsh run my-app` | Linux(기본값) |
| 웹 | `nsh run my-app --target web` | macOS, Windows, Linux |
| Mac Catalyst | `nsh run my-app --target catalyst` | macOS |

#### iOS 시뮬레이터

```sh
nsh run my-app --target ios
nsh run my-app --target ios --device "iPhone 16 Pro"
```

`nsh`는 실행 중인 시뮬레이터를 사용합니다. 실행 중인 기기가 없으면 가장 최신 iPhone 시뮬레이터를 시작하고 앱(Rust 앱 실행기 또는 개발 호스트)을 설치합니다. `--device`로 시뮬레이터 이름이나 UDID를 지정할 수 있습니다.

#### Android

```sh
nsh run my-app --target android
```

에뮬레이터를 시작하거나 실제 기기를 연결하세요. `adb devices`가 표시하는 기기 중 첫 번째 기기에 `nsh`가 앱을 설치합니다. Rust 앱의 경우 앱 자체의 `libnsh.so`를 사용합니다. TSX 앱의 경우 런타임이 변경될 때만 다시 빌드하는 개발 호스트 APK를 사용합니다. `adb reverse`로 개발 서버 포트를 기기에 전달하므로 같은 네트워크에 접속하지 않아도 USB로 연결해 사용할 수 있습니다.

#### macOS (AppKit)

```sh
nsh run my-app --target macos
```

Mac 데스크톱 앱이 크기를 조절할 수 있는 AppKit 창으로 열리며 마우스, 트랙패드, 키보드로 조작할 수 있습니다. macOS 14 이상이 필요합니다. [macOS](../../docs/macos.md)를 참고하세요.

#### Windows (Windows 기본값)

```sh
nsh run my-app
```

1200×800포인트 크기의 데스크톱 창이 열리며 마우스, 터치패드, 터치, 키보드 입력을 지원합니다. Windows 10 1809 이상이 필요합니다. [Windows](../../docs/windows.md)를 참고하세요.

#### Linux (Linux 기본값)

```sh
nsh run my-app
```

390×844포인트 크기의 GTK 창이 열립니다. GTK 4.14 이상이 필요합니다. [Linux](../../docs/linux.md)를 참고하세요.

#### 웹

```sh
nsh run my-app --target web
```

브라우저에서 `nsh`가 출력한 주소를 여세요. Rust 앱은 앱 크레이트를 포함한 WebAssembly로 실행됩니다. 코드를 바꾸면 열려 있는 페이지가 새 빌드를 자동으로 불러옵니다. [웹](../../docs/web.md)을 참고하세요.

#### Mac Catalyst

```sh
nsh run my-app --target catalyst
```

Mac에서 iPhone 크기(390×844포인트)의 창이 열립니다. 시뮬레이터를 시작할 필요가 없어 휴대전화 화면을 빠르게 확인할 수 있습니다. iOS와 마찬가지로 UIKit을 사용하므로 마우스 휠 스크롤 같은 Mac 고유의 동작을 제외하면 iPhone과 같은 화면을 볼 수 있습니다. Mac 앱 자체를 실행하려면 `--target macos`를 사용하세요.

### 코드 수정

`nsh run`을 실행한 채로 버튼의 문구를 바꿔 보세요.

```rust tabs
text("Add one")
    .text_size(px(17.0))
    .text_color(move || theme().primary_foreground)
```

```tsx tabs
<Text style={{ color: "white", fontSize: 17 }}>Add one</Text>
```

저장하면 Rust 앱은 다시 빌드되고 새 문구로 재시작하며 카운트도 초기화됩니다. TSX 앱은 핫 업데이트를 받습니다. CLI가 변경 내용을 빌드해 패치를 보내면 호스트가 해당 모듈을 다시 실행하고 코드가 바뀐 컴포넌트만 다시 렌더링합니다. 이 예제에서는 `count` 시그널을 소유한 `App`이 바뀌었으므로 카운트가 초기화됩니다.

UI 일부를 별도 컴포넌트나 요소를 반환하는 일반 함수로 분리해 보세요.

```rust tabs
/// The app's button: a view that answers presses.
fn tap_button(on_press: impl Fn() + 'static) -> impl IntoElement {
    view()
        .p_3()
        .rounded(px(10.0))
        .bg(move || theme().primary)
        .active(|s| s.opacity(0.6))
        .on_press(on_press)
        .child(text("Tap").text_size(px(17.0)).text_color(move || theme().primary_foreground))
}

// In App: .child(tap_button(move || count.update(|value| *value += 1)))
```

```tsx tabs
function TapButton(props: { onPress: () => void }) {
  return (
    <View onPress={props.onPress} activeStyle={{ opacity: 0.6 }} style={{ padding: 12, borderRadius: 10, backgroundColor: "#0969da" }}>
      <Text style={{ color: "white", fontSize: 17 }}>Tap</Text>
    </View>
  );
}

// In App: <TapButton onPress={() => count.update((c) => c + 1)} />
```

TSX에서는 상태를 소유하지 않는 컴포넌트를 수정하면 해당 상태가 유지됩니다. 버튼을 여러 번 누른 뒤 `TapButton`의 색을 바꿔 보세요. 카운트는 그대로이고 버튼만 달라집니다. 핫 교체는 이름이 대문자로 시작하고 본문에 JSX가 있는 최상위 컴포넌트 단위로 작동합니다. 컴포넌트 자체의 코드가 바뀌거나 모듈 내 컴포넌트 바깥의 코드(사용하는 상수나 헬퍼 등)가 바뀌면 컴포넌트를 다시 렌더링합니다. 실행 중인 앱에서 처음으로 새 NSH 모듈을 가져오거나 `nsh.json`의 권한을 바꾸는 등 변경 사항을 부분적으로 적용할 수 없을 때는 호스트가 앱 전체를 다시 불러옵니다.

#### 오류

Rust 컴파일 오류는 Cargo 출력과 함께 터미널에 표시됩니다. 다음에 저장하면 `nsh run`이 다시 빌드합니다. TSX 빌드 오류는 터미널과 기기의 빨간색 오버레이에 파일, 줄, 열 번호와 함께 표시되며, 앱은 마지막으로 성공한 빌드를 계속 실행합니다. 다음 빌드에 성공하면 오버레이가 사라집니다. 눌러서 먼저 닫을 수도 있습니다. 런타임 예외도 같은 방식으로 표시되며 스택의 위치는 원래 `.tsx` 소스에 대응합니다.

## 기존 앱에 임베딩

네이티브 앱은 자신의 뷰 안에 NSH를 표시합니다. iOS에서는 `NshAppView`(`UIView`), Android에서는 `NshView`(Android `View`), macOS, Windows, Linux 및 GPUI 앱에서는 `AppView`를 사용합니다. 다른 뷰와 마찬가지로 원하는 화면에 필요한 수만큼 배치하세요.

Rust 루트는 앱을 빌드할 때 포함됩니다. macOS, Windows, Linux, GPUI, iOS에서 앱이 크레이트를 링크하고 `AppSource::Rust`로 `AppView`에 루트를 마운트합니다([Rust 앱 작성](./rust.md#rust로-앱-작성하기)). TSX 앱은 뷰가 불러오는 모듈입니다. 개발 중에는 개발 서버에서, 릴리스에서는 미리 빌드한 번들이나 다운로드한 서명 패키지에서 불러옵니다. 아래 예제는 TSX 모듈을 불러오는 방법입니다.

### iOS

Debug 빌드에는 `NativeShellDev.xcframework`, Release 빌드에는 `NativeShell.xcframework`를 링크한 뒤([iOS 앱에 임베딩](../../docs/ios.md)) 다음과 같이 작성하세요.

```swift
import NativeShell

let app = NshAppView()
app.onError = { message in print("nsh error: \(message)") }
view.addSubview(app)   // and constrain it like any view

#if DEBUG
try app.connectDevServer(URL(string: "ws://127.0.0.1:8790/ws")!)   // `nsh dev my-app` on your Mac
#else
try app.loadBundle(at: Bundle.main.url(forResource: "bundle", withExtension: "nshc", subdirectory: "nsh")!)
#endif
```

`nsh dev my-app`은 호스트 앱에 모듈을 제공하고 저장할 때마다 변경 사항을 전송합니다. `nsh run`이 자체 호스트에서 하는 일과 같습니다. 릴리스 빌드에서는 `nsh build my-app --release --out MyApp/nsh`로 번들을 생성하고 해당 폴더를 앱 리소스에 추가하세요.

### Android

디버그 빌드에는 `nsh-dev.aar`, 릴리스 빌드에는 `nsh.aar`를 추가한 뒤([Android 앱에 임베딩](../../docs/android.md)) 다음과 같이 작성하세요.

```kotlin
val nsh = NshView(this)
nsh.setErrorListener { message -> Log.e("nsh", message) }
setContentView(nsh)   // or add it to any layout

if (BuildConfig.DEBUG) {
    nsh.connectDevServer("ws://127.0.0.1:8790/ws")   // with `adb reverse tcp:8790 tcp:8790`
} else {
    nsh.loadAssetBundle("nsh")   // assets/nsh, from `nsh build my-app --release --out app/src/main/assets/nsh`
}
```

### macOS, Windows, Linux

Rust로 작성한 데스크톱 앱은 플랫폼별 크레이트(`nsh-platform-macos`, `nsh-platform-windows`, `nsh-platform-linux`, `nsh-platform-gpui`)의 `AppView`로 NSH를 임베딩합니다. 각 플랫폼의 `NSView`, 자식 창, GTK 위젯, GPUI 뷰가 Rust 루트를 마운트하거나 빌드된 번들을 불러옵니다. [macOS](../../docs/macos.md#embedding-in-a-mac-app), [Windows](../../docs/windows.md#embedding-in-a-win32-app), [Linux](../../docs/linux.md#embedding-in-a-gtk-app), [GPUI](./gpui.md)에서 자세한 내용을 확인하세요. [플랫폼](./platforms.md#네이티브-앱에-임베드하기)에서는 호스트별 방식을 비교할 수 있습니다.

### 모듈 권한 설정

iOS와 Android에서는 호스트가 모듈을 불러오기 전에 권한을 정합니다. `grantCapabilities`는 모듈이 접근할 수 있는 네트워크 호스트와 기능을 지정합니다. 메모리, 호출 시간, 노드 수에는 상한을 설정할 수 있습니다([샌드박스와 권한](./sandbox-and-capabilities.md)). 지도, 차트, 디자인 시스템 컴포넌트처럼 앱 고유의 뷰를 모듈에 제공하려면 [네이티브 컴포넌트](../../docs/native-components.md)로 등록하세요.

## 서명된 패키지로 업데이트 배포

출시 후 앱에서 다운로드하는 TSX 모듈은 서명된 패키지로 배포합니다. 먼저 루트 키를 한 번 생성해 오프라인에 보관하고, 이 키로 릴리스 키를 인증한 뒤 패키지를 게시하세요.

```sh
nsh keygen keys/root && nsh keygen keys/release
nsh certify --root keys/root.secret --key keys/release.pub --apps 'com.example.*' -o keys/release.cert
nsh publish my-app --key keys/release.secret --cert keys/release.cert --build 2
```

앱은 루트 공개 키를 신뢰하도록 설정하고 다운로드한 패키지를 불러옵니다. NSH는 코드를 실행하기 전에 서명, 인증서, 폐기 목록을 확인합니다.

```swift
let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
let trust = try NshTrust(stateDirectory: support.appendingPathComponent("nsh"))
try trust.addRoot(rootPublicKey)   // the contents of keys/root.pub
let info = try await app.loadPackage(at: downloadedFile, trust: trust)
```

Android에서도 `NshTrust`와 `NshPackage`로 같은 절차를 따릅니다. 호스팅, 배포, 폐기 절차는 [서명된 패키지](../../docs/signed-packages.md)를 참고하세요.

## 독립 실행형 앱으로 배포

`nsh package`는 별도의 Xcode 프로젝트나 Gradle 빌드 설정 없이 Rust 또는 TSX로 작성한 앱을 설치 가능한 형태로 빌드합니다.

```sh
nsh package my-app --target ios-simulator --run
nsh package my-app --target android
nsh package my-app --target macos
nsh package my-app --target linux
nsh package my-app --target windows
nsh package my-app --target web
```

Rust 앱은 앱 크레이트와 NSH 크레이트로 빌드하고 루트를 마운트한 자체 실행기와 함께 패키징합니다. TSX 앱은 미리 빌드한 런타임과 함께 패키징합니다. 옵션, 서명, 아이콘 설정은 [패키징](../../docs/packaging.md)을 참고하세요.

## 터미널에서 테스트

`nsh test`는 기기나 창 없이 앱을 실행합니다. Rust 앱의 `nsh.json` 또는 TSX 앱의 진입 파일을 전달하면 앱을 열고 실행한 뒤 무엇을 마운트했는지 출력합니다. `--trace`를 추가하면 네이티브 뷰 작업과 각 커밋의 소요 시간도 확인할 수 있습니다([테스트와 추적](../../docs/testing-and-trace.md)).

```sh
nsh test my-app/nsh.json --trace     # a Rust app's root
nsh test my-app/App.tsx --trace      # a TSX app's entry
```

디렉터리를 전달하면 앱의 테스트를 실행합니다. 두 템플릿의 `App.test.tsx`는 앱을 열고 버튼을 누른 뒤 표시된 결과를 검사합니다(`nsh test my-app`). Rust 앱에서는 `open(".")`으로 루트를 엽니다. `cargo test`에서 `nsh_testing::Headless`로 테스트할 수도 있습니다([Rust에서 테스트](./rust.md#테스트)).

## 다음 단계

- [Rust로 개발하기](./rust.md)에서 Rust 앱, 테마, Rust 크레이트에 TSX를 결합하는 방법을 알아보세요.
- [JSX와 컴포넌트](../../docs/jsx-and-components.md), [반응성](../../docs/reactivity.md)에서 Rust와 TSX의 컴포넌트와 시그널을 살펴보세요.
- [플랫폼](./platforms.md)에서 각 플랫폼이 지원하는 기능과 개별 문서([iOS](../../docs/ios.md), [Android](../../docs/android.md), [macOS](../../docs/macos.md), [Windows](../../docs/windows.md), [Linux](../../docs/linux.md))를 확인하세요.
- [서명된 패키지](../../docs/signed-packages.md), [샌드박스와 권한](./sandbox-and-capabilities.md)에서 모듈 배포와 권한 관리를 알아보세요.
- [프로젝트 구조](./project-structure.md)에서 `nsh.json`, 에셋, 파일 배치를 확인하세요.
