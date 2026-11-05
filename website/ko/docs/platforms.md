---
title: 지원 플랫폼
description: "iOS, Android, macOS, Windows, Linux, GPUI, 터미널과 웹에서 NSH가 사용하는 백엔드와 플랫폼별 요구 사항을 살펴보고, Rust 또는 TSX 앱을 실행하고 패키징하거나 기존 앱에 임베드하는 방법을 알아봅니다."
order: 10
group: platforms
---

# 지원 플랫폼

NSH는 iOS, Android, macOS, Windows, Linux, GPUI, 터미널과 웹에서 실행됩니다. 각 백엔드는 해당 플랫폼의 UI로 화면을 그립니다. 예를 들어 iPhone에서는 UIKit 뷰, Mac에서는 AppKit 뷰, Linux에서는 GTK 4 위젯, GPUI 앱에서는 GPUI 요소, 터미널에서는 문자 셀, 브라우저에서는 HTML을 사용합니다. 텍스트와 입력, 접근성, 다크 모드도 플랫폼을 따릅니다. 백엔드 위에서 동작하는 반응성 그래프, 렌더러, Taffy 레이아웃, 컴포넌트와 네이티브 모듈은 모두 같은 Rust 코드이므로 하나의 앱을 여러 플랫폼에서 실행할 수 있습니다. Rust 앱은 이 코드와 개발자가 작성한 크레이트로 구성되며 JavaScript VM이 필요하지 않습니다. TSX 앱의 모듈은 이 기반 위에서 QuickJS로 실행됩니다. [Rust로 앱 작성하기](./rust.md)에서 두 방식의 차이를 설명합니다.

어느 플랫폼에서든 `nsh package`로 설치 가능한 앱을 만들거나 기존 네이티브 앱 안에 NSH를 임베드할 수 있습니다. 호스트는 Rust 루트를 링크하거나 런타임에 TSX를 불러옵니다. 개발할 때는 `nsh run`을 사용합니다. Rust 코드를 수정하면 앱이 다시 빌드되고 재시작되며, TSX 수정 사항은 핫 업데이트로 반영됩니다.

## 플랫폼 한눈에 보기

| 플랫폼 | 백엔드와 텍스트 처리 | 실행 환경 | 개발 시 실행 방법 | 패키징 결과 |
|---|---|---|---|---|
| [iOS](../../docs/ios.md) | UIKit, CoreText | iOS 15.0 이상 기기 및 시뮬레이터 | `nsh run --target ios` | 시뮬레이터용 `.app`, 서명된 `.ipa` |
| [Android](../../docs/android.md) | Android 뷰, `StaticLayout` | Android 9(API 28) 이상, `arm64-v8a`, `armeabi-v7a`, `x86_64` | `nsh run --target android` | 서명된 `.apk` |
| [macOS](../../docs/macos.md) | AppKit과 Core Animation, CoreText | macOS 14 이상 | `nsh run --target macos` | 애드혹 서명된 `.app` |
| [Windows](../../docs/windows.md) | Direct2D로 그리는 Win32 창, DirectWrite | Windows 10 1809 이상, Windows 11 | `nsh run`(Windows 기본값) | 디렉터리와 `.zip` |
| [Linux](../../docs/linux.md) | GTK 4 위젯, Pango | GTK 4.14 이상 | `nsh run`(Linux 기본값) | 디렉터리와 `.tar.gz` |
| [GPUI](./gpui.md) | GPUI, GPUI 텍스트 시스템 | macOS, Windows, Linux(Vulkan) | GPUI 앱 안에서 실행. TSX는 `nsh run --target gpui`로도 실행 가능 | 임베드한 GPUI 앱에 포함 |
| [터미널](../../docs/tui.md) | 터미널의 문자 셀 격자에 그리는 ratatui, 터미널 글꼴 | macOS, Windows, Linux의 터미널 | `nsh run --target tui` | 임베드한 터미널 앱에 포함 |
| [웹](../../docs/web.md) | HTML과 CSS, 브라우저 글꼴 | 데스크톱과 모바일의 최신 브라우저 | `nsh run --target web` | 정적 사이트 |

- **[GPUI](./gpui.md)** 백엔드는 macOS, Windows, Linux에서 GPUI Kit의 기반 프레임워크인 [GPUI](https://gpui-kit.com)로 화면을 그립니다. 기존 GPUI 또는 GPUI Kit 앱은 자신의 요소 트리에 `AppView`를 넣어 NSH 화면을 표시합니다. 독립 실행형 데스크톱 앱은 각 운영체제의 고유 백엔드(AppKit, Win32, GTK 4)로 실행하고 패키징합니다.
- **[터미널](../../docs/tui.md)** 백엔드는 터미널의 색상과 문자 셀 격자를 사용합니다. 키보드 중심으로 조작할 수 있고 마우스도 지원합니다. 현재 터미널에서 실행하려면 `nsh run --target tui`를 사용합니다. ratatui 앱에서는 `AppView`로 임베드합니다.
- **[웹](../../docs/web.md)** 백엔드는 브라우저 페이지에 HTML을 렌더링합니다. `nsh package --target web`은 어떤 웹 서버나 CDN에도 배포할 수 있는 정적 사이트를 만듭니다. Rust 앱의 엔진과 크레이트는 WebAssembly로 컴파일됩니다. 기존 웹 페이지에는 `createPlayer` 또는 iframe으로 앱을 넣을 수 있습니다. 이 사이트의 실행 예제와 [플레이그라운드](/playground)도 이 백엔드를 사용합니다.
- **Mac Catalyst**는 Mac에서 iPhone 크기의 창(390×844)으로 iOS 백엔드를 실행하는 개발용 타깃입니다. 휴대폰 화면에서 앱이 어떻게 보이는지 확인하려면 `--target catalyst`를 지정하세요.

## 개발과 패키징

`--target`을 생략하면 `nsh run`은 현재 컴퓨터의 백엔드를 선택합니다. macOS에서는 AppKit, Linux에서는 GTK 창, Windows에서는 Win32 창을 사용합니다. iOS용 앱을 실행하려면 Xcode를 설치한 Mac이 필요합니다. Android용 앱은 Android SDK를 설치한 macOS, Linux 또는 Windows에서 실행할 수 있습니다. [호스트별 지원 명령](./installation.md#호스트별-지원-명령)를 참고하세요.

Rust 앱은 `nsh.json`의 `"root"`에 루트 컴포넌트를 지정합니다. 앱 디렉터리에서 실행하거나 패키징하면 `nsh`는 해당 타깃에 맞춰 앱의 크레이트를 포함하는 전용 실행기를 빌드합니다. 이 실행기는 QuickJS를 포함하지 않습니다. TSX 앱에서는 진입 파일을 지정합니다.

```sh
# A Rust app, from its directory
nsh run --target macos             # or ios, android, windows, linux, tui, web, catalyst
nsh package --target macos         # or ios, ios-simulator, android, windows, linux, web

# A TSX app
nsh run App.tsx --target macos     # or ios, android, windows, linux, gpui, tui, web, catalyst
nsh package App.tsx --target macos
```

데스크톱 앱은 대상 운영체제에서 그 운영체제의 백엔드로 패키징합니다. Mac 앱은 Mac에서, Windows 앱은 Windows에서, Linux 앱은 Linux에서 패키징하세요. 타깃별 출력물과 서명, 아이콘은 [패키징](../../docs/packaging.md)에서 설명합니다.

## 네이티브 앱에 임베드하기

| 플랫폼 | 호스트 뷰 | 실행할 수 있는 코드 | 배포 형태 | 호스트 언어 |
|---|---|---|---|---|
| macOS | `AppView`(`NSView`) | Rust 루트, TSX 번들 | `nsh-platform-macos` 크레이트 | Rust |
| Windows | `AppView`(자식 창) | Rust 루트, TSX 번들 | `nsh-platform-windows` 크레이트 | Rust |
| Linux | `AppView`(GTK 위젯) | Rust 루트, TSX 번들 | `nsh-platform-linux` 크레이트 | Rust |
| GPUI(macOS, Windows, Linux) | `AppView`(GPUI 뷰) | Rust 루트, TSX 번들, 서명된 패키지 | `nsh-platform-gpui` 크레이트 | Rust |
| iOS | `AppView`(`UIView`) | Rust 루트, TSX 번들 | `nsh-platform-ios` 크레이트 | Rust |
| iOS | `NshAppView`(`UIView`) | TSX 번들, 서명된 패키지 | `NativeShell.xcframework` | Swift |
| Android | `NshView`(`FrameLayout`) | TSX 번들, 서명된 패키지 | `nsh.aar` | Java 또는 Kotlin |
| 터미널 | `AppView`(ratatui 프레임의 한 영역) | Rust 루트, TSX 번들 | `nsh-platform-tui` 크레이트 | Rust |
| 웹 | `createPlayer`(페이지 요소) 또는 iframe | TSX 번들, 사이트로 배포한 Rust 앱 | `nsh package --target web`이 생성하는 런타임 파일 | JavaScript 또는 HTML |

Rust 호스트는 Rust 루트(`Root::of::<App>()`)를 `AppSource::Rust` 또는 `App::Rust`로 마운트합니다. `set_ui_sets`로 앱의 UI 세트를 엔진에 전달하면 이후 불러온 TSX에서도 앱 고유의 컴포넌트와 모듈을 가져올 수 있습니다. 뷰마다 엔진이 하나씩 있으므로 한 앱에 여러 뷰를 표시할 수 있습니다. 모든 플랫폼에서 호스트는 앱이 사용할 네이티브 뷰를 직접 등록할 수 있습니다. 자세한 내용은 [네이티브 컴포넌트](../../docs/native-components.md)를 참고하세요.

> [!NOTE]
> Rust 호스트에서 NSH를 임베드할 때는 NSH 저장소의 플랫폼 크레이트를 사용합니다. macOS, Windows, Linux의 `AppView`는 기본 제한을 적용하고 앱 자체의 파일에서 번들이나 소스 코드를 불러옵니다. GPUI 크레이트와 iOS·Android SDK에서는 [서명된 패키지](../../docs/signed-packages.md)도 불러올 수 있고 뷰마다 자원 제한과 권한을 설정할 수 있습니다. [샌드박스와 권한](./sandbox-and-capabilities.md)에서 설정 방법을 설명합니다.

## 여러 플랫폼을 위한 코드 작성

요소와 스타일, 레이아웃, 모듈은 플랫폼 전반에서 동일하게 동작하므로 대부분의 앱 코드에서는 플랫폼을 확인할 필요가 없습니다. 차이가 있는 부분은 다음과 같이 처리합니다. [플랫폼별 코드](../../docs/platform-specific.md)에서 자세히 살펴볼 수 있습니다.

- **실행 환경 확인.** Rust에서는 `nsh_core::prelude`의 `os()`, `idiom()`, `is_desktop()`을 사용합니다. TSX에서는 `nsh`의 `platform.os`, `platform.idiom`, `platform.desktop`, `platform.select`를 사용합니다. OS 값은 iOS, Android, macOS, Windows, Linux 또는 웹입니다. 터미널에서 실행하면 idiom은 `"terminal"`이고 OS는 컴퓨터의 운영체제입니다.
- **플랫폼별 속성.** TSX의 `ios={{…}}`, `android={{…}}`, `web={{…}}`, `desktop={{…}}`에 지정합니다. `desktop`에는 macOS, Windows, Linux, 데스크톱 브라우저와 터미널이 포함됩니다.
- **입력 방식.** 휴대폰에서는 터치와 소프트웨어 키보드를, 데스크톱에서는 마우스와 트랙패드, 키보드를 사용합니다.

NSH의 기본 요소로 그린 컴포넌트는 휴대폰과 데스크톱에서 같은 모습으로 동작합니다. 기기에 맞춰 레이아웃을 바꾸려면 갤러리 앱처럼 기본 요소(Rust의 `view`, `text`, `scroll`, `virtual_list` 등, TSX의 `View`, `Text`, `Scroll`, `List` 등)를 조합하고 데스크톱 여부를 확인합니다.

```rust tabs
use nsh_core::prelude::*;

#[component]
pub fn Shell() -> impl IntoElement {
    // macOS, Windows, Linux and desktop browsers get a sidebar; phones get tabs.
    view()
        .flex_1()
        .child(if is_desktop() { sidebar_layout() } else { tab_layout() })
}

fn sidebar_layout() -> AnyElement {
    view().flex_1().flex_row().into_element()
}

fn tab_layout() -> AnyElement {
    view().flex_1().flex_col().into_element()
}
```

```tsx tabs
import { View } from "nsh";
import { platform } from "nsh";

export function Shell() {
  // macOS, Windows, Linux and desktop browsers get a sidebar; phones get tabs.
  return <View class="flex-1">{platform.desktop ? <SidebarLayout /> : <TabLayout />}</View>;
}

function SidebarLayout() {
  return <View class="flex-1 flex-row" />;
}

function TabLayout() {
  return <View class="flex-1 flex-col" />;
}
```

백엔드별 차이는 [macOS](../../docs/macos.md#what-differs-on-macos), [Windows](../../docs/windows.md#what-differs-on-windows), [Linux](../../docs/linux.md#what-differs-on-linux), [GPUI](./gpui.md#백엔드별-차이), [터미널](../../docs/tui.md) 문서에서 확인할 수 있습니다.

## 함께 보기

- [시작하기](./getting-started.md)에서는 첫 앱을 만들고 각 플랫폼에서 실행합니다.
- [Rust로 앱 작성하기](./rust.md)에서는 Rust만 사용하거나 TSX와 함께 사용하는 방법을 설명합니다.
- [설치](./installation.md)에서는 플랫폼에 필요한 도구를 확인할 수 있습니다.
- [아키텍처](../../docs/architecture.md)에서는 여러 플랫폼이 공유하는 부분과 각 백엔드가 담당하는 부분을 설명합니다.
