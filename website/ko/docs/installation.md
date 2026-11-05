---
title: 설치
description: 배포판 또는 소스에서 Native Shell CLI를 설치하고 iOS, Android, macOS, Windows, Linux 개발에 필요한 도구를 준비합니다.
order: 3
group: start
---

# 설치

Native Shell에서는 `nsh` 명령 하나로 Rust 또는 TSX 앱을 빌드하고, 각 플랫폼에서 실행하고, 개발용 호스트에 TSX 변경 사항을 즉시 반영하고, 설치 가능한 앱으로 패키징할 수 있습니다. 미리 빌드한 배포판을 설치하거나 저장소의 소스에서 직접 빌드하세요.

Rust 앱에는 [Rust 툴체인](#rust-앱-준비)도 필요합니다. TSX 앱의 언어 도구는 `nsh`에 모두 들어 있습니다. 이와 별도로 각 플랫폼에 필요한 도구는 다음과 같습니다.

| 대상 플랫폼 | 개발 및 패키징 환경 | 필요한 도구와 조건 |
|---|---|---|
| iOS | macOS | Xcode 및 iOS 시뮬레이터 런타임 |
| Android | macOS, Windows 또는 Linux | Android SDK, JDK, 에뮬레이터 또는 실제 기기 |
| macOS | macOS | Xcode 명령줄 도구. 앱 실행에는 macOS 14 이상 필요 |
| Windows | Windows | Windows 10 1809 이상. 웹 뷰에는 WebView2 런타임 필요 |
| Linux | Linux | GTK 4.14 이상. 런타임을 빌드하려면 개발 패키지도 필요 |
| [GPUI](./gpui.md) (GPUI 앱에 임베딩) | macOS, Windows 또는 Linux | GPU(Metal, DirectX 또는 Vulkan). Linux에서는 Wayland 또는 X11 필요 |

macOS의 기본 호스트는 AppKit입니다. Mac Catalyst(`--target catalyst`)에는 iOS와 마찬가지로 Xcode가 필요합니다. 자세한 내용은 [플랫폼별 도구](#플랫폼별-도구)를 참고하세요.

## 릴리스 설치

[GitHub 릴리스](https://github.com/longbridge/nsh/releases)마다 macOS(Apple Silicon), Linux x86_64, Windows x86_64용 배포판을 제공합니다. 어떤 방법으로 설치하든 `PATH`에서 같은 `nsh` 명령을 실행할 수 있습니다.

**macOS와 Linux**에서는 설치 스크립트를 실행합니다.

```sh
curl -fsSL https://nsh.dev/install.sh | sh
```

스크립트는 배포판을 `~/.nsh`에 설치하고 `~/.local/bin/nsh`에서 실행할 수 있도록 링크합니다. 다시 실행하면 업데이트됩니다. `NSH_VERSION=v0.1.0`처럼 버전을 고정하거나 `NSH_INSTALL_DIR`로 설치 디렉터리를 지정할 수 있습니다.

**Windows**에서는 PowerShell에서 실행합니다.

```powershell
irm https://nsh.dev/install.ps1 | iex
```

`%LOCALAPPDATA%\nsh`에 설치하고 그 아래의 `bin`을 사용자 `PATH`에 추가합니다. 설치한 뒤 새 터미널을 여세요. 버전과 설치 경로는 위와 같이 `$env:NSH_VERSION`과 `$env:NSH_INSTALL_DIR`로 지정합니다.

**Homebrew**(macOS, Linux)

```sh
brew install longbridge/tap/nsh
```

**Scoop**(Windows)

```powershell
scoop install https://github.com/longbridge/nsh/releases/latest/download/nsh.json
```

**mise**(세 운영체제 모두)

```sh
mise use -g github:longbridge/nsh
```

아카이브 이름(`nsh-macos-arm64.tar.gz`, `nsh-linux-x86_64.tar.gz`, `nsh-windows-x86_64.zip`)에는 버전 번호가 없으므로 `https://github.com/longbridge/nsh/releases/latest/download/<archive>`에서 항상 최신 버전을 받을 수 있습니다. 각 파일에는 `.sha256` 체크섬이 함께 제공됩니다. CLI가 필요 없는 네이티브 앱을 위해 임베딩 SDK도 `nsh-embed-ios.zip`과 `nsh-embed-android.zip`으로 별도 배포합니다.

> [!NOTE]
> 바이너리는 아직 서명되지 않았습니다. macOS는 브라우저로 받은 파일에 격리 속성을 설정합니다. 설치 스크립트, Homebrew, mise는 `curl`로 받으므로 영향을 받지 않습니다. 그 외의 경우 `xattr -dr com.apple.quarantine <dir>`로 속성을 제거하세요. Linux 런타임에는 glibc 2.39 이상과 GTK 4.14 이상이 필요합니다(Ubuntu 24.04 이상). Intel Mac과 arm64 Linux·Windows에서는 소스에서 직접 빌드하세요.

## Rust 앱 준비

[rustup](https://rustup.rs)으로 Rust를 설치하세요. 개발용으로는 [huacnlee/cargo](#소스에서-빌드)를 권장합니다. `nsh`는 Cargo로 타깃별 Rust 앱 실행기를 빌드합니다. 이때 앱 크레이트와 CLI 버전에 맞는 crates.io의 NSH 크레이트를 사용하므로, 실행하려는 플랫폼의 Rust 타깃을 추가하세요.

```sh
rustup target add aarch64-apple-ios-sim aarch64-apple-ios aarch64-apple-ios-macabi   # iOS simulator, devices, Mac Catalyst
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android # Android (with the SDK's NDK)
rustup target add wasm32-unknown-unknown                                             # the web
```

데스크톱 타깃은 현재 머신에 맞춰 빌드합니다. Rust 앱 디렉터리에서 `cargo run`을 실행하면 데스크톱 백엔드로 앱을 바로 시작합니다. 빌드에는 해당 백엔드의 [개발 라이브러리](#플랫폼별-도구)가 필요합니다. Android 빌드에는 [Android 런타임을 빌드할 때](#android-런타임-빌드)와 마찬가지로 SDK의 **NDK**도 필요합니다.

## 배포판 구성

배포판 디렉터리 `dist/nsh`에는 CLI와 앱 패키징에 사용하는 플랫폼별 네이티브 런타임이 들어 있습니다.

```sh
dist/nsh/bin/nsh                                             # the CLI
dist/nsh/runtime/ios/iphoneos/nsh-ios-app                    # runs packaged apps on iOS devices
dist/nsh/runtime/ios/iphonesimulator/{nsh-ios-app,nsh-ios-host}
dist/nsh/runtime/ios/maccatalyst/nsh-ios-host                # the Mac Catalyst development host
dist/nsh/runtime/android/{release,dev}/                      # classes.dex, lib/<abi>/libnsh.so, symbols/<abi>/libnsh.so
dist/nsh/runtime/macos/{nsh-macos-host,nsh-macos-app}        # the macOS (AppKit) development host and release runner
dist/nsh/runtime/linux/{nsh-linux-host,nsh-linux-app}        # the Linux development host and release runner
dist/nsh/runtime/windows/{nsh-windows-host,nsh-windows-app}.exe  # the Windows development host and release runner
dist/nsh/runtime/gpui/nsh-gpui-host[.exe]                    # the GPUI development host, for this OS
dist/nsh/runtime/web/{nsh.wasm,nsh-dev.wasm,nsh.js}          # the web runtimes (nsh package / run --target web)
dist/nsh/embed/ios/{NativeShell,NativeShellDev}.xcframework  # embedding in iOS apps
dist/nsh/embed/android/{nsh,nsh-dev}.aar                     # embedding in Android apps
```

`dist/nsh/bin`을 `PATH`에 추가하거나 `dist/nsh/bin/nsh`를 직접 실행하세요. 디렉터리 구조는 그대로 유지해야 합니다. `nsh`는 실행 파일을 기준으로 `bin/../runtime`에서 런타임을 찾습니다. 설치 프로그램이 만든 심볼릭 링크에서 실행해도 같은 경로를 찾습니다.

```sh
export PATH="$PWD/dist/nsh/bin:$PATH"
nsh --version
```

소스 체크아웃에서 `cargo xtask dist`를 실행하면 현재 머신용 `nsh` 바이너리가 들어 있는 배포판이 생성됩니다. Xcode가 설치된 Mac에서는 iOS와 macOS(Apple Silicon) 런타임, Linux에서는 해당 머신용 Linux 런타임, Windows에서는 Windows 런타임과 `bin\nsh.exe`가 포함됩니다. 모든 배포판에는 해당 머신용 [GPUI](./gpui.md) 개발 호스트와 Android 런타임이 포함됩니다. 단, Windows에서는 Android SDK를 찾은 경우에만 Android 런타임이 들어갑니다.

## 최신 소스 설치

macOS와 Linux에서는 Git, Rust, 아래의 플랫폼별 빌드 도구를 준비한 뒤 최신 `main` 브랜치를 설치할 수 있습니다.

```sh
curl -fsSL https://nsh.dev/install.sh | sh -s -- --source
nsh -V # nsh 0.1.1 (abcdef0)
```

설치 프로그램은 체크아웃을 `~/.nsh/source`에 보관하고 릴리스 모드로 CLI를 빌드한 뒤 `~/.local/bin`에 링크합니다. 업데이트할 때는 로컬 소스 변경 사항을 먼저 저장한 다음 같은 명령을 실행하세요. NSH는 보관된 소스에서 필요할 때 네이티브 런타임을 빌드합니다. Cargo는 기존 설정과 타깃 디렉터리를 사용합니다. `NSH_INSTALL_DIR`로 설치 디렉터리를 바꿀 수 있습니다. `NSH_VERSION`은 미리 빌드한 릴리스를 선택할 때만 적용됩니다. `-V`와 `--version`은 모두 버전과 소스 리비전의 앞 일곱 글자를 표시합니다.

## 소스에서 빌드

[rustup](https://rustup.rs)으로 Rust를 설치하세요. 저장소의 `rust-toolchain.toml`이 stable 툴체인을 선택하고 처음 사용할 때 iOS 및 Android 타깃을 설치합니다.

개발에는 Cargo를 그대로 대체할 수 있는 포크인 [huacnlee/cargo](https://github.com/huacnlee/cargo)를 권장합니다. 별도 설정 없이 Git 워크트리들이 빌드 디렉터리를 공유하고, 디렉터리 크기도 자동으로 제한됩니다. 이 포크는 `~/.cargo/bin/cargo`를 대체하며 `rust-toolchain.toml`과 `cargo +nightly`를 지원하고 자체적으로 업데이트됩니다. rustup의 Cargo로 되돌리는 방법은 포크의 README에 나와 있습니다.

```sh
curl -fsSL https://huacnlee.github.io/cargo/install.sh | sh     # Windows: irm https://huacnlee.github.io/cargo/install.ps1 | iex
```

```sh
git clone https://github.com/longbridge/nsh.git
cd nsh
cargo build -p nsh-cli          # ./target/debug/nsh
```

그런 다음 실행할 타깃별로 개발 호스트를 한 번씩 빌드하세요. 체크아웃을 업데이트했을 때도 다시 빌드합니다.

```sh
cargo build -p nsh-ios-host --target aarch64-apple-ios-macabi   # nsh run (Mac Catalyst window)
cargo build -p nsh-ios-host --target aarch64-apple-ios-sim      # nsh run --target ios
cargo xtask android --dev                                       # nsh run --target android
cargo build -p nsh-macos-host                                   # nsh run --target macos (built for you in a checkout)
cargo build -p nsh-linux-host                                   # nsh run on Linux (built for you in a checkout)
cargo build -p nsh-windows-host                                 # nsh run on Windows (built for you in a checkout)
cargo build --manifest-path crates/gpui/Cargo.toml --target-dir target -p nsh-gpui-host   # nsh run --target gpui (built for you in a checkout)
cargo build --profile web -p nsh-platform-web --target wasm32-unknown-unknown   # nsh run --target web (built for you in a checkout)
```

`nsh package`를 사용하려면 릴리스용 실행기도 빌드하세요.

```sh
cargo build -p nsh-ios-app --target aarch64-apple-ios-sim       # nsh package --target ios-simulator
cargo build -p nsh-ios-app --target aarch64-apple-ios           # nsh package --target ios
cargo xtask android                                             # nsh package --target android
cargo build -p nsh-macos-app                                    # nsh package --target macos
cargo build -p nsh-linux-app                                    # nsh package --target linux
cargo build -p nsh-windows-app                                  # nsh package --target windows
# nsh package --target web uses the same web runtime as nsh run --target web
```

소스에서 빌드한 CLI는 자신의 프로필에 맞는 런타임을 `target/<triple>/<profile>`과 `target/android/<profile>[-dev]`에서 찾습니다. 현재 머신용 macOS, Linux, Windows, GPUI 런타임은 `target/<profile>`에 있습니다. `./target/debug/nsh`는 디버그 런타임을, `./target/release/nsh`는 릴리스 런타임을 사용합니다. CLI와 런타임은 같은 프로필로 빌드하세요. 앱 실행 속도를 높이려면 모든 빌드 명령에 `--release`를 붙이세요.

> [!TIP]
> `NSH_RUNTIME=/path/to/runtime`으로 배포판의 `runtime/`과 구조가 같은 디렉터리를 지정하면 CLI 위치와 관계없이 그곳의 런타임을 사용합니다.

## 플랫폼별 도구

### iOS와 Mac Catalyst

- 명령줄 도구가 포함된 **Xcode**가 필요합니다. `nsh run`은 `xcrun simctl`로 시뮬레이터를 시작하고 개발 호스트를 설치합니다. `nsh package`는 `actool`로 앱 아이콘을 처리하고 `codesign`으로 서명합니다.
- iOS 시뮬레이터 런타임이 필요합니다. 설치하려면 `xcodebuild -downloadPlatform iOS`를 실행하세요.
- 앱의 최소 지원 버전은 iOS 15.0입니다.

### Android

- **Android SDK**는 `ANDROID_HOME`, `ANDROID_SDK_ROOT` 또는 Android Studio의 기본 경로(macOS의 `~/Library/Android/sdk`, Windows의 `%LOCALAPPDATA%\Android\Sdk`)에서 찾습니다. 다음 항목을 설치하세요.
  - `platforms;android-35`와 최신 `build-tools`. NSH는 Gradle 없이 여기에 들어 있는 `aapt2`, `zipalign`, `apksigner`를 사용합니다.
  - `platform-tools`(`adb`).
- **JDK**가 필요합니다. 처음 패키징할 때 `keytool`이 `~/.android/debug.keystore`에 디버그 키 저장소를 만들고, `apksigner`는 Java로 실행됩니다.
- 실행 중인 에뮬레이터 또는 `adb`로 연결한 기기가 필요합니다. 앱은 Android 9(API 28) 이상에서 실행됩니다.

```sh
sdkmanager "platforms;android-35" "build-tools;35.0.1" "platform-tools"
```

### macOS

- **Xcode 명령줄 도구**(`xcode-select --install`)가 필요합니다. SDK와 clang으로 런타임을 빌드하고 `codesign`과 `sips`로 앱을 패키징합니다.
- 앱은 macOS 14 이상에서 실행됩니다.

자세한 내용은 [macOS](../../docs/macos.md)를 참고하세요.

### Linux

- 런타임 빌드에는 **GTK 4.14** 이상과 개발 패키지, clang이 필요합니다. Debian 및 Ubuntu 24.04 이상에서는 `libgtk-4-dev clang`, Arch에서는 `gtk4 clang`, Fedora에서는 `gtk4-devel clang`을 설치하세요.
- 웹 뷰에는 **WebKitGTK 6.0**(버전 2.40 이상)이 필요합니다. 런타임을 빌드하려면 Debian 및 Ubuntu에서 `libwebkitgtk-6.0-dev`, Arch에서 `webkitgtk-6.0`, Fedora에서 `webkitgtk6.0-devel`을 설치하세요. 앱을 실행하는 환경에도 WebKitGTK가 필요합니다. 빌드 환경에 이 라이브러리가 없으면 `cargo xtask dist`는 웹 뷰 자리에 자리표시자를 넣은 런타임을 만듭니다.
- 앱에서 재생할 동영상 형식을 지원하는 **GStreamer** 플러그인이 필요합니다.

자세한 내용은 [Linux](../../docs/linux.md)를 참고하세요.

### Windows

- **Windows 10 1809** 이상 또는 Windows 11이 필요합니다. 웹 뷰에는 WebView2 런타임이 필요합니다. Windows 11에는 기본으로 포함되며, Windows 10에서는 Edge와 함께 설치됩니다.
- 런타임을 빌드하려면 **Visual Studio Build Tools**의 “Desktop development with C++” 워크로드와 Windows SDK, **LLVM**이 필요합니다. QuickJS 바인딩에 쓰이는 libclang은 `C:\Program Files\LLVM`에서 찾습니다. 다른 위치에 설치했다면 `LIBCLANG_PATH`를 설정하세요.

자세한 내용은 [Windows](../../docs/windows.md)를 참고하세요.

### Android 런타임 빌드

소스에서 Android 런타임을 빌드하려면(`cargo xtask android`) SDK의 **NDK**(`ndk;<version>`)와 `javac`가 추가로 필요합니다. 설치된 최신 NDK가 불완전하면 `ANDROID_NDK_HOME`으로 사용할 NDK를 지정하세요.

## 호스트별 지원 명령

| 명령 | macOS | Linux | Windows |
|---|---|---|---|
| `nsh init`, `nsh build`, `nsh test`, `nsh dev`, `nsh check` | 지원 | 지원 | 지원 |
| `nsh run --target catalyst` (Mac Catalyst) | 지원 | 미지원 | 미지원 |
| `nsh run --target macos`, `nsh package --target macos` | 지원 | 미지원 | 미지원 |
| `nsh run --target linux` (Linux 기본값), `nsh package --target linux` | 미지원 | 지원 | 미지원 |
| `nsh run --target windows` (Windows 기본값), `nsh package --target windows` | 미지원 | 미지원 | 지원 |
| `nsh run --target gpui` (GPUI 앱 임베딩용 백엔드) | 지원 | 지원 | 지원 |
| `nsh run --target ios`, `nsh package --target ios` / `ios-simulator` | Xcode 필요 | 미지원 | 미지원 |
| `nsh run --target android`, `nsh package --target android` | 지원 | 소스 빌드 필요 | Android 런타임 필요 |
| `nsh keygen`, `certify`, `publish`, `verify`, `revoke` | 지원 | 지원 | 지원 |

Linux에서는 소스에서 CLI와 Android 런타임을 빌드하세요(`cargo build -p nsh-cli` 및 `cargo xtask android --dev`). NDK의 Linux 툴체인이 자동으로 선택됩니다. Windows에서는 같은 방식으로 NDK의 Windows 툴체인이 선택되며, `cargo xtask dist`가 Android SDK를 찾으면 배포판에 Android 런타임도 포함합니다.

## 편집기 설정

Rust 앱은 Cargo 프로젝트입니다. VS Code, Zed, Neovim, RustRover에서 rust-analyzer를 사용하면 다른 크레이트와 마찬가지로 코드를 검사하고 문서 주석과 함께 NSH API 자동 완성을 제공합니다.

TSX 개발에 npm은 필요하지 않습니다. `nsh init`이 `tsconfig.json`과 NSH 전체 타입 선언(`.nsh/types`)을 생성하고, 앱을 빌드하는 모든 명령이 선언을 최신 상태로 유지합니다. TypeScript 언어 서버를 사용하는 편집기(VS Code, Zed, Neovim, WebStorm)는 이 선언으로 앱의 타입을 검사합니다. [프로젝트 구조](./project-structure.md#tsconfigjson과-nshtypes)를 참고하세요.

## 다음 단계

[시작하기](./getting-started.md)에서 앱을 만들고 실행해 보세요.
