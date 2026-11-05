---
title: 프레임워크와 UI 배포 방식 비교
description: Native Shell을 React Native, Flutter, Qt, GPUI, Slint, Electron, Tauri와 비교하고, 출시 후 UI를 배포하는 방식으로서 웹 뷰 및 서버 주도 UI와 어떻게 다른지 살펴봅니다.
order: 2
group: start
---

# 프레임워크와 UI 배포 방식 비교

Native Shell은 iOS, Android, macOS, Windows, Linux, 웹에서 앱 전체를 개발하는 프레임워크인 동시에 출시 후 앱에 배포한 UI를 실행하는 런타임입니다. 먼저 앱을 개발하는 크로스 플랫폼 프레임워크를 비교하고, 이어서 처음 배포할 때 포함되지 않은 UI를 기존 앱에서 실행하는 방법을 살펴봅니다. 초록색은 지원, 노란색은 앱에서 추가 작업을 하거나 별도 프로젝트를 사용해야 하는 경우, 빨간색은 미지원을 뜻합니다. 색상은 기능 지원 여부를 나타내며 성능 점수가 아닙니다. 표의 버전은 문서 작성 당시 각 프레임워크가 명시한 최소 버전입니다. 실제로 사용하는 릴리스의 요구 사항도 확인하세요.

범례: <span class="status status--yes" role="img" aria-label="지원"></span> 지원 · <span class="status status--partial" role="img" aria-label="부분 지원"></span> 부분 지원 · <span class="status status--no" role="img" aria-label="미지원"></span> 미지원

## 크로스 플랫폼 프레임워크

| 기능 | NSH | React Native | Flutter | Qt 6 | GPUI | GPUI Fast | Slint |
|---|---|---|---|---|---|---|---|
| 렌더링 | 플랫폼 UI | 플랫폼 뷰 | 자체 엔진 | 장면 그래프 | 자체 GPU 렌더러 | 자체 GPU 렌더러 | 자체 렌더러 |
| UI 모델 | 유지 모드 | 유지 모드 | 유지 모드 | 유지 모드 | 즉시 모드 | 즉시 + 유지 모드 | 반응형, 유지 모드 |
| 언어 | Rust, TSX | JS, TS | Dart | C++, QML | Rust | Rust | Slint + Rust, C++ |
| iOS | <span class="status status--yes" role="img" aria-label="지원"></span> 15+ | <span class="status status--yes" role="img" aria-label="지원"></span> 15.1+ | <span class="status status--yes" role="img" aria-label="지원"></span> 15+ | <span class="status status--yes" role="img" aria-label="지원"></span> 17+ | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> Rust |
| Android | <span class="status status--yes" role="img" aria-label="지원"></span> 9+ | <span class="status status--yes" role="img" aria-label="지원"></span> 7+ | <span class="status status--yes" role="img" aria-label="지원"></span> 7+ | <span class="status status--yes" role="img" aria-label="지원"></span> 9+ | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> |
| macOS | <span class="status status--yes" role="img" aria-label="지원"></span> 14+ | <span class="status status--partial" role="img" aria-label="부분 지원"></span> 별도 프로젝트 | <span class="status status--yes" role="img" aria-label="지원"></span> 12+ | <span class="status status--yes" role="img" aria-label="지원"></span> 13+ | <span class="status status--yes" role="img" aria-label="지원"></span> 12+ | <span class="status status--yes" role="img" aria-label="지원"></span> 12+ | <span class="status status--yes" role="img" aria-label="지원"></span> 13+ |
| Windows | <span class="status status--yes" role="img" aria-label="지원"></span> 10 1809+ | <span class="status status--partial" role="img" aria-label="부분 지원"></span> 별도 프로젝트 | <span class="status status--yes" role="img" aria-label="지원"></span> 10+ | <span class="status status--yes" role="img" aria-label="지원"></span> 10 1809+ | <span class="status status--yes" role="img" aria-label="지원"></span> 10+ | <span class="status status--yes" role="img" aria-label="지원"></span> 10+ | <span class="status status--yes" role="img" aria-label="지원"></span> 10+ |
| Linux | <span class="status status--yes" role="img" aria-label="지원"></span> GTK 4.14+ | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> Debian 10+ | <span class="status status--yes" role="img" aria-label="지원"></span> Debian 11+ | <span class="status status--yes" role="img" aria-label="지원"></span> Vulkan | <span class="status status--yes" role="img" aria-label="지원"></span> Vulkan | <span class="status status--yes" role="img" aria-label="지원"></span> |
| 웹 | <span class="status status--yes" role="img" aria-label="지원"></span> HTML | <span class="status status--partial" role="img" aria-label="부분 지원"></span> 라이브러리 | <span class="status status--yes" role="img" aria-label="지원"></span> Canvas | <span class="status status--yes" role="img" aria-label="지원"></span> Canvas | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> Canvas |
| 시스템 텍스트 입력 | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> |
| 기기별 UX | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> |
| JS 런타임 없이 실행 | <span class="status status--yes" role="img" aria-label="지원"></span> Rust 앱 | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> C++ | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> |
| 서명 기반 배포 | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> OTA | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> QML | <span class="status status--partial" role="img" aria-label="부분 지원"></span> Shell | <span class="status status--partial" role="img" aria-label="부분 지원"></span> Shell | <span class="status status--no" role="img" aria-label="미지원"></span> |
| 라이선스 | Apache-2.0 | MIT | BSD-3-Clause | LGPL, GPL, 상용 | Apache-2.0 | Apache-2.0 | GPL, 상용 |

즉시 모드는 프레임마다 UI를 다시 기술하고, 유지 모드는 UI 구조를 보관하면서 변경된 부분만 갱신합니다. GPUI Fast는 GPUI 위에서 두 방식을 결합하며 GPUI와 같은 플랫폼에서 실행됩니다. 표의 ‘Shell’은 [GPUI Shell](https://gpui-kit.com/shell)을 가리킵니다.

표의 프레임워크 가운데 다음 특성을 함께 제공하는 것은 NSH입니다. 플랫폼 자체의 UI와 텍스트 입력을 사용하고, 하나의 앱이 휴대폰, 데스크톱, 웹에서 실행되며, 웹에서는 HTML을 렌더링합니다. Rust 앱에는 JavaScript 런타임이 필요하지 않으며, 출시 후에는 서명된 UI를 배포해 샌드박스에서 실행할 수 있습니다.

### 비교 항목의 의미

- **렌더링과 UI 모델.** NSH는 각 플랫폼의 뷰를 마운트한 뒤 유지하고 변경이 발생한 뷰만 갱신합니다. React Native도 iOS와 Android에서 플랫폼 뷰를 마운트합니다. Flutter, Qt Quick, GPUI, Slint는 자체 렌더러를 사용합니다. GPUI는 프레임마다 요소 트리를 새로 구성하는 즉시 모드를 사용하고, [GPUI Fast](https://github.com/longbridge/gpui-fast)는 변경되지 않은 부분을 재사용합니다.
- **플랫폼.** NSH의 최소 사양은 [플랫폼 문서](./platforms.md)를 기준으로 합니다. 다른 프레임워크의 사양은 [React Native 0.76](https://reactnative.dev/blog/2024/10/23/release-0.76-new-architecture)(iOS 15.1, Android 7/API 24, macOS와 Windows는 별도의 react-native-macos 및 react-native-windows 프로젝트, 웹은 react-native-web 라이브러리), [Flutter 3.47](https://docs.flutter.dev/reference/supported-platforms), [Qt 6.10](https://doc.qt.io/qt-6.10/supported-platforms.html), GPUI의 경우 [Zed 설치 요구 사항](https://zed.dev/docs/installation), Slint의 경우 [플랫폼 가이드](https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/general/)(iOS는 Rust에서 사용)를 따릅니다. 표의 ‘Canvas’는 프레임워크가 웹 페이지의 캔버스에 그린다는 뜻입니다. NSH 웹 앱은 CSS로 스타일을 적용한 HTML 요소이며, 스크롤, 선택, 편집은 브라우저가 담당합니다([웹](../../docs/web.md)).
- **시스템 텍스트 입력.** NSH는 `UITextView`, `EditText`, `NSTextView`, GTK의 `TextView`, 브라우저의 텍스트 편집기 등 각 플랫폼의 텍스트 필드를 사용해 시스템 입력기, 선택 핸들, 편집 메뉴를 제공합니다. Windows에서는 NSH 자체 편집기가 DirectWrite와 시스템 입력기(IMM32)에 연결됩니다. React Native도 플랫폼의 텍스트 입력 컨트롤을 사용합니다. 자체 렌더러를 쓰는 프레임워크는 텍스트 편집을 직접 구현하고 시스템 입력기와 연동합니다.
- **기기별 UX.** NSH의 기본 컴포넌트는 기기에 맞게 동작합니다. 같은 `Menu`도 포인터로 열면 누른 위치에 팝업으로 뜨고, 휴대폰에서는 화면 아래에서 시트로 올라옵니다. `ContextMenu`는 마우스 오른쪽 버튼을 클릭하거나 손가락으로 길게 눌러 엽니다. 다른 프레임워크에서는 기기별 상호작용을 맞추는 작업을 주로 앱이 맡습니다.
- **JS 런타임 없이 실행.** Rust 앱의 실행기는 어느 플랫폼에서도 JavaScript 엔진을 링크하지 않습니다. 같은 NSH 엔진에서 TSX를 사용하는 방식도 선택할 수 있습니다. Qt Quick의 QML은 Qt의 JavaScript 엔진에서 실행되지만 C++로 작성하는 Qt Widgets에는 이 엔진이 필요하지 않습니다.
- **서명 기반 배포.** NSH는 신뢰하는 키와 폐기 목록으로 서명된 `.nshpkg` 패키지를 검증하고, 호스트가 설정한 한도와 권한이 적용된 VM에서 각 패키지를 실행합니다([서명 패키지](../../docs/signed-packages.md), [샌드박스와 권한](./sandbox-and-capabilities.md)). React Native의 OTA 업데이트는 신뢰하는 앱 코드를 배포합니다. Qt는 QML 파일을 불러올 수 있고, GPUI 앱은 [GPUI Shell](https://gpui-kit.com/shell)을 통해 JavaScript 모듈을 실행할 수 있습니다.

### NSH의 특징

- **플랫폼 고유의 화면 구성과 상호작용.** 각 백엔드는 UIKit, Android 뷰, AppKit, Direct2D를 사용하는 Win32, GTK 4, HTML 등 플랫폼의 뷰, 텍스트, 입력기를 기반으로 그 플랫폼에 익숙한 조작 방식을 제공합니다. 텍스트 선택, 입력기, 접근성은 시스템이 처리하므로 OS에서 이 기능을 개선하면 앱도 그 변화를 반영합니다([소개](./index.md#네이티브의-의미)).
- **휴대폰, 데스크톱, 웹을 하나의 앱으로 개발.** 하나의 프레임워크가 여섯 플랫폼을 백엔드로 제공하며 레이아웃은 화면에, 입력은 기기에 맞게 조정됩니다. 웹에서는 앱이 HTML로 렌더링되고 정적 사이트로 배포됩니다.
- **Rust를 중심으로 개발.** Rust의 타입, 스레드, 크레이트를 사용해 앱 전체를 Rust로 작성하고 JavaScript 런타임 없이 네이티브 코드로 컴파일할 수 있습니다. 빠르게 화면을 작성하고 싶을 때는 같은 엔진에서 TSX를 사용할 수 있습니다([Rust로 앱 개발](./rust.md)).
- **일반적인 앱 화면에 맞는 뷰 유지 방식.** 변경이 발생한 뷰만 전달하므로 대화창, 폼, 목록, 문서는 화면 변화가 없을 때 CPU 사용량이 유휴 상태에 가깝습니다([소개](./index.md#뷰를-유지하는-렌더링-방식)).
- **출시 후 서명된 UI를 배포하고 샌드박스에서 실행.** NSH 앱과 각 플랫폼의 도구로 개발한 기존 네이티브 앱 모두 팀이 작성하거나 AI가 생성한 TSX 모듈을 실행 중에 불러올 수 있습니다([동적 UI](./index.md#동적-ui와-ai-생성-ui)).
- **개발자와 에이전트가 같은 도구 사용.** 하나의 `nsh` 실행 파일로 모든 대상 플랫폼의 앱을 생성하고 실행하며, 실제 플랫폼에서 테스트하고 패키징과 서명까지 수행합니다.

### React Native

목표가 가장 가까운 것은 React Native입니다. 두 프레임워크 모두 클라이언트 개발의 중복을 줄이고 플랫폼 뷰를 사용합니다. React Native는 React와 그 패키지 생태계에서 JavaScript 또는 TypeScript로 iOS와 Android 앱을 개발하는 데 중점을 두며, 데스크톱 지원은 별도 프로젝트가 담당합니다. NSH에서는 Rust가 중심입니다. 런타임, 렌더러, 모듈이 Rust로 구현되고 앱 전체도 Rust로 작성할 수 있습니다. TSX는 두 가지 개발 방식 중 하나입니다. 데스크톱과 웹을 포함한 여섯 플랫폼이 같은 프레임워크의 백엔드입니다.

### Flutter

Flutter는 자체 엔진으로 픽셀을 직접 그리므로 앱이 모든 플랫폼에서 같은 모습으로 표시됩니다. NSH는 플랫폼의 뷰, 텍스트, 텍스트 입력을 사용합니다. 웹에서는 HTML 요소와 브라우저의 스크롤, 선택, 입력을 사용합니다. 텍스트 편집, 입력기, 선택 기능도 각 플랫폼이 제공합니다.

### Qt

Qt는 Qt Widgets와 Qt Quick이라는 두 UI 기술을 갖춘 성숙한 툴킷으로, 여러 플랫폼과 개발 도구를 오랫동안 지원해 왔습니다. NSH는 Rust를 중심으로 TSX도 사용할 수 있고, 자동화와 AI를 활용하는 개발에 맞춰 도구를 설계했습니다. 웹에서는 HTML을 렌더링하고, 모바일·데스크톱·웹의 상호작용을 하나의 컴포넌트 모델 안에서 조정합니다.

### GPUI와 GPUI Fast

[GPUI](https://gpui-kit.com)는 GPU로 직접 그리는 Rust UI 프레임워크로, 높은 성능을 요구하는 데스크톱 앱을 위해 만들어졌습니다. Longbridge의 포크인 [GPUI Fast](https://github.com/longbridge/gpui-fast)는 변경되지 않은 뷰, 레이아웃, 텍스트 측정 결과를 재사용하고 스크롤 레이어와 네이티브 창 합성을 지원합니다. NSH는 각 플랫폼의 뷰를 기반으로 텍스트 입력, 선택, 입력기, 시스템 상호작용을 제공하며 휴대폰과 웹도 지원합니다. NSH의 [GPUI 백엔드](./gpui.md)로 기존 GPUI 및 GPUI Kit 앱에 NSH 앱을 임베딩할 수 있습니다.

두 방식은 잘 맞는 부하가 다릅니다. [NSH 성능 벤치마크](https://github.com/longbridge/nsh-perf-bench)는 Linux에서 500행의 시세표를 사용하며 NSH는 GTK 4 백엔드에서 실행합니다. 초당 시세 1,000건을 처리하는 일반적인 부하에서는 NSH Rust가 CPU 코어 하나의 18%, GPUI Fast가 20%를 사용해 비슷합니다. 고밀도 갱신이 지속되면 GPUI Fast가 앞섭니다. 초당 시세 20,000건에서 GPUI Fast는 코어 하나의 25%, NSH는 73~78%를 사용합니다. 상자 1,000개를 매 프레임 애니메이션으로 움직이면 GPUI Fast의 CPU 사용량은 NSH의 약 3분의 1입니다.

### Slint

Slint는 고유 언어와 경량 렌더러를 갖춘 선언형 UI 툴킷으로, 데스크톱과 임베디드 기기에서 Rust, C++, JavaScript, Python과 함께 사용합니다. 두 프레임워크 모두 네이티브 성능을 중시합니다. NSH는 각 플랫폼의 UI와 상호작용, 웹의 HTML, 앱 개발 언어로서의 Rust와 TSX, 여섯 플랫폼을 아우르는 AI 기반 개발을 결합합니다.

### Electron과 Tauri

Electron은 Chromium을 앱과 함께 배포하고 Tauri는 시스템 웹 뷰를 사용합니다. 둘 다 웹 콘텐츠와 브라우저의 텍스트·입력 기능으로 데스크톱 앱을 만들며 Tauri로는 모바일 앱도 개발할 수 있습니다. NSH는 각 플랫폼의 뷰로 화면을 구성하고 웹에서만 HTML을 렌더링합니다.

## 출시 후 UI 배포

초기 앱에 없던 UI를 나중에 불러와야 할 때가 있습니다. 모델이 생성한 답변, 파트너나 다른 팀이 제공하는 기능, 앱을 새로 출시하지 않고 바뀌는 화면 등이 그 예입니다. 휴대폰과 데스크톱에서 선택할 수 있는 방식은 화면을 누가 그리는지, 배포된 코드가 무엇을 할 수 있는지, 호스트 앱을 얼마나 수정해야 하는지에서 차이가 납니다.

| 기능 | NSH | 웹 뷰 | 서버 주도 UI | 미니 프로그램 | OTA 업데이트 |
|---|---|---|---|---|---|
| 렌더링 | 플랫폼 UI | 브라우저 엔진 | 앱의 뷰 | 웹 엔진 + 추가 계층 | 프레임워크 렌더러 |
| 네이티브 외형과 동작 | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> |
| 독립적인 상태, 로직, 레이아웃 | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> |
| 호스트의 네이티브 컴포넌트 | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> 브리지 | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--yes" role="img" aria-label="지원"></span> |
| 호스트가 설정하는 샌드박스 | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> 동일 출처 정책 | <span class="status status--yes" role="img" aria-label="지원"></span> 스키마 | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> |
| 서명과 폐기가 가능한 패키지 | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> |
| AI 생성 UI에 적합 | <span class="status status--yes" role="img" aria-label="지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> 웹 페이지 | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--partial" role="img" aria-label="부분 지원"></span> | <span class="status status--no" role="img" aria-label="미지원"></span> |
| 호스트 앱이 준비할 것 | 뷰 하나와 라이브러리 | 뷰마다 브라우저 엔진 | 컴포넌트마다 렌더러 | 컨테이너 SDK | 앱 전체를 해당 프레임워크로 작성 |

- **웹 뷰(H5).** 브라우저 엔진으로 페이지를 표시합니다. HTML로 표현할 수 있는 내용을 브라우저의 모습으로 렌더링하고, 네이티브 코드와의 연동에는 직접 작성한 브리지를 사용합니다.
- **서버 주도 UI.** JSON으로 앱에 이미 들어 있는 컴포넌트와 동작을 조합합니다. 생성하기 쉽고, 스키마가 정의한 범위에서 로컬 상태와 로직을 사용합니다.
- **미니 프로그램 컨테이너.** 큰 컨테이너 SDK 안에서 웹 엔진 위에 플랫폼 전용 프레임워크와 API를 제공합니다.
- **OTA 업데이트.** 코드 푸시 서비스를 연동한 React Native가 한 예입니다. 프레임워크의 모든 기능을 쓸 수 있지만 업데이트는 신뢰하는 앱 코드로 실행되며 앱 자체를 해당 프레임워크로 작성해야 합니다.
- **NSH.** 자체 상태, 로직, 애니메이션, 레이아웃을 갖춘 TSX 모듈을 플랫폼 고유 UI로 렌더링하며, 호스트가 등록한 네이티브 뷰도 사용할 수 있습니다. 앱이 서명된 패키지를 검증한 다음 독립된 VM에서 메모리, 호출 시간, 노드 수에 한도를 두고 실행합니다. 모듈은 호스트가 허용한 네트워크 호스트, 웹 페이지, 시스템 설정에만 접근할 수 있습니다([샌드박스와 권한](./sandbox-and-capabilities.md)). 호스트는 모듈을 표시할 위치에 뷰 하나를 추가하고([플랫폼](./platforms.md#네이티브-앱에-임베드하기)), 같은 번들이 모든 플랫폼에서 실행됩니다. 이것이 [GPUI Shell](./index.md#native-shell과-gpui-shell)과 별도로 NSH를 개발한 이유입니다.

## 다른 방식이 더 잘 맞는 경우

- 수천 개의 뷰를 높은 빈도로 계속 갱신하는 화면이라면 GPUI Fast의 유지 모드 GPU 렌더링으로 CPU 사용량을 크게 줄일 수 있습니다. 초당 수만 건의 시세를 스트리밍하는 금융 단말이나 프레임마다 다시 그리는 맞춤 시각화가 그 예입니다([GPUI와 GPUI Fast](#gpui와-gpui-fast)).
- 모든 플랫폼에서 픽셀 단위까지 같은 화면이 필요하면 Flutter나 Slint처럼 전체 화면을 자체 렌더러로 그리는 프레임워크가 맞습니다.
- 당장 폭넓은 서드파티 패키지 생태계가 필요하면 React Native, Flutter, Qt에는 오랜 기간 축적된 패키지가 있습니다.
- UI가 문서나 기존 웹 페이지라면 웹 뷰에 그대로 표시할 수 있습니다.
- 화면 종류가 적고 고정되어 있으며 내용과 순서만 바뀐다면 서버 주도 UI로 별도의 런타임 없이 구현할 수 있습니다.

## 관련 문서

- [소개](./index.md) — NSH의 역할과 NSH에서 네이티브가 뜻하는 것
- [아키텍처](../../docs/architecture.md) — NSH 내부의 각 부분이 실행되는 위치
- [성능](../../docs/performance.md) — NSH의 성능 원칙과 앱 측정 방법
