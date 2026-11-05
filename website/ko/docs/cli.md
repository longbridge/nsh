---
title: CLI
description: "앱 생성·빌드·검사·실행·패키징과 서명·검증·폐기에 사용하는 nsh 하위 명령 및 옵션을 설명합니다."
order: 6
group: start
---

# CLI

`nsh`는 Rust 또는 TSX로 작성한 NSH 앱을 빌드하는 도구입니다. 앱을 빌드하는 모든 하위 명령은 `[ENTRY]`로 앱 디렉터리나 TSX 진입 모듈을 받습니다. 디렉터리를 지정하면 Rust 앱에서는 `nsh.json`의 `"root"` 컴포넌트를, TSX 앱에서는 `App.tsx`를 실행합니다. 기본값은 현재 디렉터리이므로 `cd my-app && nsh run`으로 시작할 수 있습니다. CLI는 앱 루트의 `nsh.json`을 읽고 `.nsh/types`의 타입 선언을 갱신합니다.

```text
nsh <COMMAND>

  init         Create an app in Rust or TSX
  build        Build an app: describe a Rust app's crates, or bundle a TSX app and compile it to QuickJS bytecode
  test         Run an app's tests headless, and its Rust crates' with cargo test
  check        Type check an app with TypeScript's native compiler, and its Rust crates with cargo check
  dev          Serve an app to dev hosts (simulators, devices) and push rebuilds
  run          Like dev, and also run the app in a development host
  package      Package an installable app from the prebuilt runtime
  keygen       Create a signing key
  certify      Certify a release key with a root key
  publish      Build an app in release mode and sign it into a package (.nshpkg)
  verify       Verify a package
  symbolicate  Map the positions in a reported error stack to the app's sources
  revoke       Sign a revocation list
```

`nsh <command> --help`는 해당 명령의 옵션을 출력합니다. `nsh -V` 또는 `nsh --version`은 버전과 빌드에 사용한 Git 리비전을 출력합니다.

## 출력

모든 명령은 단계마다 결과를 한 줄씩 출력합니다. 자세한 내용은 실패한 단계에 표시됩니다.

```text
✓ built the Rust crate gallery-native (5.0s)
✓ built App.tsx (500 B js, 809 B bytecode, 21ms)
✓ built the development host for the iOS simulator (12.7s)
• iOS Simulator (iPhone 16 Plus) connected, build 1
warn iOS Simulator: image failed: An SSL error has occurred and a secure connection to the server cannot be made.
✓ rebuilt App.tsx (18ms), 1 device
```

`✓`는 완료된 단계, `→`는 접속할 주소, `•`는 기기에서 발생한 동작을 뜻합니다. 문제가 생기면 `warn` 또는 `error`로 시작하는 줄이 표시되며, 별도 표시가 없는 줄은 앱이 직접 출력한 내용(`console.log`)입니다. 터미널에서는 실행 중인 단계에 진행 표시기가 나타나고 색상으로 출력을 구분합니다(`NO_COLOR`를 설정하면 해제). 파일로 리디렉션하면 색상 없이 결과 줄만 기록됩니다. 앱 크레이트를 빌드하는 Cargo나 개발용 호스트 등 하위 도구가 실패하면 해당 도구의 출력이 `error` 줄 아래 표시됩니다.

모든 명령에 `-v` 또는 `--verbose`를 추가하거나 `NSH_VERBOSE=1`을 설정하면 각 로그의 수준과 출처, Cargo와 호스트의 출력을 실시간으로 확인할 수 있습니다.

## nsh init

```sh
nsh init [DIR] [--name <NAME>] [--template <ts|rust|mixed>] [--nsh-source <DIR>]
```

템플릿으로 `DIR`에 앱을 만듭니다. 기본값은 현재 디렉터리이며, 지정한 디렉터리가 없으면 생성합니다. `--template`을 생략하면 터미널에서는 템플릿을 선택하라는 메시지가 나옵니다. Enter를 누르면 `ts`를 선택하며 스크립트나 CI에서도 `ts`를 기본으로 사용합니다.

| 템플릿 | 생성 내용 |
|---|---|
| `rust` | Cargo 프로젝트(`Cargo.toml`, 루트 컴포넌트가 있는 `src/lib.rs`, `cargo run`용 `src/main.rs`)를 생성합니다. `nsh.json`에는 `"rust": "."` 및 `"root": "App"`을 지정합니다. 루트 컴포넌트 테스트 `App.test.tsx`, `tsconfig.json`, `.nsh/types`, `.gitignore`, TSX 진입점 추가 방법을 설명하는 README도 생성합니다. `nsh run`과 `nsh package`는 브라우저를 포함한 모든 대상에서 QuickJS 없이 루트를 마운트하며, `nsh test`는 TypeScript 테스트에서 루트를 엽니다. `cargo run`은 macOS·Windows·Linux에서 직접 실행합니다. [Rust로 앱 만들기](./rust.md) |
| `ts` | TSX 앱에 필요한 `App.tsx`, `nsh test`로 실행할 `App.test.tsx`, `nsh.json`, `tsconfig.json`, `.gitignore`, `.nsh/types`를 생성합니다. 기존 파일은 유지하며, 모든 파일이 이미 있을 때만 실패합니다. |
| `mixed` | 루트에 Rust 크레이트를 포함한 TSX 앱을 생성합니다. `App.tsx`, `App.test.tsx`, `tsconfig.json`, `"rust": "."`를 지정한 `nsh.json`, `.gitignore`, `.nsh/types`, README가 포함됩니다. `App.tsx`와 같은 위치에 크레이트(`Cargo.toml`, `src/lib.rs`)가 있고 `#[native_module("<name>/native")]`가 `App.tsx`에서 사용할 컴포넌트, 모듈, 클래스를 공개합니다. `"root"`가 없으므로 진입점은 `App.tsx`입니다. |

| 옵션 | 설명 |
|---|---|
| `--name <NAME>` | `nsh.json`에 기록할 표시 이름입니다. 기본값은 디렉터리 이름입니다. ID는 소문자 영문과 숫자만 사용해 `com.example.<name>`으로 만들고 크레이트의 모듈 지정자는 소문자 단어를 `-`로 연결해 `<name>/native`로 만듭니다. |
| `--template <TEMPLATE>` | 위의 `rust`, `ts`, `mixed` 중 하나입니다. `rust`와 `mixed`에는 Rust가 필요합니다. `cargo run`에는 데스크톱 백엔드의 개발 라이브러리도 필요합니다. |
| `--nsh-source <DIR>` | `rust` 또는 `mixed`에서 `DIR`에 체크아웃한 NSH 크레이트를 경로 의존성으로 사용하여 해당 소스로 앱을 빌드합니다. |

`rust`와 `mixed`의 크레이트는 기본적으로 crates.io에서 CLI와 버전이 같은 NSH 크레이트에 의존합니다(`--nsh-source` 지정 시 해당 체크아웃을 사용). `nsh`는 같은 크레이트로 앱 실행기와 호스트를 빌드합니다. 앱에 명령을 실행하기 전 앱 크레이트와 NSH 코어로 컴포넌트, 모듈, 클래스를 설명하는 작은 프로그램을 빌드하고, 크레이트가 변경되었을 때 실행합니다. CLI는 이 설명을 바탕으로 직접 번들링, 컴파일, 타입 검사, 개발 서버 제공 및 패키징을 수행합니다. Rust 앱을 빌드할 때는 앱 크레이트와 NSH 코어만 컴파일합니다. 두 템플릿은 새 앱 디렉터리에 생성하세요. 대상 파일 중 하나라도 이미 있으면 파일을 쓰기 전에 충돌을 보고합니다.

Rust 크레이트가 Git을 통해 NSH에 의존하는 앱은 Cargo가 앱의 잠금 파일에서 결정한 체크아웃을 사용합니다. 생성된 호스트도 이 체크아웃을 공유하며, 앱의 Cargo 패치로 경로를 바꾼 NSH 의존성도 그대로 사용합니다.

## nsh run

```sh
nsh run [ENTRY] [--target catalyst|ios|android|macos|linux|windows|gpui|tui|web] [--device <DEVICE>] [--port <PORT>] [--release] [--limits [<LIMITS>]] [--trace] [--fps] [--name <NAME>] [--points-per-cell <WIDTHxHEIGHT>] [--window-size <WIDTHxHEIGHT>] [--display <N>]
```

```sh
nsh run my-app                     # a Rust app's root, or a TSX app's App.tsx, on this machine's backend
nsh run my-app --target ios        # in the iOS simulator
nsh run my-app/App.tsx --target web
```

TSX 진입점에서 `nsh run`은 `nsh dev`처럼 개발 서버를 시작하고, 서버에서 앱을 불러오는 개발용 호스트를 실행한 뒤 저장할 때마다 다시 빌드합니다. 핫 모듈 교체(HMR)로 실행 중인 앱에 변경 사항을 반영하며 앱 로그와 오류는 터미널에 표시합니다. Ctrl-C를 누르거나 Catalyst·macOS·Linux·Windows·GPUI 창을 닫으면 종료됩니다. Ctrl-C, `kill`·`pkill`, 오류 등 종료 원인에 관계없이 명령이 시작한 호스트도 종료합니다. 여기에는 데스크톱 호스트와 시뮬레이터 또는 Android 기기의 개발용 호스트가 포함됩니다.

네이티브 창과 모바일 대상에서 [Rust 앱](./rust.md)(`nsh.json`의 `"root"` 지정)은 개발 서버와 QuickJS 없이 실행됩니다. `nsh run`은 루트 컴포넌트를 마운트하는 대상별 실행기를 빌드합니다. 데스크톱에서는 디버그 빌드를 실행하고 로그를 터미널에 표시하며, 시뮬레이터와 Android에는 앱을 설치해 실행합니다. Rust 코드를 수정하면 다시 빌드하고 재시작합니다. `--target web`은 페이지 주소를 출력하며 열려 있는 페이지가 새 런타임을 자동으로 로드합니다. TSX 진입점을 대신 실행하려면 `nsh run App.tsx`로 지정하세요.

| 옵션 | 기본값 | 설명 |
|---|---|---|
| `--target <TARGET>` | macOS는 `macos`, Linux는 `linux`, Windows는 `windows` | `catalyst`는 iPhone 크기의 Mac Catalyst 창(390×844), `ios`는 iOS 시뮬레이터, `android`는 Android 기기나 에뮬레이터입니다. [macOS](../../docs/macos.md)의 `macos`는 AppKit 창(1200×800), [Linux](../../docs/linux.md)의 `linux`는 GTK 창(390×844), [Windows](../../docs/windows.md)의 `windows`는 Win32 창(1200×800)입니다. [GPUI](./gpui.md)의 `gpui`는 macOS·Windows·Linux에서 GPUI 창(1200×800)을 열어 GPUI 앱에 내장했을 때와 같은 방식으로 TSX 앱을 표시합니다. `tui`는 현재 [터미널](../../docs/tui.md), `web`은 개발 서버가 주소를 출력하는 [웹 페이지](../../docs/web.md#develop-in-the-browser)입니다. |
| `--device <DEVICE>` | | iOS 시뮬레이터를 이름(`"iPhone 16 Pro"`)이나 UDID로 선택합니다. 기본적으로 이미 부팅한 시뮬레이터를 사용하며, 없으면 가장 최신 iPhone 시뮬레이터를 부팅합니다. |
| `--port <PORT>` | `8790` | 개발 서버 포트입니다. 같은 포트를 사용하던 이전 NSH 개발 세션이 정리를 마치면 새 세션으로 교체합니다. |
| `--release` | | 코드를 축소하고 디버그 정보를 제거합니다. 릴리스 번들에는 HMR이 없어 저장할 때마다 앱을 다시 로드합니다. |
| `--limits [<LIMITS>]` | | TSX 앱을 호스트에 내장한 것처럼 리소스 제한을 적용해 실행합니다. `memory`와 `stack`은 `KB`·`MB`·`GB`, `load`와 `call`은 `ms`·`s` 단위이며 `nodes`는 노드 수입니다. 예: `--limits memory=128MB,call=500ms`. 생략한 항목은 기본 제한을 유지하며 `--limits`만 주면 모두 기본 제한을 사용합니다. 이 옵션이 없으면 패키징한 독립 실행형 앱처럼 동작합니다. [리소스 제한](./sandbox-and-capabilities.md#리소스-제한) |
| `--trace` | | 호스트가 각 커밋의 소요 시간(JS, 레이아웃, 마운트 항목)을 터미널에 기록합니다. |
| `--fps` | | 성능 모니터에 프레임 속도, 프레임·커밋 소요 시간, CPU·GPU·메모리 사용량을 표시합니다. [macOS](../../docs/macos.md#performance-monitor)에서는 앱 위에 표시되며, macOS·Linux·Windows·iOS에서는 초당 한 번 로그로 기록합니다. `tui`에서는 모서리 한 줄에 FPS/유휴 상태, 프레임 p95, 적용·그리기·플러시 소요 시간, 바이트 수, 변경된 셀 수를 표시합니다. `C-h p`로 켜거나 끕니다. |
| `--name <NAME>` | `nsh.json`의 name | 메뉴 막대의 앱 표시 이름과 초기 창 제목입니다(TSX 진입점, `--target macos`). |
| `--points-per-cell <WIDTHxHEIGHT>` | `nsh.json`의 `tui.pointsPerCell`, 없으면 `8x16` | 터미널 셀의 너비와 높이입니다. 단위는 포인트입니다(`--target tui`). |
| `--window-size <WIDTHxHEIGHT>` | 호스트 기본값 | 창의 초기 크기를 포인트 단위로 지정합니다(예: `800x600`). TSX 진입점의 `--target macos`, `linux`, `windows`, `gpui`에서 사용합니다. |
| `--display <N>` | 주 디스플레이 | macOS의 TSX 앱 창을 어느 디스플레이 가운데에 열지 정합니다. `1`은 메뉴 막대가 있는 주 디스플레이, `2`는 그다음 디스플레이입니다. 없는 번호를 지정하면 주 디스플레이에 엽니다. |

`--target android`는 `adb devices` 목록의 첫 번째 기기를 사용하고, `adb reverse`로 개발 서버 포트를 기기에 포워딩합니다. [시작하기](./getting-started.md#실행)와 [개발 서버 및 디버깅](../../docs/dev-server-and-debugging.md)을 참고하세요.

기존 세션을 교체할 때는 정리를 마칠 때까지 최대 10초 기다립니다. 다른 프로그램이나 이전 버전의 NSH CLI가 포트를 점유하면 포트 번호와 함께 오류를 표시합니다. 앱 여러 개를 동시에 실행하려면 서로 다른 `--port` 값을 지정하세요. `--port 0`은 운영체제에서 빈 포트를 할당받습니다.

`--target tui`는 현재 터미널에서 앱을 실행하고 저장 시 핫 업데이트합니다. CLI와 호스트 진단 로그는 앱의 `.nsh/tui.log`에 기록합니다. `C-h e`로 로그·오류 패널을 열고 Escape나 `C-g`로 닫으며 `r`로 다시 로드합니다. 앱 오류가 발생하면 패널이 자동으로 열리고 업데이트되면 앱 화면으로 돌아옵니다. 기본 셀 크기는 8×16포인트입니다. `--points-per-cell 10x20`은 `nsh.json`의 `"tui": { "pointsPerCell": "10x20" }`보다 우선합니다. 두 차원 모두 유한한 양수여야 하며 실행을 시작할 때 적용됩니다. Rust 앱은 생성된 터미널 개발용 호스트에서 루트를 실행하며 Rust 코드 변경 시 다시 빌드하고 재시작합니다.

Unix에서 터미널 호스트는 SIGHUP·SIGTERM·SIGINT 신호 또는 터미널 연결 해제를 감지하면 접근 가능한 터미널 설정을 복원하고 종료합니다. 앱이 유휴 상태면 입력 확인 사이에 대기합니다. 대화형 실행은 실제 터미널 입력을 사용하며 `--headless`에서는 테스트용으로 리디렉션한 입력을 받습니다.

## nsh dev

```sh
nsh dev [ENTRY] [--target catalyst|ios|android|macos|linux|windows|gpui|tui|web] [--device <DEVICE>] [--server-only] [--port <PORT>] [--release] [--limits [<LIMITS>]]
```

개발 서버를 시작하고 현재 시스템의 네이티브 호스트(macOS는 AppKit, Linux는 GTK, Windows는 Win32)를 엽니다. 모바일 개발은 `--target ios` 또는 `--target android`를 명시하세요. 호스트를 직접 시작하려면 `--server-only`를 사용합니다. 서버는 앱을 빌드하고 진입점 디렉터리의 변경을 감시하며, 다시 빌드할 때마다 연결된 호스트에 전송합니다. `--server-only`는 같은 컴퓨터에서 접속할 URL과 기기용 LAN 주소별 URL을 출력합니다. 웹 런타임이 있으면 브라우저에서 앱을 실행할 [페이지](../../docs/web.md#develop-in-the-browser)도 표시합니다.

```text
✓ built App.tsx (3.2 KB js, 9.8 KB bytecode, 21ms)
→ dev server ws://127.0.0.1:8790/ws
  browsers: http://127.0.0.1:8790/
  devices on this network: ws://192.168.1.20:8790/ws
  browsers on this network: http://192.168.1.20:8790/
```

개발용 SDK(`NativeShellDev.xcframework`, `nsh-dev.aar`)를 내장한 앱에서는 `nsh dev --server-only`를 사용하세요. `--target`, `--device`, `--port`, `--release`, `--limits`는 `nsh run`과 같습니다. `--server-only`는 호스트를 열지 않고 서버만 시작합니다.

## nsh test

```sh
nsh test [PATH] [--filter <NAME>] [--trace] [--fps] [--junit <FILE>] [--target <TARGET>] [--browser <PATH>] [--web-host <JS>] [--display <N>]
```

앱의 [테스트](../../docs/testing-and-trace.md)를 창 없이 실행합니다. `PATH`로 테스트 파일 하나 또는 디렉터리(기본값은 현재 디렉터리)를 지정합니다. 디렉터리에서는 `nsh/test`를 가져오는 `*.test.tsx`, `*.test.ts`를 찾으며 숨김 디렉터리, `node_modules`, `target`은 건너뜁니다. 자체 Rust 크레이트가 있는 앱 디렉터리(`nsh.json`의 `"rust"` 지정)라면 먼저 각 크레이트의 디렉터리에서 `cargo test`를 실행합니다. Rust 테스트는 `src/lib.rs › tests::adds`처럼 테스트 대상 파일 아래에 표시하며 TypeScript 테스트와 함께 집계합니다. Rust 테스트만 있는 앱도 같은 방식으로 테스트합니다.

각 테스트 파일은 개별 엔진에서 실행되며, 테스트가 여는 앱마다 새 엔진을 사용합니다. 테스트마다 결과를 한 줄씩 출력하고, 실패 시 예외와 테스트 파일 내 위치, 당시 화면의 텍스트를 표시합니다. 하나라도 실패하면 0이 아닌 종료 코드를 반환합니다. `nsh test App.tsx` 또는 Rust 앱의 `nsh test nsh.json`처럼 진입점을 주면 네트워크 접근이 가능한 상태로 앱을 열고 1.2초 동안 실행한 뒤 표준 입력으로 받은 동작(`await app.press("Add")` 등)을 수행합니다. 여기서 `app`은 열린 앱입니다. TSX 앱은 CLI 프로세스 안에서 실행하고, 자체 Rust 크레이트가 있는 앱은 해당 크레이트로 빌드한 헤드리스 실행기에서 CLI가 빌드한 진입점을 실행합니다. 헤드리스 실행은 테스트의 `open`에서 `platform`을 Android로 지정하지 않는 한 iOS처럼 동작합니다. `platform.os`는 `"ios"`이며 `ios:` 클래스와 `ios={{…}}` 속성을 적용합니다.

```text
✓ App.test.tsx › counts taps (321ms)
✓ 1 test: 1 passed, 0 failed (343ms)
```

| 옵션 | 설명 |
|---|---|
| `--filter <NAME>` | 상위 `describe` 블록의 이름을 포함한 전체 테스트 이름에 `NAME`이 들어가는 항목만 실행합니다. Rust 크레이트에는 `cargo test <NAME>`을 사용합니다. |
| `--trace` | 각 앱의 마운트 배치를 해당 동작 아래(`── press(Add): 7 items` 등)에 출력하고 모든 커밋 시간을 표시합니다. 첫 프레임, 개별 동작, 스크롤 한 프레임의 비용을 확인할 수 있습니다. `--target` 지정 시 호스트에서 측정한 배치와 시간을 사용합니다. |
| `--fps` | `--target linux`, `macos`, `windows`, `ios`의 호스트가 초당 한 번 프레임 속도·단계별 시간·커밋·CPU·메모리를 로그에 기록합니다. `-v`로 표시합니다. |
| `--junit <FILE>` | `FILE`에도 JUnit XML 형식으로 결과를 저장합니다. CI 서비스에서 표시하는 보고서 형식으로 테스트 파일마다 `testsuite`, 테스트마다 `testcase`를 만듭니다. |
| `--target <TARGET>` | 테스트가 여는 앱을 지정한 플랫폼의 개발용 호스트에서 실행합니다. `macos`, `linux`, `windows`, `gpui`, `tui`, `web`, `ios`, `android`를 지원합니다. `tui`는 메모리 내 터미널, `web`은 헤드리스 Chrome·Edge·Chromium을 사용합니다. [실제 플랫폼 테스트](../../docs/testing-and-trace.md#on-a-real-platform)는 플랫폼 고유 입력을 사용합니다. `open`은 해당 환경에서 지원할 수 없는 옵션(iOS와 Android의 `width`·`height`, 다른 기기 종류의 `platform`)을 `ERR_NSH_TEST_UNSUPPORTED`로 거부합니다. 크레이트의 Rust 테스트는 `--target`을 생략할 때만 현재 컴퓨터에서 실행합니다. |
| `--browser <PATH>` | `--target web`에서 사용할 Chrome, Edge 또는 Chromium 실행 파일입니다. 기본값은 현재 컴퓨터에 설치된 브라우저입니다. |
| `--display <N>` | `--target macos`에서 앱 창을 가운데에 열 디스플레이입니다. `1`은 메뉴 막대가 있는 주 디스플레이, `2`는 그다음입니다. 번호에 해당하는 디스플레이가 없으면 주 디스플레이에 엽니다. 기본값은 미러링하지 않는 첫 보조 디스플레이이므로 테스트 화면을 따로 둘 수 있습니다. 화면이 하나면 주 디스플레이에 엽니다. 테스트 동작은 화면에 관계없이 같습니다. |
| `--web-host <JS>` | 헤드리스 앱에 제공할 웹 페이지의 호스트 모듈입니다. 기본값은 `nsh.json`의 `web.host`이며 `createHost(player)`를 공개합니다. 이 모듈은 앱의 [`host`](../../modules/nsh.md#host) 호출에 응답하고 `player.hostEvent`를 통해 `host.on`으로 이벤트를 보냅니다. 이 옵션은 헤드리스 실행에만 적용되며 `--target web`에서는 `nsh.json`의 `web.host`를 실행합니다. [페이지의 호스트 테스트](../../docs/testing-and-trace.md#the-pages-host) |

## nsh check

```sh
nsh check [ENTRY] [--tsc <TSC>] [-- <TSC ARGS>...]
```

앱의 타입을 검사합니다. `nsh.json`에 `"rust"`가 있으면 먼저 각 Rust 크레이트의 디렉터리에서 `cargo check --all-targets`를 실행해 테스트까지 검사합니다. Cargo 오류가 발생하면 0이 아닌 종료 코드를 반환합니다. `"root"`를 지정한 Rust 앱에서도 `tsconfig.json`이 있으면 TypeScript 파일을 검사합니다. 여기에는 `nsh init --template rust`가 생성한 설정과 `App.test.tsx`도 포함됩니다.

TypeScript 검사 시 `.nsh/types`와 `.nsh/tsconfig.json`을 갱신하고 앱의 `tsconfig.json`으로 `tsc -p <app directory> --noEmit`을 실행합니다. Node 없이 실행되는 네이티브 컴파일러 TypeScript 7을 사용합니다. 처음 실행하면 npm 레지스트리에서 약 9 MB를 다운로드해 NSH에 기록된 체크섬과 대조한 뒤 캐시에 보관합니다. 기본 경로는 macOS에서 `~/Library/Caches/nsh`, Linux에서 `~/.cache/nsh`, Windows에서 `%LOCALAPPDATA%\nsh`이며 `NSH_CACHE`로 바꿀 수 있습니다. 타입 오류가 있으면 0이 아닌 종료 코드를 반환합니다. `tsconfig.json`에서 NSH의 TypeScript 설정인 `./.nsh/tsconfig.json`을 확장하지 않으면 경고를 표시합니다. [프로젝트 구조](./project-structure.md#tsconfigjson과-nshtypes)

| 옵션 | 설명 |
|---|---|
| `--tsc <TSC>` | 지정한 `tsc`를 대신 실행합니다(`NSH_TSC`로도 지정 가능). 예를 들어 `node_modules`에 설치했거나 오프라인 컴퓨터에 복사해 둔 컴파일러를 사용할 수 있습니다. |
| `-- <TSC ARGS>` | 뒤의 인수를 `tsc`에 전달합니다. 예: `nsh check -- --pretty false`. |

타입 선언, 실행기 소스, `.nsh/out/`의 기본 빌드·패키징 결과물 등 NSH가 생성하는 파일은 `.nsh/` 아래에 둡니다. Cargo는 기존 `target/` 캐시와 `CARGO_TARGET_DIR` 설정을 사용하며 NSH가 위치를 바꾸는 Cargo 설정을 추가하지 않습니다. 출력 경로는 `--out`으로 지정할 수 있습니다.

## nsh build

```sh
nsh build [ENTRY] [-o <OUT>] [--release] [--split]
```

앱을 빌드합니다. Rust 앱(`nsh.json`의 `"root"` 지정)은 크레이트를 빌드하고 설명 정보를 생성한 다음 `app.json`과 함께 배포할 파일(`assets/`)을 작성합니다. 앱 실행기가 루트 컴포넌트를 마운트합니다. TSX 앱은 번들링한 뒤 QuickJS 바이트코드로 컴파일합니다. NSH를 내장한 호스트 앱에서 이 번들을 직접 배포할 수 있습니다.

| 옵션 | 기본값 | 설명 |
|---|---|---|
| `-o, --out <OUT>` | `.nsh/out` | 출력 디렉터리입니다. |
| `--release` | | 코드를 축소하고 디버그 정보를 제거합니다. |
| `--split` | | `import()`로 가져올 코드를 별도 청크 파일로 분리해 처음 가져올 때 다운로드합니다. 웹 앱에 사용할 수 있습니다. 생략하면 번들 하나에 앱 전체가 들어갑니다. [번들 분할](../../docs/web.md#split-bundles) |

생성되는 파일은 다음과 같습니다.

| 파일 | 내용 |
|---|---|
| `bundle.js` | JavaScript 번들 |
| `bundle.js.map` | 스택 추적을 원본 코드에 대응시키는 소스 맵 |
| `bundle.nshc` | 앱이 로드하는 컴파일된 바이트코드 번들 |
| `<Chunk>.js`, `<Chunk>.js.map`, `<Chunk>.nshc` | `--split` 사용 시 청크와 각 청크의 소스 맵·바이트코드(`import("./Screen")`의 청크는 `Screen.js`) |
| `capabilities.json` | `nsh.json`에 선언한 `capabilities`. 생략하면 이 파일을 제거합니다. |
| `assets/` | 가져온 모든 이미지와 글꼴. 앱 루트를 기준으로 한 상대 경로를 유지합니다. |

## nsh package

```sh
nsh package [ENTRY] --target ios|ios-simulator|android|macos|linux|windows|web [-o <OUT>] [flags]
```

릴리스 모드에서 설치 가능한 앱을 빌드합니다. [Rust 앱](./rust.md)(`nsh.json`의 `"root"` 지정)은 앱 크레이트와 NSH 크레이트로 Cargo가 빌드한 자체 실행기에서 QuickJS 없이 루트를 마운트합니다. TSX 앱은 미리 빌드된 런타임을 사용합니다. NSH가 직접 앱 번들을 구성하므로 Xcode 프로젝트나 Gradle 빌드가 필요하지 않습니다. iOS는 Xcode의 `actool`과 `codesign`, Android는 SDK의 `aapt2`, `zipalign`, `apksigner`를 사용합니다. macOS는 `codesign`으로 애드혹 서명한 `.app`을 만들며 아이콘에는 `sips`를 사용합니다. Linux는 실행기를 복사하고 `tar`로 `.tar.gz`를 만듭니다. Windows는 실행기를 복사하고 아이콘을 삽입한 뒤 별도 도구 없이 `.zip`을 만듭니다. 웹은 별도 도구 없이 정적 사이트를 생성합니다. 메타데이터는 `nsh.json`에서 읽으며 옵션으로 덮어쓸 수 있습니다.

| 옵션 | 설명 |
|---|---|
| `--target <TARGET>` | 필수입니다. `ios`는 iOS 기기용 서명된 `.ipa`(`--sign` 생략 시 서명하지 않은 `.app`), `ios-simulator`는 애드혹 서명한 `.app`, `android`는 서명한 `.apk`입니다. [macOS](../../docs/macos.md#packaging)의 `macos`는 애드혹 서명한 `.app`입니다. [Linux](../../docs/linux.md#packaging)의 `linux`는 실행 파일, `nsh/`, `.desktop` 파일이 있는 `<Name>/`와 `<Name>.tar.gz`를 생성합니다. [Windows](../../docs/windows.md#packaging)의 `windows`는 `<Name>.exe`, `nsh\`가 있는 `<Name>\`와 `<Name>.zip`을 생성합니다. [웹](../../docs/web.md#publish-an-app)의 `web`은 `index.html`과 콘텐츠 기반 파일명을 사용하는 정적 사이트 디렉터리 `<Name>/`를 생성합니다. |
| `-o, --out <OUT>` | 출력 디렉터리입니다. 기본값은 `.nsh/out`입니다. |
| `--name <NAME>` | 앱 표시 이름입니다. |
| `--id <ID>` | 번들 식별자입니다. 예: `com.example.app`. |
| `--app-version <VERSION>` | 버전입니다. 예: `1.2.0`. |
| `--build <BUILD>` | 빌드 번호입니다. |
| `--icon <ICON>` | 정사각형 PNG 앱 아이콘입니다. 권장 크기는 1024×1024입니다. |
| `--sign <IDENTITY>` | iOS 기기용 코드 서명 ID입니다. 예: `"Apple Development: …"`. |
| `--profile <FILE>` | iOS 기기의 프로비저닝 프로파일(`.mobileprovision`)입니다. 앱에 부여하는 권리(entitlements)는 이 파일에서 가져옵니다. |
| `--keystore <FILE>` | Android 릴리스 키 저장소입니다. 기본값은 `~/.android/debug.keystore`의 디버그 키이며 없으면 생성합니다. |
| `--key-alias <ALIAS>` | Android 키 저장소의 키 별칭입니다. |
| `--keystore-password <PASSWORD>` | Android 키 저장소의 암호입니다. `NSH_KEYSTORE_PASSWORD`에서도 읽습니다. |
| `--abi <ABI>` | Android APK에 포함할 ABI(`arm64-v8a`, `armeabi-v7a`, `x86_64`)입니다. 옵션을 반복하거나 쉼표로 구분합니다. 기본값은 `arm64-v8a`와 `armeabi-v7a`이며, `--run` 사용 시 기기의 ABI도 추가합니다. |
| `--run` | 패키징 후 앱을 실행합니다. `ios-simulator`는 시뮬레이터, Android는 연결된 기기에 설치하고 실행합니다. `macos`, `linux`, `windows`는 앱을 열고 `web`은 현재 컴퓨터에서 사이트를 제공하며 접속 주소를 출력합니다. |

```sh
nsh package my-app --target ios-simulator --run
nsh package my-app --target ios --sign "Apple Development: Jane Doe (ABCDE12345)" --profile dev.mobileprovision
nsh package my-app --target android --keystore release.jks --key-alias upload
nsh package my-app --target macos
nsh package my-app --target linux
nsh package my-app --target windows
nsh package my-app --target web
```

자세한 내용은 [패키징](../../docs/packaging.md)을 참고하세요.

## 서명 명령

다음 명령으로 서명된 패키지(`.nshpkg`)를 만들고 검증합니다. NSH를 내장한 앱은 이 번들을 다운로드한 뒤 검증하고 실행합니다. 오프라인 루트 키가 지정된 앱 ID에 대한 릴리스 키를 인증하는 방식은 [서명된 패키지](../../docs/signed-packages.md)에서 설명합니다.

### nsh keygen

```sh
nsh keygen <NAME>
```

Ed25519 키 쌍 `<NAME>.secret`과 `<NAME>.pub`를 생성합니다(예: `nsh keygen keys/root`). 루트 키의 `.secret`은 오프라인으로 보관하며 앱은 `.pub`를 신뢰합니다.

### nsh certify

```sh
nsh certify --root <ROOT.secret> --key <KEY.pub> --apps <PATTERN>... [--days <DAYS>] -o <OUT>
```

지정한 앱 ID와 유효 기간에 사용할 릴리스 키를 루트 키로 인증합니다. 인증서는 패키지에 포함됩니다.

| 옵션 | 설명 |
|---|---|
| `--root <ROOT>` | 루트의 비밀 키입니다. |
| `--key <KEY>` | 릴리스 키의 공개 키입니다. |
| `--apps <APPS>` | 서명할 수 있는 앱 ID입니다. `*`, `com.example.*` 또는 특정 ID를 지정합니다. 반복할 수 있습니다. |
| `--days <DAYS>` | 지금부터 서명을 허용할 일수입니다. 기본값은 365입니다. |
| `-o, --out <OUT>` | 작성할 인증서 파일입니다. |

### nsh publish

```sh
nsh publish [ENTRY] --key <KEY.secret> --cert <CERT> [-o <OUT>] [--id <ID>] [--app-version <V>] [--build <N>]
            [--source-map package|file|none] [--strip-debug] [--min-runtime <VERSION>]
```

릴리스 모드에서 앱을 빌드하고 서명한 패키지를 생성합니다. 메타데이터는 `nsh.json`에서 읽으며 옵션으로 덮어쓸 수 있습니다.

`ENTRY`로 기존 `.nshpkg` 파일을 주면 지정한 키로 다시 서명합니다. 플랫폼이 관리하는 앱의 패키지를 자체 키 하나로 서명할 때 사용할 수 있습니다. 패키지의 파일과 헤더 내용은 유지하고 인증서, 서명 시각, 서명 값은 새 키의 것으로 교체합니다. `-o`에 입력 파일과 같은 경로를 지정할 수 있으며 이 경우 적용되는 옵션은 `--key`, `--cert`, `-o`뿐입니다. [패키지 재서명](../../docs/signed-packages.md#sign-a-package-again)

| 옵션 | 설명 |
|---|---|
| `--key <KEY>` | 릴리스 키의 비밀 키입니다. |
| `--cert <CERT>` | `nsh certify`로 발급한 인증서입니다. |
| `-o, --out <OUT>` | 출력 패키지입니다. 기본값은 `.nsh/out/<id>-<build>.nshpkg`입니다. |
| `--id <ID>` | 앱 ID입니다. |
| `--app-version <VERSION>` | 앱 버전입니다. |
| `--build <BUILD>` | 빌드 번호입니다. 릴리스할 때마다 증가해야 합니다. |
| `--source-map <WHERE>` | 소스 맵 위치입니다. `package`(기본값), `file`(패키지 옆의 `<package>.map`), `none` 중 하나입니다. [배포한 패키지의 오류](../../docs/signed-packages.md#errors-from-published-packages) |
| `--strip-debug` | 바이트코드에서 줄·열 번호 테이블을 제거합니다. 파일 크기가 줄지만 오류 스택에 위치가 표시되지 않습니다. |
| `--min-runtime <VERSION>` | 패키지를 실행할 최소 런타임 버전입니다(예: `0.2.0`). 이보다 오래된 런타임은 실행을 거부합니다. 기본값은 현재 CLI 버전입니다. [패키지 실행 가능 버전](../../docs/signed-packages.md#which-runtimes-run-a-package) |

### nsh verify

```sh
nsh verify <PACKAGE> [--root <ROOT.pub>]... [--revocations <LIST>]
```

지정한 루트를 신뢰하는 앱과 같은 방식으로 패키지를 검증합니다. `--root`가 없으면 패키지 자체의 서명만 검사합니다. 출력하는 패키지의 SHA-256 해시는 `nsh revoke --package`에 사용할 수 있습니다.

### nsh symbolicate

```sh
nsh symbolicate <MAP> [INPUT]
```

패키지가 보고한 오류에 포함된 번들 위치(`bundle.js:LINE:COL`)를 원본 소스의 위치로 바꿔 출력합니다. `MAP`에는 소스 맵(`nsh publish --source-map file`의 `<package>.map` 또는 `bundle.js.map`)이나 소스 맵을 포함한 패키지를 지정합니다. `INPUT`은 오류 텍스트이며 기본값은 표준 입력입니다.

### nsh revoke

```sh
nsh revoke --root <ROOT.secret> [--extend <LIST>] [--sequence <N>] [--key <ID>]... [--package <SHA256>]... -o <OUT>
```

앞으로 로드할 수 없는 릴리스 키 및 패키지를 나열한 폐기 목록에 서명합니다.

| 옵션 | 설명 |
|---|---|
| `--root <ROOT>` | 루트의 비밀 키입니다. |
| `--extend <EXTEND>` | 지정한 목록(직전에 배포한 목록)의 항목을 유지합니다. |
| `--sequence <N>` | 이전에 배포한 모든 목록보다 큰 일련번호입니다. `--extend` 사용 시 기본값은 기존 목록의 번호에 1을 더한 값입니다. |
| `--key <ID>` | 폐기할 릴리스 키 ID입니다. 반복할 수 있습니다. |
| `--package <SHA256>` | 폐기할 패키지의 SHA-256 해시입니다. 반복할 수 있습니다. |
| `-o, --out <OUT>` | 작성할 목록 파일입니다. |

## 환경 변수

| 변수 | 사용처 | 설명 |
|---|---|---|
| `ANDROID_HOME`, `ANDROID_SDK_ROOT` | Android 대상 | Android SDK 경로입니다. 지정하지 않으면 `~/Library/Android/sdk`를 사용합니다. |
| `CARGO_TARGET_DIR` | Rust 크레이트가 있는 앱 | 앱 실행기를 빌드할 디렉터리입니다. 기본값은 앱의 `target/`입니다. |
| `NSH_KEYSTORE_PASSWORD` | `package --target android` | 키 저장소의 암호입니다. |
| `NSH_RUNTIME` | `run`, `package` | CLI와 같은 위치에 있는 런타임 대신 사용할 디렉터리입니다. |
| `NSH_VERBOSE` | 모든 명령 | `1`이면 `--verbose`와 같습니다. |
| `NSH_WASM_BINDGEN` | `run`, `package`의 `--target web` | 앱 크레이트가 [브라우저 API를 호출](../../docs/web.md#browser-apis-from-rust)할 때 쓰는 `wasm-bindgen` CLI입니다. 기본값은 `PATH`에서 찾은 실행 파일입니다. |
| `NO_COLOR` | 모든 명령 | 설정하면 터미널에서 색상 없이 출력합니다. |
