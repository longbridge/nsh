---
title: 소개
description: Native Shell은 Rust를 중심으로 iOS, Android, macOS, Windows, Linux, 웹에서 각 플랫폼의 UI와 상호작용 방식을 활용하는 네이티브 UI 프레임워크입니다.
order: 1
group: start
---

# 소개

Native Shell(줄여서 NSH)은 여러 플랫폼을 위한 네이티브 UI 프레임워크입니다. 컴포넌트, 시그널, 스타일, API를 공유하는 하나의 앱을 작성하면 **iOS, Android, macOS, Windows, Linux, 웹**에서 각 플랫폼의 뷰와 텍스트 편집 기능, 사용자에게 익숙한 조작 방식을 그대로 활용할 수 있습니다. 앱 전체를 **Rust**로 작성하거나 화면을 **TSX**로 구성할 수 있고, 하나의 엔진에서 둘을 함께 쓸 수도 있습니다. 이미 배포한 앱은 같은 엔진에서 팀이 작성하거나 모델이 생성한 새 UI를 실행 중에 불러올 수 있습니다. 이때 UI는 서명된 모듈로 전달되며 호스트가 설정한 샌드박스 안에서 실행됩니다.

> [!IMPORTANT]
> NSH는 네이티브 뷰와 상호작용을 제공하고 앱은 시각 디자인을 결정합니다. 컴포넌트, 상태, API는 모든 플랫폼에서 공유하고 색상, 서체, 간격, 모양은 앱에서 정합니다. 텍스트 입력, 포커스, 메뉴 조작은 각 기기의 관례를 따릅니다. [목표와 설계 원칙](./goals.md)을 참고하세요.

## 네이티브의 의미

NSH에서 네이티브란 UI를 구현하는 기반과 사용자가 UI를 조작하는 방식 두 가지를 가리킵니다.

**플랫폼의 UI 기반.** 각 백엔드는 해당 플랫폼이 제공하는 UI 기술을 사용합니다.

| 플랫폼 | 뷰 | 텍스트 | 텍스트 입력 |
|---|---|---|---|
| iOS | UIKit | CoreText | 시스템 텍스트 필드 및 텍스트 뷰 |
| Android | Android 뷰 | `StaticLayout` | `EditText` |
| macOS | AppKit, Core Animation | CoreText | `NSTextField`, `NSTextView` |
| Windows | Direct2D로 그리는 Win32 창 | DirectWrite | 시스템 입력기(IMM32)와 연동되는 NSH 편집기 |
| Linux | GTK 4 | Pango | GTK의 `Text` 및 `TextView` |
| 웹 | CSS로 스타일을 지정한 HTML 요소 | 브라우저 글꼴 | 브라우저 입력 요소 |

텍스트 선택과 선택 메뉴, 입력기, 접근성, 스크롤은 플랫폼이 제공합니다. OS 업데이트로 개선된 기능도 앱에 그대로 반영됩니다. 지도, 웹 뷰, 동영상 플레이어처럼 호스트가 등록한 네이티브 컴포넌트는 같은 계층 안의 뷰로 취급되어 다른 뷰와 함께 배치되고 잘리거나 스크롤됩니다([네이티브 컴포넌트](../../docs/native-components.md)).

**플랫폼의 조작 방식.** [`nsh/base`](../../modules/nsh-base.md)의 컴포넌트는 한 번 작성하면 기기별 관례에 맞게 동작합니다.

| 컴포넌트 | 데스크톱 및 데스크톱 브라우저 | 휴대전화 및 태블릿 |
|---|---|---|
| `Menu` | 메뉴를 연 요소 옆에 팝업이 뜨고 하위 메뉴는 해당 항목 옆에 펼쳐짐 | 아래에서 시트가 올라오고 하위 메뉴로 들어가면 기존 목록이 교체됨 |
| `ContextMenu` | 마우스 오른쪽 버튼을 누른 위치에 열림 | 요소를 길게 누르면 시트로 열림 |
| `Tooltip` | 요소 위에 포인터를 잠시 두면 표시됨 | 앱에서 열 때 표시됨 |
| `Popover` | 바깥을 누르면 닫히고 포인터 아래의 요소에도 입력이 전달됨 | 바깥을 누르면 닫힘 |

페이지와 대화상자, 키보드, 상태 표시줄에도 같은 원칙이 적용됩니다. 탐색 스택에서 화면을 열거나 닫을 때 각 플랫폼의 전환 효과를 사용하며, 뒤로 가기 버튼과 Escape 키, 뒤로 가기 스와이프도 사용자에게 익숙한 방식으로 동작합니다([탐색](../../docs/navigation.md)).

## 휴대전화에서 데스크톱과 웹까지

NSH의 크로스 플랫폼 앱은 화면 크기에 따라 배율만 조정하는 대신, 기기에 맞게 레이아웃과 조작 방식을 바꿉니다.

- **하나의 앱 모델.** 컴포넌트, 상태, 비즈니스 로직, 화면을 공유합니다. 각 백엔드는 이를 해당 플랫폼의 UI로 구현합니다.
- **화면에 맞춰 바뀌는 레이아웃.** 모든 플랫폼에서 [Taffy](https://github.com/DioxusLabs/taffy)가 flexbox와 CSS grid를 계산합니다. Tailwind 중단점(`md:`, `lg:`)과 창 크기(Rust의 `window()`)에 따라 구조를 바꿀 수 있습니다. 예를 들어 채팅 앱은 넓은 창에 사이드바, 대화 목록, 메시지를 나란히 표시하고 휴대전화에서는 한 번에 한 화면씩 표시합니다([레이아웃](../../docs/layout.md), [Tailwind](../../docs/tailwind.md)).
- **기기에 맞는 입력.** 데스크톱에서는 포인터, 호버, 오른쪽 클릭, 단축키, 포커스를 사용하고 휴대전화에서는 터치, 제스처, 소프트웨어 키보드, 페이지 탐색을 사용합니다. 직접 플랫폼별 동작을 지정하려면 `desktop={{…}}` 속성과 `platform.select`를 사용하세요([플랫폼별 코드](../../docs/platform-specific.md)).
- **모든 타깃에서 테스트.** `nsh test`는 창 없이 테스트를 실행할 수 있습니다. 같은 테스트를 macOS, Windows, Linux, GPUI 창이나 iOS 시뮬레이터, Android 기기, 브라우저에서 각 플랫폼의 입력 방식으로 실행할 수도 있습니다([테스트와 추적](../../docs/testing-and-trace.md)).

채팅 클라이언트를 예로 들어 보겠습니다. 개발자는 데스크톱에서 `nsh run`으로 앱을 만들고, 같은 프로젝트에서 `nsh run --target ios`로 시뮬레이터를 열어 휴대전화 레이아웃을 확인한 뒤, `nsh run --target web`으로 브라우저에서 확인합니다. 마지막으로 `nsh package`로 설치 가능한 앱과 웹사이트를 만듭니다. 백엔드 개발이 동시에 진행되는 동안, 플랫폼별 팀이 나누어 맡던 클라이언트 작업을 한 사람이 이어갈 수 있습니다.

## Rust 중심의 개발

NSH 자체가 Rust로 구현되었고, 앱 전체를 작성하는 언어로도 Rust를 지원합니다.

- **Rust로 앱 전체 작성.** UI, 로직, 컴포넌트를 Rust로 작성하면 각 플랫폼에서 네이티브 코드로 컴파일됩니다. Rust의 소유권, 타입, 스레드와 crates.io 생태계를 활용하면서 대량 데이터 처리, 파싱, 시스템 작업과 연산을 네이티브 속도로 실행할 수 있습니다([Rust로 개발하기](./rust.md)).
- **TSX로 화면 작성.** TSX는 빠르게 작성하는 화면과 플러그인, 작은 앱, 실행 중에 전달할 UI에 적합합니다. Rust와 같은 엔진에서 기본 요소, 기본 컴포넌트, 시그널을 공유합니다.
- **Rust와 TSX 결합.** Rust 앱에서 TSX 화면을 불러올 수 있고, TSX 화면에서 앱의 Rust 컴포넌트, 모듈, 클래스를 가져올 수 있습니다.
- **하나의 툴체인.** 모든 타깃에서 `nsh` 실행 파일 하나로 앱을 생성하고 빌드하며 타입 검사, 핫 리로드, 테스트, 패키징, 서명을 수행합니다. TSX 번들러와 TypeScript 검사기를 내장하고 있어 Rust, Swift, Kotlin, C++에 익숙한 개발자도 Rust와 `nsh`를 설치하면 시작할 수 있습니다([설치](./installation.md), [CLI](./cli.md)).

## 앱 예제

같은 카운터를 Rust와 TSX로 작성한 예제입니다.

```rust tabs
use nsh_core::prelude::*;

#[component]
pub fn Counter() -> impl IntoElement {
    let count = signal(0);
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_3()
        .child(text(move || count.get().to_string()).text_5xl().font_weight(FontWeight::BOLD))
        .child(
            view()
                .rounded_lg()
                .bg(rgb(0x2563eb))
                .px_5()
                .py_3()
                .active(|s| s.opacity(0.6))
                .on_press(move || count.update(|n| *n += 1))
                .child(text("Tap").text_lg().text_color(white())),
        )
}
```

```tsx tabs
import { mount, signal, Text, View } from "nsh";

function Counter() {
  const count = signal(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 12 }}>
      <Text style={{ fontSize: 48, fontWeight: "700" }}>{count()}</Text>
      <View
        onPress={() => count.update((n) => n + 1)}
        activeStyle={{ opacity: 0.6 }}
        class="rounded-lg bg-blue-600 px-5 py-3"
      >
        <Text class="text-white text-lg">Tap</Text>
      </View>
    </View>
  );
}

mount(Counter);
```

뷰(`view()`, `<View>`)는 iOS에서 `UIView`, Android에서 `ViewGroup`, macOS에서 `NSView`, Linux에서 GTK 위젯, Windows에서 Direct2D로 그리는 지속형 뷰, 웹에서 HTML 요소가 됩니다. 텍스트는 네이티브 텍스트 뷰가 되며, 버튼을 누를 때의 피드백(`.active(..)`, `activeStyle`)은 메인 스레드에서 처리합니다. 버튼을 누르면 `count.update`가 시그널 하나를 바꾸고 이를 읽는 텍스트만 갱신합니다. 컴포넌트 함수는 다시 실행되지 않습니다. Rust 앱은 전체가 네이티브 코드이고, TSX 앱은 모듈 자체의 코드만 JavaScript입니다.

## 뷰를 유지하는 렌더링 방식

NSH는 각 요소를 플랫폼이 계속 보유하는 뷰로 표현합니다. 화면을 갱신할 때는 바뀐 부분만 수정합니다.

- **세밀한 갱신.** Solid와 유사한 [시그널](../../docs/reactivity.md)을 네이티브로 구현했습니다. 컴포넌트는 한 번 실행되며, 시그널이 바뀌면 그 값을 읽은 표현식만 다시 실행됩니다.
- **변경 사항만 스레드 간에 전달.** 한 차례 실행에서 발생한 변경을 모아 레이아웃을 계산하고, 프레임마다 뷰 작업을 한 묶음으로 메인 스레드에 넘깁니다. 바뀌지 않은 스타일과 프레임은 전송하지 않으며 수정할 필요가 없는 뷰는 그대로 둡니다.
- **네이티브 애니메이션.** 시간 기반 애니메이션, 스프링 및 감쇠 애니메이션, 제스처, 스크롤 연동 값은 플랫폼의 프레임 주기에 맞춰 계산됩니다. 매 프레임 JS를 호출할 필요가 없습니다([애니메이션](../../docs/animation.md)).

채팅, 메일, 양식, 목록, 문서, 설정 화면이나 AI 답변 스트리밍처럼 한 번에 몇 개의 뷰만 바뀌는 화면에 알맞은 방식입니다. 화면이 바뀌지 않는 동안 CPU 사용량은 유휴 상태에 가깝고, ProMotion 디스플레이에서는 스크롤과 애니메이션이 120 FPS를 유지합니다. [NSH 성능 벤치마크](https://github.com/longbridge/nsh-perf-bench)에서 시세표 500행을 초당 1,000회 갱신할 때 Linux용 NSH Rust는 CPU 코어 하나의 18%를, [GPUI Fast](https://github.com/longbridge/gpui-fast)로 직접 작성한 같은 화면은 20%를 사용했습니다. 연산량이 많을 때는 Rust가 적합합니다. TSX는 인터프리터인 QuickJS에서 실행되지만, TSX에서 호출하더라도 Rust 모듈은 네이티브 속도로 실행됩니다. 이렇게 100,000행을 정렬하면 순수 Rust에서와 마찬가지로 약 3ms가 걸립니다([성능](../../docs/performance.md)).

## 실행 원리

툴체인은 `nsh` 바이너리 하나로 구성됩니다. Rust와 TSX를 단독으로 쓰거나 함께 쓸 수 있으며 두 언어는 서로 다른 과정을 거쳐 같은 Rust 엔진에서 실행됩니다.

### Rust 앱

Rust 앱은 Rust와 네이티브 도구로 빌드하는 크레이트로, JavaScript 런타임이 필요하지 않습니다.

1. **빌드.** `nsh`는 앱의 크레이트를 링크해 iOS, Android, macOS, Windows, Linux 또는 웹 타깃용 실행기를 빌드합니다. 컴포넌트, 상태, 핸들러, 엔진이 함께 네이티브 코드로 컴파일됩니다.
2. **실행.** 앱의 루트 컴포넌트는 엔진 스레드에서 Rust 코드로 실행됩니다. Rust 전용 앱은 어느 플랫폼에서도 실행기에 QuickJS를 포함하지 않습니다.

### TSX 앱

TSX 앱(모듈)은 보통 `App.tsx`를 진입점으로 하는 하나 이상의 `.tsx` 및 `.ts` 파일로 구성됩니다.

1. **빌드.** `nsh`는 rolldown과 Oxc로 모듈의 파일을 번들링하고 자체 컴파일러로 JSX를 컴파일합니다. `nsh` 및 `nsh/*`의 import는 런타임이 네이티브로 제공하므로 그대로 두고, 번들에는 앱 코드만 담습니다. 완성된 번들은 QuickJS 바이트코드로 컴파일됩니다.
2. **실행.** 엔진 스레드는 뷰 하나에 VM 하나를 할당해 [QuickJS](https://github.com/quickjs-ng/quickjs)에서 바이트코드를 실행합니다. 컴파일된 JSX는 네이티브 렌더러를 직접 호출합니다. JavaScript는 UI 기술과 이벤트 처리를 맡고 스타일 파싱, Tailwind 클래스 해석, 변경 감지, 레이아웃, 텍스트 처리는 Rust가 맡습니다.

### 실행 방식 비교

| 항목 | Rust 앱 | TSX 앱 |
|---|---|---|
| 작성 언어 | Rust | TSX(TypeScript와 JSX) |
| 빌드 결과 | 타깃별 실행기에 링크되는 네이티브 코드 | 런타임이 불러오는 QuickJS 바이트코드 |
| JavaScript 엔진 | 없음 | 뷰마다 QuickJS VM 하나 |
| 개발 중(`nsh run`) | 저장할 때 다시 빌드하고 재시작 | 저장할 때 핫 업데이트 |
| 배포 후 | 앱과 함께 제공 | 서명된 패키지로 실행 중 전달해 샌드박스에서 실행 가능 |
| 적합한 용도 | 앱 전체, 연산 집약적 작업, 시스템 작업 | 빠른 화면 개발, 플러그인, AI 생성 UI |

두 방식을 결합할 수도 있습니다. Rust 앱이 TSX 화면을 불러오고 TSX에서 앱의 Rust 컴포넌트, 모듈, 클래스를 가져올 수 있습니다. 이 경우 TSX를 실행하기 위해 QuickJS를 링크합니다([Rust로 개발하기](./rust.md)).

### 공통 엔진

엔진 단계부터는 두 방식이 같습니다. 한 차례 실행에서 발생한 변경을 모아 Taffy(flexbox 및 CSS grid)로 레이아웃을 계산하고, 프레임마다 뷰 작업을 한 묶음으로 메인 스레드에 전달하면 메인 스레드가 이를 네이티브 뷰에 적용합니다. 텍스트 측정과 배치는 엔진 스레드에서 한 번 수행합니다. Apple 플랫폼은 CoreText, Android는 `StaticLayout`, Windows는 DirectWrite, Linux는 Pango, 웹은 브라우저의 글꼴 메트릭을 사용합니다. 메인 스레드는 텍스트를 그리기만 합니다.

웹에서는 같은 엔진이 페이지 안에서 실행됩니다. 브라우저가 CSS를 적용한 HTML 요소를 표시하고 스크롤, 텍스트 선택, 입력을 처리합니다. `nsh package --target web`은 파일 내용에 따라 이름이 정해지는 정적 사이트를 생성하므로 CDN에 게시하기 쉽습니다([웹](../../docs/web.md)).

> [!NOTE]
> 메인 스레드는 앱 코드의 실행이 끝나기를 기다리지 않습니다. 시스템이 동기적으로 답을 받아야 하는 경우(글자를 하나 더 입력해도 되는지, 스와이프로 뒤로 갈 수 있는지, 시트를 닫아도 되는지 등)에는 미리 지정한 `maxLength`, `popOnBack`, `closeOnSwipe` 같은 속성에 따라 네이티브에서 바로 판단합니다. 이벤트는 그 후에 앱 핸들러로 전달됩니다.

## 동적 UI와 AI 생성 UI

NSH는 네이티브 앱이 출시 후 새 UI를 받을 수 있도록 하는 데서 출발했고, 지금도 주요 용도 중 하나입니다. 앱은 모듈을 표시할 위치에 iOS의 `NshAppView`, Android의 `NshView`, macOS·Windows·Linux·GPUI 앱의 `AppView`, 웹페이지의 `createPlayer`를 배치합니다. NSH는 그 안에서 TSX 모듈을 실행합니다.

- **출시 후 배포.** 모듈은 앱이 실행 중 다운로드하는 서명된 패키지(`.nshpkg`)입니다. 앱은 실행 전에 신뢰하는 키와 폐기 목록을 바탕으로 서명을 검증합니다([서명된 패키지](../../docs/signed-packages.md)). 호스트 앱이 런타임을 제공하면 각 팀은 그 API에 맞춰 모듈을 빌드하고 자체 일정에 따라 배포합니다.
- **호스트가 권한 관리.** 모듈마다 독립된 QuickJS VM에서 실행되며 메모리, 호출 시간, 노드 수에 제한이 적용됩니다. 네트워크 호스트, 웹페이지, 상태 표시줄 등 호스트가 허용한 기능에만 접근할 수 있습니다([샌드박스와 권한](./sandbox-and-capabilities.md)). 여러 모듈을 나란히 실행할 수도 있습니다.
- **기존 앱과 일관된 UI.** 모듈은 플랫폼의 UI로 렌더링됩니다. 호스트가 지도, 플레이어, 디자인 시스템 컴포넌트 같은 자체 네이티브 뷰를 등록하면 모듈에서도 사용할 수 있습니다([네이티브 컴포넌트](../../docs/native-components.md)).

같은 방식으로 앱이 **사용자가 바로 조작할 수 있는 UI로 응답**하게 할 수 있습니다. 모델이 양식, 차트, 예약 카드 같은 작은 TSX 모듈을 작성하면 서버가 검사하고 빌드해 서명한 뒤 앱이 네이티브 UI로 표시합니다.

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/ai-flow-light.svg" alt="프롬프트에서 네이티브 UI까지의 과정. 모델이 TSX 모듈을 작성하면 서버가 빌드하고 검사해 서명합니다. 신뢰 경계 반대편의 앱은 패키지를 검증하고, 제한과 권한을 지정한 샌드박스에서 실행한 뒤 플랫폼 고유의 UI로 표시합니다.">
<img class="diagram diagram-dark" src="/diagrams/ai-flow-dark.svg" alt="프롬프트에서 네이티브 UI까지의 과정. 모델이 TSX 모듈을 작성하면 서버가 빌드하고 검사해 서명합니다. 신뢰 경계 반대편의 앱은 패키지를 검증하고, 제한과 권한을 지정한 샌드박스에서 실행한 뒤 플랫폼 고유의 UI로 표시합니다.">
</div>

iOS 호스트에 필요한 코드는 다음과 같습니다.

```swift
let app = NshAppView()                         // a UIView: place it anywhere
try app.grantCapabilities(#"{ "network": { "hosts": ["api.example.com"] } }"#)
let info = try await app.loadPackage(at: downloadedFile, trust: trust)   // verified, then run
```

## AI를 활용한 개발

NSH는 모델이 생성하는 개별 화면부터 AI와 함께 개발하는 앱 전체까지 지원하도록 설계했습니다.

- **모든 플랫폼에서 하나의 앱 모델.** 모델은 UI를 한 가지 방식으로 작성하면 되고, 결과는 휴대전화, 데스크톱, 웹에서 실행됩니다. NSH가 플랫폼별 조작 방식을 처리하므로 같은 기능을 여섯 번 생성할 필요가 없습니다.
- **작업에 맞춰 Rust와 TSX 선택.** 앱 전체와 성능이 중요한 모듈은 Rust로, 화면과 플러그인, 요청별로 생성하는 UI는 TSX로 작성해 같은 엔진에서 실행합니다.
- **소스에서 생성하는 정확한 타입 정의.** 모든 요소, 속성, 스타일 키, 모듈에 대한 TypeScript 선언을 Rust 소스에서 생성합니다(`.nsh/types`). Rust API에도 타입이 명시되어 있어 컴파일러로 생성한 코드를 검사할 수 있습니다. 이 문서도 모델이 읽을 수 있는 일반 텍스트로 제공합니다([`llms.txt`](/llms.txt), [`llms-full.txt`](/llms-full.txt)).
- **명령줄에서 끝까지 개발.** 에이전트는 `nsh`로 앱을 만들고 빌드해 타깃에서 실행하고, 테스트 결과를 읽고 수정한 뒤 패키징할 수 있습니다. `nsh test`는 기기 없이 테스트를 실행하고 오류와 생성된 뷰를 출력합니다. 컴파일러는 프로세스 안에서(`nsh_bundler::compile_module`) 또는 브라우저에서도 실행됩니다([플레이그라운드](/playground)).
- **외부 코드의 실행 제한.** 실행 중 전달된 생성 코드는 신뢰할 수 없는 코드로 취급합니다. 서명된 모듈을 자원 제한이 있는 VM에서 실행하고 호스트가 부여한 권한만 허용합니다.

## 지원 플랫폼

| 플랫폼 | 요구 사항 | 백엔드 | 임베딩 방식 |
|---|---|---|---|
| [iOS](../../docs/ios.md) | iOS 15.0 이상 | UIKit | `NativeShell.xcframework` |
| [Android](../../docs/android.md) | Android 9 이상 | Android 뷰 | `nsh.aar` |
| [macOS](../../docs/macos.md) | macOS 14 이상 | AppKit | `AppView` |
| [Windows](../../docs/windows.md) | Windows 10 1809 이상 | Win32, Direct2D | `AppView` |
| [Linux](../../docs/linux.md) | GTK 4.14 이상 | GTK 4 | `AppView` |
| [웹](../../docs/web.md) | 최신 브라우저 | HTML | `createPlayer`, iframe |
| [GPUI](./gpui.md) | 데스크톱 | GPUI | `AppView`(GPUI 및 GPUI Kit 앱) |
| Mac Catalyst | macOS | UIKit | 개발용(`--target catalyst`) |

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/platforms-light.svg" alt="하나의 모듈을 여러 호스트에서 실행합니다. 같은 번들과 Rust 런타임을 iOS, Android, Linux, AppKit 기반 macOS, Win32와 Direct2D 기반 Windows, 세 데스크톱 운영체제의 GPUI, HTML로 표시하는 웹페이지에서 사용합니다.">
<img class="diagram diagram-dark" src="/diagrams/platforms-dark.svg" alt="하나의 모듈을 여러 호스트에서 실행합니다. 같은 번들과 Rust 런타임을 iOS, Android, Linux, AppKit 기반 macOS, Win32와 Direct2D 기반 Windows, 세 데스크톱 운영체제의 GPUI, HTML로 표시하는 웹페이지에서 사용합니다.">
</div>

## Native Shell과 GPUI Shell

[Longbridge](https://longbridge.com)는 자체 클라이언트 앱에 쓸 목적으로 NSH를 개발합니다. 처음에 필요했던 것은 모바일 클라이언트용 플러그인 계층이었습니다. [GPUI](https://gpui-kit.com) 데스크톱 앱을 JavaScript로 확장하는 [GPUI Shell](https://gpui-kit.com/shell)이 출발점이었습니다. 하지만 휴대전화에서 GPUI가 UI를 직접 그리면 텍스트 선택과 메뉴(복사, 공유, 번역, 찾아보기), 입력기, 접근성, iOS 새 버전의 시각 효과 등을 다시 구현해야 하고, 각 모바일 팀은 새로운 UI 기술을 도입해야 했습니다. 이에 NSH는 각 플랫폼 고유의 UI로 렌더링하는 방식을 택했고, 모바일 플러그인 계층에서 여러 플랫폼의 앱 전체를 만들 수 있는 프레임워크로 발전했습니다.

GPUI Shell은 GPUI 데스크톱 앱에 확장 기능을 더합니다. Native Shell은 휴대전화, 데스크톱, 웹용 네이티브 앱을 만드는 프레임워크이며, GPUI 백엔드를 통해 GPUI 및 GPUI Kit 앱 안에 NSH 앱을 임베딩할 수도 있습니다([GPUI](./gpui.md)).

## 다른 접근 방식과 비교

앱 전체를 개발하는 용도로 보면 NSH는 React Native, Flutter, Qt, GPUI, Slint, Electron, Tauri 같은 크로스 플랫폼 프레임워크와 비교할 수 있습니다. 출시 후 UI를 전달하는 용도로 보면 웹 뷰, 서버 주도 UI, 미니 프로그램 컨테이너와 비교할 수 있습니다. NSH는 하나의 앱 모델 아래 각 플랫폼의 UI와 조작 방식을 활용하며 Rust를 주 언어로, TSX를 함께 사용합니다. [접근 방식 비교](./comparison.md)에서 각각을 살펴보세요.

## 다음 단계

- [설치](./installation.md)와 [시작하기](./getting-started.md)를 따라 `nsh`를 설치하고 Rust 또는 TSX로 첫 앱을 작성해 각 플랫폼에서 실행하세요.
- [Rust로 개발하기](./rust.md)에서 Rust 앱 작성과 TSX 화면 추가 방법을 알아보세요.
- [플랫폼](./platforms.md)에서 타깃별 지원 기능과 실행, 패키징, 임베딩 방법을 확인하세요.
- [JSX와 컴포넌트](../../docs/jsx-and-components.md) 및 [반응성](../../docs/reactivity.md)에서 Rust와 TSX 개발의 기본 개념을 살펴보세요.
- [서명된 패키지](../../docs/signed-packages.md)와 [샌드박스와 권한](./sandbox-and-capabilities.md)에서 모듈의 배포 방식과 권한 설정을 확인하세요.
