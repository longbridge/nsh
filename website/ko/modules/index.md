---
title: 모듈
description: "NSH 앱에서 가져오는 모듈의 구조와 각 모듈이 제공하는 API를 살펴봅니다."
order: 1
---

# 모듈

Native Shell 앱은 필요한 기능을 몇 개의 모듈에서 가져옵니다. 반응성 그래프, 렌더러, 각 모듈은 모두 런타임 내부에 Rust로 구현되어 있습니다. 번들러는 `import ... from "nsh/..."`를 그대로 유지하므로 앱 번들에는 개발자가 작성한 코드만 들어갑니다.

```tsx
import { createAnimatedValue, mount, signal, windowSize } from "nsh";
import { Button } from "nsh/base";
import { TabBar } from "nsh/mobile";
```

## UI의 세 계층

NSH의 UI는 GPUI Kit의 `gpui`, `gpui-base`, `gpui-component`에 대응하는 세 계층으로 구성됩니다.

| 계층 | 모듈 | 앱에 제공하는 기능 |
|---|---|---|
| 기본 요소 | `nsh`, `nsh/mobile`, `nsh/desktop` | 반응성, 호스트 요소, 레이아웃, 목록, 캔버스, 애니메이션, 제스처, 기기 정보와 플랫폼별 API |
| 기본 컴포넌트 | [`nsh/base`](../../modules/nsh-base.md) | 버튼, Markdown·HTML 문서, 내비게이션 스택, 대화상자와 시스템 대화상자. 모든 플랫폼에서 동작, 상태, 포커스, 접근성을 처리하며 스타일은 앱에서 지정합니다. |
| 컴포넌트 라이브러리 | 앱 자체 또는 커뮤니티 라이브러리 | 앱 디자인에 맞게 스타일을 적용한 버튼, 카드, 행, 화면 |

앱의 시각적 디자인은 세 번째 계층에서 결정합니다. [호스트 요소](../elements/index.md)와 `nsh/base`로 컴포넌트를 만들고 `style`, [Tailwind](../../docs/tailwind.md) 클래스와 테마로 꾸밀 수 있습니다. [앱의 크레이트](../docs/rust.md)에 Rust 컴포넌트를 포함하거나 같은 기반으로 만들어진 커뮤니티 라이브러리를 사용해도 됩니다.

각 API는 한 모듈에서만 제공합니다. iOS와 Android 전용 API는 `nsh/mobile`에, macOS, Windows, Linux 전용 API는 `nsh/desktop`에 있습니다. 두 모듈 모두 모든 플랫폼에서 가져올 수 있어 번들 하나를 여러 환경에서 실행할 수 있습니다. 대상이 아닌 플랫폼에서 호출하면 즉시 반환합니다. 실행 중인 플랫폼은 `platform.os`로 확인합니다.

타입은 런타임에서 생성합니다. `nsh init`, `nsh build`, `nsh dev`, `nsh test`를 실행하면 앱 디렉터리의 `.nsh/types`에 TypeScript 선언이 생성되므로 빌드에 사용한 런타임과 선언이 일치합니다. [프로젝트 구조](../docs/project-structure.md)에서 파일 배치를 확인할 수 있습니다.

## 전체 모듈

| 모듈 | 내보내는 API | 문서 |
|---|---|---|
| `nsh` | `signal`, `computed`, `effect`, `batch`, `untracked`, `cleanup`, `context`, `split`, `mount`, `ErrorBoundary`, 기본 요소 `View`, `Text`, `Scroll`, [`List`](../../modules/nsh-list.md), [`Image`](../../modules/nsh-image.md) (`Image.getSize`, `Image.prefetch` 포함), [`Canvas`](../../modules/nsh-canvas.md) (`Path2D`, `CanvasRenderingContext2D`, `CanvasGradient` 포함), [`WebView`](../../modules/nsh-webview.md), [`Video`](../../modules/nsh-video.md), `safeArea`, `safeAreaInsets`, `Keyboard`, `keyboardFrame`, [애니메이션](../../modules/nsh.md#animation) (`createAnimatedValue`, `AnimatedValue`, `AnimatedNode`, `Animated`, `Easing`, `LayoutAnimation`), [실행 환경](../../modules/nsh.md#device) (`windowSize`, `colorScheme`, `setColorScheme`, `fontScale`, `pixelRatio`, `locale`, `platform`, `openUrl`), [`PanResponder`](../../modules/nsh.md#panresponder), [`Clipboard`](../../modules/nsh.md#clipboard), [`Picker`](../../modules/nsh.md#picker), [`Storage`](../../modules/nsh.md#storage), [`host`](../../modules/nsh.md#host) (`available`, `call`, `on`, 네이티브 SDK의 `params`와 `post`), [`keymap`](../../modules/nsh.md#keymap) (키보드 단축키 `mod+k`, `ctrl+shift+p`, ...), [`nativeComponent`, `nativeModule`](../../modules/nsh.md#nativecomponent-and-nativemodule) (호스트 앱이 등록하는 컴포넌트와 함수), [접근성](../../modules/nsh.md#accessibility) (`announce`, `setAccessibilityFocus`, `screenReaderEnabled`, `reduceMotion`), 요소·스타일·이벤트·JSX의 타입 (`ViewRef`, `InputRef`, ..., `ViewStyle`, ...) | [nsh](../../modules/nsh.md) |
| `nsh/base` | `Button`, `Input`, `TextView`, `createNavStack`, `NavStack`, `NavPage`, `Dialog`, `AlertDialog` | [nsh/base](../../modules/nsh-base.md) |
| `nsh/mobile` | `isAvailable`, `TabBar` (iOS 및 Android의 시스템 탭 바), `InputAccessoryView` (iOS), `BackHandler` (Android) | [nsh/mobile](../../modules/nsh-mobile.md) |
| `nsh/desktop` | `isAvailable`, `Window` (제목, 제목 표시줄, 최대화, 전체 화면), `titleBar`, `Menu` (앱 메뉴, 컨텍스트 메뉴) | [nsh/desktop](../../modules/nsh-desktop.md) |
| 표준 모듈 | `buffer`, `console`, `crypto`, `fs/promises`, `net`, `os`, `path`, `process`, `url`, `websocket`, `zlib` | [표준 모듈](../../modules/standard-modules.md) |
| 전역 API | `fetch`, `URL`, `TextEncoder`, `Blob`, `FormData`, `AbortController`, `Intl`, 타이머, `console`, `requestAnimationFrame`, ... (가져오기 없이 사용) | [전역 API](../../modules/globals.md) |

다음 문서에서는 모듈 전반에 걸쳐 쓰이는 스타일을 정리합니다.

| 문서 | 내용 |
|---|---|
| [스타일 레퍼런스](../../modules/style-reference.md) | 각 스타일 속성의 이름, 타입, 의미 |
| [Tailwind 레퍼런스](../../modules/tailwind-reference.md) | `class`에서 사용할 수 있는 Tailwind 유틸리티와 변형 |

## 호스트 요소

호스트 요소도 컴포넌트처럼 가져옵니다. `View`, `Text`, `Scroll`, `Image`, `List`, `Canvas`, `WebView`, `Video`, `Overlay`는 `nsh`에서, 기본 컴포넌트(`Button`, `Input`, `Dialog`, `NavStack`, `NavPage`)는 `nsh/base`에서, `InputAccessoryView`는 `nsh/mobile`에서 가져옵니다. `If`와 `Each`만 전역으로 제공됩니다. 컴파일러가 사용 위치에 요소를 직접 생성하므로 가져오기에 따른 런타임 비용은 없습니다. 각 요소의 속성은 [요소](../elements/index.md) 문서에서 설명합니다.
