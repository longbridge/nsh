---
title: GPUI
description: GPUI 또는 GPUI Kit 앱에 AppView로 Rust와 TSX 앱을 임베딩하고, macOS·Windows·Linux에서 GPUI로 그립니다. TSX 앱을 개발 호스트에서 실행하는 방법과 GPUI 백엔드의 동작 차이도 설명합니다.
order: 15.5
group: platforms
---

# GPUI

Native Shell의 GPUI 백엔드는 macOS, Windows, Linux에서 [GPUI](https://gpui-kit.com)(GPUI Fast)로 NSH 앱을 그립니다. GPUI는 GPUI Kit의 기반이 되는 GPU 가속 UI 프레임워크입니다. 기존 GPUI 또는 GPUI Kit 앱의 요소 트리 어디에나 GPUI 뷰인 `AppView`를 배치하고, 여기에 앱의 Rust 루트 컴포넌트나 동적으로 불러온 TSX를 표시할 수 있습니다. 독립 실행형 앱은 [macOS](../../docs/macos.md)의 AppKit, [Windows](../../docs/windows.md)의 Win32, [Linux](../../docs/linux.md)의 GTK 백엔드로 실행하고 패키징합니다. 코어, 반응성 그래프, 렌더러, Taffy 레이아웃, 네이티브 모듈을 다른 플랫폼과 공유하므로 같은 앱이 모든 플랫폼에서 동작합니다. `os()`(TSX에서는 `platform.os`)는 실행 중인 운영체제(macOS, Windows 또는 Linux)를 반환하며, TSX의 `desktop={{…}}` 속성이 적용됩니다.

[macOS](../../docs/macos.md)(AppKit), [Windows](../../docs/windows.md)(Win32), [Linux](../../docs/linux.md)(GTK) 백엔드의 모든 요소와 모듈을 제공합니다. GPUI가 다르게 그리는 항목은 [백엔드별 차이](#백엔드별-차이)에서 확인할 수 있습니다.

## 동작 방식

- **그리기 영역마다 요소 하나를 사용합니다.** 부모가 할당한 공간을 하나의 GPUI 요소가 채우고, 사각형, 그림자, 경로, 글리프, 이미지 등 GPUI의 기본 요소로 뷰 트리 전체를 그립니다. Windows 백엔드처럼 NSH가 노드마다 뷰 하나를 유지하고, 커밋으로 변경 사항이 생길 때만 다시 그립니다.
- **엔진 스레드에서 글리프를 배치합니다.** GPUI의 텍스트 시스템이 앱의 글꼴로 텍스트 셰이핑을 수행하고, NSH가 줄을 나눕니다. 메인 스레드는 글리프 그리기만 맡습니다.
- **커밋마다 메인 스레드로 한 번만 전달합니다.** 엔진은 별도 스레드에서 실행됩니다. 커밋 하나를 클로저 하나로 GPUI 메인 스레드에 보내 뷰에 변경을 알립니다.
- **입력은 GPUI 이벤트를 통해 받습니다.** 마우스, 스크롤, 키, 입력기 이벤트를 처리합니다. 입력 대상 판정, 누르기, 리스폰더 시스템, 스크롤, 텍스트 편집에는 Windows 백엔드와 같은 로직을 사용합니다.

## 실행 환경

- macOS, Windows 또는 Linux와 GPUI가 사용할 수 있는 GPU가 필요합니다. macOS는 Metal, Windows는 DirectX, Linux는 Vulkan을 사용합니다.
- macOS에서 소스 체크아웃으로 GPUI 앱이나 런타임을 빌드하면 Xcode의 Metal Toolchain(`xcodebuild -downloadComponent MetalToolchain`)이 GPUI 셰이더를 미리 컴파일합니다. Metal Toolchain이 없는 Mac에서는 `nsh run`이 GPUI의 `runtime-shaders` 기능으로 런타임을 빌드하고 이 사실을 알립니다. 이때 셰이더는 앱을 처음 시작할 때 1초 이내에 컴파일되며 이후에는 macOS의 캐시를 사용합니다.
- Linux에는 Vulkan 드라이버, `libxkbcommon`과 함께 사용할 Wayland 또는 X11, 웹 뷰용 GTK 3 및 WebKitGTK 4.1, 동영상용 GStreamer 1.20 이상과 base 및 good 플러그인이 필요합니다. 웹 뷰에는 X11이 필요하므로 Wayland 데스크톱에서는 `WAYLAND_DISPLAY=`로 값을 비우고 Xwayland에서 앱을 실행하세요. 그렇지 않으면 웹 뷰 자리에 대체 화면이 표시됩니다. 체크아웃에서 런타임을 빌드하려면 개발용 패키지도 필요합니다. Debian과 Ubuntu에서는 다음과 같이 설치합니다.

  ```sh
  sudo apt-get install clang libclang-dev pkg-config libfontconfig-dev libfreetype-dev libwayland-dev libx11-xcb-dev \
    libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libzstd-dev libvulkan1 libgtk-3-dev libwebkit2gtk-4.1-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-good
  ```

## 앱 실행

Rust 앱은 GPUI 앱 안에서 실행합니다. [GPUI 앱에 임베딩](#gpui-앱에-임베딩)에 따라 Rust 루트를 `AppView`에 연결한 뒤 GPUI 앱에서 `cargo run`을 실행하세요. 앱과 크레이트가 하나의 Cargo 빌드로 컴파일됩니다. Rust 앱을 독립 실행할 때는 운영체제의 자체 백엔드를 사용합니다(`nsh run --target macos`, `windows`, `linux`).

TSX 앱은 GPUI 개발 호스트에서 실행할 수 있습니다.

```sh
nsh run App.tsx --target gpui
```

앱이 기본 크기 1200×800인 GPUI 창에 열립니다. 실제 GPUI 앱에 임베딩한 것과 같은 화면을 보면서 다른 대상 플랫폼처럼 핫 리로드, 터미널 로그, 오류 오버레이를 사용할 수 있습니다. 개발 호스트(`nsh-gpui-host`)는 모든 운영체제에서 별도의 앱 번들 없이 실행됩니다. 소스 체크아웃에서는 `nsh run`이 호스트를 먼저 빌드합니다. 배포판에서는 `runtime/gpui/`에 들어 있습니다. 빌드한 번들을 호스트에서 직접 실행할 수도 있습니다.

```sh
nsh build my-app --release -o build
dist/nsh/runtime/gpui/nsh-gpui-host --bundle build/bundle.nshc --window-size 390x844   # in a checkout: target/debug/nsh-gpui-host
```

`--window-size`는 창 크기를 포인트 단위의 `<width>x<height>`로 지정합니다.

## 패키징

GPUI 앱은 [GPUI 앱에 임베딩](#gpui-앱에-임베딩)에 따라 NSH 앱의 Rust 루트 또는 TSX 번들을 `AppView`로 임베딩하고, NSH를 자체 실행 파일에 포함해 평소처럼 GPUI 앱으로 빌드하고 배포합니다. 독립 실행형 NSH 앱은 운영체제에 해당하는 [`--target macos`](../../docs/macos.md#packaging), [`--target windows`](../../docs/windows.md#packaging) 또는 [`--target linux`](../../docs/linux.md#packaging)로 [패키징](../../docs/packaging.md)합니다.

Linux에서 웹 뷰와 동영상을 사용하는 GPUI 앱은 WebKitGTK 4.1과 GStreamer를 동적으로 링크합니다. 실행할 컴퓨터에도 Vulkan 드라이버와 `libxkbcommon` 외에 GTK 3, WebKitGTK 4.1, GStreamer 및 base·good 플러그인을 설치하세요. Debian과 Ubuntu의 관련 패키지는 `libwebkit2gtk-4.1-0`, `libgstreamer1.0-0`, `libgstreamer-plugins-base1.0-0`, `gstreamer1.0-plugins-good`입니다.

## GPUI 앱에 임베딩

NSH 저장소의 `nsh-platform-gpui` 크레이트는 Rust GPUI 앱에 `AppView`를 제공합니다. `AppView` 하나는 NSH 엔진 하나를 담은 GPUI 뷰(`Entity<AppView>`)로, 부모 요소가 할당한 공간을 채웁니다. 각 뷰가 독립적으로 동작하므로 여러 개를 동시에 실행할 수 있습니다. 다음은 앱 크레이트의 Rust 루트 컴포넌트를 실행하는 예입니다.

```rust
use gpui::{AppContext, WindowOptions};
use nsh_platform_gpui::{set_ui_sets, App, AppOptions, AppView, Root};

// Once, before the first view: the UI sets the apps import (NSH's and your crate's).
set_ui_sets(vec![nsh_base::registry(), my_app::registry()]);
gpui_platform::application().run(|cx| {
    cx.open_window(WindowOptions::default(), |window, cx| {
        cx.new(|cx| {
            let view = AppView::new(AppOptions::default(), window, cx);
            view.load(App::Rust(Root::of::<my_app::App>()));
            view
        })
    })
    .unwrap();
});
```

`Root::tokens`로 루트에 [테마](./rust.md#테마)를 지정합니다(`nsh_base::Theme::from_json(json)?.into_tokens()`). `Theme`는 이 크레이트에서도 다시 내보냅니다. `Root::app_id`는 데이터 저장에 사용하는 앱 ID를 지정합니다. `nsh build`로 만든 TSX 번들도 같은 뷰에 불러올 수 있습니다.

```rust
use nsh_platform_gpui::{App, Bundle};

let code = std::fs::read_to_string("app/bundle.js").expect("bundle");
view.load(App::Js(Bundle::source(code)));
```

다른 GPUI 뷰를 배치하듯 이 엔티티를 요소 트리에 넣으세요. GPUI Kit의 `Root`, 패널, 도크에도 넣을 수 있습니다. `AppView::load`는 실행 중인 앱을 새 앱으로 교체합니다. `stop`을 호출하거나 뷰를 드롭하면 앱이 종료됩니다. `AppOptions::asset_base`는 `asset:/` URI가 가리키는 위치(`AssetBase::Directory` 또는 `AssetBase::Http`)를 지정합니다. 바이트코드 번들(`bundle.nshc`)은 `unsafe { Bundle::bytecode(bytes) }` 또는 마찬가지로 unsafe인 `Bundle::read_trusted(path)`로 불러옵니다. QuickJS는 바이트코드를 신뢰하므로 앱이 직접 관리하는 파일에서만 불러오세요.

- `open_app_window(source, options, cx)`는 실행 중인 GPUI 앱에서 `AppView` 하나를 담은 창을 엽니다.
- `run_app(source, options)`는 GPUI를 시작하고 창을 닫을 때까지 그 안에서 앱 하나를 실행합니다. 개발 호스트도 이 방식을 사용합니다.
- `AppView::register_component(name, factory)`는 앱을 불러오기 전에 호스트의 [네이티브 컴포넌트](../../docs/native-components.md)를 등록합니다.
- `AppOptions::cache`는 앱의 캐시 위치와 [다운로드한 이미지](../../docs/images-and-assets.md#kept-on-disk)의 보관 용량을 설정합니다. 실행 중인 뷰에서 다음에 불러올 앱에 적용하려면 `AppView::set_cache`를 사용하세요. `CacheOptions { location, images }`에서 위치는 `CacheLocation::System`(기본값), `Dir(path)`, `Off` 중 하나를, 용량 정책은 `CachePolicy::Default`(256 MiB), `MaxSize(ByteSize::mib(n))`, `Off` 중 하나를 선택합니다.
- `AppView::host()`는 뷰와 그 안의 앱이 소통하는 채널([`host`](../../modules/nsh.md#host))입니다. 어느 스레드에서나 이 `HostChannel`을 사용할 수 있습니다. 앱을 불러오기 전에 `set_params(map)`으로 시작 매개변수(`host.params`)를 설정하고, `send(name, data)`로 이벤트(`host.on`)를 보냅니다. `handle(name, |call| …)`은 앱의 `host.call(name, data)`에 응답하고 `host.post`를 받습니다. `call.resolve(json)`과 `call.reject(message)`는 어느 스레드에서든 즉시 또는 나중에 호출할 수 있습니다. 핸들러는 엔진 스레드에서 실행되므로, `cx.spawn` 작업이 읽는 채널을 통해 GPUI 엔티티로 호출을 전달하고 그곳에서 응답하세요. macOS, Windows, Linux의 `AppView`도 같은 `host()`를 제공합니다. 채널의 `handle_open_url(|request| …)`은 앱이 여는 [URL](../../modules/nsh.md#openurl)을 시스템보다 먼저 받습니다. 호스트 앱이 URL을 직접 처리했으면 `request.handled()`를 호출하고, GPUI의 `open_url`로 시스템에 넘기려면 `request.decline()`을 호출하거나 요청을 드롭하세요.
- `AppOptions::limits`는 JS 힙, 스택, 실행 시간, 노드 수 등 [VM의 한도](./sandbox-and-capabilities.md)를 지정합니다. 다음에 불러올 앱에 적용하려면 `AppView::set_limits`를 사용하세요. 임베딩된 앱은 힙이 256 MB인 `Limits::default()`를 사용합니다. 앱을 단독으로 실행하는 개발 호스트는 `Limits::standalone()`을 사용합니다.
- `AppOptions::transparent`를 켜면 뷰가 창 배경을 그리지 않고, `open_app_window`와 `run_app`이 투명한 창을 엽니다. 앱이 그리지 않은 영역에는 창 뒤의 내용이 비치므로 데스크톱 위젯이나 오버레이를 만들 수 있습니다. 기본값은 `false`입니다.
- `AppView::connect_dev_server(url, cx)`(`dev` 기능)는 `nsh dev`에서 앱을 불러오고 코드를 편집하면 다시 로드합니다. 오류 오버레이가 뷰 위에 표시되며 `AppView::on_error`로도 계속 오류를 받을 수 있습니다.
- `AppView::on_app_error(|error| …)`는 앱 오류를 전달합니다. `error.kind`는 앱 로드 실패, 처리되지 않은 예외, VM 중단 등의 종류를 나타냅니다. `error.is_fatal()`은 앱이 더 이상 JS를 실행할 수 없을 때 참입니다. `error.message`, `error.stack`, `error.package`(오류가 발생한 서명 패키지의 ID와 빌드 정보)도 확인할 수 있습니다. `AppView::on_error`로는 같은 오류를 텍스트로 받습니다.
- `AppView::set_locale(locale, cx)`와 `AppView::set_font_scale(scale, cx)`는 시스템 설정 대신 호스트 앱의 언어와 글자 크기를 적용합니다. `None`을 넘기면 다시 시스템 설정을 따릅니다. `locale()`, `Intl`, `fontScale()`과 화면의 텍스트에 즉시 반영됩니다([기기 및 환경](../../docs/device-and-environment.md#in-an-embedding-app)).
- `AppView::run_package(&package)`는 `verify_package` 또는 `load_package`로 얻은 [서명 패키지](../../docs/signed-packages.md#loading-packages-in-a-host-app)를 실행합니다. 호스트 앱에서 패키지가 정상임을 확인한 뒤 `accept_package`로 기록합니다.
- `set_ui_sets(sets)`는 모든 엔진에서 제공할 UI 세트를 등록합니다. NSH의 세트(`nsh_base::registry()`)와 `#[native_module]`이 생성한 앱 크레이트의 세트(`my_app::registry()`, [Rust로 앱 개발](./rust.md))를 함께 등록할 수 있습니다. 앱은 [`nativeModule`](../../modules/nsh.md#nativemodule)을 통해 이름으로 세트의 함수와 클래스에 접근하고 기본 `nsh build`로 빌드합니다. 저장소의 `examples/embed-gpui`에서는 이 방식으로 호스트가 거래 데스크를 관리하고, 임베딩된 주문 화면 두 개와 호스트 자체 위젯이 이를 공유합니다.

## 백엔드별 차이

GPUI와 운영체제별 네이티브 데스크톱 백엔드는 다음 항목의 렌더링이나 동작이 다릅니다.

- **변환.** 뷰를 회전하거나 기울이거나 투영하면 배경, 테두리, 그림자에 변환이 적용됩니다. 텍스트는 똑바로 표시되며 이미지, 동영상, 캔버스는 각 요소의 경계에 맞춰 그립니다.
- **불투명도.** 그룹 전체가 아닌 각 색상에 적용되므로 반투명 뷰에서 자식 요소가 겹치면 서로 비쳐 보입니다. 이미지의 불투명도(페이드인, 반투명 뷰)는 1/16 단위로 변합니다.
- **둥근 모서리로 자르기.** 반지름과 `overflow: "hidden"`을 지정하면 자식의 배경과 이미지는 둥근 모서리에 맞춰 잘립니다. 텍스트, 테두리, 동영상, 네이티브 컴포넌트는 직사각형 경계에서 잘립니다.
- **그리기 효과.** `filter: blur()`, `backdropFilter`, `mixBlendMode`는 그리지 않습니다. 반투명 채움 너머로 그림자가 보이며, `borderCurve: "continuous"`는 원호 모서리로 표시합니다.
- **캔버스 텍스트.** 회전해도 글자가 똑바로 표시됩니다. `maxWidth`는 글자 폭을 줄이는 대신 글자 전체를 축소하고, 그라디언트는 단색으로 표시됩니다. `strokeText`는 글자의 내부를 채웁니다. 글자 그림자는 흐려지지 않으며 합성 연산과 `clearRect`도 텍스트에는 적용되지 않습니다.
- **스크롤 뷰.** `zoomTo`와 확대·축소 속성이 적용되지 않습니다.
- **동영상.** Linux에서는 GStreamer로 재생합니다. macOS와 Windows에서는 대체 화면을 표시하고 `onError`를 전달합니다.
- **웹 뷰.** 지정한 위치에서 NSH가 그리는 모든 항목 위에 네이티브 뷰로 표시됩니다. Linux에서는 [실행 환경](#실행-환경)에 설명한 대로 X11이 필요합니다. Xwayland에서 페이지가 검게 표시되고 로그에 “Failed to create GBM buffer”가 나오면 `WEBKIT_DISABLE_DMABUF_RENDERER=1`을 설정해 실행하세요. macOS와 Windows에서는 상위 요소의 클리핑이 웹 뷰에 적용되지 않으며, 로드가 실패해도 `onError`가 전달되지 않습니다. `baseUrl`, `scrollable`, `allowsBackForwardNavigationGestures`는 지원하지 않습니다.
- **접근성.** AccessKit을 사용합니다. 모든 플랫폼에서 스크린 리더가 요소를 활성화하거나 포커스하고, 값을 설정하거나 스크롤해 화면에 표시할 수 있습니다. 사용자 정의 동작과 컨텍스트 메뉴 동작은 전달되지 않습니다.

운영체제별 네이티브 백엔드와 비교하려면 [macOS](../../docs/macos.md#what-differs-on-macos), [Windows](../../docs/windows.md#what-differs-on-windows), [Linux](../../docs/linux.md#what-differs-on-linux) 문서를 참고하세요.

## 관련 문서

- [Rust로 앱 개발](./rust.md) — GPUI 앱에 루트 컴포넌트를 임베딩할 크레이트
- [CLI](./cli.md) — `nsh run --target gpui`
- [패키징](../../docs/packaging.md) — 운영체제별 대상 플랫폼을 사용하는 독립 실행형 앱
- [플랫폼 개요](./platforms.md)
