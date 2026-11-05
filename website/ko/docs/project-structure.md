---
title: 프로젝트 구조
description: Rust 또는 TSX로 작성한 Native Shell 앱의 파일 구성, nsh.json의 각 설정, .nsh/types의 TypeScript 선언, 에셋 관리, 모듈과 패키지를 가져오는 방법을 설명합니다.
order: 5
group: start
---

# 프로젝트 구조

Native Shell 앱은 `nsh.json`과 소스 코드가 있는 디렉터리입니다. 소스는 `nsh.json`에서 루트 컴포넌트를 지정하는 Rust 크레이트일 수도, TSX 진입 모듈일 수도 있으며 두 가지를 함께 사용할 수도 있습니다. 추가 모듈, 이미지, 글꼴, JSON 파일은 크레이트의 모듈과 `assets/` 디렉터리 또는 TSX 진입점의 import를 통해 찾으므로 별도의 파일 목록을 관리할 필요가 없습니다.

Rust 앱은 Cargo 프로젝트입니다.

```text
my-app/
├── nsh.json           name, id, version, icon, the crate ("rust") and its root component ("root"), theme
├── Cargo.toml         the crate: nsh-core, nsh-base and your dependencies
├── src/
│   ├── lib.rs         the root component and the crate's UI set (#[native_module])
│   ├── main.rs        cargo run: the root on this machine's desktop backend
│   └── screens/       your own modules (any layout you like)
├── assets/            files the crate ships: asset!("logo@2x.png"), fonts
├── .gitignore         .nsh/ and target/
├── .nsh/              generated: the runners' sources, test entries, build output (.nsh/out)
└── target/            Cargo's build directory
```

TSX 앱은 진입점과 그곳에서 가져오는 모듈로 구성됩니다.

```text
my-app/
├── App.tsx            the entry module: calls mount()
├── App.test.tsx       its tests (nsh test)
├── nsh.json           name, id, version, icon, Tailwind theme, capabilities
├── tsconfig.json      TypeScript settings for editors
├── .gitignore         .nsh/ and target/
├── components/        your own modules (any layout you like)
│   └── Header.tsx
├── assets/
│   ├── logo@2x.png    imported images
│   └── Inter.ttf      imported fonts
└── .nsh/              generated: type declarations, development host builds, test entries, build output (.nsh/out)
```

Rust와 TSX를 함께 쓸 때는 `"rust"`에 크레이트를 나열하고 TSX 진입점에서 가져옵니다([Rust 앱에 TSX 추가](./rust.md#tsx로-rust-앱-확장하기)).

모든 명령은 앱 디렉터리 또는 진입점을 인수로 받습니다(`nsh run my-app`, `nsh run my-app/App.tsx`). `nsh.json`과 진입점이 있는 디렉터리가 앱의 루트입니다. 이곳에서 `nsh.json`을 읽고 에셋의 상대 경로를 해석하며 `.nsh/`를 생성합니다. `nsh.json`에 `"root"`를 지정한 Rust 앱은 디렉터리를 전달하면 루트 컴포넌트를 실행하고, TSX 진입 파일을 지정하면 해당 파일을 실행합니다.

## 루트 컴포넌트

Rust 앱의 루트는 크레이트에 정의한 컴포넌트입니다. `nsh.json`의 `"root"`에 크레이트 내 경로(`App`, `shell::Gallery` 등)를 지정합니다. TSX의 진입점은 마지막에 루트 컴포넌트를 렌더링하는 일반 모듈입니다.

```rust tabs
// src/lib.rs; nsh.json: { "rust": ".", "root": "App" }
mod screens;

use nsh_core::prelude::*;

/// What a TSX entry may import from the crate.
#[native_module("my-app/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>().assets(assets!());
}

#[component]
pub fn App() -> impl IntoElement {
    screens::home()
}
```

```tsx tabs
// App.tsx
import { mount } from "nsh";
import { Home } from "./components/Home";

function App() {
  return <Home />;
}

mount(App);
```

`nsh run`과 `nsh package`는 모든 플랫폼에서 QuickJS 없이 Rust 루트를 마운트합니다. TSX의 `mount(component, props?)`는 컴포넌트를 앱 화면에 렌더링하고, 마운트를 해제하는 함수를 반환합니다. 앱에서는 `mount`를 한 번 호출합니다. 진입 모듈의 최상위 코드는 번들을 불러올 때 첫 프레임이 표시되기 전에 실행됩니다.

## nsh.json

`nsh.json`은 앱 루트에 있습니다. `nsh init`은 `name`, `id`, `version`, `build`를 기록하며, Rust 앱이라면 크레이트와 루트 컴포넌트도 추가합니다.

```json
{
  "name": "Gallery",
  "id": "com.example.gallery",
  "version": "1.0.0",
  "build": 1,
  "rust": ".",
  "root": "App",
  "theme": "theme.json"
}
```

다른 키는 모두 선택 사항입니다. TSX 앱은 앞의 네 키도 생략할 수 있습니다. 다음은 TSX 앱의 `nsh.json` 예입니다.

```json
{
  "name": "Gallery",
  "id": "com.example.gallery",
  "version": "1.0.0",
  "build": 1,
  "icon": "icon.png",
  "tailwind": {
    "colors": { "brand": { "500": "#7c3aed", "600": "#6d28d9" } },
    "radius": { "card": 14 }
  },
  "capabilities": {
    "network": { "hosts": ["api.example.com", "*.cdn.example.com"] }
  }
}
```

| 키 | 타입 | 사용처 | 설명 |
|---|---|---|---|
| `name` | string | `package` | 앱 아이콘 아래에 표시할 이름. 기본값은 진입점이 있는 디렉터리의 이름입니다. |
| `id` | string | `package`, `publish`, `Storage` | 역방향 DNS 형식의 번들 또는 앱 ID(예: `com.yourcompany.app`). `package` 및 `publish`에서 필수입니다. 생략하면 `nsh run`은 `com.example.<name>`을 사용합니다. |
| `version` | string | `package`, `publish` | `1.2.0`처럼 사용자에게 표시할 버전(`CFBundleShortVersionString`, Android의 `versionName`). |
| `build` | number | `package`, `publish` | 빌드 번호(`CFBundleVersion`, Android의 `versionCode`). 서명된 패키지는 릴리스할 때마다 번호를 올려야 합니다. |
| `minRuntime` | string | `publish` | 서명된 패키지를 실행할 수 있는 최소 NSH 런타임 버전(예: `0.2.0`). 기본값은 CLI 버전입니다([서명된 패키지](../../docs/signed-packages.md#which-runtimes-run-a-package)). |
| `icon` | string | `package` | `nsh.json`을 기준으로 한 정사각형 PNG의 경로. 권장 크기는 1024×1024입니다. |
| `tailwind` | object | 모든 빌드 | `class` 문자열을 해석하는 데 사용하는 Tailwind 테마를 확장합니다([Tailwind](../../docs/tailwind.md#theme)). |
| `appearance` | string | 모든 빌드, `package` | `"light"`, `"dark"`, `"system"`(기본값) 중 하나입니다. 밝은 모드나 어두운 모드를 고정하려면 여기서 지정하세요. 앱을 시작할 때부터 `colorScheme()`, `dark:` 클래스, 컴포넌트 테마와 플랫폼 컨트롤에 적용됩니다([기기와 환경](../../docs/device-and-environment.md#dark-mode)). |
| `theme` | string 또는 object | 모든 빌드 | 앱의 디자인 토큰입니다. 기본 `neutral` 테마에 shadcn/ui의 `cssVars`를 덮어쓰며 토큰 클래스, `token()`, Rust 컴포넌트가 이 값을 사용합니다. `nsh.json` 기준 테마 파일 경로 또는 테마 객체를 직접 지정합니다([Rust 테마](./rust.md#테마), [테마 토큰](../../docs/tailwind.md#theme-tokens)). |
| `statusBar` | object | 모든 빌드, `package` | iOS와 Android의 페이지 및 대화상자에 적용할 상태 표시줄 기본값입니다. `{ "foreground": "auto" \| "light" \| "dark", "hidden": boolean }`에서 두 필드 모두 선택 사항입니다. 패키징한 iOS 앱은 시작 화면부터 이를 적용합니다([상태 표시줄](../../modules/nsh-base.md#status-bar)). |
| `capabilities` | object | 모든 빌드 | 네트워크 호스트, 웹 뷰 호스트, 상태 표시줄 등 앱이 VM 밖에서 접근할 수 있는 기능입니다([샌드박스와 권한](./sandbox-and-capabilities.md)). |
| `resolve` | object | 모든 빌드 | 패키지명 형태의 import를 해석할 때 쓸 npm 패키지의 export 조건과 별칭을 지정합니다([npm 패키지](#npm-패키지)). |
| `rust` | string 또는 string[] | 모든 명령 | `nsh.json` 기준 Rust 크레이트의 경로 또는 목록입니다. TSX에서 각 크레이트의 `#[native_module]` 지정자로 해당 컴포넌트와 모듈을 가져옵니다([여러 크레이트 사용](./rust.md#여러-크레이트-사용하기)). |
| `web` | object | `package` | 웹 패키지가 페이지에 추가할 내용입니다. `web.host`는 `nsh.json` 기준 JS 모듈의 경로입니다. 이 모듈은 앱의 [`host`](../../modules/nsh.md#host) 호출에 응답하는 `createHost(player)`를 내보냅니다. `web.connect`에는 모듈 자체의 요청이 접근하는 호스트를 `capabilities.network.hosts` 패턴으로 지정하며 페이지의 콘텐츠 보안 정책(CSP)이 이를 허용합니다([웹](../../docs/web.md#the-hosts-functions)). |
| `root` | string | `run`, `package` | Rust 앱의 루트 컴포넌트 경로(`App`, `shell::Gallery` 등)입니다. 기준은 `rust`에 지정한 크레이트이며 목록인 경우 첫 번째 크레이트입니다. 앱 디렉터리에 대해 `nsh run`이나 `nsh package`를 실행하면 모든 플랫폼에서 QuickJS 없이 이 루트를 마운트합니다. `nsh run App.tsx`는 TSX 진입점을 실행합니다([Rust로 개발하기](./rust.md)). |

`nsh package`와 `nsh publish`의 플래그(`--name`, `--id`, `--app-version`, `--build`, `--icon`, `--min-runtime`)는 파일의 값보다 우선합니다([CLI](./cli.md)).

앱을 빌드할 때 `tailwind`, `theme`, `appearance`, `statusBar`, `capabilities`, `resolve`를 검사합니다. 설정에 오타가 있으면 빌드 오류로 알려 줍니다.

```text
error building App.tsx failed
nsh.json: tailwind: unknown theme key `colours`
```


### capabilities

독립 실행형 앱은 개발자가 직접 관리하므로 `capabilities` 키가 없으면 네트워크, 웹 뷰, 상태 표시줄을 자유롭게 사용할 수 있습니다. 키를 설정하면 개발 중에도 패키징한 앱에서도 목록에 명시한 항목만 허용됩니다.

```json
"capabilities": {
  "network": {
    "hosts": ["api.example.com", "*.cdn.example.com"],
    "http": [{ "host": "quotes.example.com", "methods": ["GET"], "pathPrefix": "/v1/" }]
  },
  "webview": { "hosts": ["*.example.com"] },
  "statusBar": true
}
```

`network`는 `fetch`, 원격 이미지, 동영상 소스에 적용됩니다. 로컬 에셋은 항상 허용됩니다. NSH를 임베딩한 호스트 앱은 모듈이 선언한 범위보다 적은 권한을 부여할 수 있습니다. 자세한 내용은 [샌드박스와 권한](./sandbox-and-capabilities.md)을 참고하세요.

## tsconfig.json과 .nsh/types

NSH는 `node_modules` 없이도 작동합니다. 앱에 필요한 명령(`init`, `build`, `test`, `dev`, `run`, `package`, `publish`, `check`)을 실행하면 NSH의 TypeScript 선언 전체를 `.nsh/types`에, TypeScript 설정을 `.nsh/tsconfig.json`에 기록합니다. 앱의 `tsconfig.json`은 이 설정을 확장합니다.

```json
{
  "extends": "./.nsh/tsconfig.json",
  "include": ["**/*.ts", "**/*.tsx"]
}
```

`.nsh/tsconfig.json`은 `nsh` import와 앱의 크레이트를 `.nsh/types`에 연결합니다. 전역 API, 에셋, 토큰의 선언을 포함하고 NSH 앱에 맞는 컴파일러 옵션(`lib`, `jsx`, `strict` 등)을 설정합니다. 새 버전에서 라이브러리나 경로가 추가되면 다음 빌드에서 자동 반영되며, 앱의 `tsconfig.json`은 그대로 유지됩니다. 직접 추가한 옵션은 NSH의 설정보다 우선합니다.

| `.nsh/types/nsh/`의 파일 | 선언 내용 |
|---|---|
| `index.d.ts` | `nsh`의 `signal`, `computed`, `effect`, `mount`, `View`, `Text` 등을 선언하고 아래 타입을 다시 내보냄 |
| `types.d.ts` | 스타일(`ViewStyle`, `TextStyle` 등), 각 요소의 속성과 이벤트, `ref`가 받는 참조(`ViewRef`, `InputRef` 등) |
| `jsx-runtime.d.ts` | TypeScript가 `jsxImportSource`로 찾는 JSX 런타임(`JSX` 네임스페이스) |
| `globals.d.ts` | `If`, `Each`, `console`, 타이머, `requestAnimationFrame`, `fetch` 등 [전역 API](../../modules/globals.md) |
| `assets.d.ts` | 이미지 또는 글꼴을 가져왔을 때의 타입 |
| `tokens.d.ts` | 앱의 `"theme"`에서 생성한 디자인 토큰 타입 `Tokens`. [`token()`](../../modules/nsh.md#token)의 타입 검사에 사용 |
| `mobile.d.ts`, `desktop.d.ts` | 모듈별 선언 파일(`nsh/mobile`, `nsh/desktop`) |
| `base.d.ts` | `nsh/base` |

`lib`는 `DOM`을 제외한 `ES2023`이므로 `document`와 `window`는 없습니다. `.nsh` 파일은 내용이 바뀔 때만 다시 생성되며 `.gitignore`에서 이미 제외합니다. 직접 수정하거나 커밋하지 마세요. 프로젝트를 복제한 뒤 편집기에서 `nsh`를 찾지 못하면 `nsh check` 같은 빌드 명령을 한 번 실행하세요.

`.nsh/tsconfig.json`은 `"ESNext.Disposable"`을 추가해 런타임이 지원하는 `using` 선언과 `Symbol.dispose`의 타입을 제공합니다. 예를 들어 `using account = new Account()`는 블록 끝에서 [Rust 클래스](./rust.md)의 값을 해제합니다. `jsxImportSource: "nsh"`와 `jsx: "preserve"`는 편집기에만 적용됩니다. NSH는 `tsconfig.json` 설정과 관계없이 자체 컴파일러로 JSX를 컴파일합니다. TypeScript 타입 검사는 편집기 또는 `nsh check`가 수행하며 다른 명령은 타입을 제거하고 진행합니다.

이전 버전의 `nsh init`으로 만든 앱이 기본 템플릿 그대로의 `tsconfig.json`을 사용하면 다음 빌드 때 NSH가 새 설정 파일로 갱신하고 그 사실을 출력합니다. 직접 수정한 `tsconfig.json`은 직접 관리합니다. `./.nsh/tsconfig.json`을 확장하지 않았다는 `nsh check` 안내가 나오면 `extends`를 추가하고 확장한 설정과 겹치는 옵션을 제거하세요.

## 모듈

Rust 앱은 일반 크레이트처럼 `mod screens;` 등으로 코드를 모듈화하고 crates.io에서 의존성을 가져옵니다. NSH의 주요 크레이트는 기본 요소, 시그널, 타입 지정 스타일을 제공하는 `nsh-core`(`use nsh_core::prelude::*`)와 기본 컴포넌트 및 테마를 제공하는 `nsh-base`입니다.

TSX에서는 앱을 원하는 수의 `.ts` 및 `.tsx` 모듈로 나눈 뒤 상대 경로로 가져옵니다. 확장자는 생략할 수 있습니다.

```tsx
import { Header } from "./components/Header";
import { formatPrice } from "./lib/format";
import strings from "./strings.json";
```

이 모듈들은 모두 하나의 스크립트로 번들링됩니다. JSON 파일을 가져오면 파싱된 값을 받습니다.

### 바이너리에서 선택 기능 제외

NSH를 임베딩하거나 직접 작성한 `main`을 사용해 Cargo로 실행 파일을 빌드할 때는 선택 기능을 제외할 수 있습니다. 플랫폼 크레이트는 기본적으로 모든 기능을 켭니다. 플랫폼 크레이트와 `nsh-base`에 `default-features = false`를 지정하고 `js`, `shader`, `editor`(`DiffView` 포함), `rich-editor`, `plot`, `highlight`, `math`, `zlib`, `crypto` 중 필요한 기능을 명시하세요. 빌드에서 제외한 기능을 가져오는 앱은 로드 시점에 실패합니다. `nsh package`로 빌드한 앱에는 항상 모든 기능이 포함됩니다.

### 필요한 시점에 코드 불러오기

브라우저에서와 마찬가지로 앱 모듈을 `import()`하면 해당 모듈의 내보내기 값을 담은 Promise가 반환됩니다.

```tsx
const open = async () => {
  const { Settings } = await import("./screens/Settings");
  screen.set(() => Settings);
};
```

기기에 배포하는 앱은 하나의 번들에 모듈이 모두 들어 있으므로 Promise가 즉시 이행됩니다. 분할 빌드한 웹 앱(`nsh build --split`)은 해당 모듈과 그 모듈에서만 사용하는 코드를 별도 파일에 담고 첫 `import()` 호출 때 다운로드합니다([웹](../../docs/web.md#split-bundles)). 번들러가 대상 모듈을 알아야 하므로 지정자는 상대 경로 문자열 리터럴이어야 합니다.

### NSH 모듈

`nsh` 및 `nsh/...` import는 런타임이 네이티브로 제공하므로 번들에 들어가지 않습니다.

| 모듈 | 주요 기능 |
|---|---|
| `nsh` | 반응성(`signal`, `computed`, `effect` 등), `mount`, `split`, `context`, 기본 요소 `View`, `Text`, `Scroll`, `Image`, `List`, `Canvas`, `WebView`, `Video`(`View`의 `selectable`로 텍스트 선택), 안전 영역 여백(`safeArea`, `safeAreaInsets`), 소프트웨어 키보드(`Keyboard`, `keyboardFrame`), 네이티브 애니메이션, `PanResponder`, 환경 정보(`platform`, `windowSize`, `colorScheme`, `fontScale`, `pixelRatio`, `locale`), 화면 읽기 프로그램 및 동작 줄이기 상태, 접근성 안내, 클립보드, 사진 선택기, 저장소, 호스트 채널, 단축키, 임베딩 앱이 등록한 컴포넌트와 함수([레퍼런스](../../modules/nsh.md)) |
| `nsh/base` | 스타일이 지정되지 않은 기본 UI 컴포넌트. 탐색 스택과 페이지(`NavStack`, `NavPage`, `createNavStack`), 대화상자(`Dialog`), 버튼(`Button`), 텍스트 입력(`Input`), 시스템 경고 대화상자(`AlertDialog`). 페이지와 대화상자는 상태 표시줄(`statusBar`)을 설정할 수 있습니다([레퍼런스](../../modules/nsh-base.md)). |
| `nsh/mobile` | iOS 및 Android용 기능. `TabBar`, `InputAccessoryView`(iOS), `BackHandler`(Android)([레퍼런스](../../modules/nsh-mobile.md)) |
| `nsh/desktop` | macOS, Windows, Linux용 기능. `Window`, `titleBar`, `Menu`([레퍼런스](../../modules/nsh-desktop.md)) |

각 모듈의 상세 내용은 [모듈 레퍼런스](../modules/index.md)에 있습니다. NSH에 없는 모듈(`nsh/foo`)을 가져오면 빌드 오류가 발생합니다. `import { Buton } from "nsh/base"`처럼 내보내지 않는 이름을 코드에 쓴 경우도 마찬가지입니다. 오류는 import 위치를 가리키고 가장 유사한 이름을 안내합니다(``did you mean `Button`?``). 타입으로만 사용하는 이름은 `nsh check`로 검사합니다.

### npm 패키지

NSH 자체는 npm 의존성이 없습니다. 패키지 관리자로 패키지를 설치했다면 번들러가 `node_modules`에서 패키지명으로 된 import를 해석합니다. 런타임은 `fetch`, 스트림, `URL`, `TextEncoder` 같은 웹 API를 제공하고 Node 전용 API(`fs`, `Buffer`, `process`)를 제공하지 않으므로 패키지의 브라우저용 빌드를 선택합니다. DOM은 제공하지 않으므로 DOM에 접근하는 패키지는 실행할 수 없습니다.

브라우저용 빌드에서 DOM을 쓰는 패키지도 워커나 React Native용 빌드를 따로 제공하기도 합니다. `resolve.conditions`로 기본값 `["browser"]` 대신 사용할 export 조건을 지정하세요. `import`, `require`, `module`, `default`는 항상 적용되고, `browser`를 빼면 패키지의 `module` 및 `main` 필드로 해석합니다. `resolve.alias`는 import를 `nsh.json` 기준 경로(`./` 또는 `../`), 다른 패키지, 또는 빈 모듈을 뜻하는 `false`로 바꿉니다. TypeScript(`nsh check`, 편집기)는 소스에 작성한 import 이름으로 타입을 검사합니다. 설치된 패키지가 아닌 이름은 앱의 `.d.ts` 파일에서 타입을 제공하세요.

```json
"resolve": {
  "conditions": ["worker"],
  "alias": { "dom-only-helper": "./shims/helper.ts", "node-polyfill": false }
}
```

```ts
// shims/aliases.d.ts
declare module "node-polyfill";
```

`solid-js`를 가져오면 번들러가 오류를 내고 대신 사용할 `nsh`의 `signal`, `computed`, `effect`를 안내합니다.

## 에셋

이미지와 글꼴은 앱과 함께 배포됩니다. Rust 크레이트는 `asset!`으로 컴파일 시점에 `assets/`의 파일을 읽고, `#[native_module]` 함수에서 `registry.assets(assets!())`로 등록해 함께 배포합니다. TSX에서는 모듈처럼 가져오면 번들러가 각 파일과 이미지의 픽셀 밀도별 변형을 기록합니다. `nsh build`는 이를 `.nsh/out/assets/`에 복사하고 `nsh package`는 앱에 포함합니다. 개발 중에는 앱에 직접 제공합니다.

```rust tabs
use nsh_core::element::{FontAsset, ImageAsset, asset};

const LOGO: ImageAsset = asset!("logo@2x.png");      // its url, and its size: 64×64 points
const INTER: FontAsset = asset!("Inter.ttf");        // its uri, and its family: "Inter"

image(LOGO)
text("Hello").font_family(INTER.family)
```

```tsx tabs
import { Image, Text } from "nsh";
import logo from "./assets/logo@2x.png";   // { url: "asset:/assets/logo@2x.png", size: { width: 64, height: 64 } }
import Inter from "./assets/Inter.ttf";     // "Inter"

<Image src={logo} />
<Text style={{ fontFamily: Inter }}>Hello</Text>
```

| 확장자 | import 결과 |
|---|---|
| `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.heic` | 포인트 단위 크기를 가진 `ImageAsset`(`{ url, size, scales? }`). `@2x`, `@3x` 접미사가 있으면 픽셀 크기를 각각 2와 3으로 나눕니다. 접미사가 없는 파일을 가져오면 같은 이름의 `@1.5x`~`@4x` 파일도 함께 배포하며 `scales`에 기록된 배율 중 화면에 맞는 변형을 불러옵니다([픽셀 밀도](../../docs/images-and-assets.md#pixel-densities)). |
| `.svg` | SVG의 `width`와 `height` 또는 `viewBox`를 기준으로 크기를 포인트 단위로 지정한 `ImageAsset`. |
| `.ttf`, `.otf`, `.woff`, `.woff2` | 글꼴 패밀리 이름 문자열. 첫 프레임 전에 글꼴을 등록합니다. |
| `.json` | 파싱한 JSON 값. |

[이미지와 에셋](../../docs/images-and-assets.md), [글꼴](../../docs/fonts.md)도 참고하세요.

## 생성되는 디렉터리

- **`.nsh/`**에는 NSH가 생성한 파일이 모입니다. 타입 선언(`.nsh/types`), Rust 앱 실행기의 소스(`.nsh/rust`), `nsh run`이 시뮬레이터와 기기에 설치하는 Catalyst 호스트 앱 및 Android 개발 APK, 기본 출력 디렉터리 `.nsh/out`이 여기 들어갑니다. `nsh build`는 `bundle.js`, `bundle.js.map`, `bundle.nshc`, `capabilities.json`, `assets/`를 출력하며, Rust 앱이면 `app.json`과 `assets/`를 출력합니다. `nsh package`는 `.app`, `.ipa`, `.apk` 또는 웹사이트를 출력합니다. `-o`로 출력 경로를 바꿀 수 있습니다. `.nsh/`는 삭제해도 다시 생성됩니다.
- **`target/`**은 앱 크레이트와 실행기를 위한 Cargo 빌드 디렉터리입니다. `CARGO_TARGET_DIR`로 위치를 지정할 수 있습니다.
