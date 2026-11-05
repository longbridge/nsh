---
title: Rust로 앱 만들기
description: "JavaScript 가상 머신 없이 Rust로 앱을 만드는 방법과 TSX 화면을 결합하는 방법을 설명합니다. 두 방식의 선택 기준과 언어 간 컴포넌트·모듈 공유 방식을 살펴봅니다."
order: 5.5
group: start
---

# Rust로 앱 만들기

Native Shell 앱은 Rust, TSX 또는 두 언어를 함께 사용해 작성합니다. 세 방식 모두 같은 엔진을 사용합니다. 반응형 그래프, 렌더러, Taffy 레이아웃, 네이티브 모듈(탐색, 대화 상자, 키보드, 상태 표시줄)은 Rust로 작성되어 있으며, 각 플랫폼은 고유한 UI로 화면을 그립니다. 달라지는 것은 앱 코드를 작성하는 언어와 JavaScript 가상 머신의 포함 여부입니다.

| | Rust | TSX | Rust와 TSX |
|---|---|---|---|
| UI, 상태, 이벤트 처리 | Rust | QuickJS에서 실행하는 TSX | Rust 컴포넌트·모듈을 TSX 화면에서 사용 |
| 실행 환경 | 모든 플랫폼과 브라우저에서 엔진과 앱 크레이트만 실행 | 엔진과 QuickJS(브라우저에서는 자체 JavaScript 엔진) | 앱 크레이트, 엔진, QuickJS(브라우저에서는 자체 JavaScript 엔진) |
| 적합한 경우 | 복잡한 비즈니스 로직을 다루는 중대형 앱 | 실행 중에 배포하는 앱·모듈, 빠르게 수정할 UI | 코어는 Rust로 유지하고 일부 화면을 자주 바꾸는 앱 |
| 수정 시 동작 | 다시 빌드하고 재시작 | 핫 업데이트 | TSX는 핫 업데이트, Rust는 다시 빌드하고 재시작 |
| 시작 명령 | `nsh init my-app --template rust` | `nsh init my-app --template ts` | `nsh init my-app --template mixed` |

## 개발 방식 선택

**Rust**는 거래 앱, 편집기, 대량의 데이터를 다루는 도구처럼 복잡한 비즈니스 로직이 많은 앱에 적합합니다. 앱 전체에서 Rust 타입 체계와 컴파일 시 검사를 활용하고, crates.io 생태계를 이용하며, 앱 코드 자체도 네이티브 성능으로 실행할 수 있습니다. `cargo test`로 실제 UI를 화면 없이 테스트할 수 있습니다. iOS, Android, macOS, Windows, Linux, GPUI, 웹 모두 엔진부터 앱까지 Rust 코드로 구성되어 JavaScript 가상 머신을 배포하거나 실행할 필요가 없습니다. 컴포넌트는 TSX와 마찬가지로 한 번 실행되는 함수이며 시그널을 표시할 요소에 바인딩합니다. 스타일은 클래스 문자열 대신 타입이 지정된 메서드로 작성합니다. [반응성](../../docs/reactivity.md)

**TSX**는 앱 출시 후 추가하는 UI에 적합합니다. 예를 들어 네이티브 앱이 모델이 생성했거나 다른 팀이 작성한 모듈을 실행 중에 불러올 수 있습니다. 모듈은 [서명된 패키지](../../docs/signed-packages.md)로 배포하며, 호스트가 설정한 샌드박스에서 실행됩니다. 개발 중에는 핫 업데이트를 사용할 수 있습니다.

**Rust와 TSX**를 함께 사용하면 핵심 로직은 Rust로 유지하면서 일반 화면, 설정, 온보딩, 실험 기능을 TSX로 만들 수 있습니다. 핫 업데이트로 자주 수정하거나 실행 중에 [서명된 패키지](../../docs/signed-packages.md)로 배포할 부분에 적합합니다. TSX는 다른 NSH 모듈처럼 Rust 컴포넌트와 모듈을 가져옵니다. 두 언어는 타입이 있는 인터페이스로 연결되며, 앱에 포함한 QuickJS가 TSX를 실행합니다.

## Rust로 앱 작성하기

```sh
nsh init my-app --template rust
cd my-app
nsh run                        # this machine's backend; --target ios, android, web, macos, linux or windows
nsh package --target web       # or ios, ios-simulator, android, macos, linux, windows
```

앱의 컴포넌트, 상태, 핸들러, 비동기 작업은 `src/lib.rs`에 작성합니다.

```rust
use nsh_base::{button, theme};
use nsh_core::prelude::*;

/// The components available to TSX through `app/native`.
#[native_module("app/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

#[component]
pub fn App() -> impl IntoElement {
    let count = signal(0_u32);
    let quote = signal(SharedString::from("…"));
    // Async work runs in tasks; requests go through NSH's network stack.
    spawn(async move {
        if let Ok(response) = fetch("https://api.example.com/quote").await {
            quote.set(response.text().into());
        }
    });
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_4()
        .child(text(move || format!("Count: {}", count.get())))
        .child(text(quote))
        .child(
            // nsh_base's unstyled button, drawn with the theme's colors.
            button()
                .px(px(20.0))
                .py(px(12.0))
                .rounded(move || theme().radius)
                .bg(move || theme().primary)
                .active(|s| s.bg(move || theme().primary.opacity(0.9)))
                .on_press(move || count.update(|c| *c += 1))
                .child(text("Tap").text_color(move || theme().primary_foreground)),
        )
}
```

요소와 컴포넌트는 같은 방법으로 만듭니다. `view()`, `text(..)`, `nsh_base`의 `button()`, `#[component] fn Chart`에서 생성되는 `chart()`처럼 함수를 호출해 만든 뒤 메서드로 속성과 리스너를 지정합니다. 속성마다 값을 받는 세터가 하나씩 있으며 값, 시그널, 클로저를 전달할 수 있습니다. 리스너는 이벤트마다 하나씩 있고 이벤트 데이터를 받는 클로저(`move |text: &str| ..`)를 사용합니다. 데이터가 없는 이벤트에는 `move || ..`를 사용합니다. `.child(..)`는 요소 하나뿐 아니라 요소를 담은 `Option`, `Vec`, 배열, 반복자도 받습니다. 예를 들어 `.child(rows.iter().map(|r| row(r)))`로 행을 추가할 수 있습니다.

`nsh.json`에 루트 컴포넌트를 지정하면 `nsh`가 이를 마운트합니다.

```json
{
  "name": "My App",
  "rust": ".",
  "root": "App",
  "theme": "theme.json",
  "appearance": "system"
}
```

`"root"`는 크레이트 안의 컴포넌트 경로입니다(예: `App`, `shell::Gallery`). [크레이트가 여러 개](#여러-크레이트-사용하기)라면 첫 번째 크레이트를 기준으로 합니다. `"theme"`는 컴포넌트가 사용할 [테마](#테마)를 정하고 `"appearance"`는 TSX 앱과 같은 방식으로 적용됩니다. 앱 디렉터리에서 `nsh run`이나 `nsh package`를 실행하면 루트 컴포넌트를 마운트하는 대상 플랫폼용 실행기를 빌드합니다. 이 실행기는 QuickJS를 링크하지 않습니다.

| 대상 | 실행 방식 |
|---|---|
| macOS, Windows, Linux | 앱 크레이트가 포함된 릴리스 실행기가 창을 엽니다. `nsh run`은 디버그 빌드를 실행하고 터미널에 앱 로그를 표시합니다. |
| iOS, iOS 시뮬레이터, Mac Catalyst | 앱 크레이트를 포함한 iOS 실행기가 앱 안에서 실행됩니다. |
| Android | 런타임 액티비티가 앱 크레이트를 포함한 `libnsh.so`를 시작합니다. |
| 웹 | 페이지에서 엔진과 앱 크레이트를 포함한 `nsh.wasm`의 루트 컴포넌트를 마운트합니다. Rust 갤러리 전체는 3.3 MB이며 Brotli로 압축하면 0.75 MB입니다. |

`nsh`는 필요한 부분만 빌드합니다. 먼저 앱 크레이트와 NSH 코어로 작은 프로그램을 빌드해 컴포넌트와 모듈 정보를 CLI에 전달합니다. 이후 작업은 CLI가 맡으며, 실제로 실행하거나 패키징하는 대상의 실행기만 빌드합니다. 새 Rust 앱의 첫 빌드에는 앱 크레이트와 NSH 코어를 포함해 약 60개 크레이트가 컴파일됩니다. `nsh test`는 같은 방식으로 구성한 앱 크레이트의 실행기에서 화면 없이 테스트를 수행합니다.

Linux에서는 앱 크레이트가 선택적 플랫폼 기능을 지정합니다. 웹 뷰를 쓰려면 `nsh-platform-linux` 의존성의 `webview` 기능을 활성화하세요. 그러면 WebKitGTK 6.0을 링크하므로 빌드 및 실행 환경에 이를 설치해야 합니다. 생성된 실행기에도 Cargo의 기능 통합(feature unification)을 통해 같은 설정이 적용됩니다.

`nsh run` 실행 중 Rust 코드를 바꾸면 실행기를 다시 빌드하고 앱을 재시작합니다. 브라우저에서는 열려 있던 페이지가 새 런타임을 자동으로 불러옵니다. `cargo run`은 데스크톱 백엔드에서 같은 루트를 직접 실행합니다(`src/main.rs`). NSH를 내장한 macOS, Windows, Linux, GPUI, iOS 앱은 `AppView`의 `AppSource::Rust`를 사용해 Rust 루트를 마운트합니다.

웹을 포함한 모든 플랫폼에서 NSH의 서비스를 사용할 수 있습니다. HTTP 요청에는 `fetch`([네트워킹](../../docs/networking.md#from-rust-components)), 대기에는 `timer`, `set_timeout`, `set_interval`, 시각 측정에는 `nsh_core::time::Instant`와 `nsh_core::time::unix_ms()`를 사용합니다. 네이티브 플랫폼에서는 `background`와 `blocking`으로 별도 스레드에서 작업을 실행합니다.

## 데스크톱 제목 표시줄

Rust 앱은 `nsh_core::element::desktop`으로 자체 제목 표시줄을 그릴 수 있습니다. `set_title_bar(TitleBarStyle::Hidden, Some(height))`를 호출하고 배경에 `.window_drag(true)`를 지정한 다음, `title_bar()`로 시스템 창 버튼의 공간을 확보합니다. `top`은 표시줄의 높이이며 `left`와 `right`는 시스템 버튼이 차지하는 너비입니다. 이 값을 읽으면 전체 화면 전환을 비롯한 변경 사항을 구독합니다. 창 드래그와 제목 표시줄 더블 클릭은 플랫폼이 사용자 설정에 맞게 처리합니다. 특정 자식을 드래그 영역에서 제외하려면 `.window_drag(false)`를 지정하세요.

```rust
use nsh_core::{
    desktop::TitleBarStyle,
    element::{desktop::{close_window, set_title_bar, title_bar}}, prelude::*,
};

fn window_bar() -> impl IntoElement {
    set_title_bar(TitleBarStyle::Hidden, Some(px(42.0)));
    view()
        .h(px(42.0))
        .flex_row()
        .items_center()
        .window_drag(true)
        .pl(move || title_bar().left + px(12.0))
        .pr(move || title_bar().right + px(12.0))
        .child(text("My app").flex_1())
        .child(text("Close").window_drag(false).on_press(close_window))
}
```

`set_title_bar(TitleBarStyle::Default, None)`으로 시스템 제목 표시줄을 복원합니다. `close_window()`는 호스트의 일반적인 창 닫기 동작을 따릅니다. 이 모듈에서는 `set_title`, `minimize`, `toggle_full_screen` 등의 창 함수와 `set_application_menu`, `popup_menu` 같은 메뉴 함수도 제공합니다. [nsh/desktop](../../modules/nsh-desktop.md)

## 파일 및 폴더

데스크톱에서 Rust 코드는 시스템 파일 대화 상자를 열거나 로컬 경로를 시스템에 전달할 수 있습니다. 각 결과는 비동기 작업에서 기다립니다. 파일이나 폴더를 선택하려면 `OpenDialog`(`OpenDialog::files()`, `::folders()`, `::files_and_folders()`)와 함께 `pick_paths`를 호출합니다. 저장 경로는 `SaveDialog`와 `pick_save_path`로 선택합니다. `open_path`는 기본 앱으로 파일을 열고, `reveal_path`는 Finder, 파일 탐색기 또는 파일 관리자에서 항목을 선택합니다. `move_to_trash`는 항목을 휴지통으로 보내며 macOS가 알려 주는 경우 이동한 경로도 반환합니다. 사용자가 취소하면 대화 상자는 `Ok(None)`을 반환합니다. 파일 선택 포털이 없는 Linux나 iOS, Android, 웹처럼 대화 상자를 열 수 없는 환경에서는 오류를 반환합니다. iOS, Android, 웹에서는 `open_path`, `reveal_path`, `move_to_trash`도 오류를 반환합니다. 실패한 요청은 모두 구체적인 오류를 반환합니다.

```rust
use nsh_core::{
    file_dialog::{FileFilter, OpenDialog},
    prelude::*,
};

fn open_notes() {
    let picked = pick_paths(
        OpenDialog::files()
            .multiple(true)
            .filter(FileFilter::new("Markdown", ["md", "markdown"]))
            .button_label("Open"),
    );
    spawn(async move {
        match picked.await {
            Ok(Some(paths)) => {
                for path in paths {
                    let _ = reveal_path(path).await;
                }
            }
            Ok(None) => {} // Cancelled.
            Err(error) => log::warn!("no file dialog: {error}"),
        }
    });
}
```

## TSX로 Rust 앱 확장하기

크레이트의 `#[native_module("app/native")]` 함수에서 TSX가 가져올 항목을 지정합니다. 컴포넌트는 `registry.register::<Chart>()`, 함수 모듈은 `registry.module::<Portfolio>()`(유닛 구조체와 `impl`에 `#[module]` 지정), 클래스는 `registry.class::<Ledger>()`(구조체와 `impl`에 `#[class]` 지정)로 등록하며 에셋도 등록할 수 있습니다. 이 어트리뷰트는 크레이트 루트에 `pub fn registry()`도 생성합니다. 테스트와 다른 Rust 코드에서는 `nsh_base::registry()`와 함께 `my_app_native::registry()`로 같은 항목들을 사용할 수 있습니다. `#[module]` 또는 `#[class]`의 `impl`에서는 `pub fn`과 `pub(crate)` 함수가 외부로 공개되며, 가시성을 지정하지 않은 함수는 Rust 내부에서만 사용합니다. 문서 주석을 포함한 TypeScript 선언은 Rust 타입으로부터 `.nsh/types`에 생성되므로 `nsh check`와 편집기에서 TSX 코드를 검사할 수 있습니다.

```rust
/// A price chart.
#[derive(Clone, PartialEq, Props)]
pub struct ChartProps {
    /// The symbol to chart.
    pub symbol: Prop<SharedString>,
    /// The range shown.
    #[prop(default = SharedString::from("1D"))]
    pub range: Prop<SharedString>,
}

#[derive(Event)]
pub enum ChartEvent {
    /// A price was selected.
    #[args(price)]
    Select(f64),
}

/// A price chart; a press selects the last price.
#[component]
pub fn Chart(props: ChartProps, cx: Scope<ChartEvent>) -> impl IntoElement {
    view()
        .h(px(160.0))
        .bg(move || theme().muted)
        .on_press(move || cx.emit(ChartEvent::Select(101.5)))
        .child(text(move || format!("{} · {}", props.symbol.get(), props.range.get())))
}

/// A ledger kept in Rust.
#[class]
pub struct Ledger {
    entries: Vec<f64>,
}

#[class]
impl Ledger {
    /// An empty ledger.
    pub fn new() -> Self {
        Ledger { entries: Vec::new() }
    }

    /// Adds `amount`; the new balance.
    pub fn add(&mut self, amount: f64) -> f64 {
        self.entries.push(amount);
        self.balance()
    }

    /// The balance.
    #[getter]
    pub fn balance(&self) -> f64 {
        self.entries.iter().sum()
    }

    /// Uploads the entries; resolves when the server has them.
    pub async fn upload(this: Instance<Ledger>, url: String) -> Result<(), String> {
        let body = this.with(|l| format!("{:?}", l.entries));
        fetch(HttpRequest::post(url, body)).await.map(|_| ())
    }
}

/// The app's holdings.
#[module]
pub struct Portfolio;

#[module]
impl Portfolio {
    /// The value of the holdings.
    pub fn total() -> f64 {
        1250.0
    }
}
```

```tsx
import { mount, signal, View, Text } from "nsh";
import { Chart, Ledger, Portfolio } from "app/native";   // Rust

const ledger = new Ledger();   // its state lives in Rust

function Overview() {
  const balance = signal(ledger.add(Portfolio.total()));   // Rust, called at once
  return (
    <View class="gap-2 p-4">
      <Text>Balance: {balance()}</Text>
      <Chart symbol="NSH" range="1M" onSelect={(e) => balance.set(ledger.add(e.price))} />
      <View onPress={() => ledger.upload("https://example.com/ledger")} role="button" class="rounded-lg bg-blue-600 px-4 py-2 active:bg-blue-700">
        <Text class="text-white">Upload</Text>
      </View>
    </View>
  );
}

mount(() => <Overview />);
```

TypeScript에 표시되는 문서는 해당 항목을 선언한 Rust 코드의 문서 주석에서 가져옵니다. 컴포넌트는 `#[component] fn`(`Chart`), 모듈과 클래스는 구조체(`Portfolio`, `Ledger`)에 작성합니다. 속성, 이벤트, 함수, 멤버에는 각각 주석을 작성합니다. 세터에는 게터와 별도의 주석이 필요하지 않습니다. `Props` 구조체와 이벤트 열거형 자체, 일반 `impl` 블록의 문서 주석은 Rust에만 남습니다. `#[module]`이나 `#[class]`가 지정된 `impl` 블록 자체에 문서 주석을 달면 TypeScript에 표시할 위치가 없어 컴파일 오류가 발생합니다. `.nsh/types`를 작성하는 명령은 TSX에 공개되는 항목에 문서 주석이 없으면 `Ledger.add`, `Chart.range`처럼 항목 이름을 명시해 경고합니다.

TypeScript에는 문서 주석의 첫 번째 제목 앞까지만 표시됩니다. `&self`가 무엇을 빌리는지, 함수가 `cx`를 받는 이유처럼 Rust 호출자에게 필요한 내용은 `# Rust` 또는 rustdoc의 `# Panics`, `# Examples` 아래에 적으면 Rust 문서에만 표시됩니다.

```rust
/// Puts `text` on the clipboard; resolves with the number of characters copied.
///
/// # Rust
///
/// Taking `cx` makes it an op, run in order with the UI changes TS made before it.
pub fn copy(text: String, cx: &mut ModuleCx) -> u32 {
    let count = text.chars().count() as u32;
    cx.set_clipboard(text);
    count
}
```

컴포넌트의 속성은 `Props` 구조체의 필드, 이벤트는 `Event` 열거형의 변형이며 TSX에서는 모두 카멜 표기법을 사용합니다(예: `onSelect`). Rust에서는 생성자, 속성별 세터, 이벤트별 리스너를 사용해 같은 컴포넌트를 배치합니다. 예를 들어 `chart().symbol("NSH").range("1M").on_select(move |price: &f64| last.set(*price))`로 작성합니다. 콘텐츠, 이벤트, 상태는 Rust가 관리하고 TSX는 속성을 전달하고 이벤트를 수신합니다. 속성 타입이 잘못되면(예: `u32`에 `level={1.5}` 전달) 기본 요소에서처럼 경고가 발생합니다. 해당 속성은 기존 값을 유지하고 컴포넌트의 나머지 부분은 렌더링됩니다.

함수와 멤버의 실행 방식은 Rust 시그니처에 따라 결정됩니다. 동기 함수는 TSX의 호출 중에 실행되어 즉시 값을 반환하며 계산하거나 상태를 보관할 수 있습니다. 예를 들어 `Portfolio.total()`은 숫자를 반환하고 `ledger.balance`는 속성으로 읽습니다. `async fn`은 Promise를 반환하고 NSH의 서비스(`fetch`, `timer`, 대화 상자)를 사용하는 비동기 작업으로 실행됩니다.

`cx: &mut ModuleCx`를 받는 모듈 함수는 UI 변경과 순서를 맞춰 토스트나 상태 표시줄 같은 플랫폼 기능을 호출합니다. 컴포넌트의 ref 메서드도 마찬가지입니다. 반환 타입이 `()`이면 TSX에서는 `void`이므로 응답을 기다리지 않습니다. `Result<(), E>`는 `Promise<void>`가 되며 `Err`의 메시지와 함께 거부될 수 있습니다. 작업은 TSX 호출이 끝난 뒤 UI 변경과 함께 실행되므로 오류가 Promise를 통해 전달됩니다. 아무도 기다리거나 처리하지 않은 오류는 [처리되지 않은 Promise 거부](../../docs/dev-server-and-debugging.md#logs)로 기록됩니다. 값을 반환하는 메서드는 `Promise<T>`가 됩니다.

인수와 결과 타입은 시그니처에서 결정됩니다. 숫자, 문자열, `Option`, `Vec`, `#[derive(JsObject)]` 구조체, `#[derive(PropEnum)]` 열거형, 클래스 인스턴스, TSX 함수를 지원합니다. Rust는 `Callback<A, R>`를 호출 중에 바로 실행하거나 `post`와 `call_async`로 나중에 실행합니다. `Callback<f64>`는 TS에서 `(value: number) => void`이며, `#[args(balance)] listener: Callback<f64>`로 인수 이름을 지정하면 `(balance: number) => void`가 됩니다. 구독 함수는 `on(event, listener)`라는 이름으로 `Stop`(`Stop::new(|| …)`)을 반환합니다. TSX는 이를 구독 해제 함수(`() => void`)로 받습니다. 직접 호출하거나 `on`의 호출 주체(컴포넌트 또는 핫 리로드 시 모듈 실행)가 종료될 때 한 번 실행됩니다.

이벤트 핸들러는 인수로 객체 하나를 받습니다. 열거형 변형의 값은 기본적으로 `value` 필드에 들어가며, `#[args(price)]`를 지정하면 `onSelect({ price })`처럼 받습니다. 값이 여러 개면 `#[args(open, reason)]`으로 각각 이름을 지정합니다. 인수 타입이 틀리면 `TypeError`가 발생하거나, Promise를 반환하는 호출이라면 Promise가 거부됩니다. `Err`도 해당 메시지와 함께 예외를 발생시키거나 Promise를 거부합니다. 클래스 객체는 Rust 값을 소유합니다. 객체를 가비지 컬렉터가 회수하면 값도 해제되며, `ledger.dispose()` 또는 `using ledger = new Ledger()`로 해제 시점을 정할 수도 있습니다.

TSX 진입점을 지정해 `nsh run App.tsx` 또는 `nsh package App.tsx --target …`로 실행하거나 패키징합니다. TSX를 수정하면 핫 업데이트되고 Rust를 수정하면 앱을 다시 빌드하고 재시작합니다. CLI는 크레이트가 제공하는 설명에서 공개 항목을 파악하고 직접 TSX를 번들링·컴파일합니다. 개발용 호스트와 `nsh test`의 헤드리스 실행기에는 앱 크레이트와 QuickJS가 포함됩니다.

저장소의 `examples/rust-exports`는 Rust 코드를 공개하는 방법을 보여 주는 작은 TSX 앱입니다. 크레이트는 컴포넌트 하나(필수 속성, 기본값이 있는 속성, 선택적 속성, 이벤트, 자식), 모듈 하나(종류별 함수), 클래스 하나(생성자, 메서드, 게터·세터, 정적 멤버, 비동기 멤버, 다른 인스턴스를 인수로 전달하는 예, Rust가 나중에 실행하는 콜백, `dispose()`, `using`)를 공개합니다. 각 항목에는 짧은 문서 주석이 있으며, Rust 및 TypeScript(`App.test.tsx`) 테스트에서 앱을 조작하며 표시 결과를 확인합니다.

저장소의 `examples/gallery-rs`는 모든 예제를 Rust로 작성한 완전한 앱입니다. `nsh run examples/gallery-rs --target web`(또는 `ios`, `android` 등)으로 루트를 실행하고 `nsh test examples/gallery-rs`로 헤드리스 테스트를 수행합니다. 데스크톱 백엔드에서는 `cargo run -p gallery-native`로 시작할 수 있습니다.

### 렌더 함수와 앱의 클래스

컴포넌트가 가진 값(데이터 행이나 문서의 코드 블록 등)에 앱 고유의 콘텐츠를 그리려면 렌더 함수를 사용합니다. 속성 타입은 Rust에서 `Option<Renderer<T>>`, TypeScript에서 `(value: T) => JSX.Element`입니다. 컴포넌트가 콘텐츠를 배치할 곳에서 `render(key, value)`를 호출하면 다른 뷰처럼 스타일을 적용하고 배치할 수 있는 뷰가 반환됩니다. `text(..)` 안에서는 인라인 뷰가 됩니다. 콘텐츠는 키별로 유지되며 값이 달라질 때 다시 렌더링됩니다. Rust의 부모 컴포넌트가 속성 세터에 클로저를 전달하면(`quote().render_badge(|symbol| ..)`) 세터가 `Renderer`를 만듭니다.

컴포넌트 내부 스타일에는 타입을 사용합니다. 앱이 내부 요소에 Tailwind 클래스를 적용하려면 클래스 속성을 전달하고 해당 요소에서 `.app_class(..)`를 호출합니다. 클래스는 요소 자체 스타일 뒤에 적용되고 일반 요소의 `class`처럼 앱 테마, `dark:` 및 상태별 변형을 지원합니다.

```rust
/// A quote: its symbol, then content of the app's.
#[derive(Clone, Copy, PartialEq, Props)]
pub struct QuoteProps {
    /// The symbol.
    pub symbol: Prop<SharedString>,
    /// Tailwind classes of the symbol's text.
    pub symbol_class: Prop<Option<SharedString>>,
    /// Content of the app's after the symbol (a change, a badge).
    pub render_badge: Prop<Option<Renderer<SharedString>>>,
}

/// A quote of a symbol.
#[component]
pub fn Quote(props: QuoteProps) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap_2()
        .child(text(props.symbol).app_class(props.symbol_class))
        .child(dynamic(move || {
            props.render_badge.get().map(|r| r.render("badge", props.symbol.get()).flex_1())
        }))
}
```

```tsx
<Quote
  symbol="NSH"
  symbolClass="text-lg font-semibold"
  renderBadge={(symbol) => <Text class="text-green-600">{`${symbol} ▲ 2.4%`}</Text>}
/>
```

`nsh/base`의 `TextView`가 이 방식으로 만들어졌습니다. `renderCodeBlock`, `renderMath` 등은 렌더 함수이며 `classes`는 앱에서 부분별로 지정하는 클래스입니다. [TextView](../../elements/text-view.md)

## 테스트

Rust 앱의 테스트는 `cargo test`와 `nsh test`에서 같은 드라이버를 사용합니다. 실제 엔진을 창 없이 60 Hz의 화면 프레임으로 구동하므로 타이머, 비동기 작업, 애니메이션은 화면에 표시될 때처럼 동작합니다. 드라이버는 앱을 시작하고 표시 중인 뷰의 모델을 유지합니다. 텍스트, 접근성 레이블, 테스트 ID 선택자(`text:`, `label:`, `id:`)로 뷰를 찾은 뒤 `press`, `type_text`, `submit`, `scroll`, `wait_for` 같은 동작을 수행합니다. 동작에 실패하면 당시 화면의 텍스트도 함께 표시합니다.

`cargo test`에서는 `nsh_testing::Headless`가 루트를 실행합니다. 기본 기능으로 QuickJS 없이 동작하며 `app.screen()`으로 화면 텍스트와 앱이 보낸 명령을 확인합니다. `EngineConfig::new`는 별도 설정 없이 휴대폰 크기의 엔진을 생성합니다. `nsh test`는 앱 크레이트를 포함해 빌드한 헤드리스 실행기에서 [TypeScript 테스트](../../docs/testing-and-trace.md)를 실행하며 `open(".")`으로 Rust 앱의 루트를 시작합니다.

```toml
[dev-dependencies]
nsh-engine = "0.1"
nsh-testing = "0.1"
```

```rust tabs
// tests/app.rs: cargo test
use nsh_engine::{EngineConfig, Root};
use nsh_testing::Headless;

#[test]
fn a_tap_counts() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<my_app::App>())).unwrap();
    app.wait_for("Count: 0").unwrap();
    app.press("Tap").unwrap();
    app.wait_for("Count: 1").unwrap();
}
```

```tsx tabs
// App.test.tsx, next to nsh.json: nsh test
import { expect, open, test } from "nsh/test";

test("a tap counts", async () => {
  const app = await open(".");
  await app.press("Tap");
  expect(app.texts()).toContain("Count: 1");
});
```

TSX 진입점은 `nsh run`과 마찬가지로 호스트가 제공하는 UI 세트에 맞춰 빌드합니다. 테스트에서는 `nsh-testing`의 `bundle` 기능을 켜고 NSH의 레지스트리와 `#[native_module]`이 크레이트별로 생성한 `registry()`를 전달합니다.

```toml
[dev-dependencies]
nsh-testing = { version = "0.1", features = ["bundle"] }
nsh-base = "0.1"
```

```rust
#[test]
fn the_module_formats_money() {
    let sets = vec![nsh_base::registry(), my_app_native::registry()];
    let mut app = nsh_testing::tsx("../App.tsx", sets).unwrap();
    app.wait_for("Formatted at once: $1,234.50").unwrap();
}
```

`nsh_testing::compile_tsx`는 여러 테스트가 사용할 진입점을 한 번만 빌드합니다. `nsh_testing::start`는 화면 모델을 직접 관리하는 테스트에 배치를 전달합니다. `examples/rust-exports`는 이 방식으로 TSX 진입점을 테스트합니다. `examples/gallery-rs`는 대부분의 예제마다 TypeScript 테스트 파일을 두고 Rust 루트를 검증합니다.

## 테마

NSH는 기본 요소와 스타일이 지정되지 않은 `nsh_base` 컴포넌트를 제공합니다. 그 위에 앱의 크레이트가 스타일 계층, 즉 앱 고유의 컴포넌트 라이브러리를 구성합니다. Rust 컴포넌트는 `nsh_base`의 `theme()`에서 색상, 모서리 반지름, 글꼴을 가져오므로 파일 하나로 앱 전체의 컴포넌트 스타일을 바꿀 수 있습니다. 각 색상에는 라이트 및 다크 값이 있습니다. 시스템 모드가 바뀌면 `.bg(move || theme().primary)` 등의 바인딩이 다시 렌더링하지 않고 네이티브에서 색상을 바꿉니다. [세 계층](../../docs/architecture.md#three-layers)

테마는 [shadcn/ui](https://ui.shadcn.com/docs/theming)의 디자인 토큰을 사용합니다. 기본값은 흑백 팔레트와 10 pt 모서리 반지름을 사용하는 shadcn의 `neutral` 테마입니다. 직접 만든 테마를 사용하려면 `nsh.json`의 `"theme"`에 파일 경로(`nsh.json` 기준) 또는 테마 내용을 바로 지정합니다.

```json
{
  "name": "My App",
  "rust": ".",
  "theme": "brand-theme.json"
}
```

테마 파일은 shadcn 테마와 레지스트리 항목에 쓰는 `cssVars` 형식입니다. [shadcn 테마](https://ui.shadcn.com/themes)나 [tweakcn](https://tweakcn.com)에서 내보낸 테마를 그대로 사용할 수 있습니다. `light`와 `dark`는 각 모드의 값을 지정하고 `theme`는 두 모드에 공통으로 적용됩니다.

```json
{
  "cssVars": {
    "theme": {
      "radius": "0.5rem",
      "font-sans": "Inter, sans-serif"
    },
    "light": {
      "primary": "oklch(0.541 0.281 293.009)",
      "primary-foreground": "oklch(0.969 0.016 293.756)",
      "ring": "var(--primary)",
      "up": "#16a34a"
    },
    "dark": {
      "primary": "oklch(0.702 0.183 293.541)",
      "primary-foreground": "oklch(0.283 0.141 291.089)",
      "up": "#4ade80"
    }
  }
}
```

- 기본 테마 위에 사용자 테마를 적용하므로 바꿀 값만 지정하면 됩니다. `light`에 지정하고 `dark`에서 생략한 값은 두 모드 모두에 쓰입니다.
- 값은 CSS처럼 작성합니다. 색상은 CSS 색상(`#7c3aed`, `rgb(…)`, `hsl(…)`, `oklch(…)`, `white`), `color-mix(in oklab, var(--primary) 80%, white)`, 또는 같은 모드의 다른 토큰(`var(--primary)`)을 사용할 수 있습니다. `var(--brand, #7c3aed)`처럼 대체값도 지정할 수 있습니다. 길이 단위는 `rem`(16포인트) 또는 `px`(포인트)입니다. 글꼴은 패밀리 목록으로 지정합니다. 일반 패밀리가 아닌 첫 번째 패밀리를 사용하며 목록 첫 값이 `system-ui`나 `sans-serif` 같은 일반 패밀리면 시스템 글꼴을 사용합니다.
- **앱 전용 토큰.** Tailwind의 `@theme` 변수처럼 별도의 이름을 앱 토큰으로 추가할 수 있습니다. 위의 `"up"`은 Rust에서 `theme().color("up")`, 클래스에서 `bg-up` 또는 `text-up/70`, TSX에서 `token("up")`으로 사용합니다. `"radius-card": "14px"`와 같은 숫자 토큰은 `theme().number("radius-card")`와 `rounded-(--radius-card)`로 사용합니다. [테마 토큰](../../docs/tailwind.md#theme-tokens)
- 번들러는 빌드 시 테마를 검사합니다. 파일 누락, 잘못된 JSON 또는 `cssVars` 형식 오류가 있으면 `nsh.json: theme: …` 오류를 표시하고 빌드에 실패합니다.

다음은 shadcn의 `neutral` 테마에 정의된 기본 라이트·다크 토큰 값입니다.

| 토큰 | `theme()` | 라이트 | 다크 | 역할 |
|---|---|---|---|---|
| `background` / `foreground` | `background` / `foreground` | `oklch(1 0 0)` / `oklch(0.145 0 0)` | `oklch(0.145 0 0)` / `oklch(0.985 0 0)` | 앱의 배경과 본문 텍스트 |
| `card` / `card-foreground` | `card` / `card_foreground` | `oklch(1 0 0)` / `oklch(0.145 0 0)` | `oklch(0.205 0 0)` / `oklch(0.985 0 0)` | 카드·그룹 배경과 텍스트 |
| `popover` / `popover-foreground` | `popover` / `popover_foreground` | `oklch(1 0 0)` / `oklch(0.145 0 0)` | `oklch(0.205 0 0)` / `oklch(0.985 0 0)` | 팝오버·메뉴·툴팁의 배경과 텍스트 |
| `primary` / `primary-foreground` | `primary` / `primary_foreground` | `oklch(0.205 0 0)` / `oklch(0.985 0 0)` | `oklch(0.922 0 0)` / `oklch(0.205 0 0)` | 기본 버튼과 같은 주요 동작의 배경과 텍스트 |
| `secondary` / `secondary-foreground` | `secondary` / `secondary_foreground` | `oklch(0.97 0 0)` / `oklch(0.205 0 0)` | `oklch(0.269 0 0)` / `oklch(0.985 0 0)` | 보조 동작의 배경과 텍스트 |
| `muted` / `muted-foreground` | `muted` / `muted_foreground` | `oklch(0.97 0 0)` / `oklch(0.556 0 0)` | `oklch(0.269 0 0)` / `oklch(0.708 0 0)` | 은은한 배경과 보조 텍스트(캡션, 레이블, 자리표시자) |
| `accent` / `accent-foreground` | `accent` / `accent_foreground` | `oklch(0.97 0 0)` / `oklch(0.205 0 0)` | `oklch(0.269 0 0)` / `oklch(0.985 0 0)` | 마우스를 올리거나 강조한 항목(메뉴 항목, 목록 행 등)의 배경과 텍스트 |
| `destructive` | `destructive` | `oklch(0.577 0.245 27.325)` | `oklch(0.704 0.191 22.216)` | 삭제 등 파괴적 동작과 오류 |
| `border` | `border` | `oklch(0.922 0 0)` | `oklch(1 0 0 / 10%)` | 구분선과 테두리 |
| `input` | `input` | `oklch(0.922 0 0)` | `oklch(1 0 0 / 15%)` | 입력란 테두리 |
| `ring` | `ring` | `oklch(0.708 0 0)` | `oklch(0.556 0 0)` | 포커스 링 |
| `chart-1` … `chart-5` | `chart_1` … `chart_5` | `oklch(0.87 0 0)` … `oklch(0.269 0 0)` | 라이트와 동일 | 차트의 데이터 계열 |
| `sidebar`, `sidebar-foreground`, `sidebar-primary`, `sidebar-primary-foreground`, `sidebar-accent`, `sidebar-accent-foreground`, `sidebar-border`, `sidebar-ring` | `sidebar`, `sidebar_foreground`, ... | `sidebar` `oklch(0.985 0 0)`, `sidebar-primary` `oklch(0.205 0 0)` | `sidebar` `oklch(0.205 0 0)`, `sidebar-primary` `oklch(0.488 0.243 264.376)` | 사이드바의 배경, 주요 동작, 강조 항목, 테두리, 포커스 링 |
| `radius` | `radius` | `0.625rem` (10 pt) | | 모서리 반지름 |
| `font-sans` | `font_sans` | 시스템 글꼴 | | 별도 글꼴을 지정하지 않은 앱 텍스트 |
| `font-mono` | `font_mono` | 시스템 고정폭 글꼴 | | 고정폭 글꼴(`font-mono`, `TextView`의 코드) |

## 여러 크레이트 사용하기

앱의 Rust 코드를 여러 크레이트로 나누고 각각 다른 모듈 지정자로 가져올 수 있습니다. `"rust"`에는 크레이트 경로의 목록을 지정하고 각 크레이트에 개별 `#[native_module]` 함수를 둡니다.

```json
{
  "rust": ["native/charts", "native/storage"],
  "root": "App"
}
```

```tsx
import { Chart } from "app/charts";        // native/charts: #[native_module("app/charts")]
import { Store } from "app/storage";       // native/storage: #[native_module("app/storage")]
```

모든 실행기와 호스트는 목록의 크레이트를 전부 링크하고 `.nsh/types`에는 각 모듈 지정자가 선언됩니다. 크레이트끼리는 Cargo 경로 의존성(`shared = { path = "../shared" }`)으로 코드를 공유합니다. 예를 들어 회사 공통 컴포넌트 크레이트를 여러 크레이트에서 사용하고 여러 앱의 목록에 등록할 수 있습니다. TSX에 항목을 공개하는 것은 `"rust"`에 등록한 크레이트뿐입니다. 각 크레이트의 패키지 이름과 모듈 지정자는 고유해야 하며 중복되면 `nsh`가 충돌한 두 경로를 표시합니다. Rust 앱의 `"root"`는 첫 번째로 등록한 크레이트의 컴포넌트입니다. `nsh run`과 `nsh dev` 실행 중 목록의 크레이트나 그 경로 의존성이 변경되면 앱을 다시 빌드하고 재시작합니다.

## 웹에서 브라우저 API 사용하기

웹에서 크레이트는 wasm-bindgen과 `web-sys`를 통해 WebSocket, `IndexedDB` 등의 브라우저 API를 호출할 수 있습니다. `nsh`는 필요한 연결 코드를 런타임과 함께 빌드합니다. [웹](../../docs/web.md#browser-apis-from-rust)

## 참고 문서

- [프로젝트 구조](./project-structure.md)에서 `nsh.json`의 모든 키를 확인할 수 있습니다.
- [웹](../../docs/web.md#apps-with-rust-components)은 브라우저에서 Rust 앱을 실행하는 방법을 설명합니다.
- [네트워킹](../../docs/networking.md#from-rust-components)은 Rust에서 `fetch`를 사용하는 방법을 다룹니다.
- [CLI](./cli.md#nsh-init)에서 `nsh init --template rust` 및 `--template mixed`를 확인할 수 있습니다.
